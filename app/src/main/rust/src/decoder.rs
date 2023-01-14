use std::sync::Arc;
use webrtc::{rtp_transceiver::rtp_receiver::RTCRtpReceiver, track::track_remote::TrackRemote};
use webrtc_helper::{codecs::Codec, decoder::DecoderBuilder};

pub struct AndroidDecoder {}

impl DecoderBuilder for AndroidDecoder {
    fn supported_codecs(&self) -> &[Codec] {
        todo!()
    }

    fn build(self: Box<Self>, track: Arc<TrackRemote>, rtp_receiver: Arc<RTCRtpReceiver>) {
        todo!()
    }
}
