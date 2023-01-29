use crate::{
    media::{MediaEngine, MediaFormat, MediaStatus, MediaTimeout, MimeType},
    window::NativeWindow,
    MediaPlayerEvent, NativeLibSingleton,
};
use bytes::{Buf, Bytes};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver};
use webrtc::{
    peer_connection::peer_connection_state::RTCPeerConnectionState, rtcp,
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication, rtp,
    rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote,
    util::marshal::Unmarshal,
};
use webrtc_helper::{
    codecs::{Codec, CodecType, H264Codec, H264PayloadReader, H264PayloadReaderError, H264Profile},
    decoder::DecoderBuilder,
    WebRtcPeer,
};

const RTP_PACKET_MAX_SIZE: usize = 1500;
const READ_TIMEOUT_MILLIS: u64 = 5000;
const RTCP_PLI_INTERVAL_MILLIS: u64 = 50;
const NALU_TYPE_BITMASK: u8 = 0x1F;

pub struct AndroidDecoderBuilder {
    singleton: Arc<NativeLibSingleton>,
    codecs: Vec<Codec>,
    codec_map: HashMap<MimeType, String>,
}

impl DecoderBuilder for AndroidDecoderBuilder {
    fn supported_codecs(&self) -> &[Codec] {
        &self.codecs
    }

    fn codec_type(&self) -> CodecType {
        CodecType::Video
    }

    fn build(
        self: Box<Self>,
        track: Arc<TrackRemote>,
        rtp_receiver: Arc<RTCRtpReceiver>,
        peer: Arc<WebRtcPeer>,
    ) {
        let singleton = self.singleton;
        let codec_map = self.codec_map;

        let handle = tokio::runtime::Handle::current();
        handle.spawn(async move {
            log::info!("start_decoder");
            if let Err(e) = start_decoder(track, rtp_receiver, peer, singleton, codec_map).await {
                log::error!("Decoder failure: {e:?}");
            }
            log::info!("start_decoder exit");
        });
    }
}

#[derive(Debug)]
enum DecoderError {
    MediaEngine(MediaStatus),
    RtcpSend(webrtc::Error),
    AttachThread(jni::errors::Error),
    SetAspectRatio(jni::errors::Error),
    UnknownMimeType,
    FailedToGetReceiver,
    HeaderParsing,
    NativeWindowCreate,
    NoDecoderFound,
    WebRtcDisconnected,
    ApplicationClosed,
}

macro_rules! impl_from {
    ($t:ty, $e:tt) => {
        impl From<$t> for DecoderError {
            #[inline]
            fn from(e: $t) -> Self {
                DecoderError::$e(e)
            }
        }
    };
}

impl_from!(MediaStatus, MediaEngine);
impl_from!(webrtc::Error, RtcpSend);
impl_from!(jni::errors::Error, AttachThread);

async fn start_decoder(
    track: Arc<TrackRemote>,
    _rtp_receiver: Arc<RTCRtpReceiver>,
    peer: Arc<WebRtcPeer>,
    singleton: Arc<NativeLibSingleton>,
    codec_map: HashMap<MimeType, String>,
) -> Result<(), DecoderError> {
    while peer.connection_state() != RTCPeerConnectionState::Connected {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // TODO: Check sdp_fmtp_line for SPS/PPS
    let codec_params = track.codec().await;
    let mime_type = MimeType::from_str(&codec_params.capability.mime_type)
        .map_err(|_| DecoderError::UnknownMimeType)?;

    let decoder_name = codec_map
        .get(&mime_type)
        .ok_or(DecoderError::NoDecoderFound)?;

    let mut buf = vec![0u8; RTP_PACKET_MAX_SIZE];
    let mut receiver = singleton
        .get_event_receiver()
        .ok_or(DecoderError::FailedToGetReceiver)?;

    let decoder = create_decoder(
        &singleton,
        &track,
        &peer,
        mime_type,
        decoder_name,
        &mut receiver,
        &mut buf,
    )
    .await?;

    let pli = PictureLossIndication {
        sender_ssrc: 0,
        media_ssrc: track.ssrc(),
    };
    let pli = [Box::new(pli) as Box<(dyn rtcp::packet::Packet + Send + Sync + 'static)>];
    let mut last_pli_time = SystemTime::UNIX_EPOCH;

    async fn send_pli(
        peer: &Arc<WebRtcPeer>,
        last_pli_time: &mut SystemTime,
        pli: &[Box<dyn rtcp::packet::Packet + Send + Sync>],
    ) -> Result<(), DecoderError> {
        const PLI_INTERVAL: Duration = Duration::from_millis(RTCP_PLI_INTERVAL_MILLIS);
        let now = SystemTime::now();
        if let Ok(duration) = now.duration_since(*last_pli_time) {
            if duration > PLI_INTERVAL {
                peer.write_rtcp(&pli).await?;
                *last_pli_time = now;
            }
        }
        Ok(())
    }

    let mut input_buffer = decoder.dequeue_input_buffer(MediaTimeout::INFINITE)?;
    let mut has_reference_frame = false;
    let mut render = true;
    let mut reader = H264PayloadReader::new(&mut input_buffer);
    let mut last_sequence_number = None;

    loop {
        if peer.connection_state() != RTCPeerConnectionState::Connected {
            return Err(DecoderError::WebRtcDisconnected);
        }

        match receiver.try_recv() {
            Ok(msg) => match msg {
                MediaPlayerEvent::MainActivityDestroyed => {
                    return Err(DecoderError::ApplicationClosed);
                }
                MediaPlayerEvent::SurfaceCreated(surface) => {
                    let env = singleton.vm.attach_current_thread()?;
                    let native_window = NativeWindow::new(&env, &surface.as_obj())
                        .ok_or(DecoderError::NativeWindowCreate)?;

                    // Rendering is possible again
                    render = true;
                    decoder.set_output_surface(&native_window)?;
                }
                MediaPlayerEvent::SurfaceDestroyed => {
                    // Stop rendering when there is no surface to render to
                    render = false;
                }
            },
            Err(TryRecvError::Disconnected) => {
                return Err(DecoderError::ApplicationClosed);
            }
            Err(TryRecvError::Empty) => {
                match tokio::time::timeout(
                    Duration::from_millis(READ_TIMEOUT_MILLIS),
                    track.read(&mut buf),
                )
                .await
                {
                    Err(_) => {
                        log::error!("Timed-out while reading from `TrackRemote`");
                        continue;
                    }
                    Ok(read_result) => match read_result {
                        Err(_) => {
                            send_pli(&peer, &mut last_pli_time, &pli).await?;
                            reader = H264PayloadReader::new(&mut input_buffer);
                        }
                        Ok((n, _)) => {
                            let mut b = &buf[..n];

                            // Unmarshaling the header would move `b` to point to the payload
                            let Some(header) = unmarshal_header(&mut b) else {
                                return Err(DecoderError::HeaderParsing);
                            };

                            // Check sequence number for skipped values
                            if let Some(last_sequence_number) = &mut last_sequence_number {
                                if header.sequence_number.wrapping_sub(*last_sequence_number) != 1 {
                                    has_reference_frame = false;
                                    send_pli(&peer, &mut last_pli_time, &pli).await?;
                                }
                                *last_sequence_number = header.sequence_number;
                            } else {
                                last_sequence_number = Some(header.sequence_number);
                            }

                            match reader.read_payload(b) {
                                Ok(num_bytes) => {
                                    if !has_reference_frame {
                                        let nalu_type = input_buffer[4] & NALU_TYPE_BITMASK;
                                        if nalu_type != 5 {
                                            send_pli(&peer, &mut last_pli_time, &pli).await?;
                                            reader = H264PayloadReader::new(&mut input_buffer);
                                            continue;
                                        } else {
                                            has_reference_frame = true;
                                        }
                                    }

                                    match decoder.queue_input_buffer(
                                        input_buffer,
                                        num_bytes as _,
                                        0,
                                        0,
                                    ) {
                                        Ok(_) => {
                                            match decoder.release_output_buffer(
                                                MediaTimeout::INFINITE,
                                                render,
                                            ) {
                                                Ok(_) => {
                                                    input_buffer = decoder.dequeue_input_buffer(
                                                        MediaTimeout::INFINITE,
                                                    )?;
                                                    reader =
                                                        H264PayloadReader::new(&mut input_buffer);
                                                    continue;
                                                }
                                                Err(e) => {
                                                    log::error!("release_output_buffer error: {e}")
                                                }
                                            }
                                        }
                                        Err(e) => log::error!("queue_input_buffer error: {e}"),
                                    }

                                    has_reference_frame = false;
                                    send_pli(&peer, &mut last_pli_time, &pli).await?;
                                    input_buffer =
                                        decoder.dequeue_input_buffer(MediaTimeout::INFINITE)?;
                                    reader = H264PayloadReader::new(&mut input_buffer);
                                }
                                Err(H264PayloadReaderError::NeedMoreInput(r)) => reader = r,
                                Err(_) => {
                                    has_reference_frame = false;
                                    send_pli(&peer, &mut last_pli_time, &pli).await?;
                                    reader = H264PayloadReader::new(&mut input_buffer);
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

async fn create_decoder(
    singleton: &Arc<NativeLibSingleton>,
    track: &Arc<TrackRemote>,
    peer: &Arc<WebRtcPeer>,
    mime_type: MimeType,
    decoder_name: &str,
    receiver: &mut UnboundedReceiver<MediaPlayerEvent>,
    buf: &mut [u8],
) -> Result<MediaEngine, DecoderError> {
    let mut native_window = None;
    let mut format = None;
    let mut parameter_sets = None;

    let mut payload_buf = vec![0u8; RTP_PACKET_MAX_SIZE];
    let mut reader = H264PayloadReader::new(&mut payload_buf);

    let pli = PictureLossIndication {
        sender_ssrc: 0,
        media_ssrc: track.ssrc(),
    };
    let pli = [Box::new(pli) as Box<(dyn rtcp::packet::Packet + Send + Sync + 'static)>];
    let mut last_pli_time = SystemTime::UNIX_EPOCH;

    async fn send_pli(
        peer: &Arc<WebRtcPeer>,
        last_pli_time: &mut SystemTime,
        pli: &[Box<dyn rtcp::packet::Packet + Send + Sync>],
    ) -> Result<(), DecoderError> {
        const PLI_INTERVAL: Duration = Duration::from_millis(RTCP_PLI_INTERVAL_MILLIS);
        let now = SystemTime::now();
        if let Ok(duration) = now.duration_since(*last_pli_time) {
            if duration > PLI_INTERVAL {
                peer.write_rtcp(&pli).await?;
                *last_pli_time = now;
            }
        }
        Ok(())
    }

    loop {
        if peer.connection_state() != RTCPeerConnectionState::Connected {
            return Err(DecoderError::WebRtcDisconnected);
        }

        // If everything has been gathered, build the decoder
        if native_window.is_some() && format.is_some() && parameter_sets.is_some() {
            let mut decoder = MediaEngine::create_by_name(decoder_name)?;
            decoder.initialize(&format.unwrap(), native_window, false)?;

            let data: Bytes = parameter_sets.unwrap();
            decoder.submit_codec_config(&data)?;
            return Ok(decoder);
        }

        match receiver.try_recv() {
            Ok(msg) => match msg {
                MediaPlayerEvent::MainActivityDestroyed => {
                    return Err(DecoderError::ApplicationClosed)
                }
                MediaPlayerEvent::SurfaceCreated(surface) => {
                    let env = singleton.vm.attach_current_thread()?;
                    native_window = Some(
                        NativeWindow::new(&env, &surface.as_obj())
                            .ok_or(DecoderError::NativeWindowCreate)?,
                    );
                }
                MediaPlayerEvent::SurfaceDestroyed => {
                    native_window = None;
                }
            },
            Err(TryRecvError::Disconnected) => return Err(DecoderError::ApplicationClosed),
            Err(TryRecvError::Empty) => {
                match tokio::time::timeout(
                    Duration::from_millis(READ_TIMEOUT_MILLIS),
                    track.read(buf),
                )
                .await
                {
                    Err(_) => {
                        log::info!("Timed-out while reading from `TrackRemote`");
                        continue;
                    }
                    Ok(read_result) => match read_result {
                        Err(_) => {
                            send_pli(peer, &mut last_pli_time, &pli).await?;
                            continue;
                        }
                        Ok((n, _)) => {
                            let mut b = &buf[..n];

                            // Unmarshaling the header would move `b` to point to the payload
                            let Some(_header) = unmarshal_header(&mut b) else {
                                return Err(DecoderError::HeaderParsing);
                            };

                            match reader.read_payload(b) {
                                Ok(num_bytes) => {
                                    if let Some((width, height)) =
                                        H264Codec::get_resolution(&payload_buf[..num_bytes])
                                    {
                                        let width = width as i32;
                                        let height = height as i32;
                                        let env = singleton.vm.attach_current_thread()?;
                                        singleton
                                            .set_media_player_aspect_ratio(&env, width, height)
                                            .map_err(|e| DecoderError::SetAspectRatio(e))?;
                                        format = Some(MediaFormat::new()?);
                                        if let Some(format) = &mut format {
                                            format.set_mime_type(mime_type);
                                            format.set_realtime_priority(true);
                                            format.set_resolution(width, height);
                                            format.set_max_resolution(width, height);
                                            if singleton.get_api_level(&env)? >= 30 {
                                                format.set_low_latency(true);
                                            }
                                        }
                                        parameter_sets =
                                            Some(Bytes::copy_from_slice(&payload_buf[..num_bytes]));
                                    } else {
                                        send_pli(peer, &mut last_pli_time, &pli).await?;
                                    }
                                    reader = H264PayloadReader::new(&mut payload_buf);
                                }
                                Err(H264PayloadReaderError::NeedMoreInput(_)) => {
                                    send_pli(peer, &mut last_pli_time, &pli).await?;
                                    reader = H264PayloadReader::new(&mut payload_buf);
                                }
                                Err(_) => {
                                    send_pli(peer, &mut last_pli_time, &pli).await?;
                                    reader = H264PayloadReader::new(&mut payload_buf);
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

fn unmarshal_header(buffer: &mut &[u8]) -> Option<rtp::header::Header> {
    let header = rtp::header::Header::unmarshal(buffer).ok()?;
    if header.padding {
        let payload_len = buffer.remaining();
        if payload_len > 0 {
            let padding_len = buffer[payload_len - 1] as usize;
            if padding_len <= payload_len {
                *buffer = &buffer[..payload_len - padding_len];
                Some(header)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        Some(header)
    }
}

impl AndroidDecoderBuilder {
    pub fn new(
        singleton: Arc<NativeLibSingleton>,
    ) -> Result<AndroidDecoderBuilder, jni::errors::Error> {
        let mut codecs = Vec::new();
        let mut codec_map = HashMap::new();
        {
            // Array of (mime type str, Android profile id -> Codec)
            let mime_types: [(MimeType, fn(i32) -> Option<Codec>); 3] = [
                (MimeType::VideoAv1, |_| None),
                (MimeType::VideoH265, |_| None),
                (MimeType::VideoH264, |id| {
                    h264_profile_from_android_id(id).map(|profile| H264Codec::new(profile).into())
                }),
            ];

            let env = singleton.global_vm().attach_current_thread()?;

            for (mime_type, converter) in mime_types {
                let decoder_name = match singleton.choose_decoder_for_type(&env, mime_type) {
                    Ok(Some(decoder_name)) => decoder_name,
                    Ok(None) => {
                        log::info!("No decoder for {mime_type:?}");
                        continue;
                    }
                    Err(e) => {
                        log::error!("Error while finding decoder: {e}");
                        continue;
                    }
                };
                let profiles =
                    match singleton.list_profiles_for_decoder(&env, &decoder_name, mime_type) {
                        Ok(Some(profiles)) => profiles,
                        Ok(None) => {
                            log::info!("Possibly invalid decoder name: {decoder_name}");
                            continue;
                        }
                        Err(e) => {
                            log::error!("Error while listing profiles: {e}");
                            continue;
                        }
                    };
                for id in profiles {
                    if let Some(codec) = converter(id) {
                        codecs.push(codec);
                    }
                }
                codec_map.insert(mime_type, decoder_name);
            }
        }
        Ok(AndroidDecoderBuilder {
            singleton,
            codecs,
            codec_map,
        })
    }
}

// https://developer.android.com/reference/android/media/MediaCodecInfo.CodecProfileLevel
fn h264_profile_from_android_id(id: i32) -> Option<H264Profile> {
    match id {
        1 => Some(H264Profile::Baseline),
        2 => Some(H264Profile::Main),
        4 => Some(H264Profile::Extended),
        8 => Some(H264Profile::High),
        16 => Some(H264Profile::High10),
        32 => Some(H264Profile::High422),
        64 => Some(H264Profile::High444),
        65536 => Some(H264Profile::ConstrainedBaseline),
        524288 => Some(H264Profile::ConstrainedHigh),
        id => {
            log::info!("Unknown H.264 profile id: {}", id);
            None
        }
    }
}
