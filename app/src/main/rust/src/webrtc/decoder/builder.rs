use std::{sync::Arc, collections::HashMap};
use webrtc::{track::track_remote::TrackRemote, rtp_transceiver::rtp_receiver::RTCRtpReceiver};
use webrtc_helper::{DecoderBuilder, Codec, codecs::{CodecType, h264::{H264Codec, H264Profile}}, WebRtcPeer};
use crate::{NativeLibSingleton, media::MimeType};

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
            if let Err(e) = super::start_decoder(track, rtp_receiver, peer, singleton, codec_map).await {
                log::error!("Decoder failure: {e:?}");
            }
            log::info!("start_decoder exit");
        });
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
