use crate::{
    media::{MediaCodec, MediaFormat},
    window::NativeWindow,
    MediaPlayerEvent, NativeLibSingleton,
};
use bytes::Bytes;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::broadcast;
use webrtc::{
    peer_connection::peer_connection_state::RTCPeerConnectionState,
    rtcp::payload_feedbacks::picture_loss_indication::PictureLossIndication, rtp,
    rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote,
    util::marshal::Unmarshal,
};
use webrtc_helper::{
    codecs::{Codec, H264Codec, H264PayloadReader, H264PayloadReaderError, H264Profile},
    decoder::DecoderBuilder,
    WebRtcPeer,
};

const RTP_PACKET_MAX_SIZE: usize = 1500;

pub struct AndroidDecoderBuilder {
    singleton: Arc<NativeLibSingleton>,
    codecs: Vec<Codec>,
    codec_map: HashMap<String, String>,
}

impl DecoderBuilder for AndroidDecoderBuilder {
    fn supported_codecs(&self) -> &[Codec] {
        &self.codecs
    }

    fn build(
        self: Box<Self>,
        track: Arc<TrackRemote>,
        _rtp_receiver: Arc<RTCRtpReceiver>,
        peer: Arc<WebRtcPeer>,
    ) {
        let singleton = self.singleton;
        let codec_map = self.codec_map;

        tokio::spawn(async move {
            while peer.connection_state() != RTCPeerConnectionState::Connected {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }

            // TODO: Check sdp_fmtp_line for SPS/PPS
            let codec_params = track.codec().await;
            let mime_type = &codec_params.capability.mime_type;
            let clock_rate = codec_params.capability.clock_rate;
            let decoder_name = codec_map
                .get(mime_type)
                .expect("No decoder for chosen MIME type");

            let mut buf = vec![0u8; RTP_PACKET_MAX_SIZE];
            let mut receiver = singleton.get_event_receiver();

            let decoder = create_decoder(
                &singleton,
                &track,
                &peer,
                mime_type,
                decoder_name,
                &mut receiver,
                &mut buf,
            )
            .await
            .expect("Unable to create decoder");

            let pli = PictureLossIndication {
                sender_ssrc: 0,
                media_ssrc: track.ssrc(),
            };

            let mut index = decoder.dequeue_input_buffer(-1).unwrap();
            let mut buffer = decoder.get_input_buffer(index as _).unwrap();

            loop {
                if peer.connection_state() != RTCPeerConnectionState::Connected {
                    return;
                }

                match receiver.try_recv() {
                    Ok(msg) => match msg {
                        MediaPlayerEvent::MainActivityDestroyed => return,
                        MediaPlayerEvent::SurfaceCreated(_) => return,
                        MediaPlayerEvent::SurfaceDestroyed => return,
                    },
                    Err(broadcast::error::TryRecvError::Closed) => return,
                    Err(broadcast::error::TryRecvError::Lagged(_)) => return,
                    Err(broadcast::error::TryRecvError::Empty) => {
                        let mut reader = H264PayloadReader::new(buffer);
                        let mut last_sequence_number = None;

                        while let Ok((n, _)) = track.read(&mut buf).await {
                            let mut b = &buf[..n];

                            // Unmarshaling the header would move `b` to point to the payload
                            let header = rtp::header::Header::unmarshal(&mut b)
                                .expect("Error parsing RTP header");

                            // Check sequence number for skipped values
                            if let Some(last_sequence_number) = &mut last_sequence_number {
                                if header.sequence_number.wrapping_sub(*last_sequence_number) != 1 {
                                    peer.write_rtcp(&[Box::new(pli.clone())])
                                        .await
                                        .expect("Failed to send PLI");
                                }
                                *last_sequence_number = header.sequence_number;
                            } else {
                                last_sequence_number = Some(header.sequence_number);
                            }

                            match reader.read_payload(b) {
                                Ok(num_bytes) => {
                                    let timestamp = header.timestamp * 1_000_000 / clock_rate;
                                    decoder
                                        .queue_input_buffer(
                                            index as _,
                                            0,
                                            num_bytes as _,
                                            timestamp as _,
                                            0,
                                        )
                                        .unwrap();
                                    index = decoder.dequeue_input_buffer(-1).unwrap();
                                    buffer = decoder.get_input_buffer(index as _).unwrap();
                                    break;
                                }
                                Err(H264PayloadReaderError::NeedMoreInput(r)) => reader = r,
                                Err(_) => {
                                    peer.write_rtcp(&[Box::new(pli.clone())])
                                        .await
                                        .expect("Failed to send PLI");
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}

async fn create_decoder(
    singleton: &Arc<NativeLibSingleton>,
    track: &Arc<TrackRemote>,
    peer: &Arc<WebRtcPeer>,
    mime_type: &str,
    decoder_name: &str,
    receiver: &mut broadcast::Receiver<MediaPlayerEvent>,
    buf: &mut [u8],
) -> Option<MediaCodec> {
    let mut native_window = None;
    let mut format = None;
    let mut parameter_sets = None;

    loop {
        if peer.connection_state() != RTCPeerConnectionState::Connected {
            return None;
        }

        if native_window.is_some() && format.is_some() && parameter_sets.is_some() {
            let mut decoder =
                MediaCodec::create_by_name(decoder_name).expect("Cannot create `MediaCodec`");
            decoder
                .initialize(&format.unwrap(), native_window, false)
                .expect("Unable to initialize decoder");

            let data: Bytes = parameter_sets.unwrap();
            decoder
                .submit_codec_config(|buffer| {
                    let min_len = data.len().min(buffer.len());
                    buffer[..min_len].copy_from_slice(&data[..min_len]);
                    (min_len, 0)
                })
                .expect("Error submitting parameter sets");
            return Some(decoder);
        }

        match receiver.try_recv() {
            Ok(msg) => match msg {
                MediaPlayerEvent::MainActivityDestroyed => return None,
                MediaPlayerEvent::SurfaceCreated(surface) => {
                    let env = singleton
                        .vm
                        .attach_current_thread()
                        .expect("Unable attach VM to current thread");
                    native_window = Some(
                        NativeWindow::new(&env, &surface.as_obj())
                            .expect("Cannot create `NativeWindow`"),
                    );
                }
                MediaPlayerEvent::SurfaceDestroyed => {
                    native_window = None;
                    // Pause decoder?
                }
            },
            Err(broadcast::error::TryRecvError::Closed) => return None,
            Err(broadcast::error::TryRecvError::Lagged(_)) => return None,
            Err(broadcast::error::TryRecvError::Empty) => {
                if let Ok((n, _)) = track.read(buf).await {
                    let mut b = Bytes::copy_from_slice(&buf[..n]);

                    // Unmarshaling the header would move `b` to point to the payload
                    let _header =
                        rtp::header::Header::unmarshal(&mut b).expect("Error parsing RTP header");

                    if let Some((width, height)) = H264Codec::get_resolution(&b) {
                        let width = width as i32;
                        let height = height as i32;

                        format = Some(MediaFormat::new().expect("Cannot create `MediaFormat`"));
                        if let Some(format) = &mut format {
                            format.set_mime_type(mime_type);
                            format.set_realtime_priority(true);
                            format.set_resolution(width, height);
                            format.set_max_resolution(width, height);
                        }

                        let env = singleton
                            .vm
                            .attach_current_thread()
                            .expect("Unable attach VM to current thread");
                        singleton
                            .set_media_player_aspect_ratio(&env, width, height)
                            .expect("Unable to set aspect ratio");

                        parameter_sets = Some(b);
                    } else {
                        // send nack pli
                        let pli = PictureLossIndication {
                            sender_ssrc: 0,
                            media_ssrc: track.ssrc(),
                        };
                        peer.write_rtcp(&[Box::new(pli)])
                            .await
                            .expect("Failed to send PLI");
                    }
                }
            }
        }
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
            let mime_types: [(&str, fn(i32) -> Option<Codec>); 3] = [
                ("video/av01", |_| None),
                ("video/hevc", |_| None),
                ("video/avc", |id| {
                    h264_profile_from_android_id(id).map(|profile| H264Codec::new(profile).into())
                }),
            ];

            let env = singleton.global_vm().attach_current_thread()?;

            for (mime_type, converter) in mime_types {
                let decoder_name = match singleton.choose_decoder_for_type(&env, mime_type) {
                    Ok(Some(decoder_name)) => decoder_name,
                    Ok(None) => {
                        crate::info!("No decoder for {mime_type}");
                        continue;
                    }
                    Err(e) => {
                        crate::error!("Error while finding decoder: {e}");
                        continue;
                    }
                };
                let profiles =
                    match singleton.list_profiles_for_decoder(&env, &decoder_name, mime_type) {
                        Ok(Some(profiles)) => profiles,
                        Ok(None) => {
                            crate::info!("Possibly invalid decoder name: {decoder_name}");
                            continue;
                        }
                        Err(e) => {
                            crate::error!("Error while listing profiles: {e}");
                            continue;
                        }
                    };
                for id in profiles {
                    if let Some(codec) = converter(id) {
                        codecs.push(codec);
                    }
                }
                codec_map.insert(mime_type.to_owned(), decoder_name);
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
            crate::info!("Unknown H.264 profile id: {}", id);
            None
        }
    }
}

// async fn run_decoder(
//     manager: Arc<NativeLibSingleton>,
//     track: Arc<TrackRemote>,
//     rtp_receiver: Arc<RTCRtpReceiver>,
//     ice_connection_state: IceConnectionState,
// ) -> Option<()> {
//     let mut receiver = manager.get_event_receiver();
//     loop {
//         match receiver.recv().await {
//             Ok(msg) => match msg {
//                 MediaPlayerEvent::SurfaceCreated(java_surface) => {
//                     let env = manager.vm.attach_current_thread()?;

//                     let native_window = NativeWindow::new(&env, &java_surface.as_obj()).ok()?;

//                     let width = 1920;
//                     let height = 1080;

//                     manager.set_media_player_aspect_ratio(&env, width, height)?;

//                     let mut format = MediaFormat::new()?;
//                     format.set_resolution(width, height);
//                     format.set_max_resolution(width, height);
//                     format.set_mime_type(media::VideoType::H264);
//                     format.set_realtime_priority(true);

//                     let mut decoder = MediaCodec::new_decoder(media::VideoType::H264)?;
//                     decoder.initialize(&format, Some(native_window), false)?;

//                     crate::info!("created decoder");

//                     const FRAME_INTERVAL_MICROS: u64 = 16_666;
//                     let dur = std::time::Duration::from_micros(FRAME_INTERVAL_MICROS);
//                     let mut time = 0;

//                     decoder.submit_codec_config(|buffer| {
//                         let data = debug::CSD;
//                         let min_len = data.len().min(buffer.len());
//                         buffer[..min_len].copy_from_slice(&data[..min_len]);
//                         (min_len, 0)
//                     })?;

//                     for packet_index in 0..119 {
//                         crate::info!("decode: {packet_index}");
//                         decoder.decode(|buffer| {
//                             let data = debug::PACKETS[packet_index];
//                             let min_len = data.len().min(buffer.len());
//                             buffer[..min_len].copy_from_slice(&data[..min_len]);
//                             (min_len, time)
//                         })?;
//                         time += FRAME_INTERVAL_MICROS;
//                         decoder.render_output()?;
//                         std::thread::sleep(dur);
//                     }
//                     decoder.decode(|buffer| {
//                         let data = debug::PACKETS[119];
//                         let min_len = data.len().min(buffer.len());
//                         buffer[..min_len].copy_from_slice(&data[..min_len]);
//                         (min_len, time)
//                     })?;
//                     decoder.render_output()?;
//                 }
//                 msg => anyhow::bail!("Unexpected message while waiting for a surface: {msg:?}"),
//             },
//             Err(e) => anyhow::bail!("Channel closed: {e}"),
//         }
//     }

//     #[allow(unreachable_code)]
//     Some(())
// }
