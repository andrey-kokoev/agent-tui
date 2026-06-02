use crate::carrier_protocol::PayloadRef;
use crate::input_queue::SessionEvidenceContext;
use crate::mcp_fabric_boundary::{McpFabricBoundary, McpToolRequest};
use crate::mcp_fabric_transport::McpFabricTransportClient;
use crate::mcp_reusable_process_executor::ReusableMcpProcessExecutor;
use crate::mcp_runtime_config::{McpRuntimeAdmissionStatus, McpRuntimeConfig};
use crate::mcp_runtime_execution::{
    McpRuntimeExecutionBridge, McpRuntimeExecutionClock, McpRuntimeExecutionResult,
    McpRuntimeToolExecutor,
};
use crate::provider_dispatch::{ProviderOutputKind, ProviderOutputRecord};
use crate::turn_coordinator::{
    NoopProviderToolCallExecutor, ProviderToolCallExecutor, TurnCoordinatorClock,
};
use serde_json::{Value, json};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderToolCallBridgeStatus {
    IgnoredNonToolOutput,
    Executed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderToolCallBridgeResult {
    pub status: ProviderToolCallBridgeStatus,
    pub tool_name: Option<String>,
    pub mcp_result: Option<McpRuntimeExecutionResult>,
}

pub struct SupervisedProviderToolCallExecutor<E: McpRuntimeToolExecutor> {
    pub fabric_client: McpFabricTransportClient,
    pub boundary: McpFabricBoundary,
    pub runtime: McpRuntimeExecutionBridge<E>,
}

impl<E: McpRuntimeToolExecutor> SupervisedProviderToolCallExecutor<E> {
    pub fn new(
        fabric_client: McpFabricTransportClient,
        boundary: McpFabricBoundary,
        runtime: McpRuntimeExecutionBridge<E>,
    ) -> Self {
        Self {
            fabric_client,
            boundary,
            runtime,
        }
    }
}

impl<E: McpRuntimeToolExecutor> ProviderToolCallExecutor for SupervisedProviderToolCallExecutor<E> {
    fn handle_provider_output(
        &mut self,
        output: &ProviderOutputRecord,
        context: &SessionEvidenceContext,
        _session_jsonl_path: &Path,
        clock: &TurnCoordinatorClock,
    ) -> Result<usize, String> {
        let runtime_clock = McpRuntimeExecutionClock {
            occurred_at: clock.occurred_at.clone(),
            event_id_prefix: format!("{}_provider_tool", clock.event_id_prefix),
        };
        let result = execute_provider_tool_output(
            output,
            context.agent_id.clone(),
            &self.fabric_client,
            &self.boundary,
            context,
            &mut self.runtime,
            &runtime_clock,
        )?;
        Ok(match result.status {
            ProviderToolCallBridgeStatus::IgnoredNonToolOutput => 0,
            ProviderToolCallBridgeStatus::Executed => result
                .mcp_result
                .map(|mcp_result| {
                    mcp_result.request_evidence_written as usize
                        + mcp_result.result_evidence_written as usize
                        + mcp_result.recovery_evidence_written as usize
                })
                .unwrap_or(0),
        })
    }
}

pub fn provider_tool_call_executor_from_mcp_runtime_config(
    session_jsonl_path: impl AsRef<Path>,
    evidence_context: SessionEvidenceContext,
    mcp_runtime_config: &McpRuntimeConfig,
) -> Result<Box<dyn ProviderToolCallExecutor>, String> {
    if mcp_runtime_config.status != McpRuntimeAdmissionStatus::Configured
        || !mcp_runtime_config.mcp_fabric_access_enabled
    {
        return Ok(Box::new(NoopProviderToolCallExecutor));
    }
    let config_path = mcp_runtime_config
        .config_path
        .as_deref()
        .ok_or_else(|| "mcp_executor_config_missing_after_admission".to_string())?;
    let site_mcp_fabric = mcp_runtime_config
        .site_mcp_fabric
        .as_deref()
        .ok_or_else(|| "mcp_executor_fabric_missing_after_admission".to_string())?;
    let fabric_client = McpFabricTransportClient::from_path(config_path)?;
    let boundary =
        fabric_client.admitted_boundary(site_mcp_fabric, format!("{config_path}:mcpServers"));
    let mut runtime = McpRuntimeExecutionBridge::with_runtime_config(
        session_jsonl_path.as_ref(),
        evidence_context,
        ReusableMcpProcessExecutor::default(),
        mcp_runtime_config.clone(),
    );
    for server_name in fabric_client.servers.keys() {
        runtime.mark_server_ready(server_name.clone());
    }
    Ok(Box::new(SupervisedProviderToolCallExecutor::new(
        fabric_client,
        boundary,
        runtime,
    )))
}

pub fn provider_output_to_mcp_request(
    output: &ProviderOutputRecord,
    requesting_agent_id: impl Into<String>,
) -> Result<Option<(McpToolRequest, Value, u64)>, String> {
    if output.kind != ProviderOutputKind::ToolCallRequest {
        return Ok(None);
    }
    let tool_name = required_string(&output.payload, "tool_name")?;
    let arguments_summary = required_string(&output.payload, "arguments_summary")?;
    let arguments_ref = payload_ref_from_value(output.payload.get("arguments_ref"))?;
    let sequence = output
        .payload
        .get("sequence")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let arguments = parse_arguments(&arguments_summary)?;
    Ok(Some((
        McpToolRequest {
            tool_name,
            arguments_summary,
            arguments_ref,
            requesting_agent_id: requesting_agent_id.into(),
        },
        arguments,
        sequence,
    )))
}

pub fn execute_provider_tool_output<E: McpRuntimeToolExecutor>(
    output: &ProviderOutputRecord,
    requesting_agent_id: impl Into<String>,
    fabric_client: &McpFabricTransportClient,
    boundary: &McpFabricBoundary,
    evidence_context: &SessionEvidenceContext,
    runtime: &mut McpRuntimeExecutionBridge<E>,
    clock: &McpRuntimeExecutionClock,
) -> Result<ProviderToolCallBridgeResult, String> {
    let requesting_agent_id = requesting_agent_id.into();
    let Some((request, arguments, sequence)) =
        provider_output_to_mcp_request(output, requesting_agent_id)?
    else {
        return Ok(ProviderToolCallBridgeResult {
            status: ProviderToolCallBridgeStatus::IgnoredNonToolOutput,
            tool_name: None,
            mcp_result: None,
        });
    };
    let provider_turn_id = output
        .payload
        .get("turn_id")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let request = resolve_provider_tool_alias(request, boundary);
    let mut prepared = fabric_client.prepare_tool_call(
        boundary,
        &request,
        arguments,
        sequence,
        evidence_context,
        format!("{}_request_{}", clock.event_id_prefix, sequence),
        clock.occurred_at.clone(),
    )?;
    if let Some(turn_id) = provider_turn_id {
        prepared.request_event.payload["turn_id"] = json!(turn_id);
    }
    let tool_name = prepared.tool_name.clone();
    let result = runtime.execute_prepared_tool_call(&prepared, clock)?;
    Ok(ProviderToolCallBridgeResult {
        status: ProviderToolCallBridgeStatus::Executed,
        tool_name: Some(tool_name),
        mcp_result: Some(result),
    })
}

fn resolve_provider_tool_alias(
    request: McpToolRequest,
    boundary: &McpFabricBoundary,
) -> McpToolRequest {
    if boundary.assert_tool_access(&request.tool_name).is_ok() {
        return request;
    }
    let aliases: &[&str] = match request.tool_name.as_str() {
        "startup_sequence" | "agent_context_startup_sequence" => {
            &["startup_sequence", "agent_context_startup_sequence"]
        }
        _ => &[],
    };
    for alias in aliases {
        if boundary.assert_tool_access(alias).is_ok() {
            return McpToolRequest {
                tool_name: (*alias).to_string(),
                ..request
            };
        }
    }
    request
}

fn required_string(payload: &Value, field: &str) -> Result<String, String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| format!("provider_tool_call_missing_field:{field}"))
}

fn payload_ref_from_value(value: Option<&Value>) -> Result<Option<PayloadRef>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    serde_json::from_value(value.clone())
        .map(Some)
        .map_err(|error| format!("provider_tool_call_arguments_ref_invalid:{error}"))
}

fn parse_arguments(arguments_summary: &str) -> Result<Value, String> {
    if arguments_summary.trim().is_empty() {
        return Ok(json!({}));
    }
    serde_json::from_str(arguments_summary).map_err(|error| {
        format!(
            "provider_tool_call_arguments_not_json:{}:{error}",
            arguments_summary
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::SessionEventKind;
    use crate::mcp_fabric_boundary::{McpFabricBoundary, McpFabricPolicy, McpToolResult};
    use crate::mcp_runtime_execution::McpRuntimeToolExecutor;
    use crate::mcp_stdio_process::McpStdioProcessIoResult;
    use std::fs::{read_to_string, remove_file, write};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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
            event_id_prefix: "session_event_provider_tool".to_string(),
        }
    }

    fn temp_session_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock works")
            .as_nanos();
        std::env::temp_dir().join(format!("narada-agent-tui-provider-tool-{unique}.jsonl"))
    }

    fn turn_clock() -> TurnCoordinatorClock {
        TurnCoordinatorClock {
            occurred_at: "2026-05-30T00:00:00.000Z".to_string(),
            event_id_prefix: "session_event_provider_tool".to_string(),
            turn_id_prefix: "turn".to_string(),
        }
    }

    fn fabric_client() -> McpFabricTransportClient {
        McpFabricTransportClient::from_json_str(
            "fixture.mcp.json",
            r#"{
              "site_id":"narada-sonar",
              "carrier":"agent-tui",
              "mcpServers":{
                "sonar-site-loop":{
                  "transport":"stdio",
                  "command":"node",
                  "args":["site-loop.mjs"],
                  "tools":["site_loop_run_once"]
                }
              }
            }"#,
        )
        .expect("fabric config parses")
    }

    fn boundary() -> McpFabricBoundary {
        McpFabricBoundary::admitted(McpFabricPolicy::from_allowed_tools(
            "D:/code/narada.sonar/.ai/mcp",
            "fixture.mcp.json:mcpServers",
            ["site_loop_run_once"],
        ))
    }

    fn narada_proper_startup_boundary() -> McpFabricBoundary {
        McpFabricBoundary::admitted(McpFabricPolicy::from_allowed_tools(
            "D:/code/narada/.ai/mcp",
            "fixture.mcp.json:mcpServers",
            ["agent_context_startup_sequence"],
        ))
    }

    struct SuccessfulExecutor;

    impl McpRuntimeToolExecutor for SuccessfulExecutor {
        fn execute_tool_call(
            &mut self,
            prepared: &crate::mcp_fabric_transport::McpFabricPreparedToolCall,
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
                    duration_ms: 10,
                    result_summary: "content_items=1".to_string(),
                    result_ref: None,
                },
                response_line: "{}".to_string(),
            })
        }
    }

    #[test]
    fn executor_factory_returns_noop_when_mcp_is_not_configured() {
        let path = temp_session_path();
        let mut executor = provider_tool_call_executor_from_mcp_runtime_config(
            &path,
            context(),
            &McpRuntimeConfig::disabled(),
        )
        .expect("disabled mcp returns no-op executor");
        let written = executor
            .handle_provider_output(
                &ProviderOutputRecord::tool_call_request("turn_1", "site_loop_run_once", "{}", 1),
                &context(),
                &path,
                &turn_clock(),
            )
            .expect("noop handles provider output");

        assert_eq!(written, 0);
        assert!(!path.exists());
    }

    #[test]
    fn executor_factory_builds_supervised_executor_from_mcp_config() {
        let session_path = temp_session_path();
        let config_path = temp_session_path().with_extension("json");
        write(
            &config_path,
            r#"{
              "site_id":"narada-sonar",
              "carrier":"agent-tui",
              "mcpServers":{
                "sonar-site-loop":{
                  "transport":"stdio",
                  "command":"node",
                  "args":["site-loop.mjs"],
                  "tools":["site_loop_run_once"]
                }
              }
            }"#,
        )
        .expect("write mcp config");
        let mcp_config = McpRuntimeConfig {
            status: McpRuntimeAdmissionStatus::Configured,
            mcp_fabric_access_enabled: true,
            config_path_policy: crate::mcp_runtime_config::config_path_policy(),
            config_path: Some(config_path.display().to_string()),
            site_mcp_fabric: Some("D:/code/narada.sonar/.ai/mcp".to_string()),
            refusal_reason: None,
        };
        let mut executor = provider_tool_call_executor_from_mcp_runtime_config(
            &session_path,
            context(),
            &mcp_config,
        )
        .expect("configured mcp builds executor");
        let written = executor
            .handle_provider_output(
                &ProviderOutputRecord::text_delta("turn_1", "ignored", 1),
                &context(),
                &session_path,
                &turn_clock(),
            )
            .expect("non-tool output is ignored without spawning");

        assert_eq!(written, 0);
        let _ = remove_file(config_path);
        let _ = remove_file(session_path);
    }

    #[test]
    fn ignores_non_tool_provider_output() {
        let output = ProviderOutputRecord::text_delta("turn_1", "hello", 1);
        let request = provider_output_to_mcp_request(&output, "sonar.resident")
            .expect("bridge handles output");

        assert!(request.is_none());
    }

    #[test]
    fn converts_provider_tool_output_to_mcp_request() {
        let output =
            ProviderOutputRecord::tool_call_request("turn_1", "site_loop_run_once", "{}", 2);
        let (request, arguments, sequence) =
            provider_output_to_mcp_request(&output, "sonar.resident")
                .expect("bridge handles output")
                .expect("tool request extracted");

        assert_eq!(request.tool_name, "site_loop_run_once");
        assert_eq!(request.requesting_agent_id, "sonar.resident");
        assert_eq!(arguments, json!({}));
        assert_eq!(sequence, 2);
    }

    #[test]
    fn resolves_generic_startup_sequence_to_visible_site_alias() {
        let request = McpToolRequest {
            tool_name: "startup_sequence".to_string(),
            arguments_summary: "{}".to_string(),
            arguments_ref: None,
            requesting_agent_id: "narada.architect".to_string(),
        };

        let resolved = resolve_provider_tool_alias(request, &narada_proper_startup_boundary());

        assert_eq!(resolved.tool_name, "agent_context_startup_sequence");
    }

    #[test]
    fn rejects_non_json_inline_arguments() {
        let output =
            ProviderOutputRecord::tool_call_request("turn_1", "site_loop_run_once", "not-json", 2);
        let error = provider_output_to_mcp_request(&output, "sonar.resident")
            .expect_err("non-json arguments rejected");

        assert!(error.starts_with("provider_tool_call_arguments_not_json:not-json:"));
    }

    #[test]
    fn executes_provider_tool_output_through_supervised_runtime_bridge() {
        let path = temp_session_path();
        let mut runtime = McpRuntimeExecutionBridge::new(&path, context(), SuccessfulExecutor);
        runtime.mark_server_ready("sonar-site-loop");
        let output =
            ProviderOutputRecord::tool_call_request("turn_1", "site_loop_run_once", "{}", 2);

        let result = execute_provider_tool_output(
            &output,
            "sonar.resident",
            &fabric_client(),
            &boundary(),
            &context(),
            &mut runtime,
            &clock(),
        )
        .expect("tool output executes");

        assert_eq!(result.status, ProviderToolCallBridgeStatus::Executed);
        assert_eq!(result.tool_name.as_deref(), Some("site_loop_run_once"));
        assert!(result.mcp_result.unwrap().result_evidence_written);
        let contents = read_to_string(&path).expect("session jsonl exists");
        let events = contents
            .lines()
            .map(|line| crate::carrier_protocol::parse_session_event(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(events[0].event_kind, SessionEventKind::ToolCallRequested);
        assert_eq!(events[0].payload["turn_id"], "turn_1");
        assert_eq!(events[1].event_kind, SessionEventKind::ToolResultReceived);
        assert_eq!(events[1].payload["turn_id"], "turn_1");
        let _ = remove_file(path);
    }

    #[test]
    fn supervised_executor_namespaces_provider_tool_event_ids_away_from_turn_events() {
        let path = temp_session_path();
        let mut runtime = McpRuntimeExecutionBridge::new(&path, context(), SuccessfulExecutor);
        runtime.mark_server_ready("sonar-site-loop");
        let mut executor =
            SupervisedProviderToolCallExecutor::new(fabric_client(), boundary(), runtime);
        let output = ProviderOutputRecord::tool_call_request(
            "turn_step241_1",
            "site_loop_run_once",
            "{}",
            1,
        );
        let turn_clock = TurnCoordinatorClock {
            occurred_at: "2026-06-02T02:16:32.180Z".to_string(),
            event_id_prefix: "session_event_turn_step241".to_string(),
            turn_id_prefix: "turn_step241".to_string(),
        };

        let written = executor
            .handle_provider_output(&output, &context(), &path, &turn_clock)
            .expect("provider tool output writes evidence");

        assert_eq!(written, 2);
        let contents = read_to_string(&path).expect("session jsonl exists");
        let events = contents
            .lines()
            .map(|line| crate::carrier_protocol::parse_session_event(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            events[0].event_id,
            "session_event_turn_step241_provider_tool_request_1"
        );
        assert_eq!(
            events[1].event_id,
            "session_event_turn_step241_provider_tool_1"
        );
        assert_ne!(events[1].event_id, "session_event_turn_step241_1");
        assert_eq!(events[1].payload["turn_id"], "turn_step241_1");
        let _ = remove_file(path);
    }
}
