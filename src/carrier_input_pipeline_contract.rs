use crate::carrier_protocol::{DeliveryMode, HoldCondition, InputEvent, SourceKind};
use crate::carrier_protocol_contract::{observer_visibility_default, observer_visibility_is_valid};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CarrierInputPipelineState {
    pub active_turn: bool,
    pub composer_has_draft: bool,
    pub observer_muted: bool,
}

fn directive_visibility(input: &InputEvent) -> Option<&'static str> {
    let is_directive = input.directive_id.is_some()
        || input
            .metadata
            .get("directive")
            .is_some_and(Value::is_object);
    if !is_directive {
        return None;
    }
    Some(match input
        .metadata
        .get("directive")
        .and_then(|value| value.get("visibility"))
        .and_then(Value::as_str)
    {
        Some("record_only") => "record_only",
        Some("operator_visible") => "operator_visible",
        Some("conversation_visible") => "conversation_visible",
        Some("agent_visible") => "agent_visible",
        _ => "agent_visible",
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarrierInputPipelineAdmission {
    pub admission_action: &'static str,
    pub queue_state: Option<&'static str>,
    pub creates_turn: bool,
    pub complete_without_provider: bool,
    pub dispatch_to_provider: bool,
    pub directive_visibility: Option<&'static str>,
    pub visible_to_operator: bool,
    pub suppression_reason: Option<&'static str>,
    pub queue_event_kinds: Vec<&'static str>,
    pub admission_event_kinds: Vec<&'static str>,
    pub visible_event_kinds: Vec<&'static str>,
    pub hold_action: &'static str,
    pub hold_event_kinds: Vec<&'static str>,
    pub should_defer: bool,
}

pub fn classify_carrier_input_pipeline(
    input: &InputEvent,
    state: CarrierInputPipelineState,
) -> CarrierInputPipelineAdmission {
    let is_observer = is_observer_input_event(input);
    let visibility = observer_visibility(input);
    let directive_visibility = directive_visibility(input);
    let is_record_only_directive = directive_visibility == Some("record_only");
    let observer_interjection = is_observer && visibility != "record_only";
    let observer_suppressed = observer_interjection && state.observer_muted;
    let observer_dispatch = is_observer
        && matches!(visibility, "agent_visible" | "conversation_visible")
        && !observer_suppressed;
    let visible_to_operator = is_observer
        && matches!(visibility, "operator_visible" | "conversation_visible")
        && !observer_suppressed;
    let should_hold = !is_record_only_directive
        && is_system_directive(input)
        && state.composer_has_draft
        && input.hold_condition == Some(HoldCondition::ComposerClearRequired);
    let admission_action = if should_hold {
        "hold"
    } else if is_record_only_directive {
        "admit"
    } else if input.delivery_mode == DeliveryMode::AdmitForCurrentTurn && state.active_turn {
        "reject"
    } else if input.delivery_mode == DeliveryMode::AdmitAfterActiveTurn && state.active_turn {
        "queue"
    } else {
        "admit"
    };
    let mut queue_event_kinds = Vec::new();
    if input.delivery_mode == DeliveryMode::AdmitAfterActiveTurn && !is_record_only_directive {
        queue_event_kinds.push("input_queued_for_turn_boundary");
    }
    let mut admission_event_kinds = Vec::new();
    let mut visible_event_kinds = Vec::new();
    if is_record_only_directive && admission_action == "admit" {
        admission_event_kinds.push("directive_receipt_recorded");
        admission_event_kinds.push("directive_carrier_accepted_recorded");
    } else if is_observer {
        admission_event_kinds.push("observer_observation_recorded");
        if visibility != "record_only" {
            admission_event_kinds.push("observer_interjection_proposed");
        }
        if admission_action == "admit" {
            if observer_suppressed {
                admission_event_kinds.push("observer_interjection_suppressed");
            } else if visibility != "record_only" {
                admission_event_kinds.push("observer_interjection_admitted");
            }
            if visible_to_operator {
                visible_event_kinds.push("observer_interjection_visible");
            }
        }
    }
    let creates_turn = !is_record_only_directive
        && admission_action == "admit"
        && (!is_observer || observer_dispatch);
    if creates_turn {
        admission_event_kinds.push("input_admitted_to_turn");
    }
    let complete_without_provider = is_record_only_directive
        || (admission_action == "admit"
            && is_observer
            && (!observer_dispatch || observer_suppressed));
    CarrierInputPipelineAdmission {
        admission_action,
        queue_state: if input.delivery_mode == DeliveryMode::AdmitAfterActiveTurn {
            Some("queued_for_turn_boundary")
        } else {
            None
        },
        creates_turn,
        complete_without_provider,
        dispatch_to_provider: !is_record_only_directive && observer_dispatch,
        directive_visibility,
        visible_to_operator: !is_record_only_directive && visible_to_operator,
        suppression_reason: if observer_suppressed {
            Some("observer_muted")
        } else {
            None
        },
        queue_event_kinds,
        admission_event_kinds,
        visible_event_kinds,
        hold_action: if should_hold { "hold" } else { "none" },
        hold_event_kinds: if should_hold {
            vec!["system_directive_held"]
        } else {
            Vec::new()
        },
        should_defer: should_hold,
    }
}

fn is_system_directive(input: &InputEvent) -> bool {
    input.source_kind == SourceKind::System
        || input.metadata.get("legacy_source").and_then(Value::as_str) == Some("system_directive")
        || input
            .metadata
            .get("directive_provenance")
            .and_then(|value| value.get("kind"))
            .and_then(Value::as_str)
            == Some("system_directive")
}

fn is_observer_input_event(input: &InputEvent) -> bool {
    input
        .metadata
        .get("observer")
        .and_then(|value| value.get("role"))
        .and_then(Value::as_str)
        == Some("observer")
}

fn observer_visibility(input: &InputEvent) -> &str {
    let visibility = input
        .metadata
        .get("observer")
        .and_then(|value| value.get("visibility"))
        .and_then(Value::as_str);
    match visibility {
        Some(value) if observer_visibility_is_valid(value) => value,
        _ => observer_visibility_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol_contract::{
        carrier_protocol_contract, input_admission_action_is_valid, input_hold_action_is_valid,
        observer_suppression_reason_is_valid, queue_state_is_valid,
    };
    use serde::Deserialize;

    const INPUT_PIPELINE_CASES: &str = include_str!(
        "../../narada/packages/carrier-protocol/fixtures/carrier-input-pipeline-cases.json"
    );

    #[derive(Debug, Deserialize)]
    struct InputPipelineCases {
        schema: String,
        cases: Vec<InputPipelineCase>,
    }

    #[derive(Debug, Deserialize)]
    struct InputPipelineCase {
        name: String,
        input: InputEvent,
        state: InputPipelineCaseState,
        expected: InputPipelineCaseExpected,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct InputPipelineCaseState {
        active_turn: bool,
        composer_has_draft: bool,
        observer_muted: bool,
    }

    #[derive(Debug, Deserialize)]
    struct InputPipelineCaseExpected {
        admission_action: String,
        queue_state: Option<String>,
        creates_turn: bool,
        complete_without_provider: bool,
        dispatch_to_provider: bool,
        directive_visibility: Option<String>,
        visible_to_operator: Option<bool>,
        suppression_reason: Option<String>,
        queue_event_kinds: Vec<String>,
        admission_event_kinds: Vec<String>,
        visible_event_kinds: Option<Vec<String>>,
        hold_action: String,
        hold_event_kinds: Option<Vec<String>>,
        should_defer: bool,
    }

    #[test]
    fn shared_carrier_input_pipeline_cases_match_agent_tui_classifier() {
        let cases: InputPipelineCases =
            serde_json::from_str(INPUT_PIPELINE_CASES).expect("pipeline cases parse");
        assert_eq!(cases.schema, "narada.carrier.input_pipeline_cases.v1");
        let vocabulary = carrier_protocol_contract();

        for case in cases.cases {
            let state = CarrierInputPipelineState {
                active_turn: case.state.active_turn,
                composer_has_draft: case.state.composer_has_draft,
                observer_muted: case.state.observer_muted,
            };
            let admission = classify_carrier_input_pipeline(&case.input, state);
            assert!(input_admission_action_is_valid(admission.admission_action));
            if let Some(queue_state) = admission.queue_state {
                assert!(queue_state_is_valid(queue_state));
            }
            if let Some(reason) = admission.suppression_reason {
                assert!(observer_suppression_reason_is_valid(reason));
            }
            assert!(input_hold_action_is_valid(admission.hold_action));
            assert!(admission.queue_event_kinds.iter().all(|kind| {
                vocabulary
                    .input_pipeline_event_kind
                    .queue
                    .iter()
                    .any(|candidate| candidate == kind)
            }));
            assert!(admission.admission_event_kinds.iter().all(|kind| {
                vocabulary
                    .input_pipeline_event_kind
                    .admission
                    .iter()
                    .any(|candidate| candidate == kind)
            }));
            assert!(admission.visible_event_kinds.iter().all(|kind| {
                vocabulary
                    .input_pipeline_event_kind
                    .visible
                    .iter()
                    .any(|candidate| candidate == kind)
            }));
            assert!(admission.hold_event_kinds.iter().all(|kind| {
                vocabulary
                    .input_pipeline_event_kind
                    .hold
                    .iter()
                    .any(|candidate| candidate == kind)
            }));
            assert_eq!(
                admission.admission_action, case.expected.admission_action,
                "{}",
                case.name
            );
            assert_eq!(
                admission.queue_state,
                case.expected.queue_state.as_deref(),
                "{}",
                case.name
            );
            assert_eq!(
                admission.creates_turn, case.expected.creates_turn,
                "{}",
                case.name
            );
            assert_eq!(
                admission.complete_without_provider, case.expected.complete_without_provider,
                "{}",
                case.name
            );
            assert_eq!(
                admission.dispatch_to_provider, case.expected.dispatch_to_provider,
                "{}",
                case.name
            );
            if let Some(expected) = case.expected.directive_visibility.as_deref() {
                assert_eq!(admission.directive_visibility, Some(expected), "{}", case.name);
            }
            if let Some(expected) = case.expected.visible_to_operator {
                assert_eq!(admission.visible_to_operator, expected, "{}", case.name);
            }
            assert_eq!(
                admission.suppression_reason,
                case.expected.suppression_reason.as_deref(),
                "{}",
                case.name
            );
            assert_eq!(
                admission.queue_event_kinds,
                case.expected
                    .queue_event_kinds
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                "{}",
                case.name
            );
            assert_eq!(
                admission.admission_event_kinds,
                case.expected
                    .admission_event_kinds
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                "{}",
                case.name
            );
            assert_eq!(
                admission.visible_event_kinds,
                case.expected
                    .visible_event_kinds
                    .unwrap_or_default()
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                "{}",
                case.name
            );
            assert_eq!(
                admission.hold_action, case.expected.hold_action,
                "{}",
                case.name
            );
            assert_eq!(
                admission.hold_event_kinds,
                case.expected
                    .hold_event_kinds
                    .unwrap_or_default()
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                "{}",
                case.name
            );
            assert_eq!(
                admission.should_defer, case.expected.should_defer,
                "{}",
                case.name
            );
        }
    }
}
