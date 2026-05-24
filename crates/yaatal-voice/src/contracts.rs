use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionConfig {
    pub session_id: String,
    pub persona: Option<String>,
    pub lang: Option<String>,
    pub market: Option<String>,
}

impl SessionConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.session_id.trim().is_empty() {
            return Err("session_id must not be empty");
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoiceClientMessage {
    SessionConfig {
        session_id: String,
        persona: Option<String>,
        lang: Option<String>,
        market: Option<String>,
    },
    AudioChunk {
        audio_base64: String,
        transcript_hint: Option<String>,
    },
    ContextInjection {
        text: String,
    },
    ClientPing,
    Close,
}

impl VoiceClientMessage {
    pub fn into_session_config(self) -> Option<SessionConfig> {
        match self {
            Self::SessionConfig {
                session_id,
                persona,
                lang,
                market,
            } => Some(SessionConfig {
                session_id,
                persona,
                lang,
                market,
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum VoiceServerMessage {
    SessionReady { backend: String, session_id: String },
    Subtitle { text: String, final_chunk: bool },
    AudioChunk { audio_base64: String },
    TurnEnd { reason: Option<String> },
    Pong,
    Warning { message: String },
    Error { message: String },
}
