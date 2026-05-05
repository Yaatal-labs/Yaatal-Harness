use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use loco_rs::prelude::*;
use std::time::Instant;
use yaatal_search::contracts::SearchRequest;
use yaatal_voice::{
    client::{VoiceClientError, VoiceServiceConnection},
    contracts::{SessionConfig, VoiceClientMessage, VoiceServerMessage},
};

use crate::services::{
    profile_identity::ensure_profile_id_for_user_pid,
    search_client::{format_grounding, SearchServiceClient},
    voice_routing::{
        VoiceRouteDecision, VoiceRoutingConfig, VoiceRoutingSession, VoiceTurnSignals,
    },
};

const SEARCH_HINTS: &[&str] = &[
    "find",
    "search",
    "look for",
    "where can i",
    "get me",
    "buy",
    "trouve",
    "cherche",
    "ou trouver",
    "où trouver",
    "ou puis-je",
    "où puis-je",
];

fn looks_like_search(text: &str) -> bool {
    let lowercase = text.to_lowercase();
    SEARCH_HINTS.iter().any(|hint| lowercase.contains(hint))
}

#[derive(Debug, thiserror::Error)]
pub enum VoiceSessionError {
    #[error("voice service error: {0}")]
    Voice(#[from] VoiceClientError),
    #[error("search service error: {0}")]
    Search(#[from] crate::services::search_client::SearchClientError),
    #[error("websocket error: {0}")]
    WebSocket(#[from] axum::Error),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("application error: {0}")]
    App(#[from] loco_rs::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

#[derive(Debug, Default)]
struct TurnState {
    has_session_config: bool,
    text_buffer: String,
    search_fired_this_turn: bool,
    route_decided_this_turn: bool,
    lang: Option<String>,
    market: Option<String>,
    turn_started_at: Option<Instant>,
    audio_chunk_count: usize,
}

impl TurnState {
    fn record_session_config(&mut self, config: &SessionConfig) {
        self.has_session_config = true;
        self.lang = config.lang.clone();
        self.market = config.market.clone();
    }

    fn record_subtitle(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }

        if !self.text_buffer.is_empty() {
            self.text_buffer.push(' ');
        }
        self.text_buffer.push_str(trimmed);
    }

    fn record_audio_chunk(&mut self) {
        if self.turn_started_at.is_none() {
            self.turn_started_at = Some(Instant::now());
        }
        self.audio_chunk_count += 1;
    }

    fn should_search(&self) -> bool {
        !self.search_fired_this_turn
            && !self.text_buffer.trim().is_empty()
            && looks_like_search(&self.text_buffer)
    }

    fn current_query(&self) -> String {
        self.text_buffer.trim().to_string()
    }

    fn mark_search_fired(&mut self) {
        self.search_fired_this_turn = true;
    }

    fn mark_route_decided(&mut self) {
        self.route_decided_this_turn = true;
    }

    fn routing_signals(&self) -> VoiceTurnSignals<'_> {
        VoiceTurnSignals {
            transcript: self.text_buffer.trim(),
            duration: self.turn_started_at.map(|started_at| started_at.elapsed()),
            audio_chunk_count: self.audio_chunk_count,
            asr_confidence: None,
            intent_confidence: None,
            entity_count: 0,
            tool_candidate_count: 0,
            duplex_requested: false,
            network_available: true,
            privacy_sensitive: false,
        }
    }

    fn reset_turn(&mut self) {
        self.text_buffer.clear();
        self.search_fired_this_turn = false;
        self.route_decided_this_turn = false;
        self.turn_started_at = None;
        self.audio_chunk_count = 0;
    }
}

pub async fn handle_voice_session(
    ctx: AppContext,
    user_pid: String,
    socket: WebSocket,
) -> Result<(), VoiceSessionError> {
    let profile_id = ensure_profile_id_for_user_pid(&ctx.db, &user_pid).await?;
    let voice_service_url = std::env::var("VOICE_SERVICE_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:8082/session".to_string());
    let search_client = SearchServiceClient::from_env();
    let mut routing = VoiceRoutingSession::new(VoiceRoutingConfig::from_env());

    tracing::info!(user_pid = %user_pid, profile_id = %profile_id, "opening engine voice session");

    let mut upstream = VoiceServiceConnection::connect(&voice_service_url).await?;
    let (mut client_sink, mut client_stream) = socket.split();
    let mut state = TurnState::default();

    loop {
        tokio::select! {
            maybe_client_message = client_stream.next() => {
                match maybe_client_message {
                    Some(Ok(message)) => {
                        if let Some(client_message) = parse_client_message(message)? {
                            if let Some(config) = client_message.clone().into_session_config() {
                                state.record_session_config(&config);
                            } else if !state.has_session_config {
                                send_server_message(&mut client_sink, &VoiceServerMessage::Error {
                                    message: "first message must be session_config".to_string(),
                                }).await?;
                                break;
                            }

                            if matches!(client_message, VoiceClientMessage::AudioChunk { .. }) {
                                state.record_audio_chunk();
                            }

                            upstream.send(&client_message).await?;

                            if matches!(client_message, VoiceClientMessage::Close) {
                                break;
                            }
                        } else {
                            let _ = upstream.send(&VoiceClientMessage::Close).await;
                            break;
                        }
                    }
                    Some(Err(error)) => return Err(VoiceSessionError::Protocol(error.to_string())),
                    None => {
                        let _ = upstream.send(&VoiceClientMessage::Close).await;
                        break;
                    }
                }
            }
            maybe_server_message = upstream.recv() => {
                let Some(server_message) = maybe_server_message? else {
                    break;
                };

                if let VoiceServerMessage::Subtitle { text, final_chunk } = &server_message {
                    state.record_subtitle(text);
                    if *final_chunk {
                        maybe_route_turn(&mut client_sink, &mut routing, &mut state).await?;
                        maybe_ground_turn(&search_client, &mut upstream, &mut client_sink, &mut state)
                            .await?;
                    }
                }

                let should_reset_turn = matches!(server_message, VoiceServerMessage::TurnEnd { .. });
                send_server_message(&mut client_sink, &server_message).await?;

                if should_reset_turn {
                    maybe_route_turn(&mut client_sink, &mut routing, &mut state).await?;
                    maybe_ground_turn(&search_client, &mut upstream, &mut client_sink, &mut state)
                        .await?;
                    state.reset_turn();
                }
            }
        }
    }

    Ok(())
}

async fn maybe_route_turn(
    client_sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    routing: &mut VoiceRoutingSession,
    state: &mut TurnState,
) -> Result<(), VoiceSessionError> {
    if state.route_decided_this_turn || state.text_buffer.trim().is_empty() {
        return Ok(());
    }

    let decision = routing.decide(&state.routing_signals());
    state.mark_route_decided();

    tracing::info!(
        lane = decision.lane.as_str(),
        routing_state = decision.state.as_str(),
        effective_duration_ms = decision.effective_duration_ms,
        transcript_word_count = decision.transcript_word_count,
        repair_count = decision.repair_count,
        reasons = %decision.reasons.join(","),
        "voice route decision"
    );

    if routing.config().debug_messages {
        send_server_message(
            client_sink,
            &VoiceServerMessage::Warning {
                message: format_route_debug(&decision),
            },
        )
        .await?;
    }

    Ok(())
}

async fn maybe_ground_turn(
    search_client: &SearchServiceClient,
    upstream: &mut VoiceServiceConnection,
    client_sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    state: &mut TurnState,
) -> Result<(), VoiceSessionError> {
    if !state.should_search() {
        return Ok(());
    }

    let search_request = SearchRequest {
        query: state.current_query(),
        top_k: 3,
        lang: state.lang.clone(),
        market: state.market.clone(),
        filters: None,
    };

    match search_client.search(&search_request).await {
        Ok(response) => {
            if let Some(grounding) = format_grounding(&response) {
                upstream
                    .send(&VoiceClientMessage::ContextInjection { text: grounding })
                    .await?;
            }
            state.mark_search_fired();
        }
        Err(error) => {
            state.mark_search_fired();
            send_server_message(
                client_sink,
                &VoiceServerMessage::Warning {
                    message: format!("search unavailable: {error}"),
                },
            )
            .await?;
        }
    }

    Ok(())
}

fn format_route_debug(decision: &VoiceRouteDecision) -> String {
    let reasons = if decision.reasons.is_empty() {
        "none".to_string()
    } else {
        decision.reasons.join(",")
    };

    format!(
        "route={} state={} duration_ms={} words={} repairs={} reasons={}",
        decision.lane.as_str(),
        decision.state.as_str(),
        decision.effective_duration_ms,
        decision.transcript_word_count,
        decision.repair_count,
        reasons
    )
}

fn parse_client_message(message: Message) -> Result<Option<VoiceClientMessage>, VoiceSessionError> {
    match message {
        Message::Text(text) => Ok(Some(serde_json::from_str(text.as_str())?)),
        Message::Close(_) => Ok(None),
        Message::Ping(_) | Message::Pong(_) => Ok(Some(VoiceClientMessage::ClientPing)),
        Message::Binary(_) => Err(VoiceSessionError::Protocol(
            "binary websocket frames are not supported".to_string(),
        )),
    }
}

async fn send_server_message(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: &VoiceServerMessage,
) -> Result<(), VoiceSessionError> {
    let payload = serde_json::to_string(message)?;
    sink.send(Message::Text(payload.into()))
        .await
        .map_err(VoiceSessionError::WebSocket)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{format_route_debug, TurnState};
    use crate::services::voice_routing::{VoiceRouteDecision, VoiceRouteLane, VoiceRoutingState};

    #[test]
    fn turn_state_only_searches_for_search_like_text() {
        let mut state = TurnState::default();
        state.record_subtitle("hello there");
        assert!(!state.should_search());

        state.record_subtitle("find white fabric");
        assert!(state.should_search());
    }

    #[test]
    fn turn_state_resets_after_turn() {
        let mut state = TurnState::default();
        state.record_audio_chunk();
        state.record_subtitle("find white fabric");
        state.mark_search_fired();
        state.mark_route_decided();
        state.reset_turn();

        assert!(state.text_buffer.is_empty());
        assert!(!state.search_fired_this_turn);
        assert!(!state.route_decided_this_turn);
        assert!(state.turn_started_at.is_none());
        assert_eq!(state.audio_chunk_count, 0);
    }

    #[test]
    fn route_debug_message_is_stable() {
        let message = format_route_debug(&VoiceRouteDecision {
            lane: VoiceRouteLane::Cloud,
            state: VoiceRoutingState::CloudActive,
            effective_duration_ms: 64000,
            transcript_word_count: 18,
            repair_count: 2,
            reasons: vec![
                "utterance_over_cloud_limit".to_string(),
                "repair_pattern_detected".to_string(),
            ],
        });

        assert!(message.contains("route=cloud"));
        assert!(message.contains("state=cloud_active"));
        assert!(message.contains("reasons=utterance_over_cloud_limit,repair_pattern_detected"));
    }
}
