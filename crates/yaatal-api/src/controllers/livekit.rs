//! LiveKit HTTP surface — JWT-protected token mint + signed-webhook receiver.
//!
//! Routes:
//! - `POST /api/livekit/token`   (JWT-protected; identity forced from `auth.claims.pid`)
//! - `POST /api/livekit/webhook` (unauthenticated; verified via LiveKit signed Authorization header)
//!
//! Design lock for v1 (dogfood):
//! - Room shapes: `one_to_one` only; `multi_party` and `broadcast` accepted in
//!   the request enum but return 501 Not Implemented.
//! - Cost model: free (no escrow integration).
//! - Recording: opt-in per session via `enable_recording: bool` → sets the
//!   `room_record` grant on the LiveKit token.
//! - KYC: no gate — any authenticated user can mint a token.
//!
//! Security: the LiveKit `identity` is always derived from the caller's
//! verified Yaatal JWT (`auth.claims.pid`). Callers cannot specify their own
//! identity — that would allow impersonation in LiveKit rooms.
//!
//! Env vars (all three required; any missing → 503):
//! - `LIVEKIT_API_KEY`
//! - `LIVEKIT_API_SECRET`
//! - `LIVEKIT_URL` (echoed back to the client so RN doesn't need to hardcode)

use std::{sync::Arc, time::Duration};

use axum::{
    body::Bytes,
    debug_handler,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use livekit_api::{
    access_token::{AccessToken, TokenVerifier, VideoGrants},
    webhooks::WebhookReceiver,
};
use loco_rs::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use yaatal_analytics::{AnalyticsDispatcher, AnalyticsEvent};

// ─── Shared config (built once at app boot, injected via Extension) ───────────

#[derive(Clone, Debug)]
pub struct LiveKitConfig {
    pub api_key: String,
    pub api_secret: String,
    pub url: String,
}

impl LiveKitConfig {
    /// Read credentials from env. Returns `None` if any of the three required
    /// vars is missing or empty — callers should then return 503.
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("LIVEKIT_API_KEY")
            .ok()
            .filter(|v| !v.is_empty())?;
        let api_secret = std::env::var("LIVEKIT_API_SECRET")
            .ok()
            .filter(|v| !v.is_empty())?;
        let url = std::env::var("LIVEKIT_URL")
            .ok()
            .filter(|v| !v.is_empty())?;
        Some(Self {
            api_key,
            api_secret,
            url,
        })
    }
}

fn not_configured() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "LiveKit is not configured (set LIVEKIT_API_KEY / LIVEKIT_API_SECRET / LIVEKIT_URL)",
    )
        .into_response()
}

// ─── Token endpoint ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoomType {
    #[default]
    OneToOne,
    MultiParty,
    Broadcast,
}

#[derive(Debug, Deserialize)]
pub struct TokenRequest {
    pub room: String,
    #[serde(default)]
    pub room_type: RoomType,
    #[serde(default)]
    pub enable_recording: bool,
    #[serde(default = "default_true")]
    pub can_publish: bool,
    #[serde(default = "default_true")]
    pub can_subscribe: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub token: String,
    pub url: String,
    pub room: String,
    pub identity: String,
    pub recording_enabled: bool,
}

#[debug_handler]
pub async fn mint_token(
    auth: auth::JWT,
    Extension(maybe_cfg): Extension<Option<Arc<LiveKitConfig>>>,
    Extension(analytics): Extension<Arc<AnalyticsDispatcher>>,
    State(_ctx): State<AppContext>,
    Json(req): Json<TokenRequest>,
) -> Response {
    let cfg = match maybe_cfg {
        Some(c) => c,
        None => return not_configured(),
    };

    // Reject room types we haven't wired yet.
    if req.room_type != RoomType::OneToOne {
        return (
            StatusCode::NOT_IMPLEMENTED,
            "only one_to_one rooms are wired in v1",
        )
            .into_response();
    }

    // Validate room name: non-empty, no whitespace, reasonable length.
    if req.room.is_empty() || req.room.len() > 200 || req.room.contains(char::is_whitespace) {
        return (
            StatusCode::BAD_REQUEST,
            "room must be 1..=200 chars with no whitespace",
        )
            .into_response();
    }

    // Force identity from the verified JWT. The caller cannot choose it.
    let identity = auth.claims.pid.clone();

    let grants = VideoGrants {
        room_join: true,
        room: req.room.clone(),
        can_publish: req.can_publish,
        can_subscribe: req.can_subscribe,
        room_record: req.enable_recording,
        ..Default::default()
    };

    let token_result = AccessToken::with_api_key(&cfg.api_key, &cfg.api_secret)
        .with_identity(&identity)
        .with_name(&identity)
        .with_grants(grants)
        .with_ttl(Duration::from_secs(3600))
        .to_jwt();

    let token = match token_result {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(error = %e, "livekit token mint failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "token mint error").into_response();
        }
    };

    analytics.capture(AnalyticsEvent {
        name: "livekit.token.minted",
        distinct_id: identity.clone(),
        properties: json!({
            "room": req.room,
            "recording_enabled": req.enable_recording,
            "can_publish": req.can_publish,
            "can_subscribe": req.can_subscribe,
        }),
    });

    Json(TokenResponse {
        token,
        url: cfg.url.clone(),
        room: req.room,
        identity,
        recording_enabled: req.enable_recording,
    })
    .into_response()
}

// ─── Webhook endpoint ─────────────────────────────────────────────────────────

#[debug_handler]
pub async fn webhook(
    Extension(maybe_cfg): Extension<Option<Arc<LiveKitConfig>>>,
    Extension(analytics): Extension<Arc<AnalyticsDispatcher>>,
    State(_ctx): State<AppContext>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let cfg = match maybe_cfg {
        Some(c) => c,
        None => return not_configured(),
    };

    // LiveKit signs the request via a JWT in the Authorization header whose
    // `sha256` claim binds the body. The `Bearer ` prefix is optional —
    // strip it if present so proxied calls still verify.
    let auth_header = match headers.get(axum::http::header::AUTHORIZATION) {
        Some(h) => h,
        None => {
            tracing::warn!("livekit webhook missing Authorization header");
            return (StatusCode::UNAUTHORIZED, "missing Authorization").into_response();
        }
    };
    let auth_token = match auth_header.to_str() {
        Ok(s) => s.strip_prefix("Bearer ").unwrap_or(s),
        Err(_) => {
            return (StatusCode::UNAUTHORIZED, "non-ASCII Authorization").into_response();
        }
    };
    let body_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(_) => return (StatusCode::BAD_REQUEST, "body must be UTF-8 JSON").into_response(),
    };

    let verifier = TokenVerifier::with_api_key(&cfg.api_key, &cfg.api_secret);
    let receiver = WebhookReceiver::new(verifier);

    let event = match receiver.receive(body_str, auth_token) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "livekit webhook signature/auth failed");
            return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
        }
    };

    // Emit structured analytics. Distinct id is `"system"` when there's no
    // single participant on the event; otherwise the participant identity.
    let participant_identity = event
        .participant
        .as_ref()
        .map(|p| p.identity.clone())
        .unwrap_or_else(|| "system".to_string());
    let room_name = event
        .room
        .as_ref()
        .map(|r| r.name.clone())
        .unwrap_or_default();

    analytics.capture(AnalyticsEvent {
        name: match event.event.as_str() {
            "room_started" | "room_finished" => "livekit.room.lifecycle",
            "participant_joined" | "participant_left" => "livekit.participant.lifecycle",
            "egress_ended" => "livekit.recording.ready",
            _ => "livekit.event",
        },
        distinct_id: participant_identity,
        properties: json!({
            "event": event.event,
            "room": room_name,
            // TODO(lane7): on egress_ended, persist the R2 URL into bobo_ledger.
        }),
    });

    (StatusCode::NO_CONTENT, "").into_response()
}

// ─── Routes ───────────────────────────────────────────────────────────────────

pub fn routes() -> Routes {
    Routes::new()
        .prefix("/api/livekit")
        .add("/token", post(mint_token))
        .add("/webhook", post(webhook))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn config_from_env_requires_all_three() {
        // Save & restore to avoid polluting other tests.
        let prev_key = std::env::var("LIVEKIT_API_KEY").ok();
        let prev_secret = std::env::var("LIVEKIT_API_SECRET").ok();
        let prev_url = std::env::var("LIVEKIT_URL").ok();

        std::env::remove_var("LIVEKIT_API_KEY");
        std::env::remove_var("LIVEKIT_API_SECRET");
        std::env::remove_var("LIVEKIT_URL");
        assert!(LiveKitConfig::from_env().is_none());

        std::env::set_var("LIVEKIT_API_KEY", "k");
        std::env::set_var("LIVEKIT_API_SECRET", "s");
        std::env::set_var("LIVEKIT_URL", "wss://example");
        assert!(LiveKitConfig::from_env().is_some());

        // Empty string also counts as missing.
        std::env::set_var("LIVEKIT_API_KEY", "");
        assert!(LiveKitConfig::from_env().is_none());

        // Restore.
        match prev_key {
            Some(v) => std::env::set_var("LIVEKIT_API_KEY", v),
            None => std::env::remove_var("LIVEKIT_API_KEY"),
        }
        match prev_secret {
            Some(v) => std::env::set_var("LIVEKIT_API_SECRET", v),
            None => std::env::remove_var("LIVEKIT_API_SECRET"),
        }
        match prev_url {
            Some(v) => std::env::set_var("LIVEKIT_URL", v),
            None => std::env::remove_var("LIVEKIT_URL"),
        }
    }

    #[test]
    fn room_type_default_is_one_to_one() {
        assert_eq!(RoomType::default(), RoomType::OneToOne);
    }

    #[test]
    fn token_request_parses_with_defaults() {
        let req: TokenRequest = serde_json::from_str(r#"{"room":"r1"}"#).unwrap();
        assert_eq!(req.room, "r1");
        assert_eq!(req.room_type, RoomType::OneToOne);
        assert!(!req.enable_recording);
        assert!(req.can_publish);
        assert!(req.can_subscribe);
    }

    #[test]
    fn token_request_parses_room_type_variants() {
        let req: TokenRequest =
            serde_json::from_str(r#"{"room":"r","room_type":"multi_party"}"#).unwrap();
        assert_eq!(req.room_type, RoomType::MultiParty);
        let req: TokenRequest =
            serde_json::from_str(r#"{"room":"r","room_type":"broadcast"}"#).unwrap();
        assert_eq!(req.room_type, RoomType::Broadcast);
    }
}
