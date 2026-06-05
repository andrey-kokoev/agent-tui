use crate::app_view_model::{AppViewInput, AppViewModel, build_app_view_with_transcript_options};
use crate::composer_draft::ComposerDraftState;
use crate::composer_view_model::ComposerViewInput;
use crate::input_queue::{SessionEvidenceContext, TurnState, elapsed_label_between};
use crate::layout_model::{LayoutConfig, TerminalSize};
use crate::mcp_runtime_config::McpRuntimeConfig;
use crate::provider_adapter_admission::ProviderAdapterAdmission;
use crate::provider_dispatch::ProviderAdapter;
use crate::provider_runtime_config::ProviderRuntimeConfig;
use crate::runtime_coordinator::{
    RuntimeCoordinator, RuntimeCoordinatorClock, RuntimeOperatorSubmitResult,
};
use crate::status_view_model::{ProviderRuntimeState, RuntimePostureState, StatusViewInput};
use crate::terminal_runtime_config::TerminalRuntimeConfig;
use crate::transcript_projection::{TranscriptItem, TranscriptItemKind};
use crate::transcript_store::{TranscriptIngestSummary, TranscriptStore};
use crate::transcript_view_model::TranscriptDisplayOptions;
use crate::turn_coordinator::{
    NoopProviderToolCallExecutor, ProviderToolCallExecutor, TurnCoordinator, TurnCoordinatorClock,
};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct InteractiveStepClock {
    pub input: RuntimeCoordinatorClock,
    pub turn: TurnCoordinatorClock,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InteractiveStepResult {
    pub control_evidence_written: usize,
    pub parse_errors: usize,
    pub released_held: usize,
    pub completed_turn: Option<CompletedTurnSummary>,
    pub transcript: TranscriptIngestSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedTurnSummary {
    pub turn_id: String,
    pub input_event_id: String,
    pub evidence_written: usize,
}

pub struct AgentTuiInteractiveRuntime {
    identity: String,
    session: String,
    coordinator: RuntimeCoordinator,
    turns: TurnCoordinator,
    transcript: TranscriptStore,
    runtime_posture: RuntimePostureState,
}

impl AgentTuiInteractiveRuntime {
    pub fn new(
        identity: impl Into<String>,
        session: impl Into<String>,
        control_jsonl_path: impl Into<PathBuf>,
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
    ) -> Self {
        Self::with_provider_adapter_and_state(
            identity,
            session,
            control_jsonl_path,
            session_jsonl_path,
            evidence_context,
            Box::new(crate::provider_dispatch::ProviderDispatchStub::default()),
            RuntimePostureState::disabled(),
        )
    }

    pub fn with_provider_runtime_config(
        identity: impl Into<String>,
        session: impl Into<String>,
        control_jsonl_path: impl Into<PathBuf>,
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
        provider_runtime_config: ProviderRuntimeConfig,
    ) -> Self {
        Self::with_runtime_configs(
            identity,
            session,
            control_jsonl_path,
            session_jsonl_path,
            evidence_context,
            provider_runtime_config,
            McpRuntimeConfig::disabled(),
            TerminalRuntimeConfig::disabled(),
        )
    }

    pub fn with_runtime_configs(
        identity: impl Into<String>,
        session: impl Into<String>,
        control_jsonl_path: impl Into<PathBuf>,
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
        provider_runtime_config: ProviderRuntimeConfig,
        mcp_runtime_config: McpRuntimeConfig,
        terminal_runtime_config: TerminalRuntimeConfig,
    ) -> Self {
        let provider_adapter_admission =
            ProviderAdapterAdmission::from_runtime_config(&provider_runtime_config, None);
        Self::with_provider_adapter_and_state(
            identity,
            session,
            control_jsonl_path,
            session_jsonl_path,
            evidence_context,
            Box::new(
                crate::provider_dispatch::ProviderDispatchStub::with_runtime_config(
                    provider_runtime_config.clone(),
                ),
            ),
            RuntimePostureState::from_runtime_configs(
                &provider_runtime_config,
                &provider_adapter_admission,
                &mcp_runtime_config,
                &terminal_runtime_config,
            ),
        )
    }

    pub fn with_provider_adapter(
        identity: impl Into<String>,
        session: impl Into<String>,
        control_jsonl_path: impl Into<PathBuf>,
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
        provider_adapter: Box<dyn ProviderAdapter>,
    ) -> Self {
        Self::with_provider_adapter_and_state(
            identity,
            session,
            control_jsonl_path,
            session_jsonl_path,
            evidence_context,
            provider_adapter,
            RuntimePostureState::disabled(),
        )
    }
    pub fn with_provider_adapter_and_state(
        identity: impl Into<String>,
        session: impl Into<String>,
        control_jsonl_path: impl Into<PathBuf>,
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
        provider_adapter: Box<dyn ProviderAdapter>,
        runtime_posture: RuntimePostureState,
    ) -> Self {
        Self::with_provider_adapter_tool_executor_and_state(
            identity,
            session,
            control_jsonl_path,
            session_jsonl_path,
            evidence_context,
            provider_adapter,
            Box::new(NoopProviderToolCallExecutor),
            runtime_posture,
        )
    }

    pub fn with_provider_adapter_tool_executor_and_state(
        identity: impl Into<String>,
        session: impl Into<String>,
        control_jsonl_path: impl Into<PathBuf>,
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
        provider_adapter: Box<dyn ProviderAdapter>,
        provider_tool_call_executor: Box<dyn ProviderToolCallExecutor>,
        runtime_posture: RuntimePostureState,
    ) -> Self {
        let session_jsonl_path = session_jsonl_path.into();
        Self {
            identity: identity.into(),
            session: session.into(),
            coordinator: RuntimeCoordinator::new(
                control_jsonl_path,
                session_jsonl_path.clone(),
                evidence_context.clone(),
            ),
            turns: TurnCoordinator::with_provider_adapter_and_tool_executor(
                session_jsonl_path,
                evidence_context,
                provider_adapter,
                provider_tool_call_executor,
            ),
            transcript: TranscriptStore::new(),
            runtime_posture,
        }
    }

    pub fn coordinator_mut(&mut self) -> &mut RuntimeCoordinator {
        &mut self.coordinator
    }

    pub fn record_interrupt_and_cancel_active_turn(
        &mut self,
        clock: &RuntimeCoordinatorClock,
    ) -> Result<bool, String> {
        self.coordinator.record_composer_interrupt(clock)?;
        Ok(self.turns.request_active_turn_cancel())
    }

    pub fn run_step(
        &mut self,
        draft: &ComposerDraftState,
        clock: &InteractiveStepClock,
    ) -> Result<InteractiveStepResult, String> {
        let composer_has_draft = !draft.text.trim().is_empty();
        let poll = self
            .coordinator
            .poll_once(composer_has_draft, &clock.input)?;
        let released_held = if composer_has_draft {
            0
        } else {
            self.coordinator
                .release_held_when_composer_clear(&clock.input)?
        };
        let completed_turn = self
            .turns
            .run_one_ready_turn(self.coordinator.queue_mut(), &clock.turn)?
            .map(|turn| CompletedTurnSummary {
                turn_id: turn.turn_id,
                input_event_id: turn.input_event_id,
                evidence_written: turn.evidence_written,
            });
        let transcript = self
            .transcript
            .ingest_jsonl_file_summary(self.coordinator.session_jsonl_path())?;

        Ok(InteractiveStepResult {
            control_evidence_written: poll.evidence_written,
            parse_errors: poll.parse_errors.len(),
            released_held,
            completed_turn,
            transcript,
        })
    }

    pub fn run_background_step(
        &mut self,
        draft: &ComposerDraftState,
        clock: &InteractiveStepClock,
    ) -> Result<InteractiveStepResult, String> {
        let composer_has_draft = !draft.text.trim().is_empty();
        let poll = self
            .coordinator
            .poll_once(composer_has_draft, &clock.input)?;
        let released_held = if composer_has_draft {
            0
        } else {
            self.coordinator
                .release_held_when_composer_clear(&clock.input)?
        };
        let completed_turn = self
            .turns
            .run_one_ready_turn_background_tick(self.coordinator.queue_mut(), &clock.turn)?
            .map(|turn| CompletedTurnSummary {
                turn_id: turn.turn_id,
                input_event_id: turn.input_event_id,
                evidence_written: turn.evidence_written,
            });
        let transcript = self
            .transcript
            .ingest_jsonl_file_summary(self.coordinator.session_jsonl_path())?;

        Ok(InteractiveStepResult {
            control_evidence_written: poll.evidence_written,
            parse_errors: poll.parse_errors.len(),
            released_held,
            completed_turn,
            transcript,
        })
    }

    pub fn ingest_transcript(&mut self) -> Result<TranscriptIngestSummary, String> {
        self.transcript
            .ingest_jsonl_file_summary(self.coordinator.session_jsonl_path())
    }

    pub fn clear_transcript_projection(&mut self) {
        self.transcript.clear_projection();
    }

    pub fn apply_operator_submit_result(
        &mut self,
        result: &RuntimeOperatorSubmitResult,
    ) -> Result<(), String> {
        match result {
            RuntimeOperatorSubmitResult::ModelChanged { value } => {
                self.turns.set_provider_model(Some(value.clone()));
            }
            RuntimeOperatorSubmitResult::ThinkingChanged { value } => {
                self.turns.set_provider_thinking(Some(value.clone()));
            }
            _ => {}
        }
        Ok(())
    }

    pub fn build_view(
        &self,
        terminal_size: TerminalSize,
        draft: &ComposerDraftState,
        last_error: Option<String>,
    ) -> AppViewModel {
        self.build_view_at(terminal_size, draft, last_error, None)
    }

    pub fn build_view_at(
        &self,
        terminal_size: TerminalSize,
        draft: &ComposerDraftState,
        last_error: Option<String>,
        now: Option<&str>,
    ) -> AppViewModel {
        let mut runtime_posture = self.runtime_posture.clone();
        if self.coordinator.queue().turn_state() == crate::input_queue::TurnState::Active {
            runtime_posture.provider_state = ProviderRuntimeState::Working;
        }
        build_app_view_with_transcript_options(
            &AppViewInput {
                terminal_size,
                layout_config: LayoutConfig::default(),
                transcript_items: self.transcript.items().to_vec(),
                status: StatusViewInput {
                    identity: self.identity.clone(),
                    session: self.session.clone(),
                    turn_state: self.coordinator.queue().turn_state(),
                    active_phase: now.and_then(|now| {
                        active_tool_call_phase(self.transcript.items(), now).or_else(|| {
                            active_provider_phase(
                                self.coordinator.queue().turn_state(),
                                self.coordinator.queue().active_turn_age_label(now),
                            )
                        })
                    }),
                    active_turn_age: now
                        .and_then(|now| self.coordinator.queue().active_turn_age_label(now)),
                    queued_inputs: self.coordinator.queue().queued_count(),
                    held_system_directives: self.coordinator.queue().held_count(),
                    oldest_held_age: now
                        .and_then(|now| self.coordinator.queue().oldest_held_age_label(now)),
                    transcript_items: self.transcript.len(),
                    runtime_posture,
                    last_error,
                },
                composer: ComposerViewInput {
                    identity: self.identity.clone(),
                    draft_text: draft.text.clone(),
                    turn_state: self.coordinator.queue().turn_state(),
                    queued_operator_notes: self.coordinator.queue().queued_count(),
                    held_system_directives: self.coordinator.queue().held_count(),
                },
            },
            TranscriptDisplayOptions {
                show_tool_outputs: self.coordinator.display_tool_outputs(),
            },
        )
    }
}

fn active_tool_call_phase(items: &[TranscriptItem], now: &str) -> Option<String> {
    let latest_tool_request = items.iter().rev().find(|item| {
        matches!(
            item.kind,
            TranscriptItemKind::ProviderToolCallRequest | TranscriptItemKind::ToolResultReceived
        )
    })?;
    if latest_tool_request.kind != TranscriptItemKind::ProviderToolCallRequest {
        return None;
    }
    let tool_name = latest_tool_request.text.split('(').next()?.trim();
    if tool_name.is_empty() {
        return None;
    }
    let age = latest_tool_request
        .occurred_at
        .as_deref()
        .and_then(|started_at| elapsed_label_between(started_at, now));
    Some(match age {
        Some(age) => format!("calling {tool_name} {age}"),
        None => format!("calling {tool_name}"),
    })
}

fn active_provider_phase(turn_state: TurnState, active_turn_age: Option<String>) -> Option<String> {
    if turn_state != TurnState::Active {
        return None;
    }
    Some(match active_turn_age {
        Some(age) => format!("thinking {age}"),
        None => "thinking".to_string(),
    })
}

impl From<crate::runtime_step::RuntimeStepClock> for InteractiveStepClock {
    fn from(value: crate::runtime_step::RuntimeStepClock) -> Self {
        Self {
            input: value.input,
            turn: value.turn,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{
        SESSION_EVENT_SCHEMA, SessionEvent, SessionEventKind, parse_session_event,
    };
    use crate::mcp_runtime_config::{
        McpRuntimeConfig, mcp_config_env_var, mcp_fabric_env_var, site_mcp_fabric_env_var,
    };
    use crate::provider_adapter_admission::ProviderAdapterKind;
    use crate::provider_adapter_contract::provider_adapter_contract;
    use crate::provider_dispatch::{ProviderOutputRecord, ScriptedProviderAdapter};
    use crate::provider_runtime_config::ProviderRuntimeConfig;
    use crate::session_jsonl::append_session_event;
    use crate::terminal_runtime_config::TerminalRuntimeConfig;
    use crate::terminal_runtime_contract::terminal_runtime_contract;
    use crate::turn_coordinator::ProviderToolCallExecution;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::fs::{OpenOptions, read_to_string, remove_file};
    use std::io::Write;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    const CONTROL_FIXTURE: &str =
        include_str!("../../narada/packages/carrier-protocol/fixtures/control-input-event.json");

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock works")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "narada-agent-tui-interactive-{name}-{unique}.jsonl"
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

    fn context() -> SessionEvidenceContext {
        SessionEvidenceContext {
            carrier_session_id: "carrier_fixture_1".to_string(),
            agent_id: "sonar.resident".to_string(),
            site_id: "narada-sonar".to_string(),
            site_root: "D:/code/narada.sonar".to_string(),
        }
    }

    fn clock() -> InteractiveStepClock {
        InteractiveStepClock {
            input: RuntimeCoordinatorClock {
                occurred_at: "2026-05-30T00:00:02.000Z".to_string(),
                event_id_prefix: "session_event_runtime".to_string(),
            },
            turn: TurnCoordinatorClock {
                occurred_at: "2026-05-30T00:00:02.000Z".to_string(),
                event_id_prefix: "session_event_turn".to_string(),
                turn_id_prefix: "turn".to_string(),
            },
        }
    }
    fn scripted_interactive_provider_adapter(
        outputs: Vec<ProviderOutputRecord>,
    ) -> ScriptedProviderAdapter {
        let provider_contract = provider_adapter_contract();
        let runtime_config = ProviderRuntimeConfig::from_env_map(&BTreeMap::from([
            (
                provider_contract.provider_execution_env_var.clone(),
                "true".to_string(),
            ),
            (
                provider_contract.intelligence_provider_env_var.clone(),
                provider_contract.admitted_providers[0].clone(),
            ),
            (
                provider_contract.ai_model_env_var.clone(),
                "gpt-5.5".to_string(),
            ),
        ]));
        ScriptedProviderAdapter::try_new(runtime_config, ProviderAdapterKind::Scripted, outputs)
            .expect("scripted interactive provider admits configured runtime")
    }

    #[test]
    fn slash_model_and_thinking_apply_to_later_provider_turn() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        append(&control_path, "");
        let provider_adapter =
            scripted_interactive_provider_adapter(vec![ProviderOutputRecord::text_delta(
                "turn_1", "ack", 1,
            )]);
        let mut runtime = AgentTuiInteractiveRuntime::with_provider_adapter(
            "sonar.resident",
            "carrier_fixture_1",
            &control_path,
            &session_path,
            context(),
            Box::new(provider_adapter),
        );
        let clock = clock();

        let model_result = runtime
            .coordinator_mut()
            .handle_operator_submit("/model gpt-5.5-mini".to_string(), &clock.input)
            .expect("model command handled");
        runtime
            .apply_operator_submit_result(&model_result)
            .expect("model command applied");
        let thinking_result = runtime
            .coordinator_mut()
            .handle_operator_submit("/thinking high".to_string(), &clock.input)
            .expect("thinking command handled");
        runtime
            .apply_operator_submit_result(&thinking_result)
            .expect("thinking command applied");
        runtime
            .coordinator_mut()
            .handle_operator_submit("run startup sequence".to_string(), &clock.input)
            .expect("operator input admitted");

        runtime
            .run_step(&ComposerDraftState::default(), &clock)
            .expect("interactive step succeeds")
            .completed_turn
            .expect("turn completed");

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let events = session_jsonl
            .lines()
            .map(|line| parse_session_event(line).expect("event parses"))
            .collect::<Vec<_>>();
        let provider_request = events
            .iter()
            .find(|event| event.event_kind == SessionEventKind::ProviderRequestRecorded)
            .expect("provider request recorded");
        assert_eq!(provider_request.payload["model"], "gpt-5.5-mini");
        assert_eq!(provider_request.payload["thinking"], "high");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    struct InteractiveToolExecutor;

    impl ProviderToolCallExecutor for InteractiveToolExecutor {
        fn handle_provider_output(
            &mut self,
            output: &ProviderOutputRecord,
            context: &SessionEvidenceContext,
            session_jsonl_path: &Path,
            clock: &TurnCoordinatorClock,
        ) -> Result<ProviderToolCallExecution, String> {
            if output.kind != crate::provider_dispatch::ProviderOutputKind::ToolCallRequest {
                return Ok(ProviderToolCallExecution::default());
            }
            let request = SessionEvent {
                schema: SESSION_EVENT_SCHEMA.to_string(),
                event_kind: SessionEventKind::ToolCallRequested,
                event_id: format!("{}_interactive_tool_request", clock.event_id_prefix),
                occurred_at: clock.occurred_at.clone(),
                carrier_session_id: context.carrier_session_id.clone(),
                agent_id: context.agent_id.clone(),
                site_id: context.site_id.clone(),
                site_root: context.site_root.clone(),
                payload: json!({
                    "tool_name": output.payload["tool_name"],
                    "arguments_summary": output.payload["arguments_summary"],
                    "requesting_agent_id": context.agent_id,
                    "interactive_tool_executor": "injected"
                }),
            };
            append_session_event(session_jsonl_path, &request)?;
            Ok(ProviderToolCallExecution {
                evidence_written: 1,
                follow_up_text: None,
            })
        }
    }

    #[test]
    fn step_polls_drains_and_ingests_transcript() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        append(&control_path, CONTROL_FIXTURE);
        append(&control_path, "\n");
        let mut runtime = AgentTuiInteractiveRuntime::new(
            "sonar.resident",
            "carrier_fixture_1",
            &control_path,
            &session_path,
            context(),
        );

        let result = runtime
            .run_step(&ComposerDraftState::default(), &clock())
            .expect("interactive step succeeds");

        assert_eq!(result.control_evidence_written, 1);
        assert!(result.completed_turn.is_some());
        assert_eq!(result.transcript.total_items, 2);
        let model = runtime.build_view(
            TerminalSize {
                width: 80,
                height: 20,
            },
            &ComposerDraftState::default(),
            None,
        );
        assert_eq!(model.transcript_rows.len(), 2);
        assert_eq!(model.transcript_rows[0].actor_label, "system");
        assert_eq!(model.transcript_rows[0].text, "run startup sequence");
        assert_eq!(model.transcript_rows[1].actor_label, "agent-tui");
        assert_eq!(model.transcript_rows[1].text, "completed_without_provider");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn step_accepts_injected_provider_tool_call_executor() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        append(&control_path, CONTROL_FIXTURE);
        append(&control_path, "\n");
        let provider_adapter = scripted_interactive_provider_adapter(vec![
            ProviderOutputRecord::text_delta("turn_1", "interactive hello", 1),
            ProviderOutputRecord::tool_call_request("turn_1", "site_loop_run_once", "{}", 2),
        ]);
        let mut runtime = AgentTuiInteractiveRuntime::with_provider_adapter_tool_executor_and_state(
            "sonar.resident",
            "carrier_fixture_1",
            &control_path,
            &session_path,
            context(),
            Box::new(provider_adapter),
            Box::new(InteractiveToolExecutor),
            RuntimePostureState::disabled(),
        );

        let result = runtime
            .run_step(&ComposerDraftState::default(), &clock())
            .expect("interactive step succeeds");
        assert_eq!(result.control_evidence_written, 1);
        assert_eq!(result.completed_turn.unwrap().evidence_written, 6);
        assert_eq!(result.transcript.total_items, 4);

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let events = session_jsonl
            .lines()
            .map(|line| parse_session_event(line).expect("event parses"))
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 7);
        assert_eq!(
            events[3].event_kind,
            SessionEventKind::ProviderTextDeltaRecorded
        );
        assert_eq!(
            events[4].event_kind,
            SessionEventKind::ProviderToolCallRequested
        );
        assert_eq!(events[5].event_kind, SessionEventKind::ToolCallRequested);
        assert_eq!(events[5].payload["interactive_tool_executor"], "injected");
        assert_eq!(events[6].event_kind, SessionEventKind::TurnCompleted);

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn active_tool_call_phase_reports_latest_unresolved_tool_age() {
        let items = vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderToolCallRequest,
            actor: crate::transcript_projection::TranscriptActor::AgentTui,
            turn_id: "turn_1".to_string(),
            text: "site_loop_run_once({})".to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:00:00.000Z".to_string()),
        }];

        assert_eq!(
            active_tool_call_phase(&items, "2026-05-30T00:00:08.000Z"),
            Some("calling site_loop_run_once 8s".to_string())
        );
    }

    #[test]
    fn active_provider_phase_reports_thinking_when_no_tool_call_is_active() {
        assert_eq!(
            active_provider_phase(TurnState::Active, Some("1m 47s".to_string())),
            Some("thinking 1m 47s".to_string())
        );
        assert_eq!(
            active_provider_phase(TurnState::Idle, Some("1m".to_string())),
            None
        );
    }

    #[test]
    fn active_tool_call_phase_clears_after_tool_result() {
        let items = vec![
            TranscriptItem {
                kind: TranscriptItemKind::ProviderToolCallRequest,
                actor: crate::transcript_projection::TranscriptActor::AgentTui,
                turn_id: "turn_1".to_string(),
                text: "site_loop_run_once({})".to_string(),
                sequence: Some(1),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:00:00.000Z".to_string()),
            },
            TranscriptItem {
                kind: TranscriptItemKind::ToolResultReceived,
                actor: crate::transcript_projection::TranscriptActor::AgentTui,
                turn_id: "turn_1".to_string(),
                text: "ok site_loop_run_once in 8ms".to_string(),
                sequence: None,
                projection_key: None,
                occurred_at: Some("2026-05-30T00:00:08.000Z".to_string()),
            },
        ];

        assert_eq!(
            active_tool_call_phase(&items, "2026-05-30T00:00:09.000Z"),
            None
        );
    }

    #[test]
    fn build_view_reports_configured_provider_mcp_and_terminal_state() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let provider_contract = provider_adapter_contract();
        let terminal_contract = terminal_runtime_contract();
        let provider_runtime_config = ProviderRuntimeConfig::from_env_map(&BTreeMap::from([
            (
                provider_contract.provider_execution_env_var.clone(),
                "true".to_string(),
            ),
            (
                provider_contract.intelligence_provider_env_var.clone(),
                provider_contract.admitted_providers[0].clone(),
            ),
            (
                provider_contract.ai_model_env_var.clone(),
                "gpt-5.5".to_string(),
            ),
        ]));
        let mcp_runtime_config = McpRuntimeConfig::from_env_map(&BTreeMap::from([
            (mcp_fabric_env_var().to_string(), "true".to_string()),
            (
                mcp_config_env_var().to_string(),
                "D:/site/.ai/mcp/config.json".to_string(),
            ),
            (
                site_mcp_fabric_env_var().to_string(),
                "D:/site/.ai/mcp".to_string(),
            ),
        ]));
        let terminal_runtime_config = TerminalRuntimeConfig::from_env_map(&BTreeMap::from([
            (
                terminal_contract.terminal_rendering_env_var.clone(),
                "true".to_string(),
            ),
            (
                terminal_contract.terminal_mode_env_var.clone(),
                terminal_contract.required_terminal_mode.clone(),
            ),
        ]));
        let runtime = AgentTuiInteractiveRuntime::with_runtime_configs(
            "sonar.resident",
            "carrier_fixture_1",
            &control_path,
            &session_path,
            context(),
            provider_runtime_config,
            mcp_runtime_config,
            terminal_runtime_config,
        );

        let model = runtime.build_view(
            TerminalSize {
                width: 80,
                height: 20,
            },
            &ComposerDraftState::default(),
            None,
        );

        assert!(!model.status.compact_line.contains("provider configured"));
        assert!(!model.status.compact_line.contains("mcp configured"));
        assert!(!model.status.compact_line.contains("terminal configured"));
        assert!(
            model
                .status
                .segments
                .iter()
                .any(|segment| segment.key == "provider_state" && segment.value == "configured")
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }
}
