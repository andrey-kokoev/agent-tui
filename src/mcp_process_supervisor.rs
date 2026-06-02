use crate::input_queue::SessionEvidenceContext;
use crate::mcp_fabric_transport::McpFabricPreparedToolCall;
use crate::mcp_json_rpc::JsonRpcRequest;
use crate::rendering_boundary::{diagnostic_session_event, mcp_stderr_diagnostic};
use crate::{carrier_protocol::SessionEvent, mcp_stdio_process::McpStdioProcessIoResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpProcessState {
    NotStarted,
    Starting,
    Initializing,
    Ready,
    Failed,
    Restarting,
    Stopped,
}

impl McpProcessState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Starting => "starting",
            Self::Initializing => "initializing",
            Self::Ready => "ready",
            Self::Failed => "failed",
            Self::Restarting => "restarting",
            Self::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpProcessSupervisorState {
    pub server_name: String,
    pub state: McpProcessState,
    pub restart_count: u32,
    pub last_error: Option<String>,
    pub initialized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpProcessHandshakePlan {
    pub server_name: String,
    pub initialize_line: String,
    pub initialized_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpProcessRecoveryAction {
    KeepReady,
    Restart,
    Refuse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpProcessSupervisedCallResult {
    pub state: McpProcessSupervisorState,
    pub io_result: McpStdioProcessIoResult,
}

impl McpProcessSupervisorState {
    pub fn new(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            state: McpProcessState::NotStarted,
            restart_count: 0,
            last_error: None,
            initialized: false,
        }
    }

    pub fn starting(&mut self) {
        self.state = McpProcessState::Starting;
    }

    pub fn initializing(&mut self) {
        self.state = McpProcessState::Initializing;
    }

    pub fn ready(&mut self) {
        self.state = McpProcessState::Ready;
        self.initialized = true;
        self.last_error = None;
    }

    pub fn fail(&mut self, error: impl Into<String>) -> McpProcessRecoveryAction {
        self.state = McpProcessState::Failed;
        self.initialized = false;
        self.last_error = Some(error.into());
        if self.restart_count < 1 {
            self.restart_count += 1;
            self.state = McpProcessState::Restarting;
            McpProcessRecoveryAction::Restart
        } else {
            McpProcessRecoveryAction::Refuse
        }
    }

    pub fn stop(&mut self) {
        self.state = McpProcessState::Stopped;
        self.initialized = false;
    }

    pub fn require_ready(&self) -> Result<(), String> {
        if self.state == McpProcessState::Ready && self.initialized {
            Ok(())
        } else {
            Err(format!(
                "mcp_process_not_ready:{}:{}",
                self.server_name,
                self.state.as_str()
            ))
        }
    }

    pub fn apply_successful_call(
        &mut self,
        io_result: McpStdioProcessIoResult,
    ) -> McpProcessSupervisedCallResult {
        self.ready();
        McpProcessSupervisedCallResult {
            state: self.clone(),
            io_result,
        }
    }
}

pub fn handshake_plan(
    server_name: impl Into<String>,
    initialize_id: u64,
) -> Result<McpProcessHandshakePlan, String> {
    let server_name = server_name.into();
    let initialize =
        JsonRpcRequest::mcp_initialize(initialize_id, "narada-agent-tui").to_jsonl()?;
    let initialized = JsonRpcRequest::mcp_initialized_notification().to_jsonl()?;
    Ok(McpProcessHandshakePlan {
        server_name,
        initialize_line: initialize,
        initialized_line: initialized,
    })
}

pub fn refuse_call_until_ready(
    state: &McpProcessSupervisorState,
    prepared: &McpFabricPreparedToolCall,
) -> Result<(), String> {
    if state.server_name != prepared.server_name {
        return Err(format!(
            "mcp_process_server_mismatch:{}:{}",
            state.server_name, prepared.server_name
        ));
    }
    state.require_ready()
}

pub fn recovery_diagnostic_event(
    state: &McpProcessSupervisorState,
    context: &SessionEvidenceContext,
    event_id: impl Into<String>,
    occurred_at: impl Into<String>,
) -> SessionEvent {
    let error = state
        .last_error
        .clone()
        .unwrap_or_else(|| "unknown MCP process failure".to_string());
    let record = mcp_stderr_diagnostic(format!(
        "MCP server {} entered {} after {} restart(s): {}",
        state.server_name,
        state.state.as_str(),
        state.restart_count,
        error
    ));
    diagnostic_session_event(&record, context, event_id, occurred_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::SessionEventKind;
    use crate::mcp_fabric_transport::McpFabricPreparedToolCall;
    use crate::mcp_json_rpc::McpJsonRpcExchange;
    use serde_json::json;

    fn context() -> SessionEvidenceContext {
        SessionEvidenceContext {
            carrier_session_id: "carrier_fixture_1".to_string(),
            agent_id: "sonar.resident".to_string(),
            site_id: "narada-sonar".to_string(),
            site_root: "D:/code/narada.sonar".to_string(),
        }
    }

    fn prepared(server_name: &str) -> McpFabricPreparedToolCall {
        McpFabricPreparedToolCall {
            server_name: server_name.to_string(),
            command: "node".to_string(),
            args: vec!["site-loop.mjs".to_string()],
            env: std::collections::BTreeMap::new(),
            tool_name: "site_loop_run_once".to_string(),
            request_event: SessionEvent {
                schema: crate::carrier_protocol::SESSION_EVENT_SCHEMA.to_string(),
                event_kind: SessionEventKind::ToolCallRequested,
                event_id: "session_event_tool_request_1".to_string(),
                occurred_at: "2026-05-30T00:00:00.000Z".to_string(),
                carrier_session_id: "carrier_fixture_1".to_string(),
                agent_id: "sonar.resident".to_string(),
                site_id: "narada-sonar".to_string(),
                site_root: "D:/code/narada.sonar".to_string(),
                payload: json!({"tool_name":"site_loop_run_once"}),
            },
            json_rpc: McpJsonRpcExchange::for_tool_call(
                7,
                &crate::mcp_fabric_boundary::McpToolRequest {
                    tool_name: "site_loop_run_once".to_string(),
                    arguments_summary: "{}".to_string(),
                    arguments_ref: None,
                    requesting_agent_id: "sonar.resident".to_string(),
                },
                json!({}),
            )
            .expect("json rpc builds"),
        }
    }

    #[test]
    fn handshake_plan_records_initialize_and_initialized_frames() {
        let plan = handshake_plan("sonar-site-loop", 1).expect("plan builds");

        assert_eq!(plan.server_name, "sonar-site-loop");
        assert!(plan.initialize_line.contains("\"method\":\"initialize\""));
        assert!(plan.initialized_line.contains("notifications/initialized"));
    }

    #[test]
    fn supervisor_refuses_calls_until_ready() {
        let state = McpProcessSupervisorState::new("sonar-site-loop");
        let error = refuse_call_until_ready(&state, &prepared("sonar-site-loop"))
            .expect_err("not ready rejected");

        assert_eq!(error, "mcp_process_not_ready:sonar-site-loop:not_started");
    }

    #[test]
    fn supervisor_allows_calls_after_ready() {
        let mut state = McpProcessSupervisorState::new("sonar-site-loop");
        state.starting();
        state.initializing();
        state.ready();

        assert!(refuse_call_until_ready(&state, &prepared("sonar-site-loop")).is_ok());
    }

    #[test]
    fn supervisor_tracks_single_restart_then_refuses_repeated_failure() {
        let mut state = McpProcessSupervisorState::new("sonar-site-loop");

        assert_eq!(
            state.fail("spawn failed"),
            McpProcessRecoveryAction::Restart
        );
        assert_eq!(state.state, McpProcessState::Restarting);
        assert_eq!(state.restart_count, 1);
        assert_eq!(
            state.fail("spawn failed again"),
            McpProcessRecoveryAction::Refuse
        );
        assert_eq!(state.state, McpProcessState::Failed);
    }

    #[test]
    fn recovery_diagnostic_is_session_evidence() {
        let mut state = McpProcessSupervisorState::new("sonar-site-loop");
        state.fail("spawn failed");
        let event = recovery_diagnostic_event(
            &state,
            &context(),
            "session_event_mcp_recovery_1",
            "2026-05-30T00:00:00.000Z",
        );

        assert_eq!(
            event.event_kind,
            SessionEventKind::CarrierDiagnosticRecorded
        );
        assert_eq!(event.payload["source"], "mcp_stderr");
        assert_eq!(event.payload["terminal_write"], false);
    }
}
