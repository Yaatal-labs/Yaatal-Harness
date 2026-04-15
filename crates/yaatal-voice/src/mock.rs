use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::{
    backend::{session_channels, VoiceBackend, VoiceBackendError, VoiceSession},
    contracts::{SessionConfig, VoiceClientMessage, VoiceServerMessage},
};

const MOCK_AUDIO_BASE64: &str = "bW9jay1hdWRpbw==";

#[derive(Debug, Clone)]
pub struct MockVoiceBackend {
    audio_reply_base64: String,
}

impl Default for MockVoiceBackend {
    fn default() -> Self {
        Self {
            audio_reply_base64: MOCK_AUDIO_BASE64.to_string(),
        }
    }
}

#[async_trait]
impl VoiceBackend for MockVoiceBackend {
    async fn open_session(&self, config: SessionConfig) -> Result<VoiceSession, VoiceBackendError> {
        config
            .validate()
            .map_err(|err| VoiceBackendError::InvalidConfig(err.to_string()))?;

        let (inbound_tx, inbound_rx, outbound_tx, outbound_rx) = session_channels();
        let audio_reply_base64 = self.audio_reply_base64.clone();
        tokio::spawn(async move {
            run_mock_session(config, audio_reply_base64, inbound_rx, outbound_tx).await;
        });

        Ok(VoiceSession::new(inbound_tx, outbound_rx))
    }
}

async fn run_mock_session(
    config: SessionConfig,
    audio_reply_base64: String,
    mut inbound_rx: mpsc::Receiver<VoiceClientMessage>,
    outbound_tx: mpsc::Sender<VoiceServerMessage>,
) {
    if send_message(
        &outbound_tx,
        VoiceServerMessage::SessionReady {
            backend: "mock-personaplex".to_string(),
            session_id: config.session_id.clone(),
        },
    )
    .await
    .is_err()
    {
        return;
    }

    while let Some(message) = inbound_rx.recv().await {
        match message {
            VoiceClientMessage::AudioChunk {
                audio_base64,
                transcript_hint,
            } => {
                let transcript = transcript_hint
                    .unwrap_or_else(|| fallback_transcript(&config, audio_base64.len()));

                if send_message(
                    &outbound_tx,
                    VoiceServerMessage::Subtitle {
                        text: transcript,
                        final_chunk: true,
                    },
                )
                .await
                .is_err()
                {
                    break;
                }

                if send_message(
                    &outbound_tx,
                    VoiceServerMessage::AudioChunk {
                        audio_base64: audio_reply_base64.clone(),
                    },
                )
                .await
                .is_err()
                {
                    break;
                }

                if send_message(&outbound_tx, VoiceServerMessage::TurnEnd { reason: None })
                    .await
                    .is_err()
                {
                    break;
                }
            }
            VoiceClientMessage::ContextInjection { text } => {
                if send_message(
                    &outbound_tx,
                    VoiceServerMessage::Subtitle {
                        text: format!("mock grounding applied: {}", summarize(&text)),
                        final_chunk: true,
                    },
                )
                .await
                .is_err()
                {
                    break;
                }
            }
            VoiceClientMessage::ClientPing => {
                if send_message(&outbound_tx, VoiceServerMessage::Pong)
                    .await
                    .is_err()
                {
                    break;
                }
            }
            VoiceClientMessage::Close => break,
            VoiceClientMessage::SessionConfig { .. } => {
                if send_message(
                    &outbound_tx,
                    VoiceServerMessage::Warning {
                        message: "session is already configured".to_string(),
                    },
                )
                .await
                .is_err()
                {
                    break;
                }
            }
        }
    }
}

async fn send_message(
    outbound_tx: &mpsc::Sender<VoiceServerMessage>,
    message: VoiceServerMessage,
) -> Result<(), VoiceBackendError> {
    outbound_tx
        .send(message)
        .await
        .map_err(|_| VoiceBackendError::SessionClosed)
}

fn fallback_transcript(config: &SessionConfig, payload_len: usize) -> String {
    let persona = config.persona.as_deref().unwrap_or("default");
    format!("mock transcript from {payload_len} chars of audio for {persona}")
}

fn summarize(text: &str) -> String {
    const MAX_LEN: usize = 96;
    let trimmed = text.trim();
    if trimmed.len() <= MAX_LEN {
        return trimmed.to_string();
    }

    format!("{}...", &trimmed[..MAX_LEN])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use tokio::time::{timeout, Duration};

    use super::*;
    use crate::contracts::VoiceServerMessage;

    fn config() -> SessionConfig {
        SessionConfig {
            session_id: "session-123".to_string(),
            persona: Some("market-guide".to_string()),
            lang: Some("wo".to_string()),
            market: Some("SN-DKR".to_string()),
        }
    }

    #[tokio::test]
    async fn open_session_emits_ready_message() {
        let backend = MockVoiceBackend::default();
        let mut session = backend.open_session(config()).await.expect("session");

        let message = timeout(Duration::from_secs(1), session.recv())
            .await
            .expect("timeout")
            .expect("ready message");

        assert_eq!(
            message,
            VoiceServerMessage::SessionReady {
                backend: "mock-personaplex".to_string(),
                session_id: "session-123".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn audio_chunk_uses_transcript_hint() {
        let backend = MockVoiceBackend::default();
        let mut session = backend.open_session(config()).await.expect("session");
        let _ = session.recv().await;

        session
            .send(VoiceClientMessage::AudioChunk {
                audio_base64: "ZmFrZQ==".to_string(),
                transcript_hint: Some("find white fabric".to_string()),
            })
            .await
            .expect("send audio");

        let subtitle = timeout(Duration::from_secs(1), session.recv())
            .await
            .expect("timeout")
            .expect("subtitle");
        assert_eq!(
            subtitle,
            VoiceServerMessage::Subtitle {
                text: "find white fabric".to_string(),
                final_chunk: true,
            }
        );

        let audio = timeout(Duration::from_secs(1), session.recv())
            .await
            .expect("timeout")
            .expect("audio reply");
        assert_eq!(
            audio,
            VoiceServerMessage::AudioChunk {
                audio_base64: MOCK_AUDIO_BASE64.to_string(),
            }
        );

        let turn_end = timeout(Duration::from_secs(1), session.recv())
            .await
            .expect("timeout")
            .expect("turn end");
        assert_eq!(turn_end, VoiceServerMessage::TurnEnd { reason: None });
    }

    #[tokio::test]
    async fn context_injection_is_acknowledged() {
        let backend = MockVoiceBackend::default();
        let mut session = backend.open_session(config()).await.expect("session");
        let _ = session.recv().await;

        session
            .send(VoiceClientMessage::ContextInjection {
                text: "Top result: Awa Textiles, white basin fabric, 12000 XOF".to_string(),
            })
            .await
            .expect("send context");

        let ack = timeout(Duration::from_secs(1), session.recv())
            .await
            .expect("timeout")
            .expect("context ack");
        assert_eq!(
            ack,
            VoiceServerMessage::Subtitle {
                text: "mock grounding applied: Top result: Awa Textiles, white basin fabric, 12000 XOF".to_string(),
                final_chunk: true,
            }
        );
    }
}
