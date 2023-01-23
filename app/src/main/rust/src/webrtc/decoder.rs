use crate::NativeLibSingleton;
use std::sync::Arc;
use webrtc::{rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote};
use webrtc_helper::{
    codecs::{Codec, H264Profile},
    decoder::DecoderBuilder,
    peer::IceConnectionState,
};

pub struct AndroidDecoderBuilder {
    singleton: Arc<NativeLibSingleton>,
    codecs: Vec<Codec>,
}

impl DecoderBuilder for AndroidDecoderBuilder {
    fn supported_codecs(&self) -> &[Codec] {
        &self.codecs
    }

    fn build(
        self: Box<Self>,
        _track: Arc<TrackRemote>,
        _rtp_receiver: Arc<RTCRtpReceiver>,
        _ice_connection_state: IceConnectionState,
    ) {
        todo!()
    }
}

impl AndroidDecoderBuilder {
    pub fn new(
        singleton: Arc<NativeLibSingleton>,
    ) -> Result<AndroidDecoderBuilder, jni::errors::Error> {
        let mut codecs = Vec::new();
        {
            let env = singleton.global_vm().attach_current_thread()?;

            let mime_types = ["video/avc"]; // ["video/av01", "video/hevc", "video/avc"];
            for mime_type in mime_types {
                let decoder_name = singleton.choose_decoder_for_type(&env, mime_type)?;
                let profiles =
                    singleton.list_profile_levels_for_decoder(&env, &decoder_name, mime_type)?;
                match mime_type {
                    "video/av01" => todo!(),
                    "video/hevc" => todo!(),
                    "video/avc" => {
                        for id in profiles {
                            if let Some(profile) = h264_profile_from_android_id(id) {
                                codecs.push(Codec::h264_custom(profile, None, None));
                            }
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
        Ok(AndroidDecoderBuilder { singleton, codecs })
    }
}

// const AVCProfileBaseline: i32 = 1;
// const AVCProfileConstrainedBaseline: i32 = 65536;
// const AVCProfileConstrainedHigh: i32 = 524288;
// const AVCProfileExtended: i32 = 4;
// const AVCProfileHigh: i32 = 8;
// const AVCProfileHigh10: i32 = 16;
// const AVCProfileHigh422: i32 = 32;
// const AVCProfileHigh444: i32 = 64;
// const AVCProfileMain: i32 = 2;

// const HEVCProfileMain: i32 = 1;
// const HEVCProfileMain10: i32 = 2;
// const HEVCProfileMain10HDR10: i32 = 4096;
// const HEVCProfileMain10HDR10Plus: i32 = 8192;
// const HEVCProfileMainStill: i32 = 4;

// const AV1ProfileMain10: i32 = 2;
// const AV1ProfileMain10HDR10: i32 = 4096;
// const AV1ProfileMain10HDR10Plus: i32 = 8192;
// const AV1ProfileMain8: i32 = 1;

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
