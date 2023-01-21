use jni::strings::JNIString;
use std::sync::Arc;
use webrtc::{rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote};
use webrtc_helper::{codecs::Codec, decoder::DecoderBuilder, peer::IceConnectionState};

pub struct AndroidDecoderBuilder {}

impl DecoderBuilder for AndroidDecoderBuilder {
    fn supported_codecs(&self) -> &[Codec] {
        todo!()
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

impl AndroidDecoderBuilder {}

pub struct DecoderEntry {
    name: JNIString,
    mime_type: JNIString,
    supported_profiles: Vec<(u8, u8)>,
}
