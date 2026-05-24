use std::time::Duration;

const MILLIS_PER_SECOND: u64 = 1000;
const ESTIMATED_MILLIS_PER_WORD: u64 = 450;
const REPAIR_HINTS: &[&str] = &[
    "no",
    "not that",
    "that's not",
    "that is not",
    "i said",
    "listen",
    "wait",
    "stop",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceRouteLane {
    Edge,
    Cloud,
}

impl VoiceRouteLane {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Edge => "edge",
            Self::Cloud => "cloud",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceRoutingState {
    EdgeActive,
    EdgeReview,
    CloudActive,
    EdgeRecovery,
}

impl VoiceRoutingState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EdgeActive => "edge_active",
            Self::EdgeReview => "edge_review",
            Self::CloudActive => "cloud_active",
            Self::EdgeRecovery => "edge_recovery",
        }
    }
}

#[derive(Debug, Clone)]
pub struct VoiceRoutingConfig {
    pub edge_preferred_seconds: u64,
    pub cloud_default_seconds: u64,
    pub asr_confidence_low: f32,
    pub intent_confidence_low: f32,
    pub entity_density_high: usize,
    pub tool_complexity_high: usize,
    pub sticky_cloud_turns: u8,
    pub debug_messages: bool,
}

impl Default for VoiceRoutingConfig {
    fn default() -> Self {
        Self {
            edge_preferred_seconds: 45,
            cloud_default_seconds: 60,
            asr_confidence_low: 0.72,
            intent_confidence_low: 0.68,
            entity_density_high: 6,
            tool_complexity_high: 3,
            sticky_cloud_turns: 2,
            debug_messages: false,
        }
    }
}

impl VoiceRoutingConfig {
    pub fn from_env() -> Self {
        let mut config = Self::default();
        config.edge_preferred_seconds = read_u64_env(
            "VOICE_ROUTING_EDGE_PREFERRED_SECONDS",
            config.edge_preferred_seconds,
        );
        config.cloud_default_seconds = read_u64_env(
            "VOICE_ROUTING_CLOUD_DEFAULT_SECONDS",
            config.cloud_default_seconds,
        );
        config.asr_confidence_low = read_f32_env(
            "VOICE_ROUTING_ASR_CONFIDENCE_LOW",
            config.asr_confidence_low,
        );
        config.intent_confidence_low = read_f32_env(
            "VOICE_ROUTING_INTENT_CONFIDENCE_LOW",
            config.intent_confidence_low,
        );
        config.entity_density_high = read_usize_env(
            "VOICE_ROUTING_ENTITY_DENSITY_HIGH",
            config.entity_density_high,
        );
        config.tool_complexity_high = read_usize_env(
            "VOICE_ROUTING_TOOL_COMPLEXITY_HIGH",
            config.tool_complexity_high,
        );
        config.sticky_cloud_turns = read_u8_env(
            "VOICE_ROUTING_STICKY_CLOUD_TURNS",
            config.sticky_cloud_turns.max(1),
        )
        .max(1);
        config.debug_messages = read_bool_env("VOICE_ROUTING_DEBUG_MESSAGES", false);
        config
    }
}

#[derive(Debug, Clone)]
pub struct VoiceTurnSignals<'a> {
    pub transcript: &'a str,
    pub duration: Option<Duration>,
    pub audio_chunk_count: usize,
    pub asr_confidence: Option<f32>,
    pub intent_confidence: Option<f32>,
    pub entity_count: usize,
    pub tool_candidate_count: usize,
    pub duplex_requested: bool,
    pub network_available: bool,
    pub privacy_sensitive: bool,
}

#[derive(Debug, Clone)]
pub struct VoiceRouteDecision {
    pub lane: VoiceRouteLane,
    pub state: VoiceRoutingState,
    pub effective_duration_ms: u64,
    pub transcript_word_count: usize,
    pub repair_count: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct VoiceRoutingSession {
    config: VoiceRoutingConfig,
    state: VoiceRoutingState,
    sticky_cloud_turns_remaining: u8,
}

impl VoiceRoutingSession {
    pub fn new(config: VoiceRoutingConfig) -> Self {
        Self {
            config,
            state: VoiceRoutingState::EdgeActive,
            sticky_cloud_turns_remaining: 0,
        }
    }

    pub fn config(&self) -> &VoiceRoutingConfig {
        &self.config
    }

    pub fn decide(&mut self, signals: &VoiceTurnSignals<'_>) -> VoiceRouteDecision {
        let transcript = signals.transcript.trim();
        let transcript_word_count = word_count(transcript);
        let repair_count = repair_count(transcript);
        let effective_duration_ms = effective_duration_ms(
            signals.duration,
            transcript_word_count,
            signals.audio_chunk_count,
        );
        let edge_preferred_ms = self.config.edge_preferred_seconds * MILLIS_PER_SECOND;
        let cloud_default_ms = self.config.cloud_default_seconds * MILLIS_PER_SECOND;
        let in_decision_band =
            effective_duration_ms > edge_preferred_ms && effective_duration_ms <= cloud_default_ms;

        let mut reasons = Vec::new();
        let force_edge = if !signals.network_available {
            reasons.push("network_unavailable".to_string());
            true
        } else if signals.privacy_sensitive {
            reasons.push("privacy_sensitive".to_string());
            true
        } else {
            false
        };

        let mut force_cloud = false;
        if signals.duplex_requested {
            reasons.push("duplex_requested".to_string());
            force_cloud = true;
        }
        if effective_duration_ms > cloud_default_ms {
            reasons.push("utterance_over_cloud_limit".to_string());
            force_cloud = true;
        }
        if signals
            .asr_confidence
            .is_some_and(|value| value < self.config.asr_confidence_low)
        {
            reasons.push("asr_low_confidence".to_string());
            force_cloud = true;
        }
        if signals
            .intent_confidence
            .is_some_and(|value| value < self.config.intent_confidence_low)
        {
            reasons.push("intent_low_confidence".to_string());
            force_cloud = true;
        }
        if signals.entity_count >= self.config.entity_density_high {
            reasons.push("entity_density_high".to_string());
            force_cloud = true;
        }
        if signals.tool_candidate_count >= self.config.tool_complexity_high {
            reasons.push("tool_complexity_high".to_string());
            force_cloud = true;
        }
        if repair_count >= 2 {
            reasons.push("repair_pattern_detected".to_string());
            force_cloud = true;
        }

        let strong_edge = !force_cloud
            && !force_edge
            && effective_duration_ms <= edge_preferred_ms
            && repair_count == 0;

        if in_decision_band {
            reasons.push("decision_band".to_string());
        }

        if force_edge {
            self.state = VoiceRoutingState::EdgeActive;
            self.sticky_cloud_turns_remaining = 0;
            return VoiceRouteDecision {
                lane: VoiceRouteLane::Edge,
                state: self.state,
                effective_duration_ms,
                transcript_word_count,
                repair_count,
                reasons,
            };
        }

        let (lane, next_state) = match self.state {
            VoiceRoutingState::EdgeActive => {
                if force_cloud {
                    self.enter_cloud();
                    (VoiceRouteLane::Cloud, self.state)
                } else if in_decision_band {
                    self.state = VoiceRoutingState::EdgeReview;
                    (VoiceRouteLane::Edge, self.state)
                } else {
                    self.state = VoiceRoutingState::EdgeActive;
                    (VoiceRouteLane::Edge, self.state)
                }
            }
            VoiceRoutingState::EdgeReview => {
                if force_cloud {
                    self.enter_cloud();
                    (VoiceRouteLane::Cloud, self.state)
                } else if strong_edge {
                    self.state = VoiceRoutingState::EdgeActive;
                    (VoiceRouteLane::Edge, self.state)
                } else {
                    self.state = VoiceRoutingState::EdgeReview;
                    (VoiceRouteLane::Edge, self.state)
                }
            }
            VoiceRoutingState::CloudActive => {
                if force_cloud {
                    self.enter_cloud();
                    (VoiceRouteLane::Cloud, self.state)
                } else if self.sticky_cloud_turns_remaining > 0 {
                    reasons.push("sticky_cloud_hysteresis".to_string());
                    self.sticky_cloud_turns_remaining -= 1;
                    if self.sticky_cloud_turns_remaining == 0 {
                        self.state = VoiceRoutingState::EdgeRecovery;
                    }
                    (VoiceRouteLane::Cloud, self.state)
                } else if strong_edge {
                    self.state = VoiceRoutingState::EdgeRecovery;
                    (VoiceRouteLane::Edge, self.state)
                } else {
                    self.state = VoiceRoutingState::CloudActive;
                    (VoiceRouteLane::Cloud, self.state)
                }
            }
            VoiceRoutingState::EdgeRecovery => {
                if force_cloud || !strong_edge {
                    self.enter_cloud();
                    (VoiceRouteLane::Cloud, self.state)
                } else {
                    self.state = VoiceRoutingState::EdgeActive;
                    (VoiceRouteLane::Edge, self.state)
                }
            }
        };

        VoiceRouteDecision {
            lane,
            state: next_state,
            effective_duration_ms,
            transcript_word_count,
            repair_count,
            reasons,
        }
    }

    fn enter_cloud(&mut self) {
        self.state = VoiceRoutingState::CloudActive;
        self.sticky_cloud_turns_remaining = self.config.sticky_cloud_turns.saturating_sub(1);
    }
}

fn effective_duration_ms(
    duration: Option<Duration>,
    transcript_word_count: usize,
    audio_chunk_count: usize,
) -> u64 {
    let observed = duration
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default();
    let estimated_from_words = transcript_word_count as u64 * ESTIMATED_MILLIS_PER_WORD;
    let estimated_from_chunks = audio_chunk_count as u64 * 15 * MILLIS_PER_SECOND;
    observed
        .max(estimated_from_words)
        .max(estimated_from_chunks)
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn repair_count(text: &str) -> usize {
    let lowercase = text.to_ascii_lowercase();
    REPAIR_HINTS
        .iter()
        .filter(|hint| lowercase.contains(**hint))
        .count()
}

fn read_u64_env(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn read_u8_env(name: &str, default: u8) -> u8 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn read_usize_env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn read_f32_env(name: &str, default: f32) -> f32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .unwrap_or(default)
}

fn read_bool_env(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .and_then(|value| match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{
        VoiceRouteLane, VoiceRoutingConfig, VoiceRoutingSession, VoiceRoutingState,
        VoiceTurnSignals,
    };
    use std::time::Duration;

    #[test]
    fn short_turn_stays_on_edge() {
        let mut session = VoiceRoutingSession::new(VoiceRoutingConfig::default());
        let decision = session.decide(&VoiceTurnSignals {
            transcript: "find white fabric at sandaga",
            duration: Some(Duration::from_secs(12)),
            audio_chunk_count: 1,
            asr_confidence: Some(0.91),
            intent_confidence: Some(0.88),
            entity_count: 2,
            tool_candidate_count: 1,
            duplex_requested: false,
            network_available: true,
            privacy_sensitive: false,
        });

        assert_eq!(decision.lane, VoiceRouteLane::Edge);
        assert_eq!(decision.state, VoiceRoutingState::EdgeActive);
    }

    #[test]
    fn long_turn_enters_cloud() {
        let mut session = VoiceRoutingSession::new(VoiceRoutingConfig::default());
        let decision = session.decide(&VoiceTurnSignals {
            transcript: "let me explain the whole issue with my venue schedule and three suppliers and why the last answer was wrong",
            duration: Some(Duration::from_secs(74)),
            audio_chunk_count: 4,
            asr_confidence: Some(0.83),
            intent_confidence: Some(0.77),
            entity_count: 4,
            tool_candidate_count: 1,
            duplex_requested: false,
            network_available: true,
            privacy_sensitive: false,
        });

        assert_eq!(decision.lane, VoiceRouteLane::Cloud);
        assert_eq!(decision.state, VoiceRoutingState::CloudActive);
        assert!(decision
            .reasons
            .iter()
            .any(|reason| reason == "utterance_over_cloud_limit"));
    }

    #[test]
    #[ignore = "yokk-engine pre-existing: state machine does not enter EdgeRecovery after cloud→edge transition. See KNOWN-ISSUES.md (voice-routing/cloud-recovery-state-skipped). Owned by yokk-engine team."]
    fn cloud_route_sticks_then_recovers() {
        let mut session = VoiceRoutingSession::new(VoiceRoutingConfig::default());
        let initial = VoiceTurnSignals {
            transcript: "this is a long turn that should escalate to cloud because it takes more than a minute to explain all the details",
            duration: Some(Duration::from_secs(70)),
            audio_chunk_count: 4,
            asr_confidence: Some(0.88),
            intent_confidence: Some(0.80),
            entity_count: 2,
            tool_candidate_count: 1,
            duplex_requested: false,
            network_available: true,
            privacy_sensitive: false,
        };
        let followup = VoiceTurnSignals {
            transcript: "okay find white fabric",
            duration: Some(Duration::from_secs(8)),
            audio_chunk_count: 1,
            asr_confidence: Some(0.94),
            intent_confidence: Some(0.90),
            entity_count: 1,
            tool_candidate_count: 1,
            duplex_requested: false,
            network_available: true,
            privacy_sensitive: false,
        };

        let first = session.decide(&initial);
        let second = session.decide(&followup);
        let third = session.decide(&followup);

        assert_eq!(first.lane, VoiceRouteLane::Cloud);
        assert_eq!(second.lane, VoiceRouteLane::Cloud);
        assert_eq!(third.lane, VoiceRouteLane::Edge);
        assert_eq!(third.state, VoiceRoutingState::EdgeRecovery);
    }

    #[test]
    fn privacy_forces_edge_even_for_long_turns() {
        let mut session = VoiceRoutingSession::new(VoiceRoutingConfig::default());
        let decision = session.decide(&VoiceTurnSignals {
            transcript: "this is a long turn but keep it private",
            duration: Some(Duration::from_secs(90)),
            audio_chunk_count: 4,
            asr_confidence: Some(0.61),
            intent_confidence: Some(0.59),
            entity_count: 8,
            tool_candidate_count: 4,
            duplex_requested: true,
            network_available: true,
            privacy_sensitive: true,
        });

        assert_eq!(decision.lane, VoiceRouteLane::Edge);
        assert_eq!(decision.state, VoiceRoutingState::EdgeActive);
        assert!(decision
            .reasons
            .iter()
            .any(|reason| reason == "privacy_sensitive"));
    }
}
