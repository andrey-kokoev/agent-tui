use crate::carrier_protocol::{
    PAYLOAD_POLICY_SCHEMA, PAYLOAD_REF_SCHEMA, PayloadPolicy, PayloadRef, SESSION_EVENT_SCHEMA,
    SessionEvent, SessionEventKind,
};
use crate::input_queue::SessionEvidenceContext;
use crate::layout_model::{AgentTuiLayout, LayoutConfig, TerminalSize, compute_layout};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticSource {
    ProviderStderr,
    McpStderr,
    KnownNoiseSuppression,
    Resize,
    PayloadPolicy,
}

impl DiagnosticSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ProviderStderr => "provider_stderr",
            Self::McpStderr => "mcp_stderr",
            Self::KnownNoiseSuppression => "known_noise_suppression",
            Self::Resize => "terminal_resize",
            Self::PayloadPolicy => "payload_policy",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticBoundaryRecord {
    pub source: DiagnosticSource,
    pub level: String,
    pub message: String,
    pub suppression_policy: Option<String>,
    pub suppression_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlinePayloadDecision {
    Inline,
    RequiresRef(PayloadRef),
}

pub fn provider_stderr_diagnostic(message: impl Into<String>) -> DiagnosticBoundaryRecord {
    DiagnosticBoundaryRecord {
        source: DiagnosticSource::ProviderStderr,
        level: "warn".to_string(),
        message: message.into(),
        suppression_policy: None,
        suppression_count: None,
    }
}

pub fn mcp_stderr_diagnostic(message: impl Into<String>) -> DiagnosticBoundaryRecord {
    DiagnosticBoundaryRecord {
        source: DiagnosticSource::McpStderr,
        level: "warn".to_string(),
        message: message.into(),
        suppression_policy: None,
        suppression_count: None,
    }
}

pub fn known_noise_suppression_diagnostic(
    policy: impl Into<String>,
    suppressed_count: u64,
) -> DiagnosticBoundaryRecord {
    DiagnosticBoundaryRecord {
        source: DiagnosticSource::KnownNoiseSuppression,
        level: "info".to_string(),
        message: "known noise suppressed before transcript rendering".to_string(),
        suppression_policy: Some(policy.into()),
        suppression_count: Some(suppressed_count),
    }
}

pub fn diagnostic_session_event(
    record: &DiagnosticBoundaryRecord,
    context: &SessionEvidenceContext,
    event_id: impl Into<String>,
    occurred_at: impl Into<String>,
) -> SessionEvent {
    let mut payload = json!({
        "level": record.level,
        "message": record.message,
        "source": record.source.as_str(),
        "rendering_boundary": "mediated_diagnostic_event",
        "terminal_write": false,
    });
    if let Some(policy) = &record.suppression_policy {
        payload["suppression_policy"] = json!(policy);
    }
    if let Some(count) = record.suppression_count {
        payload["suppression_count"] = json!(count);
    }

    SessionEvent {
        schema: SESSION_EVENT_SCHEMA.to_string(),
        event_kind: SessionEventKind::CarrierDiagnosticRecorded,
        event_id: event_id.into(),
        occurred_at: occurred_at.into(),
        carrier_session_id: context.carrier_session_id.clone(),
        agent_id: context.agent_id.clone(),
        site_id: context.site_id.clone(),
        site_root: context.site_root.clone(),
        payload,
    }
}

pub fn resize_boundary(
    previous: TerminalSize,
    next: TerminalSize,
    config: LayoutConfig,
) -> (AgentTuiLayout, DiagnosticBoundaryRecord) {
    let layout = compute_layout(next, config);
    let record = DiagnosticBoundaryRecord {
        source: DiagnosticSource::Resize,
        level: "info".to_string(),
        message: format!(
            "terminal resized from {}x{} to {}x{} with composer and transcript state preserved",
            previous.width, previous.height, next.width, next.height
        ),
        suppression_policy: None,
        suppression_count: None,
    };
    (layout, record)
}

pub fn default_payload_policy() -> PayloadPolicy {
    PayloadPolicy {
        schema: PAYLOAD_POLICY_SCHEMA.to_string(),
        max_inline_chars: 4000,
        max_inline_bytes: 8000,
        sensitive_payloads_require_ref: true,
    }
}

pub fn decide_payload_inline(
    content: &str,
    sensitive: bool,
    payload_ref_id: impl Into<String>,
    summary: impl Into<String>,
    policy: &PayloadPolicy,
) -> InlinePayloadDecision {
    let too_many_chars = content.chars().count() as u64 > policy.max_inline_chars;
    let too_many_bytes = content.len() as u64 > policy.max_inline_bytes;
    if (sensitive && policy.sensitive_payloads_require_ref) || too_many_chars || too_many_bytes {
        InlinePayloadDecision::RequiresRef(PayloadRef {
            schema: PAYLOAD_REF_SCHEMA.to_string(),
            payload_ref: payload_ref_id.into(),
            reader_tool: "mcp_payload_read".to_string(),
            summary: summary.into(),
        })
    } else {
        InlinePayloadDecision::Inline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{parse_payload_policy, parse_payload_ref, parse_session_event};

    fn context() -> SessionEvidenceContext {
        SessionEvidenceContext {
            carrier_session_id: "carrier_fixture_1".to_string(),
            agent_id: "sonar.resident".to_string(),
            site_id: "narada-sonar".to_string(),
            site_root: "D:/code/narada.sonar".to_string(),
        }
    }

    #[test]
    fn provider_stderr_becomes_valid_diagnostic_event() {
        let record = provider_stderr_diagnostic("provider wrote stderr");
        let event = diagnostic_session_event(
            &record,
            &context(),
            "session_event_diag_1",
            "2026-05-30T00:00:00.000Z",
        );

        assert_eq!(
            event.event_kind,
            SessionEventKind::CarrierDiagnosticRecorded
        );
        assert_eq!(event.payload["source"], "provider_stderr");
        assert_eq!(event.payload["terminal_write"], false);
        let serialized = serde_json::to_string(&event).expect("event serializes");
        assert!(parse_session_event(&serialized).is_ok());
    }

    #[test]
    fn known_noise_suppression_records_policy_and_count() {
        let record = known_noise_suppression_diagnostic("sqlite_experimental_warning", 3);
        let event = diagnostic_session_event(
            &record,
            &context(),
            "session_event_diag_2",
            "2026-05-30T00:00:00.000Z",
        );

        assert_eq!(
            event.payload["suppression_policy"],
            "sqlite_experimental_warning"
        );
        assert_eq!(event.payload["suppression_count"], 3);
        let serialized = serde_json::to_string(&event).expect("event serializes");
        assert!(parse_session_event(&serialized).is_ok());
    }

    #[test]
    fn resize_recomputes_layout_without_direct_terminal_write() {
        let (layout, record) = resize_boundary(
            TerminalSize {
                width: 100,
                height: 20,
            },
            TerminalSize {
                width: 80,
                height: 10,
            },
            LayoutConfig::default(),
        );

        assert_eq!(layout.status.y, 6);
        assert_eq!(layout.composer.y, 7);
        assert_eq!(record.source, DiagnosticSource::Resize);
        assert!(record.message.contains("state preserved"));
    }

    #[test]
    fn payload_policy_requires_refs_for_large_or_sensitive_content() {
        let policy = default_payload_policy();
        let serialized_policy = serde_json::to_string(&policy).expect("policy serializes");
        assert!(parse_payload_policy(&serialized_policy).is_ok());

        let decision = decide_payload_inline(
            "secret",
            true,
            "mcp_payload:secret@v1",
            "sensitive payload omitted",
            &policy,
        );
        match decision {
            InlinePayloadDecision::RequiresRef(payload_ref) => {
                let serialized = serde_json::to_string(&payload_ref).expect("ref serializes");
                assert!(parse_payload_ref(&serialized).is_ok());
            }
            InlinePayloadDecision::Inline => panic!("sensitive payload must require ref"),
        }
    }
}
