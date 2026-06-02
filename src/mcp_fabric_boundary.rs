use crate::carrier_protocol::{PayloadRef, SESSION_EVENT_SCHEMA, SessionEvent, SessionEventKind};
use crate::input_queue::SessionEvidenceContext;
use serde_json::json;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpFabricAccessStatus {
    Disabled,
    Admitted,
}

impl McpFabricAccessStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Admitted => "admitted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpFabricPolicy {
    pub fabric_root: String,
    pub allowed_tools: BTreeSet<String>,
    pub policy_source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpFabricBoundary {
    pub status: McpFabricAccessStatus,
    pub tool_visibility: String,
    pub reason: String,
    pub policy: Option<McpFabricPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolRequest {
    pub tool_name: String,
    pub arguments_summary: String,
    pub arguments_ref: Option<PayloadRef>,
    pub requesting_agent_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpToolResult {
    pub tool_name: String,
    pub status: String,
    pub duration_ms: u64,
    pub result_summary: String,
    pub result_ref: Option<PayloadRef>,
}

impl McpFabricBoundary {
    pub fn disabled_until_admitted() -> Self {
        Self {
            status: McpFabricAccessStatus::Disabled,
            tool_visibility: "none".to_string(),
            reason: "Rust agent-tui has no admitted Site MCP fabric client".to_string(),
            policy: None,
        }
    }

    pub fn admitted(policy: McpFabricPolicy) -> Self {
        let tool_visibility = if policy.allowed_tools.is_empty() {
            "policy_bound_empty".to_string()
        } else {
            "policy_bound".to_string()
        };
        Self {
            status: McpFabricAccessStatus::Admitted,
            tool_visibility,
            reason: "Rust agent-tui Site MCP fabric client admitted by launcher policy".to_string(),
            policy: Some(policy),
        }
    }

    pub fn visible_tools(&self) -> Vec<String> {
        self.policy
            .as_ref()
            .map(|policy| policy.allowed_tools.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn assert_tool_access(&self, tool_name: &str) -> Result<(), String> {
        match self.status {
            McpFabricAccessStatus::Disabled => Err(format!(
                "mcp_fabric_access_disabled:{tool_name}:{}",
                self.reason
            )),
            McpFabricAccessStatus::Admitted => {
                let Some(policy) = &self.policy else {
                    return Err(format!("mcp_fabric_policy_missing:{tool_name}"));
                };
                if policy.allowed_tools.contains(tool_name) {
                    Ok(())
                } else {
                    Err(format!(
                        "mcp_tool_not_visible:{tool_name}:{}",
                        policy.policy_source
                    ))
                }
            }
        }
    }

    pub fn tool_request_event(
        &self,
        request: &McpToolRequest,
        context: &SessionEvidenceContext,
        event_id: impl Into<String>,
        occurred_at: impl Into<String>,
    ) -> Result<SessionEvent, String> {
        self.assert_tool_access(&request.tool_name)?;
        let mut payload = json!({
            "tool_name": request.tool_name,
            "arguments_summary": request.arguments_summary,
            "arguments_ref": request.arguments_ref,
            "requesting_agent_id": request.requesting_agent_id,
            "mcp_fabric_status": self.status.as_str(),
            "tool_visibility": self.tool_visibility,
        });
        if let Some(policy) = &self.policy {
            payload["policy_source"] = json!(policy.policy_source);
            payload["fabric_root"] = json!(policy.fabric_root);
        }
        Ok(session_event(
            context,
            SessionEventKind::ToolCallRequested,
            event_id,
            occurred_at,
            payload,
        ))
    }

    pub fn tool_result_event(
        &self,
        result: &McpToolResult,
        context: &SessionEvidenceContext,
        event_id: impl Into<String>,
        occurred_at: impl Into<String>,
    ) -> Result<SessionEvent, String> {
        self.assert_tool_access(&result.tool_name)?;
        Ok(session_event(
            context,
            SessionEventKind::ToolResultReceived,
            event_id,
            occurred_at,
            json!({
                "tool_name": result.tool_name,
                "status": result.status,
                "duration_ms": result.duration_ms,
                "result_summary": result.result_summary,
                "result_ref": result.result_ref,
                "mcp_fabric_status": self.status.as_str(),
                "tool_visibility": self.tool_visibility,
            }),
        ))
    }
}

impl McpFabricPolicy {
    pub fn from_allowed_tools(
        fabric_root: impl Into<String>,
        policy_source: impl Into<String>,
        allowed_tools: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            fabric_root: fabric_root.into(),
            allowed_tools: allowed_tools.into_iter().map(Into::into).collect(),
            policy_source: policy_source.into(),
        }
    }
}

fn session_event(
    context: &SessionEvidenceContext,
    kind: SessionEventKind,
    event_id: impl Into<String>,
    occurred_at: impl Into<String>,
    payload: serde_json::Value,
) -> SessionEvent {
    SessionEvent {
        schema: SESSION_EVENT_SCHEMA.to_string(),
        event_kind: kind,
        event_id: event_id.into(),
        occurred_at: occurred_at.into(),
        carrier_session_id: context.carrier_session_id.clone(),
        agent_id: context.agent_id.clone(),
        site_id: context.site_id.clone(),
        site_root: context.site_root.clone(),
        payload,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::parse_session_event;

    fn context() -> SessionEvidenceContext {
        SessionEvidenceContext {
            carrier_session_id: "carrier_fixture_1".to_string(),
            agent_id: "sonar.resident".to_string(),
            site_id: "narada-sonar".to_string(),
            site_root: "D:/code/narada.sonar".to_string(),
        }
    }

    fn admitted_boundary() -> McpFabricBoundary {
        McpFabricBoundary::admitted(McpFabricPolicy::from_allowed_tools(
            "D:/code/narada.sonar/.ai/mcp",
            "site_mcp_policy_fixture",
            ["site_loop_run_once", "task_lifecycle_next"],
        ))
    }

    #[test]
    fn default_boundary_disables_tool_visibility() {
        let boundary = McpFabricBoundary::disabled_until_admitted();

        assert_eq!(boundary.status.as_str(), "disabled");
        assert_eq!(boundary.tool_visibility, "none");
        assert!(
            boundary
                .reason
                .contains("no admitted Site MCP fabric client")
        );
    }

    #[test]
    fn disabled_boundary_rejects_tool_access() {
        let boundary = McpFabricBoundary::disabled_until_admitted();

        let error = boundary
            .assert_tool_access("site_loop_run_once")
            .expect_err("tool access rejected");

        assert!(error.starts_with("mcp_fabric_access_disabled:site_loop_run_once:"));
    }

    #[test]
    fn admitted_boundary_exposes_policy_bound_tools_only() {
        let boundary = admitted_boundary();

        assert_eq!(boundary.status, McpFabricAccessStatus::Admitted);
        assert_eq!(boundary.tool_visibility, "policy_bound");
        assert_eq!(
            boundary.visible_tools(),
            vec![
                "site_loop_run_once".to_string(),
                "task_lifecycle_next".to_string()
            ]
        );
        assert!(boundary.assert_tool_access("site_loop_run_once").is_ok());
        assert_eq!(
            boundary
                .assert_tool_access("shell_exec")
                .expect_err("not visible"),
            "mcp_tool_not_visible:shell_exec:site_mcp_policy_fixture"
        );
    }

    #[test]
    fn admitted_boundary_records_valid_tool_request_evidence() {
        let boundary = admitted_boundary();
        let event = boundary
            .tool_request_event(
                &McpToolRequest {
                    tool_name: "site_loop_run_once".to_string(),
                    arguments_summary: "{}".to_string(),
                    arguments_ref: None,
                    requesting_agent_id: "sonar.resident".to_string(),
                },
                &context(),
                "session_event_tool_request_1",
                "2026-05-30T00:00:00.000Z",
            )
            .expect("tool request admitted");

        assert_eq!(event.event_kind, SessionEventKind::ToolCallRequested);
        assert_eq!(event.payload["tool_name"], "site_loop_run_once");
        assert_eq!(event.payload["policy_source"], "site_mcp_policy_fixture");
        let serialized = serde_json::to_string(&event).expect("event serializes");
        assert!(parse_session_event(&serialized).is_ok());
    }

    #[test]
    fn admitted_boundary_records_valid_tool_result_evidence() {
        let boundary = admitted_boundary();
        let event = boundary
            .tool_result_event(
                &McpToolResult {
                    tool_name: "site_loop_run_once".to_string(),
                    status: "ok".to_string(),
                    duration_ms: 42,
                    result_summary: "loop pass complete".to_string(),
                    result_ref: None,
                },
                &context(),
                "session_event_tool_result_1",
                "2026-05-30T00:00:01.000Z",
            )
            .expect("tool result admitted");

        assert_eq!(event.event_kind, SessionEventKind::ToolResultReceived);
        assert_eq!(event.payload["status"], "ok");
        let serialized = serde_json::to_string(&event).expect("event serializes");
        assert!(parse_session_event(&serialized).is_ok());
    }
}
