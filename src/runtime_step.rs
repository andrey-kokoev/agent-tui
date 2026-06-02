use crate::input_queue::SessionEvidenceContext;
use crate::provider_dispatch::{ProviderAdapter, ProviderDispatchStub};
use crate::runtime_clock::RuntimeClock;
use crate::runtime_coordinator::{
    RuntimeCoordinator, RuntimeCoordinatorClock, RuntimeCoordinatorPollResult,
};
use crate::transcript_store::{TranscriptIngestSummary, TranscriptStore};
use crate::turn_coordinator::{
    CompletedTurn, NoopProviderToolCallExecutor, ProviderToolCallExecutor, TurnCoordinator,
    TurnCoordinatorClock,
};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RuntimeStepClock {
    pub input: RuntimeCoordinatorClock,
    pub turn: TurnCoordinatorClock,
}

pub type TranscriptStepIngestResult = TranscriptIngestSummary;

#[derive(Debug)]
pub struct RuntimeStepResult {
    pub poll: RuntimeCoordinatorPollResult,
    pub released_held: usize,
    pub completed_turn: Option<CompletedTurn>,
    pub transcript: TranscriptStepIngestResult,
}

pub struct RuntimeStep {
    runtime: RuntimeCoordinator,
    turns: TurnCoordinator,
    transcript: TranscriptStore,
    clock: RuntimeClock,
}

impl RuntimeStep {
    pub fn new(
        control_jsonl_path: impl Into<PathBuf>,
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
        clock: RuntimeClock,
    ) -> Self {
        Self::with_provider_adapter_and_tool_executor(
            control_jsonl_path,
            session_jsonl_path,
            evidence_context,
            clock,
            Box::new(ProviderDispatchStub::default()),
            Box::new(NoopProviderToolCallExecutor),
        )
    }

    pub fn with_provider_adapter(
        control_jsonl_path: impl Into<PathBuf>,
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
        clock: RuntimeClock,
        provider_adapter: Box<dyn ProviderAdapter>,
    ) -> Self {
        Self::with_provider_adapter_and_tool_executor(
            control_jsonl_path,
            session_jsonl_path,
            evidence_context,
            clock,
            provider_adapter,
            Box::new(NoopProviderToolCallExecutor),
        )
    }

    pub fn with_provider_adapter_and_tool_executor(
        control_jsonl_path: impl Into<PathBuf>,
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
        clock: RuntimeClock,
        provider_adapter: Box<dyn ProviderAdapter>,
        provider_tool_call_executor: Box<dyn ProviderToolCallExecutor>,
    ) -> Self {
        let session_jsonl_path = session_jsonl_path.into();
        Self {
            runtime: RuntimeCoordinator::new(
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
            clock,
        }
    }

    pub fn run_once(&mut self, composer_has_draft: bool) -> Result<RuntimeStepResult, String> {
        let clock = self.clock.next_step_clock();
        self.run_once_with_clock(composer_has_draft, &clock)
    }

    pub fn run_once_with_clock(
        &mut self,
        composer_has_draft: bool,
        clock: &RuntimeStepClock,
    ) -> Result<RuntimeStepResult, String> {
        let poll = self.runtime.poll_once(composer_has_draft, &clock.input)?;
        let released_held = if composer_has_draft {
            0
        } else {
            self.runtime
                .release_held_when_composer_clear(&clock.input)?
        };
        let completed_turn = self
            .turns
            .run_one_ready_turn(self.runtime.queue_mut(), &clock.turn)?;
        let transcript = self.ingest_transcript_from_session()?;

        Ok(RuntimeStepResult {
            poll,
            released_held,
            completed_turn,
            transcript,
        })
    }

    pub fn runtime(&self) -> &RuntimeCoordinator {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut RuntimeCoordinator {
        &mut self.runtime
    }

    pub fn transcript(&self) -> &TranscriptStore {
        &self.transcript
    }

    fn ingest_transcript_from_session(&mut self) -> Result<TranscriptStepIngestResult, String> {
        self.transcript
            .ingest_jsonl_file_summary(self.runtime.session_jsonl_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{
        SESSION_EVENT_SCHEMA, SessionEvent, SessionEventKind, parse_session_event,
    };
    use crate::provider_adapter_admission::ProviderAdapterKind;
    use crate::provider_adapter_contract::provider_adapter_contract;
    use crate::provider_dispatch::{ProviderOutputRecord, ScriptedProviderAdapter};
    use crate::provider_runtime_config::ProviderRuntimeConfig;
    use crate::session_jsonl::append_session_event;
    use crate::transcript_projection::{TranscriptActor, TranscriptItemKind};
    use crate::turn_coordinator::ProviderToolCallExecution;
    use crate::turn_coordinator::TurnCoordinatorClock;
    use serde_json::json;
    use std::fs::{OpenOptions, read_to_string, remove_file};
    use std::io::Write;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    const CONTROL_FIXTURE: &str =
        include_str!("../carrier-protocol/fixtures/control-input-event.json");
    static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path(name: &str) -> PathBuf {
        let unique = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "narada-agent-tui-step-{name}-{}-{unique}.jsonl",
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

    fn context() -> SessionEvidenceContext {
        SessionEvidenceContext {
            carrier_session_id: "carrier_fixture_1".to_string(),
            agent_id: "sonar.resident".to_string(),
            site_id: "narada-sonar".to_string(),
            site_root: "D:/code/narada.sonar".to_string(),
        }
    }

    fn runtime_clock() -> RuntimeClock {
        RuntimeClock::fixed("2026-05-30T00:00:02.000Z")
    }

    fn scripted_runtime_provider_adapter() -> ScriptedProviderAdapter {
        scripted_runtime_provider_adapter_with_outputs(vec![ProviderOutputRecord::text_delta(
            "turn_1",
            "runtime hello",
            1,
        )])
    }

    fn scripted_runtime_provider_adapter_with_outputs(
        outputs: Vec<ProviderOutputRecord>,
    ) -> ScriptedProviderAdapter {
        let provider_contract = provider_adapter_contract();
        let runtime_config =
            ProviderRuntimeConfig::from_env_map(&std::collections::BTreeMap::from([
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
            .expect("scripted runtime provider admits configured runtime")
    }

    struct RuntimeStepToolExecutor;

    impl ProviderToolCallExecutor for RuntimeStepToolExecutor {
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
                event_id: format!("{}_runtime_tool_request", clock.event_id_prefix),
                occurred_at: clock.occurred_at.clone(),
                carrier_session_id: context.carrier_session_id.clone(),
                agent_id: context.agent_id.clone(),
                site_id: context.site_id.clone(),
                site_root: context.site_root.clone(),
                payload: json!({
                    "tool_name": output.payload["tool_name"],
                    "arguments_summary": output.payload["arguments_summary"],
                    "requesting_agent_id": context.agent_id,
                    "runtime_step_tool_executor": "injected"
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
    fn run_once_polls_admits_drains_ready_turn_and_updates_transcript() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        append(&control_path, CONTROL_FIXTURE);
        append(&control_path, "\n");

        let mut step = RuntimeStep::new(&control_path, &session_path, context(), runtime_clock());
        let result = step.run_once(false).expect("runtime step succeeds");

        assert_eq!(result.poll.evidence_written, 1);
        assert_eq!(result.released_held, 0);
        assert!(result.completed_turn.is_some());
        assert_eq!(result.transcript.projected, 2);
        assert_eq!(result.transcript.ignored, 1);
        assert_eq!(result.transcript.duplicate, 1);
        assert_eq!(result.transcript.total_items, 2);
        assert_eq!(step.transcript().items()[0].actor, TranscriptActor::System);
        assert_eq!(
            step.transcript().items()[0].kind,
            TranscriptItemKind::InputAdmitted
        );
        assert_eq!(
            step.transcript().items()[1].kind,
            TranscriptItemKind::TurnTerminalStatus
        );

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(
            parse_session_event(lines[0])
                .expect("admission parses")
                .event_kind,
            SessionEventKind::InputAdmittedToTurn
        );
        assert_eq!(
            parse_session_event(lines[1])
                .expect("turn start parses")
                .event_kind,
            SessionEventKind::TurnStarted
        );
        assert_eq!(
            parse_session_event(lines[2])
                .expect("provider request parses")
                .event_kind,
            SessionEventKind::ProviderRequestRecorded
        );
        assert_eq!(
            parse_session_event(lines[3])
                .expect("turn complete parses")
                .event_kind,
            SessionEventKind::TurnCompleted
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn transcript_ingest_dedupes_prior_session_lines_across_runtime_steps() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        append(&control_path, CONTROL_FIXTURE);
        append(&control_path, "\n");

        let mut step = RuntimeStep::new(&control_path, &session_path, context(), runtime_clock());
        let first = step.run_once(false).expect("first runtime step succeeds");
        assert_eq!(first.transcript.projected, 2);
        assert_eq!(first.transcript.total_items, 2);

        let second = step.run_once(false).expect("second runtime step succeeds");
        assert_eq!(second.transcript.projected, 0);
        assert_eq!(second.transcript.ignored, 0);
        assert_eq!(second.transcript.duplicate, 4);
        assert_eq!(second.transcript.total_items, 2);

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn runtime_step_accepts_injected_provider_adapter() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        append(&control_path, CONTROL_FIXTURE);
        append(&control_path, "\n");

        let mut step = RuntimeStep::with_provider_adapter(
            &control_path,
            &session_path,
            context(),
            runtime_clock(),
            Box::new(scripted_runtime_provider_adapter()),
        );
        let result = step.run_once(false).expect("runtime step succeeds");
        assert!(result.completed_turn.is_some());
        assert_eq!(result.transcript.total_items, 3);

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 5);
        let provider_request = parse_session_event(lines[2]).expect("provider request parses");
        assert_eq!(
            provider_request.payload["provider_request_status"],
            "completed"
        );
        assert_eq!(provider_request.payload["provider_execution_enabled"], true);
        assert_eq!(
            provider_request.payload["provider_runtime_status"],
            "configured"
        );
        assert_eq!(
            provider_request.payload["provider_adapter_admission_status"],
            "admitted"
        );
        assert_eq!(
            provider_request.payload["provider_adapter_kind"],
            provider_adapter_contract().scripted_provider_adapter_kind
        );
        assert_eq!(
            parse_session_event(lines[3])
                .expect("provider text parses")
                .event_kind,
            SessionEventKind::ProviderTextDeltaRecorded
        );
        assert_eq!(
            parse_session_event(lines[4])
                .expect("turn complete parses")
                .payload["terminal_status"],
            "completed"
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }
    #[test]
    fn runtime_step_accepts_injected_provider_tool_call_executor() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        append(&control_path, CONTROL_FIXTURE);
        append(&control_path, "\n");

        let provider_adapter = scripted_runtime_provider_adapter_with_outputs(vec![
            ProviderOutputRecord::text_delta("turn_1", "runtime hello", 1),
            ProviderOutputRecord::tool_call_request("turn_1", "site_loop_run_once", "{}", 2),
        ]);
        let mut step = RuntimeStep::with_provider_adapter_and_tool_executor(
            &control_path,
            &session_path,
            context(),
            runtime_clock(),
            Box::new(provider_adapter),
            Box::new(RuntimeStepToolExecutor),
        );

        let result = step.run_once(false).expect("runtime step succeeds");
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
        assert_eq!(events[5].payload["runtime_step_tool_executor"], "injected");
        assert_eq!(events[6].event_kind, SessionEventKind::TurnCompleted);

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn run_once_holds_when_composer_has_draft_then_releases_on_next_clear_step() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        append(&control_path, CONTROL_FIXTURE);
        append(&control_path, "\n");

        let mut step = RuntimeStep::new(&control_path, &session_path, context(), runtime_clock());
        let first = step.run_once(true).expect("first step succeeds");
        assert_eq!(first.poll.evidence_written, 1);
        assert_eq!(first.released_held, 0);
        assert!(first.completed_turn.is_none());
        assert_eq!(first.transcript.projected, 1);
        assert_eq!(first.transcript.total_items, 1);
        assert_eq!(step.runtime().queue().held_count(), 1);

        let second = step.run_once(false).expect("second step succeeds");
        assert_eq!(second.poll.evidence_written, 0);
        assert_eq!(second.released_held, 1);
        assert!(second.completed_turn.is_some());
        assert_eq!(second.transcript.projected, 3);
        assert_eq!(second.transcript.duplicate, 1);
        assert_eq!(second.transcript.total_items, 4);

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 5);
        assert_eq!(
            parse_session_event(lines[0])
                .expect("held parses")
                .event_kind,
            SessionEventKind::SystemDirectiveHeld
        );
        assert_eq!(
            parse_session_event(lines[1])
                .expect("released parses")
                .event_kind,
            SessionEventKind::SystemDirectiveReleased
        );
        assert_eq!(
            parse_session_event(lines[2])
                .expect("started parses")
                .event_kind,
            SessionEventKind::TurnStarted
        );
        assert_eq!(
            parse_session_event(lines[3])
                .expect("provider request parses")
                .event_kind,
            SessionEventKind::ProviderRequestRecorded
        );
        assert_eq!(
            parse_session_event(lines[4])
                .expect("completed parses")
                .event_kind,
            SessionEventKind::TurnCompleted
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }
}
