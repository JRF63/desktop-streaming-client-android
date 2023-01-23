use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use std::net::SocketAddr;
use tokio::{
    net::{TcpSocket, TcpStream},
    sync::Mutex,
};
use tokio_tungstenite::{tungstenite, WebSocketStream};
use webrtc_helper::signaling::{Message, Signaler};

/// `Signaler` implementation using WebSocket. Mirrors the one in the server.
pub struct WebSocketSignaler {
    tx: Mutex<SplitSink<WebSocketStream<TcpStream>, tungstenite::Message>>,
    rx: Mutex<SplitStream<WebSocketStream<TcpStream>>>,
}

impl WebSocketSignaler {
    /// Create a new `WebSocketSignaler`.
    pub async fn new(addr: impl Into<SocketAddr> + 'static) -> Option<WebSocketSignaler> {
        let addr: SocketAddr = addr.into();
        let socket = TcpSocket::new_v4().ok()?;
        let tcp_stream = socket.connect(addr).await.ok()?;

        let (ws_stream, _response) =
            tokio_tungstenite::client_async(format!("ws://{}", addr), tcp_stream)
                .await
                .ok()?;

        let (tx, rx) = ws_stream.split();
        Some(WebSocketSignaler {
            tx: Mutex::new(tx),
            rx: Mutex::new(rx),
        })
    }
}

/// Errors that WebSocketSignaler can emit
pub enum WebSocketSignalerError {
    Tungstenite,
    Serde,
    Eof,
}

// The conversion only cares about the error type and discards the error details.
macro_rules! impl_from {
    ($t:ty, $e:tt) => {
        impl From<$t> for WebSocketSignalerError {
            #[inline]
            fn from(_: $t) -> Self {
                WebSocketSignalerError::$e
            }
        }
    };
}

impl_from!(tungstenite::Error, Tungstenite);
impl_from!(serde_json::Error, Serde);

impl std::fmt::Display for WebSocketSignalerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebSocketSignalerError::Tungstenite => {
                write!(f, "Encountered an error in the underlying WebSocket")
            }
            WebSocketSignalerError::Serde => {
                write!(f, "Failed to deserialize the message")
            }
            WebSocketSignalerError::Eof => {
                write!(f, "WebSocket connection has been closed")
            }
        }
    }
}

#[async_trait::async_trait]
impl Signaler for WebSocketSignaler {
    type Error = WebSocketSignalerError;

    async fn recv(&self) -> Result<Message, Self::Error> {
        match self.rx.lock().await.next().await {
            Some(ws_msg) => match ws_msg?.to_text() {
                Ok(s) => {
                    let msg = serde_json::from_str::<Message>(s)?;
                    Ok(msg)
                }
                Err(_) => Err(WebSocketSignalerError::Serde),
            },
            None => Err(WebSocketSignalerError::Eof), // Closed
        }
    }

    async fn send(&self, msg: Message) -> Result<(), Self::Error> {
        let s = serde_json::to_string(&msg)?;
        let ws_msg = tungstenite::Message::text(s);
        self.tx.lock().await.send(ws_msg).await?;
        Ok(())
    }
}
