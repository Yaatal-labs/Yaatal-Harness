use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::contracts::{SessionConfig, VoiceClientMessage, VoiceServerMessage};

const SESSION_BUFFER_CAPACITY: usize = 32;

#[derive(Debug, Error)]
pub enum VoiceBackendError {
    #[error("invalid session config: {0}")]
    InvalidConfig(String),
    #[error("voice session closed")]
    SessionClosed,
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("transport error: {0}")]
    Transport(String),
}

pub struct VoiceSession {
    inbound: mpsc::Sender<VoiceClientMessage>,
    outbound: mpsc::Receiver<VoiceServerMessage>,
}

impl VoiceSession {
    pub(crate) fn new(
        inbound: mpsc::Sender<VoiceClientMessage>,
        outbound: mpsc::Receiver<VoiceServerMessage>,
    ) -> Self {
        Self { inbound, outbound }
    }

    pub async fn send(&self, message: VoiceClientMessage) -> Result<(), VoiceBackendError> {
        self.inbound
            .send(message)
            .await
            .map_err(|_| VoiceBackendError::SessionClosed)
    }

    pub async fn recv(&mut self) -> Option<VoiceServerMessage> {
        self.outbound.recv().await
    }
}

pub(crate) fn session_channels() -> (
    mpsc::Sender<VoiceClientMessage>,
    mpsc::Receiver<VoiceClientMessage>,
    mpsc::Sender<VoiceServerMessage>,
    mpsc::Receiver<VoiceServerMessage>,
) {
    let (inbound_tx, inbound_rx) = mpsc::channel(SESSION_BUFFER_CAPACITY);
    let (outbound_tx, outbound_rx) = mpsc::channel(SESSION_BUFFER_CAPACITY);
    (inbound_tx, inbound_rx, outbound_tx, outbound_rx)
}

#[async_trait]
pub trait VoiceBackend: Send + Sync {
    async fn open_session(&self, config: SessionConfig) -> Result<VoiceSession, VoiceBackendError>;
}
