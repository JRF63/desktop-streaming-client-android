mod decoder;
mod signaling;

use crate::NativeLibSingleton;
use futures_util::Future;
use std::{pin::Pin, sync::Arc};
use webrtc::data_channel::RTCDataChannel;
use webrtc_helper::{peer::Role, WebRtcPeer};

pub async fn start_webrtc(singleton: Arc<NativeLibSingleton>) {
    // TODO: Get from mDNS or something
    let addr = ([192, 168, 1, 253], 9090);

    android_logger::init_once(
        android_logger::Config::default()
            .with_min_level(log::Level::Info)
            .with_tag("client-android"),
    );

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

    let mut peer_builder = WebRtcPeer::builder(signaler, Role::Offerer);
    peer_builder
        .with_decoder(Box::new(decoder_builder))
        .with_data_channel_handler(Box::new(controls_handler));

    let Ok(peer) = peer_builder.build().await else {
        crate::error!("Failed to initialize a WebRTC connection");
        return;
    };
    peer.is_closed().await;
}

fn controls_handler(
    _data_channel: Arc<RTCDataChannel>,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
    Box::pin(async {})
}
