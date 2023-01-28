mod decoder;
mod signaling;

use crate::NativeLibSingleton;
use std::sync::Arc;
use webrtc_helper::{peer::Role, WebRtcPeer};

pub async fn start_webrtc(singleton: Arc<NativeLibSingleton>) {
    // TODO: Get from mDNS or something
    let addr = ([192, 168, 1, 253], 9090);

    let signaler = match signaling::WebSocketSignaler::new(addr).await {
        Ok(s) => s,
        Err(e) => {
            crate::error!("Creation of WebSocket signaling channel failed: {e:?}");
            return;
        }
    };

    let decoder_builder = match decoder::AndroidDecoderBuilder::new(singleton) {
        Ok(b) => b,
        Err(e) => {
            crate::error!("Failed to initialize an Android decoder: {e:?}");
            return;
        }
    };

    android_logger::init_once(
        android_logger::Config::default().with_min_level(log::Level::Trace),
    );
    
    let mut peer_builder = WebRtcPeer::builder(signaler, Role::Offerer);
    peer_builder.with_decoder(Box::new(decoder_builder));

    let Ok(peer) = peer_builder.build().await else {
        crate::error!("Failed to initialize a WebRTC connection");
        return;
    };
    peer.is_closed().await;
}
