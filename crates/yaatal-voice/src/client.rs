use futures_util::{SinkExt, StreamExt};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::contracts::{VoiceClientMessage, VoiceServerMessage};

type VoiceWebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Debug, Error)]
pub enum VoiceClientError {
    #[error("websocket connect error: {0}")]
    Connect(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub struct VoiceServiceConnection {
    socket: VoiceWebSocket,
}

impl VoiceServiceConnection {
    pub async fn connect(url: &str) -> Result<Self, VoiceClientError> {
        let (socket, _) = connect_async(url).await?;
        Ok(Self { socket })
    }

    pub async fn send(&mut self, message: &VoiceClientMessage) -> Result<(), VoiceClientError> {
        let payload = serde_json::to_string(message)?;
        self.socket.send(Message::Text(payload.into())).await?;
        Ok(())
    }

    pub async fn recv(&mut self) -> Result<Option<VoiceServerMessage>, VoiceClientError> {
        loop {
            let Some(message) = self.socket.next().await else {
                return Ok(None);
            };

            match message? {
                Message::Text(text) => {
                    let parsed = serde_json::from_str(text.as_str())?;
                    return Ok(Some(parsed));
                }
                Message::Close(_) => return Ok(None),
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Binary(_) => {
                    return Err(VoiceClientError::Protocol(
                        "unexpected binary frame from voice service".to_string(),
                    ))
                }
                Message::Frame(_) => continue,
            }
        }
    }
}
