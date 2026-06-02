use crate::carrier_protocol::{SESSION_EVENT_SCHEMA, SessionEvent, SessionEventKind};
use crate::input_queue::SessionEvidenceContext;
use crate::mcp_fabric_transport::McpFabricPreparedToolCall;
use crate::mcp_process_supervisor::{
    McpProcessSupervisorState, recovery_diagnostic_event, refuse_call_until_ready,
};
use crate::mcp_runtime_config::McpRuntimeConfig;
use crate::mcp_stdio_process::McpStdioProcessIoResult;
use crate::session_jsonl::append_session_event;
use serde_json::json;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub trait McpRuntimeToolExecutor: Send {
    fn execute_tool_call(
        &mut self,
        prepared: &McpFabricPreparedToolCall,
    ) -> Result<McpStdioProcessIoResult, String>;
}

#[derive(Debug, Clone)]
pub struct McpRuntimeExecutionClock {
    pub occurred_at: String,
    pub event_id_prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpRuntimeExecutionResult {
    pub server_name: String,
    pub request_evidence_written: bool,
    pub result_evidence_written: bool,
    pub recovery_evidence_written: bool,
    pub supervisor_state: McpProcessSupervisorState,
}

pub struct McpRuntimeExecutionBridge<E: McpRuntimeToolExecutor> {
    evidence_context: SessionEvidenceContext,
    session_jsonl_path: PathBuf,
    executor: E,
    runtime_config: McpRuntimeConfig,
    supervisors: BTreeMap<String, McpProcessSupervisorState>,
    next_event_index: u64,
}

impl<E: McpRuntimeToolExecutor> McpRuntimeExecutionBridge<E> {
    pub fn new(
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
        executor: E,
    ) -> Self {
        Self::with_runtime_config(
            session_jsonl_path,
            evidence_context,
            executor,
            McpRuntimeConfig::disabled(),
        )
    }

    pub fn with_runtime_config(
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
        executor: E,
        runtime_config: McpRuntimeConfig,
    ) -> Self {
        Self {
            evidence_context,
            session_jsonl_path: session_jsonl_path.into(),
            executor,
            runtime_config,
            supervisors: BTreeMap::new(),
            next_event_index: 1,
        }
    }

    pub fn session_jsonl_path(&self) -> &Path {
        &self.session_jsonl_path
    }

    pub fn supervisor_state(&self, server_name: &str) -> Option<&McpProcessSupervisorState> {
        self.supervisors.get(server_name)
    }

    pub fn mark_server_ready(&mut self, server_name: impl Into<String>) {
        let server_name = server_name.into();
        self.supervisor_mut(&server_name).ready();
    }

    pub fn execute_prepared_tool_call(
        &mut self,
        prepared: &McpFabricPreparedToolCall,
        clock: &McpRuntimeExecutionClock,
    ) -> Result<McpRuntimeExecutionResult, String> {
        {
            let state = self.supervisor_mut(&prepared.server_name);
            refuse_call_until_ready(state, prepared)?;
        }

        let request_event = self.tool_request_event_with_runtime_posture(prepared, clock);
        self.write_evidence(&request_event)?;
        match self.executor.execute_tool_call(prepared) {
            Ok(io_result) => {
                let result_event = self.tool_result_event(&io_result, clock);
                self.write_evidence(&result_event)?;
                let state = self
                    .supervisor_mut(&prepared.server_name)
                    .apply_successful_call(io_result);
                Ok(McpRuntimeExecutionResult {
                    server_name: prepared.server_name.clone(),
                    request_evidence_written: true,
                    result_evidence_written: true,
                    recovery_evidence_written: false,
                    supervisor_state: state.state,
                })
            }
            Err(error) => {
                let state = self.supervisor_mut(&prepared.server_name);
                state.fail(error);
                let snapshot = state.clone();
                let event_id = self.next_event_id(clock);
                let diagnostic = recovery_diagnostic_event(
                    &snapshot,
                    &self.evidence_context,
                    event_id,
                    clock.occurred_at.clone(),
                );
                self.write_evidence(&diagnostic)?;
                Ok(McpRuntimeExecutionResult {
                    server_name: prepared.server_name.clone(),
                    request_evidence_written: true,
                    result_evidence_written: false,
                    recovery_evidence_written: true,
                    supervisor_state: snapshot,
                })
            }
        }
    }

    fn supervisor_mut(&mut self, server_name: &str) -> &mut McpProcessSupervisorState {
        self.supervisors
            .entry(server_name.to_string())
            .or_insert_with(|| McpProcessSupervisorState::new(server_name))
    }

    fn tool_request_event_with_runtime_posture(
        &self,
        prepared: &McpFabricPreparedToolCall,
        _clock: &McpRuntimeExecutionClock,
    ) -> SessionEvent {
        let mut event = prepared.request_event.clone();
        event.payload["mcp_runtime_status"] = json!(self.runtime_config.status.as_str());
        event.payload["mcp_fabric_access_enabled"] =
            json!(self.runtime_config.mcp_fabric_access_enabled);
        event.payload["mcp_config_path_policy"] = json!(self.runtime_config.config_path_policy);
        event.payload["mcp_config"] = json!(self.runtime_config.config_path.clone());
        event.payload["site_mcp_fabric"] = json!(self.runtime_config.site_mcp_fabric.clone());
        event.payload["mcp_refusal_reason"] = json!(self.runtime_config.refusal_reason.clone());
        event
    }

    fn tool_result_event(
        &mut self,
        io_result: &McpStdioProcessIoResult,
        clock: &McpRuntimeExecutionClock,
    ) -> SessionEvent {
        SessionEvent {
            schema: SESSION_EVENT_SCHEMA.to_string(),
            event_kind: SessionEventKind::ToolResultReceived,
            event_id: self.next_event_id(clock),
            occurred_at: clock.occurred_at.clone(),
            carrier_session_id: self.evidence_context.carrier_session_id.clone(),
            agent_id: self.evidence_context.agent_id.clone(),
            site_id: self.evidence_context.site_id.clone(),
            site_root: self.evidence_context.site_root.clone(),
            payload: json!({
                "server_name": io_result.server_name,
                "tool_name": io_result.tool_result.tool_name,
                "turn_id": io_result.request_turn_id,
                "status": io_result.tool_result.status,
                "duration_ms": io_result.tool_result.duration_ms,
                "result_summary": io_result.tool_result.result_summary,
                "result_ref": io_result.tool_result.result_ref,
                "mcp_runtime_execution": "supervised_stdio",
                "mcp_runtime_status": self.runtime_config.status.as_str(),
                "mcp_fabric_access_enabled": self.runtime_config.mcp_fabric_access_enabled,
                "mcp_config_path_policy": self.runtime_config.config_path_policy,
                "mcp_config": self.runtime_config.config_path,
                "site_mcp_fabric": self.runtime_config.site_mcp_fabric,
                "mcp_refusal_reason": self.runtime_config.refusal_reason,
            }),
        }
    }

    fn next_event_id(&mut self, clock: &McpRuntimeExecutionClock) -> String {
        let id = format!("{}_{}", clock.event_id_prefix, self.next_event_index);
        self.next_event_index += 1;
        id
    }

    fn write_evidence(&self, event: &SessionEvent) -> Result<(), String> {
        append_session_event(&self.session_jsonl_path, event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{SESSION_EVENT_SCHEMA, SessionEventKind, parse_session_event};
    use crate::mcp_fabric_boundary::McpToolResult;
    use crate::mcp_fabric_transport::McpFabricPreparedToolCall;
    use crate::mcp_json_rpc::McpJsonRpcExchange;
    use crate::mcp_runtime_config::{
        mcp_config_env_var, mcp_fabric_env_var, site_mcp_fabric_env_var,
    };
    use std::collections::BTreeMap;
    use std::fs::{read_to_string, remove_file};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_session_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock works")
            .as_nanos();
        std::env::temp_dir().join(format!("narada-agent-tui-mcp-runtime-{unique}.jsonl"))
    }

    fn context() -> SessionEvidenceContext {
        SessionEvidenceContext {
            carrier_session_id: "carrier_fixture_1".to_string(),
            agent_id: "sonar.resident".to_string(),
            site_id: "narada-sonar".to_string(),
            site_root: "D:/code/narada.sonar".to_string(),
        }
    }

    fn clock() -> McpRuntimeExecutionClock {
        McpRuntimeExecutionClock {
            occurred_at: "2026-05-30T00:00:00.000Z".to_string(),
            event_id_prefix: "session_event_mcp_runtime".to_string(),
        }
    }

    fn prepared() -> McpFabricPreparedToolCall {
        McpFabricPreparedToolCall {
            server_name: "sonar-site-loop".to_string(),
            command: "node".to_string(),
            args: vec!["site-loop.mjs".to_string()],
            env: std::collections::BTreeMap::new(),
            tool_name: "site_loop_run_once".to_string(),
            request_event: SessionEvent {
                schema: SESSION_EVENT_SCHEMA.to_string(),
                event_kind: SessionEventKind::ToolCallRequested,
                event_id: "session_event_tool_request_1".to_string(),
                occurred_at: "2026-05-30T00:00:00.000Z".to_string(),
                carrier_session_id: "carrier_fixture_1".to_string(),
                agent_id: "sonar.resident".to_string(),
                site_id: "narada-sonar".to_string(),
                site_root: "D:/code/narada.sonar".to_string(),
                payload: json!({
                    "tool_name": "site_loop_run_once",
                    "arguments_summary": "{}",
                    "requesting_agent_id": "sonar.resident"
                }),
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

    fn configured_mcp_runtime() -> McpRuntimeConfig {
        McpRuntimeConfig::from_env_map(&BTreeMap::from([
            (mcp_fabric_env_var().to_string(), "true".to_string()),
            (
                mcp_config_env_var().to_string(),
                "D:/code/narada.sonar/.ai/mcp/agent-tui.json".to_string(),
            ),
            (
                site_mcp_fabric_env_var().to_string(),
                "D:/code/narada.sonar/.ai/mcp".to_string(),
            ),
        ]))
    }

    struct SuccessfulExecutor;

    impl McpRuntimeToolExecutor for SuccessfulExecutor {
        fn execute_tool_call(
            &mut self,
            prepared: &McpFabricPreparedToolCall,
        ) -> Result<McpStdioProcessIoResult, String> {
            Ok(McpStdioProcessIoResult {
                server_name: prepared.server_name.clone(),
                request_turn_id: prepared
                    .request_event
                    .payload
                    .get("turn_id")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string),
                tool_result: McpToolResult {
                    tool_name: prepared.tool_name.clone(),
                    status: "ok".to_string(),
                    duration_ms: 12,
                    result_summary: "content_items=1".to_string(),
                    result_ref: None,
                },
                response_line: "{}".to_string(),
            })
        }
    }

    struct FailingExecutor;

    impl McpRuntimeToolExecutor for FailingExecutor {
        fn execute_tool_call(
            &mut self,
            _prepared: &McpFabricPreparedToolCall,
        ) -> Result<McpStdioProcessIoResult, String> {
            Err("mcp_stdio_read_failed:sonar-site-loop:timeout".to_string())
        }
    }

    #[test]
    fn refuses_execution_until_supervisor_ready() {
        let path = temp_session_path();
        let mut bridge = McpRuntimeExecutionBridge::new(&path, context(), SuccessfulExecutor);

        let error = bridge
            .execute_prepared_tool_call(&prepared(), &clock())
            .expect_err("not ready rejected");

        assert_eq!(error, "mcp_process_not_ready:sonar-site-loop:not_started");
        let _ = remove_file(path);
    }

    #[test]
    fn writes_request_and_result_evidence_for_ready_supervised_call() {
        let path = temp_session_path();
        let mut bridge = McpRuntimeExecutionBridge::new(&path, context(), SuccessfulExecutor);
        bridge.mark_server_ready("sonar-site-loop");

        let result = bridge
            .execute_prepared_tool_call(&prepared(), &clock())
            .expect("execution succeeds");

        assert!(result.request_evidence_written);
        assert!(result.result_evidence_written);
        assert!(!result.recovery_evidence_written);
        assert_eq!(result.supervisor_state.state.as_str(), "ready");
        let contents = read_to_string(&path).expect("session jsonl exists");
        let events = contents
            .lines()
            .map(|line| parse_session_event(line).expect("event parses"))
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_kind, SessionEventKind::ToolCallRequested);
        assert_eq!(events[0].payload["mcp_runtime_status"], "disabled");
        assert_eq!(events[0].payload["mcp_fabric_access_enabled"], false);
        assert_eq!(
            events[0].payload["mcp_config_path_policy"],
            crate::mcp_runtime_config::config_path_policy()
        );
        assert_eq!(events[1].event_kind, SessionEventKind::ToolResultReceived);
        assert_eq!(events[1].payload["mcp_runtime_status"], "disabled");
        assert_eq!(events[1].payload["mcp_fabric_access_enabled"], false);
        assert_eq!(
            events[1].payload["mcp_config_path_policy"],
            crate::mcp_runtime_config::config_path_policy()
        );
        let _ = remove_file(path);
    }

    #[test]
    fn writes_configured_mcp_runtime_posture_in_tool_evidence() {
        let path = temp_session_path();
        let mut bridge = McpRuntimeExecutionBridge::with_runtime_config(
            &path,
            context(),
            SuccessfulExecutor,
            configured_mcp_runtime(),
        );
        bridge.mark_server_ready("sonar-site-loop");

        bridge
            .execute_prepared_tool_call(&prepared(), &clock())
            .expect("execution succeeds");

        let contents = read_to_string(&path).expect("session jsonl exists");
        let events = contents
            .lines()
            .map(|line| parse_session_event(line).expect("event parses"))
            .collect::<Vec<_>>();
        assert_eq!(events[0].payload["mcp_runtime_status"], "configured");
        assert_eq!(events[0].payload["mcp_fabric_access_enabled"], true);
        assert_eq!(
            events[0].payload["mcp_config_path_policy"],
            crate::mcp_runtime_config::config_path_policy()
        );
        assert_eq!(
            events[0].payload["mcp_config"],
            "D:/code/narada.sonar/.ai/mcp/agent-tui.json"
        );
        assert_eq!(
            events[0].payload["site_mcp_fabric"],
            "D:/code/narada.sonar/.ai/mcp"
        );
        assert_eq!(events[1].payload["mcp_runtime_status"], "configured");
        assert_eq!(events[1].payload["mcp_fabric_access_enabled"], true);
        assert_eq!(
            events[1].payload["mcp_config_path_policy"],
            crate::mcp_runtime_config::config_path_policy()
        );
        let _ = remove_file(path);
    }

    #[test]
    fn writes_recovery_diagnostic_when_executor_fails() {
        let path = temp_session_path();
        let mut bridge = McpRuntimeExecutionBridge::new(&path, context(), FailingExecutor);
        bridge.mark_server_ready("sonar-site-loop");

        let result = bridge
            .execute_prepared_tool_call(&prepared(), &clock())
            .expect("failure is captured as diagnostic result");

        assert!(result.request_evidence_written);
        assert!(!result.result_evidence_written);
        assert!(result.recovery_evidence_written);
        assert_eq!(result.supervisor_state.state.as_str(), "restarting");
        let contents = read_to_string(&path).expect("session jsonl exists");
        let events = contents
            .lines()
            .map(|line| parse_session_event(line).expect("event parses"))
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_kind, SessionEventKind::ToolCallRequested);
        assert_eq!(
            events[1].event_kind,
            SessionEventKind::CarrierDiagnosticRecorded
        );
        assert_eq!(events[1].payload["source"], "mcp_stderr");
        let _ = remove_file(path);
    }
}
