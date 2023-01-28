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
    pub async fn new(
        addr: impl Into<SocketAddr> + 'static,
    ) -> Result<WebSocketSignaler, WebSocketSignalerError> {
        let addr: SocketAddr = addr.into();
        let socket = TcpSocket::new_v4()?;
        let tcp_stream = socket.connect(addr).await?;

        let (ws_stream, _response) =
            tokio_tungstenite::client_async(format!("ws://{}", addr), tcp_stream).await?;

        let (tx, rx) = ws_stream.split();
        Ok(WebSocketSignaler {
            tx: Mutex::new(tx),
            rx: Mutex::new(rx),
        })
    }

    async fn recv_impl(&self) -> Result<Message, WebSocketSignalerError> {
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

    async fn send_impl(&self, msg: Message) -> Result<(), WebSocketSignalerError> {
        let s = serde_json::to_string(&msg)?;
        let ws_msg = tungstenite::Message::text(s);
        self.tx.lock().await.send(ws_msg).await?;
        Ok(())
    }
}

/// Errors that WebSocketSignaler can emit
#[derive(Debug)]
pub enum WebSocketSignalerError {
    Tungstenite,
    Serde,
    StdIo,
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
impl_from!(std::io::Error, StdIo);

impl std::fmt::Display for WebSocketSignalerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebSocketSignalerError::Tungstenite => {
                write!(f, "Encountered an error in the underlying WebSocket")
            }
            WebSocketSignalerError::Serde => {
                write!(f, "Failed to deserialize the message")
            }
            WebSocketSignalerError::StdIo => {
                write!(f, "Failed to initialize TCP socket")
            }
            WebSocketSignalerError::Eof => {
                write!(f, "WebSocket connection has been closed")
            }
        }
    }
}

impl std::error::Error for WebSocketSignalerError {}

#[async_trait::async_trait]
impl Signaler for WebSocketSignaler {
    async fn recv(&self) -> Result<Message, Box<dyn std::error::Error + Send>> {
        match self.recv_impl().await {
            Ok(msg) => Ok(msg),
            Err(e) => Err(Box::new(e)),
        }
    }

    async fn send(&self, msg: Message) -> Result<(), Box<dyn std::error::Error + Send>> {
        match self.send_impl(msg).await {
            Ok(()) => Ok(()),
            Err(e) => Err(Box::new(e)),
        }
    }
}
