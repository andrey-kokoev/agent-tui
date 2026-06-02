use crate::carrier_protocol::{SessionEvent, SessionEventKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptActor {
    Operator,
    OperatorSteering,
    OperatorDirective,
    System,
    Agent,
    AgentTui,
    Provider,
}

impl TranscriptActor {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Operator => "operator",
            Self::OperatorSteering => "operator steering",
            Self::OperatorDirective => "operator directive",
            Self::System => "system",
            Self::Agent => "agent",
            Self::AgentTui => "agent-tui",
            Self::Provider => "provider",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptItemKind {
    InputAdmitted,
    SystemDirectiveHeld,
    SystemDirectiveReleased,
    ProviderTextDelta,
    ProviderToolCallRequest,
    ToolResultReceived,
    TurnTerminalStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptItem {
    pub kind: TranscriptItemKind,
    pub actor: TranscriptActor,
    pub turn_id: String,
    pub text: String,
    pub sequence: Option<u64>,
    pub projection_key: Option<String>,
    pub occurred_at: Option<String>,
}

pub fn project_session_event(event: &SessionEvent) -> Option<TranscriptItem> {
    match event.event_kind {
        SessionEventKind::InputAdmittedToTurn => project_input_admitted(event),
        SessionEventKind::TurnStarted => project_turn_started(event),
        SessionEventKind::SystemDirectiveHeld => project_system_directive_held(event),
        SessionEventKind::SystemDirectiveReleased => project_system_directive_released(event),
        SessionEventKind::ProviderTextDeltaRecorded => project_provider_text_delta(event),
        SessionEventKind::ProviderToolCallRequested => project_provider_tool_call_request(event),
        SessionEventKind::ToolResultReceived => project_tool_result_received(event),
        SessionEventKind::TurnCompleted
        | SessionEventKind::TurnFailed
        | SessionEventKind::TurnInterrupted => project_turn_terminal(event),
        SessionEventKind::CarrierDiagnosticRecorded => project_carrier_diagnostic(event),
        _ => None,
    }
}
fn project_input_admitted(event: &SessionEvent) -> Option<TranscriptItem> {
    let input_event_id = event.payload.get("input_event_id")?.as_str()?;
    let source_kind = event
        .payload
        .get("source_kind")
        .and_then(|value| value.as_str())
        .unwrap_or("operator");
    let actor = actor_from_input_payload(source_kind, event);
    let text = event
        .payload
        .get("content_preview")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .unwrap_or_else(|| format!("input admitted {input_event_id}"));
    Some(TranscriptItem {
        kind: TranscriptItemKind::InputAdmitted,
        actor,
        turn_id: String::new(),
        text,
        sequence: None,
        projection_key: Some(format!("input:{input_event_id}")),
        occurred_at: Some(event.occurred_at.clone()),
    })
}

fn project_turn_started(event: &SessionEvent) -> Option<TranscriptItem> {
    let turn_id = event.payload.get("turn_id")?.as_str()?.to_string();
    let input_event_id = event.payload.get("input_event_id")?.as_str()?;
    let source_kind = event.payload.get("source_kind")?.as_str()?;
    let actor = actor_from_input_payload(source_kind, event);
    let text = event
        .payload
        .get("content_preview")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
        .or_else(|| {
            event
                .payload
                .get("input_event_id")
                .and_then(|value| value.as_str())
                .map(|input_event_id| format!("input admitted {input_event_id}"))
        })?;
    Some(TranscriptItem {
        kind: TranscriptItemKind::InputAdmitted,
        actor,
        turn_id,
        text,
        sequence: None,
        projection_key: Some(format!("input:{input_event_id}")),
        occurred_at: Some(event.occurred_at.clone()),
    })
}

fn actor_from_input_payload(source_kind: &str, event: &SessionEvent) -> TranscriptActor {
    if source_kind == "operator" && is_operator_directive_payload(event) {
        return TranscriptActor::OperatorDirective;
    }
    if source_kind == "operator" && is_operator_steering_payload(event) {
        return TranscriptActor::OperatorSteering;
    }
    actor_from_source_kind(source_kind)
}

fn actor_from_source_kind(source_kind: &str) -> TranscriptActor {
    match source_kind {
        "operator" => TranscriptActor::Operator,
        "system" => TranscriptActor::System,
        "agent" => TranscriptActor::Agent,
        _ => TranscriptActor::AgentTui,
    }
}

fn is_operator_directive_payload(event: &SessionEvent) -> bool {
    event
        .payload
        .get("directive_id")
        .and_then(|value| value.as_str())
        .is_some()
        || event
            .payload
            .get("authority_ref")
            .and_then(|value| value.as_str())
            .is_some()
        || event
            .payload
            .get("metadata")
            .and_then(|value| value.get("directive_provenance"))
            .is_some()
}

fn is_operator_steering_payload(event: &SessionEvent) -> bool {
    event
        .payload
        .get("delivery_mode")
        .and_then(|value| value.as_str())
        == Some("admit_after_active_turn")
}

fn project_system_directive_held(event: &SessionEvent) -> Option<TranscriptItem> {
    let input_event_id = event.payload.get("input_event_id")?.as_str()?;
    Some(TranscriptItem {
        kind: TranscriptItemKind::SystemDirectiveHeld,
        actor: TranscriptActor::System,
        turn_id: String::new(),
        text: format!("held {input_event_id}"),
        sequence: None,
        projection_key: None,
        occurred_at: Some(event.occurred_at.clone()),
    })
}

fn project_system_directive_released(event: &SessionEvent) -> Option<TranscriptItem> {
    let input_event_id = event.payload.get("input_event_id")?.as_str()?;
    Some(TranscriptItem {
        kind: TranscriptItemKind::SystemDirectiveReleased,
        actor: TranscriptActor::System,
        turn_id: String::new(),
        text: format!("released {input_event_id}"),
        sequence: None,
        projection_key: None,
        occurred_at: Some(event.occurred_at.clone()),
    })
}

fn project_provider_text_delta(event: &SessionEvent) -> Option<TranscriptItem> {
    let turn_id = event.payload.get("turn_id")?.as_str()?.to_string();
    let text = payload_ref_summary(event, "text_delta_ref").or_else(|| {
        event
            .payload
            .get("text_delta")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.to_string())
    })?;
    Some(TranscriptItem {
        kind: TranscriptItemKind::ProviderTextDelta,
        actor: TranscriptActor::Agent,
        turn_id,
        text,
        sequence: event
            .payload
            .get("sequence")
            .and_then(|value| value.as_u64()),
        projection_key: None,
        occurred_at: Some(event.occurred_at.clone()),
    })
}

fn project_provider_tool_call_request(event: &SessionEvent) -> Option<TranscriptItem> {
    let turn_id = event.payload.get("turn_id")?.as_str()?.to_string();
    let tool_name = event.payload.get("tool_name")?.as_str()?;
    let arguments_summary = payload_ref_summary(event, "arguments_ref").unwrap_or_else(|| {
        event
            .payload
            .get("arguments_summary")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    });
    Some(TranscriptItem {
        kind: TranscriptItemKind::ProviderToolCallRequest,
        actor: TranscriptActor::AgentTui,
        turn_id,
        text: format!("{tool_name}({arguments_summary})"),
        sequence: event
            .payload
            .get("sequence")
            .and_then(|value| value.as_u64()),
        projection_key: None,
        occurred_at: Some(event.occurred_at.clone()),
    })
}

fn project_tool_result_received(event: &SessionEvent) -> Option<TranscriptItem> {
    let tool_name = event.payload.get("tool_name")?.as_str()?;
    let status = event.payload.get("status")?.as_str()?;
    let duration_ms = event
        .payload
        .get("duration_ms")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let result_summary = payload_ref_summary(event, "result_ref").unwrap_or_else(|| {
        event
            .payload
            .get("result_summary")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    });
    Some(TranscriptItem {
        kind: TranscriptItemKind::ToolResultReceived,
        actor: TranscriptActor::AgentTui,
        turn_id: event
            .payload
            .get("turn_id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        text: tool_result_text(status, tool_name, duration_ms, &result_summary),
        sequence: None,
        projection_key: None,
        occurred_at: Some(event.occurred_at.clone()),
    })
}

fn tool_result_text(
    status: &str,
    tool_name: &str,
    duration_ms: u64,
    result_summary: &str,
) -> String {
    let base = format!("{status} {tool_name} in {duration_ms}ms");
    let summary = result_summary.trim();
    if summary.is_empty() {
        base
    } else {
        format!("{base} · {summary}")
    }
}

fn payload_ref_summary(event: &SessionEvent, field: &str) -> Option<String> {
    event
        .payload
        .get(field)
        .and_then(|value| value.as_object())
        .and_then(|object| object.get("summary"))
        .and_then(|value| value.as_str())
        .map(|summary| format!("{summary} [{field}]"))
}

fn project_turn_terminal(event: &SessionEvent) -> Option<TranscriptItem> {
    let turn_id = event.payload.get("turn_id")?.as_str()?.to_string();
    let terminal_status = event
        .payload
        .get("terminal_status")
        .and_then(|value| value.as_str())
        .unwrap_or(match event.event_kind {
            SessionEventKind::TurnCompleted => "completed",
            SessionEventKind::TurnFailed => "failed",
            SessionEventKind::TurnInterrupted => "interrupted",
            _ => "terminal",
        });
    let text = if event.event_kind == SessionEventKind::TurnInterrupted {
        event
            .payload
            .get("error_summary")
            .and_then(|value| value.as_str())
            .filter(|summary| !summary.trim().is_empty())
            .map(|summary| format!("{terminal_status} · {summary}"))
            .unwrap_or_else(|| terminal_status.to_string())
    } else {
        terminal_status.to_string()
    };
    Some(TranscriptItem {
        kind: TranscriptItemKind::TurnTerminalStatus,
        actor: TranscriptActor::AgentTui,
        turn_id,
        text,
        sequence: None,
        projection_key: None,
        occurred_at: Some(event.occurred_at.clone()),
    })
}

fn project_carrier_diagnostic(event: &SessionEvent) -> Option<TranscriptItem> {
    let source = event
        .payload
        .get("source")
        .and_then(|value| value.as_str())
        .map(humanize_token)
        .unwrap_or_else(|| "carrier diagnostic".to_string());
    let level = event
        .payload
        .get("level")
        .and_then(|value| value.as_str())
        .unwrap_or("info");
    let text = if source == "known noise suppression" {
        let policy = event
            .payload
            .get("suppression_policy")
            .and_then(|value| value.as_str())
            .map(humanize_token)
            .unwrap_or_else(|| "unknown policy".to_string());
        let count = event
            .payload
            .get("suppression_count")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        format!("diagnostic {level} {source} · policy {policy} · suppressed {count}")
    } else {
        format!("diagnostic {level} {source} · mediated")
    };
    Some(TranscriptItem {
        kind: TranscriptItemKind::TurnTerminalStatus,
        actor: TranscriptActor::AgentTui,
        turn_id: event
            .payload
            .get("turn_id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string(),
        text,
        sequence: None,
        projection_key: Some(format!("diagnostic:{}", event.event_id)),
        occurred_at: Some(event.occurred_at.clone()),
    })
}

fn humanize_token(value: &str) -> String {
    value.replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{SESSION_EVENT_SCHEMA, SessionEvent};
    use serde_json::json;

    fn event(event_kind: SessionEventKind, payload: serde_json::Value) -> SessionEvent {
        SessionEvent {
            schema: SESSION_EVENT_SCHEMA.to_string(),
            event_kind,
            event_id: "session_event_projection_1".to_string(),
            occurred_at: "2026-05-30T00:00:00.000Z".to_string(),
            carrier_session_id: "carrier_fixture_1".to_string(),
            agent_id: "sonar.resident".to_string(),
            site_id: "narada-sonar".to_string(),
            site_root: "D:/code/narada.sonar".to_string(),
            payload,
        }
    }

    #[test]
    fn projects_input_admitted_to_immediate_input_transcript_item() {
        let item = project_session_event(&event(
            SessionEventKind::InputAdmittedToTurn,
            json!({
                "input_event_id": "input_1",
                "source_kind": "operator",
                "source_id": "operator",
                "transport": "interactive_terminal",
                "content_preview": "run startup sequence"
            }),
        ))
        .expect("projection exists");

        assert_eq!(item.kind, TranscriptItemKind::InputAdmitted);
        assert_eq!(item.actor, TranscriptActor::Operator);
        assert_eq!(item.turn_id, "");
        assert_eq!(item.text, "run startup sequence");
    }

    #[test]
    fn projects_operator_turn_started_to_input_transcript_item() {
        let item = project_session_event(&event(
            SessionEventKind::TurnStarted,
            json!({
                "turn_id": "turn_1",
                "input_event_id": "input_1",
                "source_kind": "operator",
                "source_id": "operator",
                "content_preview": "run startup sequence"
            }),
        ))
        .expect("projection exists");

        assert_eq!(item.kind, TranscriptItemKind::InputAdmitted);
        assert_eq!(item.actor, TranscriptActor::Operator);
        assert_eq!(item.actor.as_str(), "operator");
        assert_eq!(item.turn_id, "turn_1");
        assert_eq!(item.text, "run startup sequence");
    }

    #[test]
    fn projects_operator_directive_input_separately_from_ordinary_operator_input() {
        let item = project_session_event(&event(
            SessionEventKind::InputAdmittedToTurn,
            json!({
                "input_event_id": "input_directive_1",
                "source_kind": "operator",
                "source_id": "operator",
                "transport": "control_jsonl",
                "content_preview": "Always include active directives.",
                "directive_id": "dir_operator_1",
                "authority_ref": "operator_explicit_directive"
            }),
        ))
        .expect("projection exists");

        assert_eq!(item.kind, TranscriptItemKind::InputAdmitted);
        assert_eq!(item.actor, TranscriptActor::OperatorDirective);
        assert_eq!(item.actor.as_str(), "operator directive");
        assert_eq!(item.text, "Always include active directives.");
    }

    #[test]
    fn projects_operator_steering_input_separately_from_ordinary_operator_input() {
        let item = project_session_event(&event(
            SessionEventKind::InputAdmittedToTurn,
            json!({
                "input_event_id": "input_steering_1",
                "source_kind": "operator",
                "source_id": "operator",
                "transport": "interactive_terminal",
                "delivery_mode": "admit_after_active_turn",
                "content_preview": "check the mailbox after this turn"
            }),
        ))
        .expect("projection exists");

        assert_eq!(item.kind, TranscriptItemKind::InputAdmitted);
        assert_eq!(item.actor, TranscriptActor::OperatorSteering);
        assert_eq!(item.actor.as_str(), "operator steering");
        assert_eq!(item.text, "check the mailbox after this turn");
    }

    #[test]
    fn projects_system_turn_started_to_input_transcript_item() {
        let item = project_session_event(&event(
            SessionEventKind::TurnStarted,
            json!({
                "turn_id": "turn_2",
                "input_event_id": "input_2",
                "source_kind": "system",
                "source_id": "narada.system",
                "content_preview": "run site loop"
            }),
        ))
        .expect("projection exists");

        assert_eq!(item.kind, TranscriptItemKind::InputAdmitted);
        assert_eq!(item.actor, TranscriptActor::System);
        assert_eq!(item.text, "run site loop");
    }

    #[test]
    fn projects_system_directive_hold_and_release() {
        let held = project_session_event(&event(
            SessionEventKind::SystemDirectiveHeld,
            json!({ "input_event_id": "input_held" }),
        ))
        .expect("held projection exists");
        assert_eq!(held.kind, TranscriptItemKind::SystemDirectiveHeld);
        assert_eq!(held.actor, TranscriptActor::System);
        assert_eq!(held.text, "held input_held");

        let released = project_session_event(&event(
            SessionEventKind::SystemDirectiveReleased,
            json!({ "input_event_id": "input_held" }),
        ))
        .expect("released projection exists");
        assert_eq!(released.kind, TranscriptItemKind::SystemDirectiveReleased);
        assert_eq!(released.actor, TranscriptActor::System);
        assert_eq!(released.text, "released input_held");
    }

    #[test]
    fn projects_provider_text_delta_to_agent_transcript_item() {
        let item = project_session_event(&event(
            SessionEventKind::ProviderTextDeltaRecorded,
            json!({
                "turn_id": "turn_1",
                "sequence": 1,
                "text_delta": "hello"
            }),
        ))
        .expect("projection exists");

        assert_eq!(item.kind, TranscriptItemKind::ProviderTextDelta);
        assert_eq!(item.actor, TranscriptActor::Agent);
        assert_eq!(item.actor.as_str(), "agent");
        assert_eq!(item.turn_id, "turn_1");
        assert_eq!(item.sequence, Some(1));
        assert_eq!(item.text, "hello");
    }

    #[test]
    fn skips_blank_provider_text_delta_without_rendering_empty_agent_blocks() {
        let blank = project_session_event(&event(
            SessionEventKind::ProviderTextDeltaRecorded,
            json!({
                "turn_id": "turn_1",
                "sequence": 1,
                "text_delta": "  \n\t  "
            }),
        ));

        assert_eq!(blank, None);
    }

    #[test]
    fn projects_provider_text_delta_ref_summary_instead_of_inline_payload() {
        let item = project_session_event(&event(
            SessionEventKind::ProviderTextDeltaRecorded,
            json!({
                "turn_id": "turn_1",
                "sequence": 1,
                "text_delta": "provider text delta omitted from transcript",
                "text_delta_ref": {
                    "schema": "narada.carrier.payload_ref.v1",
                    "payload_ref": "mcp_payload:provider_text_turn_1_1@v1",
                    "reader_tool": "mcp_payload_read",
                    "summary": "provider text delta omitted from transcript"
                }
            }),
        ))
        .expect("projection exists");

        assert_eq!(
            item.text,
            "provider text delta omitted from transcript [text_delta_ref]"
        );
    }

    #[test]
    fn projects_provider_tool_call_request_to_agent_tui_transcript_item() {
        let item = project_session_event(&event(
            SessionEventKind::ProviderToolCallRequested,
            json!({
                "turn_id": "turn_1",
                "sequence": 2,
                "tool_name": "site_loop_run_once",
                "arguments_summary": "{}"
            }),
        ))
        .expect("projection exists");

        assert_eq!(item.kind, TranscriptItemKind::ProviderToolCallRequest);
        assert_eq!(item.actor, TranscriptActor::AgentTui);
        assert_eq!(item.actor.as_str(), "agent-tui");
        assert_eq!(item.turn_id, "turn_1");
        assert_eq!(item.sequence, Some(2));
        assert_eq!(item.text, "site_loop_run_once({})");
    }

    #[test]
    fn projects_provider_tool_call_ref_summary_instead_of_inline_arguments() {
        let item = project_session_event(&event(
            SessionEventKind::ProviderToolCallRequested,
            json!({
                "turn_id": "turn_1",
                "sequence": 2,
                "tool_name": "site_loop_run_once",
                "arguments_summary": "sensitive provider tool arguments omitted from transcript",
                "arguments_ref": {
                    "schema": "narada.carrier.payload_ref.v1",
                    "payload_ref": "mcp_payload:provider_tool_args_turn_1_2@v1",
                    "reader_tool": "mcp_payload_read",
                    "summary": "sensitive provider tool arguments omitted from transcript"
                }
            }),
        ))
        .expect("projection exists");

        assert_eq!(
            item.text,
            "site_loop_run_once(sensitive provider tool arguments omitted from transcript [arguments_ref])"
        );
    }

    #[test]
    fn projects_tool_result_received_to_agent_tui_transcript_item() {
        let item = project_session_event(&event(
            SessionEventKind::ToolResultReceived,
            json!({
                "tool_name": "site_loop_run_once",
                "status": "ok",
                "duration_ms": 12,
                "result_summary": "success",
                "result_ref": null
            }),
        ))
        .expect("projection exists");

        assert_eq!(item.kind, TranscriptItemKind::ToolResultReceived);
        assert_eq!(item.actor, TranscriptActor::AgentTui);
        assert_eq!(item.text, "ok site_loop_run_once in 12ms · success");
        assert_eq!(
            item.occurred_at.as_deref(),
            Some("2026-05-30T00:00:00.000Z")
        );
    }

    #[test]
    fn projects_tool_result_without_empty_summary_separator() {
        let item = project_session_event(&event(
            SessionEventKind::ToolResultReceived,
            json!({
                "tool_name": "site_loop_run_once",
                "status": "ok",
                "duration_ms": 12,
                "result_summary": "   ",
                "result_ref": null
            }),
        ))
        .expect("projection exists");

        assert_eq!(item.text, "ok site_loop_run_once in 12ms");
        assert!(!item.text.contains(" ·"));
    }

    #[test]
    fn projects_terminal_turn_status() {
        let item = project_session_event(&event(
            SessionEventKind::TurnFailed,
            json!({
                "turn_id": "turn_1",
                "terminal_status": "failed"
            }),
        ))
        .expect("projection exists");

        assert_eq!(item.kind, TranscriptItemKind::TurnTerminalStatus);
        assert_eq!(item.actor, TranscriptActor::AgentTui);
        assert_eq!(item.turn_id, "turn_1");
        assert_eq!(item.text, "failed");
    }

    #[test]
    fn projects_carrier_diagnostic_without_raw_stderr_message() {
        let item = project_session_event(&event(
            SessionEventKind::CarrierDiagnosticRecorded,
            json!({
                "source": "provider_stderr",
                "level": "warn",
                "message": "raw provider stderr line that must not render directly",
                "rendering_boundary": "mediated_diagnostic_event",
                "terminal_write": false
            }),
        ))
        .expect("diagnostic projection exists");

        assert_eq!(item.kind, TranscriptItemKind::TurnTerminalStatus);
        assert_eq!(item.actor, TranscriptActor::AgentTui);
        assert_eq!(item.text, "diagnostic warn provider stderr · mediated");
        assert!(!item.text.contains("raw provider stderr line"));
    }

    #[test]
    fn projects_known_noise_suppression_policy_and_count() {
        let item = project_session_event(&event(
            SessionEventKind::CarrierDiagnosticRecorded,
            json!({
                "source": "known_noise_suppression",
                "level": "info",
                "message": "known noise suppressed before transcript rendering",
                "suppression_policy": "sqlite_experimental_warning",
                "suppression_count": 3,
                "rendering_boundary": "mediated_diagnostic_event",
                "terminal_write": false
            }),
        ))
        .expect("diagnostic projection exists");

        assert_eq!(
            item.text,
            "diagnostic info known noise suppression · policy sqlite experimental warning · suppressed 3"
        );
    }

    #[test]
    fn ignores_non_transcript_session_events() {
        let item = project_session_event(&event(
            SessionEventKind::InputQueuedForTurnBoundary,
            json!({ "input_event_id": "input_1" }),
        ));
        assert!(item.is_none());
    }
}
