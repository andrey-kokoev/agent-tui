use narada_agent_tui::app_view_model::AppViewModel;
use narada_agent_tui::carrier_protocol::{InputEvent, create_provider_request_payload};
use narada_agent_tui::input_queue::SessionEvidenceContext;
use narada_agent_tui::interactive_runtime::{AgentTuiInteractiveRuntime, InteractiveStepClock};
use narada_agent_tui::layout_model::TerminalSize;
use narada_agent_tui::mcp_fabric_boundary::{McpFabricBoundary, McpFabricPolicy, McpToolResult};
use narada_agent_tui::mcp_fabric_transport::McpFabricTransportClient;
use narada_agent_tui::mcp_runtime_execution::{McpRuntimeExecutionBridge, McpRuntimeToolExecutor};
use narada_agent_tui::mcp_stdio_process::McpStdioProcessIoResult;
use narada_agent_tui::provider_dispatch::{
    ProviderAdapter, ProviderCancellationToken, ProviderDispatchRecord, ProviderDispatchStatus,
    ProviderOutputRecord,
};
use narada_agent_tui::provider_tool_call_bridge::SupervisedProviderToolCallExecutor;
use narada_agent_tui::runtime_coordinator::RuntimeCoordinatorClock;
use narada_agent_tui::status_view_model::RuntimePostureState;
use narada_agent_tui::terminal_input_tick::TerminalInputTickOutcome;
use narada_agent_tui::textarea_composer::TextareaComposer;
use narada_agent_tui::transcript_projection::{TranscriptActor, TranscriptItemKind};
use narada_agent_tui::transcript_view_model::TranscriptRow;
use narada_agent_tui::tui_render_loop::{
    AgentTuiLoopState, InteractiveClockSource, InteractiveInputSource, InteractiveTerminalFrame,
    run_injected_interactive_loop,
};
use narada_agent_tui::turn_coordinator::{ProviderToolCallExecutor, TurnCoordinatorClock};
use serde_json::json;
use std::collections::VecDeque;
use std::fs::{OpenOptions, remove_file, write};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
struct ToolScenario {
    proof_slug: &'static str,
    input_text: &'static str,
    server_name: &'static str,
    tool_name: &'static str,
    arguments_summary: &'static str,
    result_summary: &'static str,
    result_text: &'static str,
    expected_operator_line: &'static str,
    expected_tool_request_line: &'static str,
    expected_tool_result_line: &'static str,
    expected_final_agent_line: &'static str,
    expected_follow_up_guidance: Option<&'static str>,
}
struct ScenarioProvider {
    scenario: ToolScenario,
    calls: Mutex<usize>,
}

impl ProviderAdapter for ScenarioProvider {
    fn dispatch_request(
        &self,
        input: &InputEvent,
        turn_id: &str,
        _cancellation: &ProviderCancellationToken,
    ) -> ProviderDispatchRecord {
        let mut calls = self
            .calls
            .lock()
            .expect("scenario provider call lock works");
        *calls += 1;
        let payload = create_provider_request_payload(
            turn_id,
            &input.event_id,
            "completed",
            true,
            "configured",
            "admitted",
            Some("test_scenario_provider".to_string()),
            None,
            None,
            None,
            false,
            "single_provider_output_batch",
            None,
            &input.content,
        );
        if *calls == 1 {
            return ProviderDispatchRecord {
                status: ProviderDispatchStatus::Completed,
                provider_execution_enabled: true,
                payload,
                outputs: vec![ProviderOutputRecord::tool_call_request(
                    turn_id,
                    self.scenario.tool_name,
                    self.scenario.arguments_summary,
                    1,
                )],
            };
        }
        assert!(
            input.content.contains("MCP tool result(s):")
                && input.content.contains(self.scenario.result_text),
            "provider follow-up input must include MCP result text"
        );
        if let Some(expected_guidance) = self.scenario.expected_follow_up_guidance {
            assert!(
                input.content.contains(expected_guidance),
                "provider follow-up input must include paged-output guidance"
            );
        }
        ProviderDispatchRecord {
            status: ProviderDispatchStatus::Completed,
            provider_execution_enabled: true,
            payload,
            outputs: vec![ProviderOutputRecord::text_delta(
                turn_id,
                self.scenario
                    .expected_final_agent_line
                    .strip_prefix("sonar.resident: ")
                    .unwrap_or(self.scenario.expected_final_agent_line),
                1,
            )],
        }
    }
}

struct SuccessfulScenarioMcpExecutor {
    scenario: ToolScenario,
}

impl McpRuntimeToolExecutor for SuccessfulScenarioMcpExecutor {
    fn execute_tool_call(
        &mut self,
        prepared: &narada_agent_tui::mcp_fabric_transport::McpFabricPreparedToolCall,
    ) -> Result<McpStdioProcessIoResult, String> {
        assert_eq!(prepared.server_name, self.scenario.server_name);
        assert_eq!(prepared.tool_name, self.scenario.tool_name);
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
                result_summary: self.scenario.result_summary.to_string(),
                result_text: Some(self.scenario.result_text.to_string()),
                result_ref: None,
            },
            response_line: "{}".to_string(),
        })
    }
}

#[derive(Debug, Default)]
struct CapturingTerminalFrame {
    frames: Vec<Vec<String>>,
}

impl InteractiveTerminalFrame for CapturingTerminalFrame {
    fn terminal_size(&mut self) -> Result<TerminalSize, String> {
        Ok(TerminalSize {
            width: 160,
            height: 24,
        })
    }

    fn draw_frame(
        &mut self,
        model: &AppViewModel,
        _composer: &TextareaComposer,
    ) -> Result<(), String> {
        self.frames.push(rendered_transcript_lines(
            &model.transcript_rows,
            "sonar.resident",
        ));
        Ok(())
    }
}

#[derive(Debug, Default)]
struct ScriptedInput {
    outcomes: VecDeque<TerminalInputTickOutcome>,
}

impl InteractiveInputSource for ScriptedInput {
    fn read_tick(&mut self, _composer: &mut TextareaComposer) -> TerminalInputTickOutcome {
        self.outcomes
            .pop_front()
            .unwrap_or(TerminalInputTickOutcome::NoInput)
    }
}

struct StepClock {
    index: u64,
}

impl InteractiveClockSource for StepClock {
    fn next_interactive_step_clock(&mut self) -> InteractiveStepClock {
        self.index += 1;
        InteractiveStepClock {
            input: RuntimeCoordinatorClock {
                occurred_at: format!("2026-06-02T02:16:3{}.000Z", self.index),
                event_id_prefix: format!("session_event_input_step{}", self.index),
            },
            turn: TurnCoordinatorClock {
                occurred_at: format!("2026-06-02T02:16:3{}.000Z", self.index),
                event_id_prefix: format!("session_event_turn_step{}", self.index),
                turn_id_prefix: format!("turn_step{}", self.index),
            },
        }
    }
}

#[test]
fn final_frame_contains_operator_agent_tool_result_and_completion_for_startup_sequence() {
    run_final_frame_tool_scenario(ToolScenario {
        proof_slug: "startup_sequence",
        input_text: "run startup sequence",
        server_name: "sonar-agent-context",
        tool_name: "agent_context_startup_sequence",
        arguments_summary: "{}",
        result_summary: "content_items=1",
        result_text: "Startup sequence completed for sonar.resident.",
        expected_operator_line: "operator -> sonar.resident: run startup sequence",
        expected_tool_request_line: "sonar.resident -> agent-tui: agent_context_startup_sequence({})",
        expected_tool_result_line: "agent-tui -> sonar.resident: ok agent_context_startup_sequence in 10ms · content_items=1",
        expected_final_agent_line: "sonar.resident: Startup sequence completed for sonar.resident.",
        expected_follow_up_guidance: None,
    });
}

#[test]
fn final_frame_contains_answer_after_paged_startup_sequence_result() {
    run_final_frame_tool_scenario(ToolScenario {
        proof_slug: "paged_startup_sequence",
        input_text: "run startup sequence",
        server_name: "sonar-agent-context",
        tool_name: "agent_context_startup_sequence",
        arguments_summary: "{}",
        result_summary: "content_items=1",
        result_text: r#"{"status":"ok","truncated":true,"ref":"mcp_output:o_6cd77433e384445e976c7fdf","output_ref":"mcp_output:o_6cd77433e384445e976c7fdf","reader_tool":"mcp_output_show","inline_limit":200}"#,
        expected_operator_line: "operator -> sonar.resident: run startup sequence",
        expected_tool_request_line: "sonar.resident -> agent-tui: agent_context_startup_sequence({})",
        expected_tool_result_line: "agent-tui -> sonar.resident: ok agent_context_startup_sequence in 10ms · content_items=1",
        expected_final_agent_line: "sonar.resident: Startup sequence completed; paged details were not accessible through the provider surface.",
        expected_follow_up_guidance: Some("emit exactly this JSON tool-call envelope"),
    });
}

#[test]
fn final_frame_contains_fs_mcp_tool_result() {
    run_final_frame_tool_scenario(ToolScenario {
        proof_slug: "fs_read_file",
        input_text: "read README through filesystem MCP",
        server_name: "sonar-filesystem",
        tool_name: "fs_read_file",
        arguments_summary: "{\"path\":\"D:/code/narada.sonar/README.md\",\"limit\":20}",
        result_summary: "content: README.md lines 1-20",
        result_text: "README.md first twenty lines are available.",
        expected_operator_line: "operator -> sonar.resident: read README through filesystem MCP",
        expected_tool_request_line: "sonar.resident -> agent-tui: fs_read_file({\"path\":\"D:/code/narada.sonar/README.md\",\"limit\":20})",
        expected_tool_result_line: "agent-tui -> sonar.resident: ok fs_read_file in 10ms · content: README.md lines 1-20",
        expected_final_agent_line: "sonar.resident: README.md first twenty lines are available.",
        expected_follow_up_guidance: None,
    });
}

#[test]
fn final_frame_contains_structured_command_mcp_tool_result() {
    run_final_frame_tool_scenario(ToolScenario {
        proof_slug: "structured_command_execute",
        input_text: "run cargo version through structured command MCP",
        server_name: "sonar-structured-command",
        tool_name: "structured_command_execute",
        arguments_summary: "{\"command\":\"cargo\",\"args\":[\"--version\"]}",
        result_summary: "exit_code=0 stdout=cargo 1.x",
        result_text: "cargo 1.x executed successfully.",
        expected_operator_line: "operator -> sonar.resident: run cargo version through structured command MCP",
        expected_tool_request_line: "sonar.resident -> agent-tui: structured_command_execute({\"command\":\"cargo\",\"args\":[\"--version\"]})",
        expected_tool_result_line: "agent-tui -> sonar.resident: ok structured_command_execute in 10ms · exit_code=0 stdout=cargo 1.x",
        expected_final_agent_line: "sonar.resident: cargo 1.x executed successfully.",
        expected_follow_up_guidance: None,
    });
}

fn run_final_frame_tool_scenario(scenario: ToolScenario) {
    let control_path = temp_path("control");
    let session_path = temp_path("session");
    append(&control_path, "");
    let context = evidence_context();
    let mut runtime = AgentTuiInteractiveRuntime::with_provider_adapter_tool_executor_and_state(
        "sonar.resident",
        "carrier_fixture_1",
        &control_path,
        &session_path,
        context.clone(),
        Box::new(ScenarioProvider {
            scenario,
            calls: Mutex::new(0),
        }),
        provider_tool_executor(&session_path, context, scenario),
        RuntimePostureState::disabled(),
    );
    let mut state = AgentTuiLoopState::default();
    let mut terminal = CapturingTerminalFrame::default();
    let mut input = ScriptedInput {
        outcomes: VecDeque::from_iter([
            TerminalInputTickOutcome::DraftEffect(
                narada_agent_tui::composer_draft::ComposerDraftEffect::SubmitRequested {
                    text: scenario.input_text.to_string(),
                },
            ),
            TerminalInputTickOutcome::NoInput,
            TerminalInputTickOutcome::NoInput,
            TerminalInputTickOutcome::NoInput,
        ]),
    };
    let mut clock = StepClock { index: 0 };

    let summary = run_injected_interactive_loop(
        &mut runtime,
        &mut state,
        &mut terminal,
        &mut input,
        &mut clock,
        4,
    )
    .expect("interactive loop runs with fake terminal, provider, and MCP");

    assert!(summary.final_drawn);
    let final_frame = terminal.frames.last().expect("final frame captured");
    assert_ordered_subsequence(
        final_frame,
        &[
            scenario.expected_operator_line,
            scenario.expected_tool_request_line,
            scenario.expected_tool_result_line,
            scenario.expected_final_agent_line,
            "agent-tui: completed",
        ],
    );
    write_guard_proof(scenario, final_frame);

    remove_file(control_path).ok();
    remove_file(session_path).ok();
}

fn provider_tool_executor(
    session_path: &Path,
    context: SessionEvidenceContext,
    scenario: ToolScenario,
) -> Box<dyn ProviderToolCallExecutor> {
    let mut runtime = McpRuntimeExecutionBridge::new(
        session_path,
        context,
        SuccessfulScenarioMcpExecutor { scenario },
    );
    runtime.mark_server_ready(scenario.server_name);
    Box::new(SupervisedProviderToolCallExecutor::new(
        mcp_fabric_client(scenario),
        mcp_boundary(scenario),
        runtime,
    ))
}

fn mcp_fabric_client(scenario: ToolScenario) -> McpFabricTransportClient {
    let config = json!({
        "site_id": "narada-sonar",
        "carrier": "agent-tui",
        "mcpServers": {
            scenario.server_name: {
                "transport": "stdio",
                "command": "node",
                "args": ["fake-mcp-server.mjs"],
                "target_site_root": "{site_root}",
                "tools": [scenario.tool_name]
            }
        }
    });
    McpFabricTransportClient::from_json_str("fixture.mcp.json", &config.to_string())
        .expect("fixture MCP fabric parses")
}

fn mcp_boundary(scenario: ToolScenario) -> McpFabricBoundary {
    McpFabricBoundary::admitted(McpFabricPolicy::from_allowed_tools(
        "D:/code/narada.sonar/.ai/mcp",
        "fixture.mcp.json:mcpServers",
        [scenario.tool_name],
    ))
}

fn evidence_context() -> SessionEvidenceContext {
    SessionEvidenceContext {
        carrier_session_id: "carrier_fixture_1".to_string(),
        agent_id: "sonar.resident".to_string(),
        site_id: "narada-sonar".to_string(),
        site_root: "D:/code/narada.sonar".to_string(),
    }
}

fn rendered_transcript_lines(rows: &[TranscriptRow], identity: &str) -> Vec<String> {
    rows.iter()
        .map(|row| format!("{}: {}", row_label(row, identity), row.text))
        .collect()
}

fn row_label(row: &TranscriptRow, identity: &str) -> String {
    match row.kind {
        TranscriptItemKind::ProviderToolCallRequest => format!("{identity} -> agent-tui"),
        TranscriptItemKind::ToolResultReceived => format!("agent-tui -> {identity}"),
        _ => match row.actor {
            TranscriptActor::Operator => format!("operator -> {identity}"),
            TranscriptActor::Agent => identity.to_string(),
            TranscriptActor::AgentTui => "agent-tui".to_string(),
            TranscriptActor::System => "system directive".to_string(),
            TranscriptActor::OperatorSteering => format!("operator steering -> {identity}"),
            TranscriptActor::OperatorDirective => format!("operator directive -> {identity}"),
            TranscriptActor::Provider => "provider".to_string(),
        },
    }
}

fn assert_ordered_subsequence(actual: &[String], expected: &[&str]) {
    let mut cursor = 0;
    for expected_line in expected {
        let Some(relative_index) = actual[cursor..]
            .iter()
            .position(|line| row_matches_expected(line, expected_line))
        else {
            panic!(
                "expected line not found after index {cursor}: {expected_line:?}\nactual frame:\n{}",
                actual
                    .iter()
                    .map(|line| format!("{line:?}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        };
        cursor += relative_index + 1;
    }
}

fn row_matches_expected(actual: &str, expected: &str) -> bool {
    actual == expected
        || actual
            .split('\n')
            .next()
            .is_some_and(|line| line == expected)
}

fn write_guard_proof(scenario: ToolScenario, final_frame: &[String]) {
    let proof_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!(
            "agent-tui-e2e-guard-proof-{}.json",
            scenario.proof_slug
        ));
    let proof = json!({
        "status": "passed",
        "test": scenario.proof_slug,
        "final_frame": final_frame,
    });
    write(proof_path, format!("{}\n", proof)).expect("write guard proof");
}

fn temp_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock works")
        .as_nanos();
    let counter = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "narada-agent-tui-e2e-{name}-{}-{unique}-{counter}.jsonl",
        std::process::id()
    ))
}

fn append(path: &Path, content: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open temp file");
    file.write_all(content.as_bytes())
        .expect("append temp file");
}
