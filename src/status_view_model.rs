use crate::input_queue::TurnState;
use crate::mcp_runtime_config::{McpRuntimeAdmissionStatus, McpRuntimeConfig};
use crate::provider_adapter_admission::{ProviderAdapterAdmission, ProviderAdapterAdmissionStatus};
use crate::provider_runtime_config::{ProviderRuntimeAdmissionStatus, ProviderRuntimeConfig};
use crate::terminal_runtime_config::{TerminalRuntimeConfig, TerminalRuntimeStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderRuntimeState {
    Disabled,
    Configured,
    Refused,
    Idle,
    Working,
    Interrupted,
    Failed,
}

impl ProviderRuntimeState {
    pub fn from_provider_runtime_config(config: &ProviderRuntimeConfig) -> Self {
        match config.status {
            ProviderRuntimeAdmissionStatus::Disabled => Self::Disabled,
            ProviderRuntimeAdmissionStatus::Configured => Self::Configured,
            ProviderRuntimeAdmissionStatus::Refused => Self::Refused,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Configured => "configured",
            Self::Refused => "refused",
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderAdapterState {
    Disabled,
    ConfiguredWithoutAdapter,
    Refused,
    Admitted,
}

impl ProviderAdapterState {
    pub fn from_provider_adapter_admission(admission: &ProviderAdapterAdmission) -> Self {
        match admission.status {
            ProviderAdapterAdmissionStatus::Disabled => Self::Disabled,
            ProviderAdapterAdmissionStatus::ConfiguredWithoutAdapter => {
                Self::ConfiguredWithoutAdapter
            }
            ProviderAdapterAdmissionStatus::Refused => Self::Refused,
            ProviderAdapterAdmissionStatus::Admitted => Self::Admitted,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::ConfiguredWithoutAdapter => "configured_without_adapter",
            Self::Refused => "refused",
            Self::Admitted => "admitted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpRuntimeState {
    Disabled,
    Configured,
    Refused,
}

impl McpRuntimeState {
    pub fn from_mcp_runtime_config(config: &McpRuntimeConfig) -> Self {
        match config.status {
            McpRuntimeAdmissionStatus::Disabled => Self::Disabled,
            McpRuntimeAdmissionStatus::Configured => Self::Configured,
            McpRuntimeAdmissionStatus::Refused => Self::Refused,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Configured => "configured",
            Self::Refused => "refused",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalRuntimeState {
    Disabled,
    Configured,
    Refused,
}

impl TerminalRuntimeState {
    pub fn from_terminal_runtime_config(config: &TerminalRuntimeConfig) -> Self {
        match config.status {
            TerminalRuntimeStatus::Disabled => Self::Disabled,
            TerminalRuntimeStatus::Configured => Self::Configured,
            TerminalRuntimeStatus::Refused => Self::Refused,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Configured => "configured",
            Self::Refused => "refused",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePostureState {
    pub provider_state: ProviderRuntimeState,
    pub provider_adapter_state: ProviderAdapterState,
    pub mcp_state: McpRuntimeState,
    pub terminal_state: TerminalRuntimeState,
}

impl RuntimePostureState {
    pub fn disabled() -> Self {
        Self {
            provider_state: ProviderRuntimeState::Disabled,
            provider_adapter_state: ProviderAdapterState::Disabled,
            mcp_state: McpRuntimeState::Disabled,
            terminal_state: TerminalRuntimeState::Disabled,
        }
    }

    pub fn from_runtime_configs(
        provider_config: &ProviderRuntimeConfig,
        provider_adapter_admission: &ProviderAdapterAdmission,
        mcp_config: &McpRuntimeConfig,
        terminal_config: &TerminalRuntimeConfig,
    ) -> Self {
        Self {
            provider_state: ProviderRuntimeState::from_provider_runtime_config(provider_config),
            provider_adapter_state: ProviderAdapterState::from_provider_adapter_admission(
                provider_adapter_admission,
            ),
            mcp_state: McpRuntimeState::from_mcp_runtime_config(mcp_config),
            terminal_state: TerminalRuntimeState::from_terminal_runtime_config(terminal_config),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusViewInput {
    pub identity: String,
    pub session: String,
    pub turn_state: TurnState,
    pub active_phase: Option<String>,
    pub active_turn_age: Option<String>,
    pub queued_inputs: usize,
    pub held_system_directives: usize,
    pub oldest_held_age: Option<String>,
    pub transcript_items: usize,
    pub runtime_posture: RuntimePostureState,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusSegment {
    pub key: String,
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusViewModel {
    pub segments: Vec<StatusSegment>,
    pub compact_line: String,
}

pub fn build_status_view(input: &StatusViewInput) -> StatusViewModel {
    let mut segments = vec![
        segment("identity", "agent", &input.identity),
        segment("session", "session", &input.session),
        segment(
            "turn_state",
            "turn",
            &turn_state_status_value(
                input.turn_state,
                input.active_phase.as_deref(),
                input.active_turn_age.as_deref(),
            ),
        ),
        segment("queued_inputs", "queued", &input.queued_inputs.to_string()),
        segment(
            "held_system_directives",
            "held",
            &input.held_system_directives.to_string(),
        ),
    ];
    if let Some(oldest_held_age) = &input.oldest_held_age {
        segments.push(segment("oldest_held_age", "oldest", oldest_held_age));
    }
    if input.turn_state == TurnState::Active {
        segments.push(segment("esc_action", "Esc", "interrupt"));
    }
    segments.extend([
        segment(
            "transcript_items",
            "transcript",
            &input.transcript_items.to_string(),
        ),
        segment(
            "provider_state",
            "provider",
            input.runtime_posture.provider_state.as_str(),
        ),
        segment(
            "provider_adapter_state",
            "provider_adapter",
            input.runtime_posture.provider_adapter_state.as_str(),
        ),
        segment("mcp_state", "mcp", input.runtime_posture.mcp_state.as_str()),
        segment(
            "terminal_state",
            "terminal",
            input.runtime_posture.terminal_state.as_str(),
        ),
        segment(
            "last_error",
            "error",
            input.last_error.as_deref().unwrap_or("none"),
        ),
    ]);
    let compact_line = segments
        .iter()
        .filter(|segment| status_segment_is_visible(segment))
        .filter(|segment| segment.key != "identity")
        .map(status_segment_compact_text)
        .collect::<Vec<_>>()
        .join(" | ");

    StatusViewModel {
        segments,
        compact_line,
    }
}

fn segment(key: &str, label: &str, value: &str) -> StatusSegment {
    StatusSegment {
        key: key.to_string(),
        label: label.to_string(),
        value: value.to_string(),
    }
}

pub(crate) fn status_segment_is_visible(segment: &StatusSegment) -> bool {
    !matches!(
        (segment.key.as_str(), segment.value.as_str()),
        ("queued_inputs", "0")
            | ("held_system_directives", "0")
            | ("transcript_items", "0")
            | ("last_error", "none")
    )
}

pub(crate) fn status_segment_compact_text(segment: &StatusSegment) -> String {
    if segment.key == "identity" {
        return segment.value.clone();
    }
    let label = status_segment_compact_label(segment);
    let value = status_segment_compact_value(segment);
    if label.is_empty() {
        value
    } else {
        format!("{label} {value}")
    }
}

fn status_segment_compact_label(segment: &StatusSegment) -> String {
    match segment.key.as_str() {
        "turn_state" => String::new(),
        "draft_chars" => "draft".to_string(),
        "queued_inputs" => "queued operator steering".to_string(),
        "held_system_directives" => "held system directives".to_string(),
        "oldest_held_age" => "oldest".to_string(),
        "esc_action" => "Esc".to_string(),
        "transcript_scroll_offset" => "scroll".to_string(),
        "provider_adapter_state" => "provider adapter".to_string(),
        "provider_state" => "provider".to_string(),
        "mcp_state" => "mcp".to_string(),
        "terminal_state" => "terminal".to_string(),
        "last_error" => "error".to_string(),
        "transcript_items" => "transcript".to_string(),
        "session" => "session".to_string(),
        _ => segment.label.clone(),
    }
}

fn status_segment_compact_value(segment: &StatusSegment) -> String {
    match segment.key.as_str() {
        "turn_state" => turn_state_display_value(&segment.value),
        "draft_chars" if segment.value == "1" => "1 char".to_string(),
        "draft_chars" => format!("{} chars", segment.value),
        "transcript_scroll_offset" if segment.value == "1" => "1 line".to_string(),
        "transcript_scroll_offset" => format!("{} lines", segment.value),
        "provider_state"
        | "provider_adapter_state"
        | "mcp_state"
        | "terminal_state"
        | "last_error" => human_status_value(&segment.value),
        _ => segment.value.clone(),
    }
}

fn human_status_value(value: &str) -> String {
    value.replace('_', " ")
}

fn turn_state_status_value(
    turn_state: TurnState,
    active_phase: Option<&str>,
    active_turn_age: Option<&str>,
) -> String {
    match (turn_state, active_phase, active_turn_age) {
        (TurnState::Idle, _, _) => "idle".to_string(),
        (TurnState::Active, Some(phase), _) => phase.to_string(),
        (TurnState::Active, None, Some(age)) => format!("active {age}"),
        (TurnState::Active, None, None) => "active".to_string(),
    }
}

pub(crate) fn turn_state_display_value(value: &str) -> String {
    if value == "active" {
        return "thinking".to_string();
    }
    if let Some(age) = value.strip_prefix("active ") {
        return format!("thinking {age}");
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_runtime_config::{
        mcp_config_env_var, mcp_fabric_env_var, site_mcp_fabric_env_var,
    };
    use crate::provider_adapter_contract::provider_adapter_contract;
    use crate::terminal_runtime_contract::terminal_runtime_contract;

    fn input() -> StatusViewInput {
        StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 2,
            held_system_directives: 1,
            oldest_held_age: None,
            transcript_items: 8,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        }
    }

    fn admitted_provider() -> &'static str {
        provider_adapter_contract()
            .admitted_providers
            .first()
            .expect("provider contract has at least one admitted provider")
            .as_str()
    }

    fn provider_env(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        let contract = provider_adapter_contract();
        pairs
            .iter()
            .map(|(semantic_key, value)| {
                let env_key = match *semantic_key {
                    "execution_enabled" => &contract.provider_execution_env_var,
                    "provider" => &contract.intelligence_provider_env_var,
                    "model" => &contract.ai_model_env_var,
                    unexpected => panic!("unknown provider env semantic key: {unexpected}"),
                };
                (env_key.clone(), value.to_string())
            })
            .collect()
    }

    fn mcp_env(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(semantic_key, value)| {
                let env_key = match *semantic_key {
                    "fabric_enabled" => mcp_fabric_env_var(),
                    "config" => mcp_config_env_var(),
                    "fabric" => site_mcp_fabric_env_var(),
                    unexpected => panic!("unknown MCP env semantic key: {unexpected}"),
                };
                (env_key.to_string(), value.to_string())
            })
            .collect()
    }

    fn terminal_env(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        let contract = terminal_runtime_contract();
        pairs
            .iter()
            .map(|(semantic_key, value)| {
                let env_key = match *semantic_key {
                    "rendering_enabled" => &contract.terminal_rendering_env_var,
                    "mode" => &contract.terminal_mode_env_var,
                    unexpected => panic!("unknown terminal env semantic key: {unexpected}"),
                };
                (env_key.clone(), value.to_string())
            })
            .collect()
    }

    #[test]
    fn builds_status_segments_in_stable_order() {
        let model = build_status_view(&input());

        assert_eq!(model.segments.len(), 11);
        assert_eq!(model.segments[0].key, "identity");
        assert_eq!(model.segments[0].label, "agent");
        assert_eq!(model.segments[0].value, "sonar.resident");
        assert_eq!(model.segments[3].key, "queued_inputs");
        assert_eq!(model.segments[3].value, "2");
        assert_eq!(model.segments[6].value, "disabled");
        assert_eq!(model.segments[7].value, "disabled");
        assert_eq!(model.segments[8].value, "disabled");
        assert_eq!(model.segments[9].value, "disabled");
        assert_eq!(model.segments[10].key, "last_error");
        assert_eq!(model.segments[10].value, "none");
    }

    #[test]
    fn builds_compact_status_line() {
        let model = build_status_view(&input());

        assert_eq!(
            model.compact_line,
            "session carrier_1 | idle | queued operator steering 2 | held system directives 1 | transcript 8 | provider disabled | provider adapter disabled | mcp disabled | terminal disabled"
        );
    }

    #[test]
    fn compact_status_hides_zero_counters_and_absent_errors() {
        let model = build_status_view(&StatusViewInput {
            queued_inputs: 0,
            held_system_directives: 0,
            transcript_items: 0,
            last_error: None,
            ..input()
        });

        assert!(!model.compact_line.contains("queued operator steering 0"));
        assert!(!model.compact_line.contains("held system directives 0"));
        assert!(!model.compact_line.contains("transcript 0"));
        assert!(!model.compact_line.contains("error none"));
        assert!(model.compact_line.contains("session carrier_1"));
        assert!(model.compact_line.contains("provider disabled"));
    }

    #[test]
    fn formats_active_phase_before_generic_turn_age() {
        let model = build_status_view(&StatusViewInput {
            turn_state: TurnState::Active,
            active_phase: Some("calling site_loop_run_once 8s".to_string()),
            active_turn_age: Some("1m 12s".to_string()),
            ..input()
        });

        assert_eq!(model.segments[2].value, "calling site_loop_run_once 8s");
        assert!(model.compact_line.contains("calling site_loop_run_once 8s"));
        assert!(!model.compact_line.contains("thinking 1m 12s"));
    }

    #[test]
    fn formats_active_turn_age_as_thinking_elapsed_time() {
        let model = build_status_view(&StatusViewInput {
            turn_state: TurnState::Active,
            active_phase: None,
            active_turn_age: Some("1m 12s".to_string()),
            ..input()
        });

        assert_eq!(model.segments[2].value, "active 1m 12s");
        assert!(model.compact_line.contains("thinking 1m 12s"));
    }

    #[test]
    fn includes_oldest_held_age_when_present() {
        let model = build_status_view(&StatusViewInput {
            oldest_held_age: Some("1m 14s".to_string()),
            ..input()
        });

        assert!(
            model
                .compact_line
                .contains("held system directives 1 | oldest 1m 14s")
        );
    }

    #[test]
    fn includes_last_error_when_present() {
        let model = build_status_view(&StatusViewInput {
            last_error: Some("provider_cancelled".to_string()),
            ..input()
        });

        assert_eq!(model.segments[10].value, "provider_cancelled");
        assert!(model.compact_line.ends_with("error provider cancelled"));
    }

    #[test]
    fn humanizes_runtime_status_values_in_compact_line() {
        let model = build_status_view(&StatusViewInput {
            runtime_posture: RuntimePostureState {
                provider_adapter_state: ProviderAdapterState::ConfiguredWithoutAdapter,
                ..RuntimePostureState::disabled()
            },
            ..input()
        });

        assert!(
            model
                .compact_line
                .contains("provider adapter configured without adapter")
        );
        assert!(!model.compact_line.contains("configured_without_adapter"));
    }

    #[test]
    fn compact_status_contract_formats_app_added_segments() {
        assert_eq!(
            status_segment_compact_text(&segment("draft_chars", "draft", "1")),
            "draft 1 char"
        );
        assert_eq!(
            status_segment_compact_text(&segment("draft_chars", "draft", "14")),
            "draft 14 chars"
        );
        assert_eq!(
            status_segment_compact_text(&segment("transcript_scroll_offset", "scroll", "1")),
            "scroll 1 line"
        );
        assert_eq!(
            status_segment_compact_text(&segment("transcript_scroll_offset", "scroll", "16")),
            "scroll 16 lines"
        );
    }

    #[test]
    fn maps_provider_runtime_config_to_provider_state() {
        let disabled = ProviderRuntimeConfig::disabled();
        assert_eq!(
            ProviderRuntimeState::from_provider_runtime_config(&disabled),
            ProviderRuntimeState::Disabled
        );

        let configured = ProviderRuntimeConfig::from_env_map(&provider_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));
        assert_eq!(
            ProviderRuntimeState::from_provider_runtime_config(&configured),
            ProviderRuntimeState::Configured
        );
        let refused =
            ProviderRuntimeConfig::from_env_map(&provider_env(&[("execution_enabled", "true")]));
        assert_eq!(
            ProviderRuntimeState::from_provider_runtime_config(&refused),
            ProviderRuntimeState::Refused
        );
    }

    #[test]
    fn maps_mcp_runtime_config_to_mcp_state() {
        let disabled = McpRuntimeConfig::disabled();
        assert_eq!(
            McpRuntimeState::from_mcp_runtime_config(&disabled),
            McpRuntimeState::Disabled
        );

        let configured = McpRuntimeConfig::from_env_map(&mcp_env(&[
            ("fabric_enabled", "true"),
            ("config", "D:/site/.ai/mcp/config.json"),
            ("fabric", "D:/site/.ai/mcp"),
        ]));
        assert_eq!(
            McpRuntimeState::from_mcp_runtime_config(&configured),
            McpRuntimeState::Configured
        );

        let refused = McpRuntimeConfig::from_env_map(&mcp_env(&[("fabric_enabled", "true")]));
        assert_eq!(
            McpRuntimeState::from_mcp_runtime_config(&refused),
            McpRuntimeState::Refused
        );
    }

    #[test]
    fn maps_terminal_runtime_config_to_terminal_state() {
        let disabled = TerminalRuntimeConfig::disabled();
        assert_eq!(
            TerminalRuntimeState::from_terminal_runtime_config(&disabled),
            TerminalRuntimeState::Disabled
        );

        let terminal_contract = terminal_runtime_contract();
        let configured = TerminalRuntimeConfig::from_env_map(&terminal_env(&[
            ("rendering_enabled", "true"),
            ("mode", terminal_contract.required_terminal_mode.as_str()),
        ]));
        assert_eq!(
            TerminalRuntimeState::from_terminal_runtime_config(&configured),
            TerminalRuntimeState::Configured
        );

        let refused =
            TerminalRuntimeConfig::from_env_map(&terminal_env(&[("rendering_enabled", "true")]));
        assert_eq!(
            TerminalRuntimeState::from_terminal_runtime_config(&refused),
            TerminalRuntimeState::Refused
        );
    }

    #[test]
    fn builds_runtime_posture_bundle_from_runtime_configs() {
        let provider = ProviderRuntimeConfig::from_env_map(&provider_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));
        let mcp = McpRuntimeConfig::from_env_map(&mcp_env(&[
            ("fabric_enabled", "true"),
            ("config", "D:/site/.ai/mcp/config.json"),
            ("fabric", "D:/site/.ai/mcp"),
        ]));
        let terminal_contract = terminal_runtime_contract();
        let terminal = TerminalRuntimeConfig::from_env_map(&terminal_env(&[
            ("rendering_enabled", "true"),
            ("mode", terminal_contract.required_terminal_mode.as_str()),
        ]));

        let provider_adapter = ProviderAdapterAdmission::from_runtime_config(&provider, None);
        let posture = RuntimePostureState::from_runtime_configs(
            &provider,
            &provider_adapter,
            &mcp,
            &terminal,
        );

        assert_eq!(posture.provider_state, ProviderRuntimeState::Configured);
        assert_eq!(
            posture.provider_adapter_state,
            ProviderAdapterState::ConfiguredWithoutAdapter
        );
        let model = build_status_view(&StatusViewInput {
            runtime_posture: posture.clone(),
            ..input()
        });
        assert!(
            model
                .compact_line
                .contains("provider adapter configured without adapter")
        );
        assert_eq!(posture.mcp_state, McpRuntimeState::Configured);
        assert_eq!(posture.terminal_state, TerminalRuntimeState::Configured);
    }

    #[test]
    fn represents_active_turn_and_provider_working() {
        let model = build_status_view(&StatusViewInput {
            turn_state: TurnState::Active,
            runtime_posture: RuntimePostureState {
                provider_state: ProviderRuntimeState::Working,
                ..RuntimePostureState::disabled()
            },
            ..input()
        });

        assert_eq!(model.segments[2].value, "active");
        assert_eq!(model.segments[5].key, "esc_action");
        assert_eq!(model.segments[5].value, "interrupt");
        assert_eq!(model.segments[7].value, "working");
        assert_eq!(model.segments[8].value, "disabled");
    }
}
