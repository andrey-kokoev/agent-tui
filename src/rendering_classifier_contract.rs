use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const TRANSCRIPT_CLASSIFIERS_CONTRACT_JSON: &str = include_str!(
    "../../narada/packages/carrier-rendering-contract/contracts/transcript-classifiers.json"
);
const EXPECTED_SCHEMA: &str = "narada.carrier.transcript_rendering_classifiers.v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TranscriptClassifiersContract {
    pub schema: String,
    pub turn_state: TurnStateClassifiers,
    pub terminal_status: TerminalStatusClassifiers,
    pub tool_result: ToolResultClassifiers,
    pub semantic_status_value: SemanticStatusValueClassifiers,
    pub runtime_status: RuntimeStatusClassifiers,
    pub diagnostic: DiagnosticClassifiers,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TurnStateClassifiers {
    pub active: String,
    pub active_display: String,
    pub thinking_prefix: String,
    pub calling_prefix: String,
    pub suppressed_markers: Vec<String>,
    pub positive_values: Vec<String>,
    pub positive_prefixes: Vec<String>,
    pub negative_values: Vec<String>,
    pub duration_phases: Vec<String>,
    pub operator_activity_actions: Vec<String>,
    pub operator_activity_actor: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TerminalStatusClassifiers {
    pub positive: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolResultClassifiers {
    pub positive_prefixes: Vec<String>,
    pub negative_prefixes: Vec<String>,
    pub positive_summaries: Vec<String>,
    pub negative_summaries: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SemanticStatusValueClassifiers {
    pub positive: Vec<String>,
    pub negative: Vec<String>,
    pub warning: Vec<String>,
    pub muted: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuntimeStatusClassifiers {
    pub positive: Vec<String>,
    pub warning_prefixes: Vec<String>,
    pub negative_prefixes: Vec<String>,
    pub muted: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiagnosticClassifiers {
    pub prefix: String,
    pub warning_severities: Vec<String>,
    pub negative_severities: Vec<String>,
    pub positive_severities: Vec<String>,
}

static TRANSCRIPT_CLASSIFIERS_CONTRACT: OnceLock<TranscriptClassifiersContract> = OnceLock::new();

pub fn transcript_classifiers_contract() -> &'static TranscriptClassifiersContract {
    TRANSCRIPT_CLASSIFIERS_CONTRACT.get_or_init(|| {
        parse_transcript_classifiers_contract(TRANSCRIPT_CLASSIFIERS_CONTRACT_JSON)
            .expect("bundled transcript classifiers contract must be valid")
    })
}

pub fn parse_transcript_classifiers_contract(
    json_text: &str,
) -> Result<TranscriptClassifiersContract, String> {
    let contract: TranscriptClassifiersContract = serde_json::from_str(json_text)
        .map_err(|error| format!("transcript_classifiers_contract_parse_failed:{error}"))?;
    if contract.schema != EXPECTED_SCHEMA {
        return Err("transcript_classifiers_contract_invalid:schema".to_string());
    }
    if contract.turn_state.active.is_empty()
        || contract.turn_state.active_display.is_empty()
        || contract.turn_state.thinking_prefix.is_empty()
        || contract.turn_state.calling_prefix.is_empty()
        || contract.turn_state.positive_values.is_empty()
        || contract.turn_state.negative_values.is_empty()
        || contract.turn_state.duration_phases.is_empty()
        || contract.turn_state.operator_activity_actions.is_empty()
        || contract.turn_state.operator_activity_actor.is_empty()
    {
        return Err("transcript_classifiers_contract_invalid:turn_state".to_string());
    }
    if contract.terminal_status.positive.is_empty() {
        return Err("transcript_classifiers_contract_invalid:terminal_status".to_string());
    }
    if contract.tool_result.positive_prefixes.is_empty()
        || contract.tool_result.negative_prefixes.is_empty()
        || contract.tool_result.positive_summaries.is_empty()
        || contract.tool_result.negative_summaries.is_empty()
    {
        return Err("transcript_classifiers_contract_invalid:tool_result".to_string());
    }
    if contract.semantic_status_value.positive.is_empty()
        || contract.semantic_status_value.negative.is_empty()
        || contract.semantic_status_value.warning.is_empty()
        || contract.semantic_status_value.muted.is_empty()
    {
        return Err("transcript_classifiers_contract_invalid:semantic_status_value".to_string());
    }
    if contract.runtime_status.positive.is_empty()
        || contract.runtime_status.warning_prefixes.is_empty()
        || contract.runtime_status.negative_prefixes.is_empty()
        || contract.runtime_status.muted.is_empty()
    {
        return Err("transcript_classifiers_contract_invalid:runtime_status".to_string());
    }
    if contract.diagnostic.prefix.is_empty() {
        return Err("transcript_classifiers_contract_invalid:diagnostic_prefix".to_string());
    }
    Ok(contract)
}

pub fn active_turn_state() -> &'static str {
    transcript_classifiers_contract().turn_state.active.as_str()
}

pub fn active_turn_display() -> &'static str {
    transcript_classifiers_contract()
        .turn_state
        .active_display
        .as_str()
}

pub fn thinking_prefix() -> &'static str {
    transcript_classifiers_contract()
        .turn_state
        .thinking_prefix
        .as_str()
}

pub fn calling_prefix() -> &'static str {
    transcript_classifiers_contract()
        .turn_state
        .calling_prefix
        .as_str()
}

pub fn thinking_phase() -> &'static str {
    transcript_classifiers_contract()
        .turn_state
        .thinking_prefix
        .trim()
}

pub fn calling_phase() -> &'static str {
    transcript_classifiers_contract()
        .turn_state
        .calling_prefix
        .trim()
}

pub fn turn_marker_is_suppressed(value: &str) -> bool {
    value.trim().is_empty()
        || transcript_classifiers_contract()
            .turn_state
            .suppressed_markers
            .iter()
            .any(|candidate| candidate == value)
}

pub fn turn_state_is_positive(value: &str) -> bool {
    let turn_state = &transcript_classifiers_contract().turn_state;
    turn_state
        .positive_values
        .iter()
        .any(|candidate| candidate == value)
        || turn_state
            .positive_prefixes
            .iter()
            .any(|candidate| value.starts_with(candidate))
}

pub fn turn_state_is_negative(value: &str) -> bool {
    transcript_classifiers_contract()
        .turn_state
        .negative_values
        .iter()
        .any(|candidate| candidate == value)
}

pub fn turn_state_duration_phase_is_known(value: &str) -> bool {
    transcript_classifiers_contract()
        .turn_state
        .duration_phases
        .iter()
        .any(|candidate| candidate == value)
}

pub fn operator_activity_action_is_known(value: &str) -> bool {
    transcript_classifiers_contract()
        .turn_state
        .operator_activity_actions
        .iter()
        .any(|candidate| candidate == value)
}

pub fn operator_activity_actor() -> &'static str {
    transcript_classifiers_contract()
        .turn_state
        .operator_activity_actor
        .as_str()
}

pub fn terminal_status_is_positive(value: &str) -> bool {
    transcript_classifiers_contract()
        .terminal_status
        .positive
        .iter()
        .any(|candidate| candidate == value)
}

pub fn tool_result_prefix_class(text: &str) -> Option<(&'static str, RenderClassification)> {
    let tool_result = &transcript_classifiers_contract().tool_result;
    for candidate in &tool_result.positive_prefixes {
        if text == candidate || text.starts_with(&format!("{candidate} ")) {
            return Some((candidate.as_str(), RenderClassification::Positive));
        }
    }
    for candidate in &tool_result.negative_prefixes {
        if text == candidate || text.starts_with(&format!("{candidate} ")) {
            return Some((candidate.as_str(), RenderClassification::Negative));
        }
    }
    None
}

pub fn tool_result_summary_class(value: &str) -> Option<RenderClassification> {
    let tool_result = &transcript_classifiers_contract().tool_result;
    if tool_result
        .positive_summaries
        .iter()
        .any(|candidate| candidate == value)
    {
        Some(RenderClassification::Positive)
    } else if tool_result
        .negative_summaries
        .iter()
        .any(|candidate| candidate == value)
    {
        Some(RenderClassification::Negative)
    } else {
        None
    }
}

pub fn semantic_status_value_class(value: &str) -> Option<RenderClassification> {
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    let semantic = &transcript_classifiers_contract().semantic_status_value;
    if semantic
        .positive
        .iter()
        .any(|candidate| candidate == &normalized)
    {
        Some(RenderClassification::Positive)
    } else if semantic
        .negative
        .iter()
        .any(|candidate| candidate == &normalized)
    {
        Some(RenderClassification::Negative)
    } else if semantic
        .warning
        .iter()
        .any(|candidate| candidate == &normalized)
    {
        Some(RenderClassification::Warning)
    } else if semantic
        .muted
        .iter()
        .any(|candidate| candidate == &normalized)
    {
        Some(RenderClassification::Muted)
    } else {
        None
    }
}

pub fn runtime_status_class(value: &str) -> Option<RenderClassification> {
    let runtime = &transcript_classifiers_contract().runtime_status;
    if runtime.positive.iter().any(|candidate| candidate == value) {
        Some(RenderClassification::Positive)
    } else if runtime
        .warning_prefixes
        .iter()
        .any(|candidate| value.starts_with(candidate))
    {
        Some(RenderClassification::Warning)
    } else if runtime
        .negative_prefixes
        .iter()
        .any(|candidate| value.starts_with(candidate))
    {
        Some(RenderClassification::Negative)
    } else if runtime.muted.iter().any(|candidate| candidate == value) {
        Some(RenderClassification::Muted)
    } else {
        None
    }
}

pub fn diagnostic_prefix() -> &'static str {
    transcript_classifiers_contract().diagnostic.prefix.as_str()
}

pub fn diagnostic_severity_class(value: &str) -> DiagnosticSeverityClass {
    let diagnostic = &transcript_classifiers_contract().diagnostic;
    if diagnostic
        .warning_severities
        .iter()
        .any(|candidate| candidate == value)
    {
        DiagnosticSeverityClass::Warning
    } else if diagnostic
        .negative_severities
        .iter()
        .any(|candidate| candidate == value)
    {
        DiagnosticSeverityClass::Negative
    } else if diagnostic
        .positive_severities
        .iter()
        .any(|candidate| candidate == value)
    {
        DiagnosticSeverityClass::Positive
    } else {
        DiagnosticSeverityClass::Muted
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderClassification {
    Positive,
    Negative,
    Warning,
    Muted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverityClass {
    Warning,
    Negative,
    Positive,
    Muted,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_transcript_classifiers_contract_is_valid() {
        let contract = transcript_classifiers_contract();

        assert_eq!(contract.schema, EXPECTED_SCHEMA);
        assert_eq!(active_turn_display(), "thinking");
        assert!(turn_marker_is_suppressed("idle"));
        assert!(turn_state_is_positive("calling mcp_output_show"));
        assert!(turn_state_is_negative("failed"));
        assert!(terminal_status_is_positive("completed_without_provider"));
        assert_eq!(
            tool_result_summary_class("interrupted"),
            Some(RenderClassification::Negative)
        );
        assert_eq!(
            semantic_status_value_class("ready"),
            Some(RenderClassification::Positive)
        );
        assert_eq!(
            runtime_status_class("configured_without_provider"),
            Some(RenderClassification::Warning)
        );
        assert_eq!(
            diagnostic_severity_class("failed"),
            DiagnosticSeverityClass::Negative
        );
    }
}
