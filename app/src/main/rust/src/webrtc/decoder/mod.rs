mod builder;
mod h264;
mod rtcp_helper;

pub use self::builder::AndroidDecoderBuilder;
use self::rtcp_helper::RateLimitedPli;
use crate::{
    media::{MediaEngine, MediaFormat, MediaStatus, MediaTimeout, MimeType},
    window::NativeWindow,
    MediaPlayerEvent, NativeLibSingleton,
};
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::mpsc::{error::TryRecvError, UnboundedReceiver};
use webrtc::{
    peer_connection::peer_connection_state::RTCPeerConnectionState,
    rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote,
};
use webrtc_helper::{
    codecs::{
        h264::H264Depacketizer,
        util::{Depacketizer, DepacketizerError},
    },
    network::reorder_buffer::{BufferedTrackRemote, ReorderBufferError},
    WebRtcPeer,
};

const PLI_INTERVAL: Duration = Duration::from_millis(50);
const NUM_BUFFERED_PACKETS: usize = 128;
const MAX_NALU_SIZE: usize = 250_000;
const NALU_TYPE_BITMASK: u8 = 0x1F;
const NALU_TYPE_IDR_PIC: u8 = 5;

#[derive(Debug)]
pub enum DecoderError {
    MediaEngine(MediaStatus),
    RtcpSend(webrtc::Error),
    AttachThread(jni::errors::Error),
    SetAspectRatio(jni::errors::Error),
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

async fn start_decoder(
    track: Arc<TrackRemote>,
    _rtp_receiver: Arc<RTCRtpReceiver>,
    peer: Arc<WebRtcPeer>,
    singleton: Arc<NativeLibSingleton>,
    codec_map: HashMap<MimeType, String>,
) -> Result<(), DecoderError> {
    while peer.connection_state() != RTCPeerConnectionState::Connected {
        tokio::time::sleep(Duration::from_millis(10)).await;
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

    let decoder = match mime_type {
        MimeType::AudioPcma => todo!(),
        MimeType::AudioPcmu => todo!(),
        MimeType::AudioOpus => todo!(),
        MimeType::VideoAv1 => todo!(),
        MimeType::VideoH264 => Arc::new(
            create_media_engine::<h264::H264Decoder>(
                &singleton,
                &track,
                &peer,
                mime_type,
                decoder_name,
                &mut receiver,
            )
            .await?,
        ),
        MimeType::VideoH265 => todo!(),
        MimeType::VideoVp8 => todo!(),
    };

    let exit = Arc::new(AtomicBool::new(false));
    let exit_clone = exit.clone();
    let peer_clone = peer.clone();
    let decoder_clone = decoder.clone();

    let join_handle = tokio::spawn(async move {
        let peer = peer_clone;
        let decoder = decoder_clone;
        let exit = exit_clone;

        let mut pli = RateLimitedPli::new(track.ssrc(), PLI_INTERVAL);

        let mut has_reference_frame = false;
        let mut reorder_buffer = BufferedTrackRemote::new(track.clone(), NUM_BUFFERED_PACKETS);
        let mut input_buffer = decoder.dequeue_input_buffer(MediaTimeout::INFINITE)?;
        let mut reader = H264Depacketizer::wrap_buffer(&mut input_buffer);

        // DEBUG
        let mut timings = DebugTimings::new();

        while !exit.load(Ordering::Acquire) {
            match reorder_buffer.recv().await {
                Ok(payload) => match reader.push(payload) {
                    Ok(()) => {
                        let n = reader.finish();
                        let nalu = &input_buffer[..n];

                        if !has_reference_frame {
                            let nalu_type = nalu[4] & NALU_TYPE_BITMASK;
                            if nalu_type != NALU_TYPE_IDR_PIC {
                                pli.send(&peer).await?;
                                reader = H264Depacketizer::wrap_buffer(&mut input_buffer);
                                continue;
                            } else {
                                has_reference_frame = true;
                            }
                        }

                        // DEBUG
                        timings.snapshot();

                        let res = decoder.queue_input_buffer(input_buffer, n as _, 0, 0);
                        input_buffer = decoder.dequeue_input_buffer(MediaTimeout::INFINITE)?;
                        reader = H264Depacketizer::wrap_buffer(&mut input_buffer);
                        match res {
                            Ok(_) => (), // TODO: Use a channel to signal the other thread?
                            Err(e) => log::error!("queue_input_buffer error: {e}"),
                        }
                    }
                    Err(DepacketizerError::NeedMoreInput) => continue,
                    Err(e) => {
                        log::error!("Depacketization error: {e:?}");
                        has_reference_frame = false;
                        reader.finish();
                        reader = H264Depacketizer::wrap_buffer(&mut input_buffer);
                        pli.send(&peer).await?;
                    }
                },
                Err(e) => {
                    match e {
                        ReorderBufferError::HeaderParsingError
                        | ReorderBufferError::TrackRemoteReadError => {
                            has_reference_frame = false;
                            reader.finish();
                            reader = H264Depacketizer::wrap_buffer(&mut input_buffer);
                            pli.send(&peer).await?;
                        }
                        ReorderBufferError::PacketTooShort => (), // Empty payload?
                        ReorderBufferError::BufferFull => {
                            // TODO: Should be NACK
                            has_reference_frame = false;
                            reader.finish();
                            reader = H264Depacketizer::wrap_buffer(&mut input_buffer);
                            pli.send(&peer).await?;
                        }
                        _ => (),
                        // ReorderBufferError::TrackRemoteReadTimeout => todo!(),
                        // ReorderBufferError::UnableToMaintainReorderBuffer => todo!(), // TODO: RENAME THIS
                    }
                }
            }
        }

        Result::<(), DecoderError>::Ok(())
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
    if let Err(e) = join_handle.await {
        log::error!("Error joining thread: {e:?}");
    }
    return Err(DecoderError::ApplicationClosed);
}

trait AndroidDecoder: Default {
    type DepacketizerType<'a>: Depacketizer;

    fn init_done(&self) -> bool;
    fn resolution(&self) -> Option<(i32, i32)>;
    fn codec_config(&self) -> Option<&[u8]>;

    fn read_payload(&mut self, payload: &[u8]) -> Result<(), ()>;
}

// TODO: AndroidDecoder should be a trait object
async fn create_media_engine<T: AndroidDecoder>(
    singleton: &Arc<NativeLibSingleton>,
    track: &Arc<TrackRemote>,
    peer: &Arc<WebRtcPeer>,
    mime_type: MimeType,
    decoder_name: &str,
    receiver: &mut UnboundedReceiver<MediaPlayerEvent>,
) -> Result<MediaEngine, DecoderError> {
    let mut pli = RateLimitedPli::new(track.ssrc(), PLI_INTERVAL);

    let mut native_window: Option<NativeWindow> = None;

    let mut reorder_buffer = BufferedTrackRemote::new(track.clone(), NUM_BUFFERED_PACKETS);
    let mut payload_buf = vec![0u8; MAX_NALU_SIZE];
    let mut reader = T::DepacketizerType::wrap_buffer(&mut payload_buf);
    let mut decoder = T::default();

    loop {
        if peer.connection_state() != RTCPeerConnectionState::Connected {
            return Err(DecoderError::ApplicationClosed);
        }

        // If everything has been gathered, build the media engine
        if native_window.is_some() && decoder.init_done() {
            let mut format = MediaFormat::new()?;
            format.set_mime_type(mime_type);
            format.set_realtime_priority(true);
            if singleton.api_level() >= 30 {
                format.set_low_latency(true);
            }
            // TODO: Additional format flags
            // format.set_integer("vendor.rtc-ext-dec-low-latency.enable", 1);

            if let Some((width, height)) = decoder.resolution() {
                format.set_resolution(width, height);
                format.set_max_resolution(width, height);

                let env = singleton.vm.attach_current_thread()?;
                singleton
                    .set_media_player_aspect_ratio(&env, width, height)
                    .map_err(|e| DecoderError::SetAspectRatio(e))?;
            }

            let mut media_engine = MediaEngine::create_by_name(decoder_name)?;
            media_engine.initialize(&format, native_window.as_ref(), false)?;

            if let Some(codec_config) = decoder.codec_config() {
                media_engine.submit_codec_config(codec_config)?;
            }

            return Ok(media_engine);
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
                match reorder_buffer.recv().await {
                    Ok(payload) => match reader.push(payload) {
                        Ok(()) => {
                            let bytes_written = reader.finish();
                            let nalu = &payload_buf[..bytes_written];
                            if let Err(_) = decoder.read_payload(nalu) {
                                pli.send(peer).await?;
                            }
                            reader = T::DepacketizerType::wrap_buffer(&mut payload_buf);
                        }
                        Err(DepacketizerError::NeedMoreInput) => continue,
                        Err(e) => {
                            log::error!("Depacketization error: {e:?}");
                            pli.send(peer).await?;
                            reader.finish();
                            reader = T::DepacketizerType::wrap_buffer(&mut payload_buf);
                        }
                    },
                    Err(e) => {
                        match e {
                            ReorderBufferError::HeaderParsingError
                            | ReorderBufferError::TrackRemoteReadError => {
                                reader.finish();
                                reader = T::DepacketizerType::wrap_buffer(&mut payload_buf);
                                pli.send(peer).await?;
                            }
                            ReorderBufferError::PacketTooShort => (), // Empty payload?
                            ReorderBufferError::BufferFull => {
                                // TODO: Should be NACK
                                reader.finish();
                                reader = T::DepacketizerType::wrap_buffer(&mut payload_buf);
                                pli.send(peer).await?;
                            }
                            _ => (),
                            // ReorderBufferError::TrackRemoteReadTimeout => todo!(),
                            // ReorderBufferError::UnableToMaintainReorderBuffer => todo!(), // TODO: RENAME THIS
                        }
                    }
                }
            }
        }
    }
}

struct DebugTimings(Vec<Instant>);

impl DebugTimings {
    fn new() -> DebugTimings {
        DebugTimings(Vec::with_capacity(100))
    }

    fn snapshot(&mut self) {
        self.0.push(std::time::Instant::now());
        if self.0.len() >= 100 {
            let mut min = u128::MAX;
            let mut max = u128::MIN;
            let mut sum: f64 = 0.0;
            for i in 1..self.0.len() {
                let delta = self.0[i] - self.0[i - 1];
                let micros = delta.as_micros();
                sum += micros as f64;
                if micros < min {
                    min = micros;
                }
                if micros > max {
                    max = micros;
                }
            }
            log::info!(
                "Min: {}, Max: {}, Ave: {}",
                min,
                max,
                sum / self.0.len() as f64
            );
            self.0.clear();
        }
    }
}
