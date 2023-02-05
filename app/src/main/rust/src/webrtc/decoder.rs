use crate::{
    media::{MediaEngine, MediaFormat, MediaStatus, MediaTimeout, MimeType},
    window::NativeWindow,
    MediaPlayerEvent, NativeLibSingleton,
};
use bytes::Bytes;
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};
use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver};
use webrtc::{
    peer_connection::peer_connection_state::RTCPeerConnectionState, rtcp,
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication,
    rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote,
};
use webrtc_helper::{
    codecs::{Codec, CodecType, H264Codec, H264PayloadReader, H264Profile},
    decoder::DecoderBuilder,
    util::reorder_buffer::{PayloadReader, ReorderBuffer, ReorderBufferError},
    WebRtcPeer,
};

const PLI_INTERVAL: Duration = Duration::from_millis(50);
const NALU_MAX_SIZE: usize = 250_000;
const NALU_TYPE_BITMASK: u8 = 0x1F;
const NALU_TYPE_IDR_PIC: u8 = 5;

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
    TokioRuntimeCreationFailed,
    ThreadJoinFailed,
    UnknownMimeType,
    FailedToGetReceiver,
    NativeWindowCreate,
    NoDecoderFound,
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

async fn send_pli(
    peer: &Arc<WebRtcPeer>,
    last_pli_time: &mut SystemTime,
    pli: &[Box<dyn rtcp::packet::Packet + Send + Sync>],
) -> Result<(), DecoderError> {
    let now = SystemTime::now();
    if let Ok(duration) = now.duration_since(*last_pli_time) {
        if duration > PLI_INTERVAL {
            peer.write_rtcp(&pli).await?;
            *last_pli_time = now;
        }
    }
    Ok(())
}

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

    let mut receiver = singleton
        .get_event_receiver()
        .ok_or(DecoderError::FailedToGetReceiver)?;

    let decoder = Arc::new(
        create_decoder(
            &singleton,
            &track,
            &peer,
            mime_type,
            decoder_name,
            &mut receiver,
        )
        .await?,
    );

    let exit = Arc::new(AtomicBool::new(false));
    let exit_clone = exit.clone();
    let peer_clone = peer.clone();
    let decoder_clone = decoder.clone();

    let thread_handle = std::thread::spawn(move || {
        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            let local = tokio::task::LocalSet::new();

            // TODO: Try unconstrained task here?
            let res =
                local.block_on(&rt, async move {
                    let peer = peer_clone;
                    let decoder = decoder_clone;
                    let exit = exit_clone;

                    let pli = PictureLossIndication {
                        sender_ssrc: 0,
                        media_ssrc: track.ssrc(),
                    };
                    let pli =
                        [Box::new(pli)
                            as Box<
                                (dyn rtcp::packet::Packet + Send + Sync + 'static),
                            >];
                    let mut last_pli_time = SystemTime::UNIX_EPOCH;

                    let mut has_reference_frame = false;
                    let mut reorder_buffer = ReorderBuffer::new(track);
                    let mut input_buffer = decoder.dequeue_input_buffer(MediaTimeout::INFINITE)?;
                    let mut reader = H264PayloadReader::new_reader(&mut input_buffer);

                    while !exit.load(Ordering::Acquire) {
                        match reorder_buffer.read_from_track(&mut reader).await {
                            Ok(n) => {
                                std::mem::drop(reader);

                                if !has_reference_frame {
                                    let nalu_type = input_buffer[4] & NALU_TYPE_BITMASK;
                                    if nalu_type != NALU_TYPE_IDR_PIC {
                                        send_pli(&peer, &mut last_pli_time, &pli).await?;
                                        reader = H264PayloadReader::new_reader(&mut input_buffer);
                                        continue;
                                    } else {
                                        has_reference_frame = true;
                                    }
                                }

                                let res = decoder.queue_input_buffer(input_buffer, n as _, 0, 0);
                                input_buffer =
                                    decoder.dequeue_input_buffer(MediaTimeout::INFINITE)?;
                                reader = H264PayloadReader::new_reader(&mut input_buffer);
                                match res {
                                    Ok(_) => (), // TODO: Use a channel to signal the other thread?
                                    Err(e) => log::error!("queue_input_buffer error: {e}"),
                                }
                            }
                            Err(e) => {
                                match e {
                                    ReorderBufferError::HeaderParsingError
                                    | ReorderBufferError::TrackRemoteReadError
                                    | ReorderBufferError::PayloadReaderError => {
                                        has_reference_frame = false;
                                        reader.reset();
                                        send_pli(&peer, &mut &mut last_pli_time, &pli).await?;
                                    }
                                    ReorderBufferError::PayloadTooShort => (), // Empty payload?
                                    ReorderBufferError::BufferFull => {
                                        // TODO: Should be NACK
                                        has_reference_frame = false;
                                        reader.reset();
                                        send_pli(&peer, &mut &mut last_pli_time, &pli).await?;
                                    }
                                    _ => (),
                                    // ReorderBufferError::TrackRemoteReadTimeout => todo!(),
                                    // ReorderBufferError::NoMoreSavedPackets => todo!(),
                                    // ReorderBufferError::UnableToMaintainReorderBuffer => todo!(), // TODO: RENAME THIS
                                    // ReorderBufferError::UninitializedSequenceNumber => todo!(),
                                }
                            }
                        }
                    }

                    Result::<(), DecoderError>::Ok(())
                });
            if let Err(e) = res {
                log::error!("{e:?}");
            }
        }
    });

    let mut render = true;

    loop {
        if peer.connection_state() != RTCPeerConnectionState::Connected {
            break;
        }

        match receiver.try_recv() {
            Ok(msg) => match msg {
                MediaPlayerEvent::MainActivityDestroyed => {
                    break;
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
                break;
            }
            Err(TryRecvError::Empty) => {
                if let Err(e) = decoder.release_output_buffer(MediaTimeout::INFINITE, render) {
                    log::error!("release_output_buffer error: {e}");
                }
            }
        }
    }

    exit.store(true, Ordering::Release);
    if let Err(e) = thread_handle.join() {
        log::error!("Error joining thread: {e:?}");
    }
    return Err(DecoderError::ApplicationClosed);
}

async fn create_decoder(
    singleton: &Arc<NativeLibSingleton>,
    track: &Arc<TrackRemote>,
    peer: &Arc<WebRtcPeer>,
    mime_type: MimeType,
    decoder_name: &str,
    receiver: &mut UnboundedReceiver<MediaPlayerEvent>,
) -> Result<MediaEngine, DecoderError> {
    let thread_res = std::thread::scope(|s| {
        let handle = s.spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|_| DecoderError::TokioRuntimeCreationFailed)?;
            let local = tokio::task::LocalSet::new();
            let res =
                local.block_on(&rt, async move {
                    let pli = PictureLossIndication {
                        sender_ssrc: 0,
                        media_ssrc: track.ssrc(),
                    };
                    let pli =
                        [Box::new(pli)
                            as Box<
                                (dyn rtcp::packet::Packet + Send + Sync + 'static),
                            >];
                    let mut last_pli_time = SystemTime::UNIX_EPOCH;

                    let mut reorder_buffer = ReorderBuffer::new(track.clone());
                    let mut payload_buf = vec![0u8; NALU_MAX_SIZE];
                    let mut reader = H264PayloadReader::new_reader(&mut payload_buf);

                    let mut native_window = None;
                    let mut format = None;
                    let mut parameter_sets = None;

                    loop {
                        if peer.connection_state() != RTCPeerConnectionState::Connected {
                            return Err(DecoderError::ApplicationClosed);
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
                            Err(TryRecvError::Disconnected) => {
                                return Err(DecoderError::ApplicationClosed)
                            }
                            Err(TryRecvError::Empty) => {
                                match reorder_buffer.read_from_track(&mut reader).await {
                                    Ok(n) => {
                                        std::mem::drop(reader);

                                        let payload = &payload_buf[..n];

                                        if let Some((width, height)) =
                                            H264Codec::get_resolution(payload)
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
                                                format.set_integer(
                                                    "vendor.rtc-ext-dec-low-latency.enable",
                                                    1,
                                                );
                                            }
                                            parameter_sets = Some(Bytes::copy_from_slice(payload));
                                        } else {
                                            send_pli(peer, &mut last_pli_time, &pli).await?;
                                        }

                                        reader = H264PayloadReader::new_reader(&mut payload_buf);
                                    }
                                    Err(e) => {
                                        match e {
                                            ReorderBufferError::HeaderParsingError
                                            | ReorderBufferError::TrackRemoteReadError
                                            | ReorderBufferError::PayloadReaderError => {
                                                reader.reset();
                                                send_pli(&peer, &mut &mut last_pli_time, &pli)
                                                    .await?;
                                            }
                                            ReorderBufferError::PayloadTooShort => (), // Empty payload?
                                            ReorderBufferError::BufferFull => {
                                                // TODO: Should be NACK
                                                reader.reset();
                                                send_pli(&peer, &mut &mut last_pli_time, &pli)
                                                    .await?;
                                            }
                                            _ => (),
                                            // ReorderBufferError::TrackRemoteReadTimeout => todo!(),
                                            // ReorderBufferError::NoMoreSavedPackets => todo!(),
                                            // ReorderBufferError::UnableToMaintainReorderBuffer => todo!(), // TODO: RENAME THIS
                                            // ReorderBufferError::UninitializedSequenceNumber => todo!(),
                                        }
                                    }
                                }
                            }
                        }
                    }
                });

            res
        });
        handle.join()
    });
    
    thread_res.map_err(|_| DecoderError::ThreadJoinFailed)?
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
