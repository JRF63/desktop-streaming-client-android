mod decoder;
mod signaling;

use crate::NativeLibSingleton;
use std::sync::Arc;
use webrtc_helper::{peer::Role, WebRtcPeer};

pub async fn start_webrtc(singleton: Arc<NativeLibSingleton>) {
    // TODO: Get from mDNS or something
    let addr = ([127, 0, 0, 1], 9090);

    let Some(signaler) = signaling::WebSocketSignaler::new(addr).await else {
        crate::error!("Creation of WebSocket signaling channel failed");
        return;
    };

    let Ok(decoder_builder) = decoder::AndroidDecoderBuilder::new(singleton) else {
        crate::error!("Failed to initialize an Android decoder");
        return;
    };

    let mut peer_builder = WebRtcPeer::builder(signaler, Role::Offerer);
    peer_builder.with_decoder(Box::new(decoder_builder));
    let Ok(peer) = peer_builder.build().await else {
        crate::error!("Failed to initialize a WebRTC connection");
        return;
    };
    peer.is_closed().await;
}
