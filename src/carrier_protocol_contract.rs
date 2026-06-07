use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const CARRIER_PROTOCOL_CONTRACT_JSON: &str =
    include_str!("../../narada/packages/carrier-protocol-contract/contracts/carrier-protocol.json");
const EXPECTED_SCHEMA: &str = "narada.carrier.protocol_contract.v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolContract {
    pub schema: String,
    pub schemas: CarrierProtocolSchemas,
    pub id_prefixes: CarrierProtocolIdPrefixes,
    pub diagnostic: CarrierProtocolDiagnostic,
    pub turn_terminal_status: CarrierProtocolTurnTerminalStatus,
    pub terminal_state: CarrierProtocolTerminalState,
    pub delivery_mode: CarrierProtocolDeliveryMode,
    pub observer_visibility: CarrierProtocolObserverVisibility,
    pub queue_state: CarrierProtocolQueueState,
    pub input_admission_action: CarrierProtocolInputAdmissionAction,
    pub input_hold_action: CarrierProtocolInputHoldAction,
    pub observer_suppression_reason: CarrierProtocolObserverSuppressionReason,
    pub tool_result_status: CarrierProtocolToolResultStatus,
    pub tool_effect_admission_action: CarrierProtocolToolEffectAdmissionAction,
    pub tool_effect_admission_reason: CarrierProtocolToolEffectAdmissionReason,
    pub input_pipeline_event_kind: CarrierProtocolInputPipelineEventKind,
}

pub fn tool_result_status_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .tool_result_status
        .values
        .iter()
        .any(|candidate| candidate == value)
}

pub fn tool_effect_admission_action_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .tool_effect_admission_action
        .values
        .iter()
        .any(|candidate| candidate == value)
}

pub fn tool_effect_admission_reason_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .tool_effect_admission_reason
        .values
        .iter()
        .any(|candidate| candidate == value)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolSchemas {
    pub input_event: String,
    pub control_input_event: String,
    pub session_event: String,
    pub payload_ref: String,
    pub payload_policy: String,
    pub provider_request_payload: String,
    pub provider_output_payload: String,
    pub turn_terminal_payload: String,
    pub session_event_fixture_manifest: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolIdPrefixes {
    pub input_event: String,
    pub control_event: String,
    pub session_event: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolDiagnostic {
    pub levels: Vec<String>,
    pub warning_level: String,
    pub info_level: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolTurnTerminalStatus {
    pub completed: Vec<String>,
    pub interrupted: Vec<String>,
    pub failed: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolTerminalState {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolDeliveryMode {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolObserverVisibility {
    pub values: Vec<String>,
    pub default: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolQueueState {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolInputAdmissionAction {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolInputHoldAction {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolObserverSuppressionReason {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolToolResultStatus {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolToolEffectAdmissionAction {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolToolEffectAdmissionReason {
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierProtocolInputPipelineEventKind {
    pub queue: Vec<String>,
    pub admission: Vec<String>,
    pub visible: Vec<String>,
    pub hold: Vec<String>,
    pub release: Vec<String>,
}

static CARRIER_PROTOCOL_CONTRACT: OnceLock<CarrierProtocolContract> = OnceLock::new();

pub fn carrier_protocol_contract() -> &'static CarrierProtocolContract {
    CARRIER_PROTOCOL_CONTRACT.get_or_init(|| {
        parse_carrier_protocol_contract(CARRIER_PROTOCOL_CONTRACT_JSON)
            .expect("bundled carrier protocol contract must be valid")
    })
}

pub fn parse_carrier_protocol_contract(json_text: &str) -> Result<CarrierProtocolContract, String> {
    let contract: CarrierProtocolContract = serde_json::from_str(json_text)
        .map_err(|error| format!("carrier_protocol_contract_parse_failed:{error}"))?;
    if contract.schema != EXPECTED_SCHEMA {
        return Err("carrier_protocol_contract_invalid:schema".to_string());
    }
    if contract.id_prefixes.input_event.is_empty()
        || contract.id_prefixes.control_event.is_empty()
        || contract.id_prefixes.session_event.is_empty()
    {
        return Err("carrier_protocol_contract_invalid:id_prefixes".to_string());
    }
    if contract.diagnostic.levels.is_empty()
        || contract.diagnostic.warning_level.is_empty()
        || contract.diagnostic.info_level.is_empty()
    {
        return Err("carrier_protocol_contract_invalid:diagnostic_levels".to_string());
    }
    if contract.turn_terminal_status.completed.is_empty()
        || contract.turn_terminal_status.interrupted.is_empty()
        || contract.turn_terminal_status.failed.is_empty()
    {
        return Err("carrier_protocol_contract_invalid:turn_terminal_status".to_string());
    }
    if contract.terminal_state.values.is_empty() {
        return Err("carrier_protocol_contract_invalid:terminal_state".to_string());
    }
    if contract.delivery_mode.values.is_empty() {
        return Err("carrier_protocol_contract_invalid:delivery_mode".to_string());
    }
    if contract.observer_visibility.values.is_empty()
        || contract.observer_visibility.default.is_empty()
    {
        return Err("carrier_protocol_contract_invalid:observer_visibility".to_string());
    }
    if contract.queue_state.values.is_empty() {
        return Err("carrier_protocol_contract_invalid:queue_state".to_string());
    }
    if contract.input_admission_action.values.is_empty() {
        return Err("carrier_protocol_contract_invalid:input_admission_action".to_string());
    }
    if contract.input_hold_action.values.is_empty() {
        return Err("carrier_protocol_contract_invalid:input_hold_action".to_string());
    }
    if contract.observer_suppression_reason.values.is_empty() {
        return Err("carrier_protocol_contract_invalid:observer_suppression_reason".to_string());
    }
    if contract.tool_result_status.values.is_empty() {
        return Err("carrier_protocol_contract_invalid:tool_result_status".to_string());
    }
    if contract.tool_effect_admission_action.values.is_empty() {
        return Err("carrier_protocol_contract_invalid:tool_effect_admission_action".to_string());
    }
    if contract.tool_effect_admission_reason.values.is_empty() {
        return Err("carrier_protocol_contract_invalid:tool_effect_admission_reason".to_string());
    }
    if contract.input_pipeline_event_kind.queue.is_empty()
        || contract.input_pipeline_event_kind.admission.is_empty()
        || contract.input_pipeline_event_kind.hold.is_empty()
    {
        return Err("carrier_protocol_contract_invalid:input_pipeline_event_kind".to_string());
    }
    Ok(contract)
}

pub fn input_event_schema() -> &'static str {
    carrier_protocol_contract().schemas.input_event.as_str()
}

pub fn control_input_event_schema() -> &'static str {
    carrier_protocol_contract()
        .schemas
        .control_input_event
        .as_str()
}

pub fn session_event_schema() -> &'static str {
    carrier_protocol_contract().schemas.session_event.as_str()
}

pub fn payload_ref_schema() -> &'static str {
    carrier_protocol_contract().schemas.payload_ref.as_str()
}

pub fn payload_policy_schema() -> &'static str {
    carrier_protocol_contract().schemas.payload_policy.as_str()
}

pub fn provider_request_payload_schema() -> &'static str {
    carrier_protocol_contract()
        .schemas
        .provider_request_payload
        .as_str()
}

pub fn provider_output_payload_schema() -> &'static str {
    carrier_protocol_contract()
        .schemas
        .provider_output_payload
        .as_str()
}

pub fn turn_terminal_payload_schema() -> &'static str {
    carrier_protocol_contract()
        .schemas
        .turn_terminal_payload
        .as_str()
}

pub fn session_event_fixture_manifest_schema() -> &'static str {
    carrier_protocol_contract()
        .schemas
        .session_event_fixture_manifest
        .as_str()
}

pub fn input_event_id_prefix() -> &'static str {
    carrier_protocol_contract().id_prefixes.input_event.as_str()
}

pub fn control_event_id_prefix() -> &'static str {
    carrier_protocol_contract()
        .id_prefixes
        .control_event
        .as_str()
}

pub fn session_event_id_prefix() -> &'static str {
    carrier_protocol_contract()
        .id_prefixes
        .session_event
        .as_str()
}

pub fn diagnostic_level_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .diagnostic
        .levels
        .iter()
        .any(|candidate| candidate == value)
}

pub fn diagnostic_warning_level() -> &'static str {
    carrier_protocol_contract()
        .diagnostic
        .warning_level
        .as_str()
}

pub fn diagnostic_info_level() -> &'static str {
    carrier_protocol_contract().diagnostic.info_level.as_str()
}

pub fn completed_turn_terminal_status_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .turn_terminal_status
        .completed
        .iter()
        .any(|candidate| candidate == value)
}

pub fn interrupted_turn_terminal_status_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .turn_terminal_status
        .interrupted
        .iter()
        .any(|candidate| candidate == value)
}

pub fn failed_turn_terminal_status_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .turn_terminal_status
        .failed
        .iter()
        .any(|candidate| candidate == value)
}

pub fn terminal_state_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .terminal_state
        .values
        .iter()
        .any(|candidate| candidate == value)
}

pub fn delivery_mode_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .delivery_mode
        .values
        .iter()
        .any(|candidate| candidate == value)
}

pub fn observer_visibility_default() -> &'static str {
    carrier_protocol_contract()
        .observer_visibility
        .default
        .as_str()
}

pub fn observer_visibility_values() -> &'static [String] {
    carrier_protocol_contract()
        .observer_visibility
        .values
        .as_slice()
}

pub fn observer_visibility_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .observer_visibility
        .values
        .iter()
        .any(|candidate| candidate == value)
}

pub fn queue_state_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .queue_state
        .values
        .iter()
        .any(|candidate| candidate == value)
}

pub fn input_admission_action_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .input_admission_action
        .values
        .iter()
        .any(|candidate| candidate == value)
}

pub fn input_hold_action_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .input_hold_action
        .values
        .iter()
        .any(|candidate| candidate == value)
}

pub fn observer_suppression_reason_is_valid(value: &str) -> bool {
    carrier_protocol_contract()
        .observer_suppression_reason
        .values
        .iter()
        .any(|candidate| candidate == value)
}

pub fn observer_muted_suppression_reason() -> &'static str {
    carrier_protocol_contract()
        .observer_suppression_reason
        .values
        .iter()
        .find(|value| value.as_str() == "observer_muted")
        .expect("carrier protocol contract must define observer_muted")
        .as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_carrier_protocol_contract_is_valid() {
        let contract = carrier_protocol_contract();

        assert_eq!(contract.schema, EXPECTED_SCHEMA);
        assert_eq!(input_event_schema(), "narada.carrier.input_event.v1");
        assert_eq!(input_event_id_prefix(), "input_");
        assert_eq!(control_event_id_prefix(), "control_");
        assert_eq!(session_event_id_prefix(), "session_event_");
        assert!(diagnostic_level_is_valid("warn"));
        assert_eq!(diagnostic_warning_level(), "warn");
        assert_eq!(diagnostic_info_level(), "info");
        assert!(completed_turn_terminal_status_is_valid(
            "completed_without_provider"
        ));
        assert!(interrupted_turn_terminal_status_is_valid("interrupted"));
        assert!(failed_turn_terminal_status_is_valid("failed"));
        assert!(terminal_state_is_valid("completed"));
        assert!(delivery_mode_is_valid("admit_after_active_turn"));
        assert_eq!(observer_visibility_default(), "operator_visible");
        assert!(observer_visibility_is_valid("conversation_visible"));
        assert!(queue_state_is_valid("queued_for_turn_boundary"));
        assert!(input_admission_action_is_valid("admit"));
        assert!(input_hold_action_is_valid("release"));
        assert!(observer_suppression_reason_is_valid("observer_muted"));
        assert!(tool_result_status_is_valid("denied"));
        assert!(tool_effect_admission_action_is_valid("deny"));
        assert!(tool_effect_admission_reason_is_valid(
            "tool_effect_admission_required"
        ));
        assert_eq!(
            contract.input_pipeline_event_kind.queue,
            vec!["input_queued_for_turn_boundary".to_string()]
        );
        assert!(
            contract
                .input_pipeline_event_kind
                .admission
                .contains(&"observer_interjection_suppressed".to_string())
        );
    }
}
