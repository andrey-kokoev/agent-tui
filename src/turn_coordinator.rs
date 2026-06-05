use crate::carrier_protocol::{
    InputEvent, SESSION_EVENT_SCHEMA, SessionEvent, SessionEventKind, create_turn_terminal_payload,
};
use crate::input_queue::{InputQueue, SessionEvidenceContext};
use crate::provider_dispatch::{
    ProviderAdapter, ProviderCancellationToken, ProviderDispatchRecord, ProviderDispatchStatus,
    ProviderDispatchStub, ProviderOutputRecord, ProviderOutputSink,
};
use crate::session_jsonl::append_session_event;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

pub trait ProviderToolCallExecutor: Send {
    fn handle_provider_output(
        &mut self,
        output: &ProviderOutputRecord,
        context: &SessionEvidenceContext,
        session_jsonl_path: &Path,
        clock: &TurnCoordinatorClock,
    ) -> Result<ProviderToolCallExecution, String>;
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderToolCallExecution {
    pub evidence_written: usize,
    pub follow_up_text: Option<String>,
}

#[derive(Debug, Default)]
pub struct NoopProviderToolCallExecutor;

impl ProviderToolCallExecutor for NoopProviderToolCallExecutor {
    fn handle_provider_output(
        &mut self,
        _output: &ProviderOutputRecord,
        _context: &SessionEvidenceContext,
        _session_jsonl_path: &Path,
        _clock: &TurnCoordinatorClock,
    ) -> Result<ProviderToolCallExecution, String> {
        Ok(ProviderToolCallExecution::default())
    }
}
#[derive(Debug, Clone)]
pub struct TurnCoordinatorClock {
    pub occurred_at: String,
    pub event_id_prefix: String,
    pub turn_id_prefix: String,
}

#[derive(Debug)]
pub struct CompletedTurn {
    pub turn_id: String,
    pub input_event_id: String,
    pub evidence_written: usize,
}

pub struct TurnCoordinator {
    evidence_context: SessionEvidenceContext,
    session_jsonl_path: PathBuf,
    provider_adapter: Option<Box<dyn ProviderAdapter>>,
    provider_tool_call_executor: Option<Box<dyn ProviderToolCallExecutor>>,
    active_worker: Option<ActiveTurnWorker>,
    pending_session_model: Option<Option<String>>,
    pending_session_thinking: Option<Option<String>>,
    next_event_index: u64,
    next_turn_index: u64,
}

struct TurnWorkerResult {
    provider_adapter: Box<dyn ProviderAdapter>,
    provider_tool_call_executor: Box<dyn ProviderToolCallExecutor>,
    next_event_index: u64,
    result: Result<CompletedTurn, String>,
}

struct ActiveTurnWorker {
    receiver: Receiver<TurnWorkerResult>,
    cancellation: ProviderCancellationToken,
}

impl TurnCoordinator {
    pub fn new(
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
    ) -> Self {
        Self::with_provider_adapter(
            session_jsonl_path,
            evidence_context,
            Box::new(ProviderDispatchStub::default()),
        )
    }

    pub fn with_provider_adapter(
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
        provider_adapter: Box<dyn ProviderAdapter>,
    ) -> Self {
        Self::with_provider_adapter_and_tool_executor(
            session_jsonl_path,
            evidence_context,
            provider_adapter,
            Box::new(NoopProviderToolCallExecutor),
        )
    }

    pub fn with_provider_adapter_and_tool_executor(
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
        provider_adapter: Box<dyn ProviderAdapter>,
        provider_tool_call_executor: Box<dyn ProviderToolCallExecutor>,
    ) -> Self {
        Self {
            evidence_context,
            session_jsonl_path: session_jsonl_path.into(),
            provider_adapter: Some(provider_adapter),
            provider_tool_call_executor: Some(provider_tool_call_executor),
            active_worker: None,
            pending_session_model: None,
            pending_session_thinking: None,
            next_event_index: 1,
            next_turn_index: 1,
        }
    }

    pub fn session_jsonl_path(&self) -> &Path {
        &self.session_jsonl_path
    }

    pub fn set_provider_model(&mut self, model: Option<String>) {
        if let Some(provider_adapter) = self.provider_adapter.as_mut() {
            provider_adapter.set_session_model(model);
            self.pending_session_model = None;
        } else {
            self.pending_session_model = Some(model);
        }
    }

    pub fn set_provider_thinking(&mut self, thinking: Option<String>) {
        if let Some(provider_adapter) = self.provider_adapter.as_mut() {
            provider_adapter.set_session_thinking(thinking);
            self.pending_session_thinking = None;
        } else {
            self.pending_session_thinking = Some(thinking);
        }
    }

    fn apply_pending_provider_settings(&mut self) {
        let Some(provider_adapter) = self.provider_adapter.as_mut() else {
            return;
        };
        if let Some(model) = self.pending_session_model.take() {
            provider_adapter.set_session_model(model);
        }
        if let Some(thinking) = self.pending_session_thinking.take() {
            provider_adapter.set_session_thinking(thinking);
        }
    }
    pub fn run_one_ready_turn(
        &mut self,
        queue: &mut InputQueue,
        clock: &TurnCoordinatorClock,
    ) -> Result<Option<CompletedTurn>, String> {
        let Some(input) = queue.next_ready_input() else {
            return Ok(None);
        };

        queue.set_turn_active_at(clock.occurred_at.clone());
        let turn_id = self.next_turn_id(clock);
        let start = self.turn_started_event(&input, &turn_id, clock);
        self.write_evidence(&start)?;

        let mut provider_adapter = self
            .provider_adapter
            .take()
            .ok_or_else(|| "turn_provider_adapter_unavailable".to_string())?;
        let mut provider_tool_call_executor = self
            .provider_tool_call_executor
            .take()
            .ok_or_else(|| "turn_provider_tool_call_executor_unavailable".to_string())?;
        let cancellation = ProviderCancellationToken::new();
        let result = run_turn_worker_inner(
            provider_adapter.as_mut(),
            provider_tool_call_executor.as_mut(),
            &self.evidence_context,
            &self.session_jsonl_path,
            input,
            turn_id,
            clock,
            &mut self.next_event_index,
            &cancellation,
        );
        self.provider_adapter = Some(provider_adapter);
        self.apply_pending_provider_settings();
        self.provider_tool_call_executor = Some(provider_tool_call_executor);
        queue.set_turn_idle();
        result.map(Some)
    }

    pub fn run_one_ready_turn_background_tick(
        &mut self,
        queue: &mut InputQueue,
        clock: &TurnCoordinatorClock,
    ) -> Result<Option<CompletedTurn>, String> {
        if let Some(active_worker) = self.active_worker.take() {
            match active_worker.receiver.try_recv() {
                Ok(worker_result) => {
                    self.provider_adapter = Some(worker_result.provider_adapter);
                    self.apply_pending_provider_settings();
                    self.provider_tool_call_executor =
                        Some(worker_result.provider_tool_call_executor);
                    self.next_event_index = worker_result.next_event_index;
                    queue.set_turn_idle();
                    return worker_result.result.map(Some);
                }
                Err(TryRecvError::Empty) => {
                    self.active_worker = Some(active_worker);
                    return Ok(None);
                }
                Err(TryRecvError::Disconnected) => {
                    queue.set_turn_idle();
                    return Err("turn_worker_disconnected".to_string());
                }
            }
        }

        let Some(input) = queue.next_ready_input() else {
            return Ok(None);
        };
        queue.set_turn_active_at(clock.occurred_at.clone());
        let turn_id = self.next_turn_id(clock);
        let start = self.turn_started_event(&input, &turn_id, clock);
        self.write_evidence(&start)?;

        let provider_adapter = self
            .provider_adapter
            .take()
            .ok_or_else(|| "turn_provider_adapter_unavailable".to_string())?;
        let provider_tool_call_executor = self
            .provider_tool_call_executor
            .take()
            .ok_or_else(|| "turn_provider_tool_call_executor_unavailable".to_string())?;
        let context = self.evidence_context.clone();
        let session_jsonl_path = self.session_jsonl_path.clone();
        let worker_clock = clock.clone();
        let worker_next_event_index = self.next_event_index;
        let cancellation = ProviderCancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = run_turn_worker(
                provider_adapter,
                provider_tool_call_executor,
                context,
                session_jsonl_path,
                input,
                turn_id,
                worker_clock,
                worker_next_event_index,
                worker_cancellation,
            );
            let _ = sender.send(result);
        });
        self.active_worker = Some(ActiveTurnWorker {
            receiver,
            cancellation,
        });
        Ok(None)
    }

    pub fn request_active_turn_cancel(&mut self) -> bool {
        let Some(active_worker) = &self.active_worker else {
            return false;
        };
        active_worker.cancellation.cancel();
        true
    }

    fn turn_started_event(
        &mut self,
        input: &InputEvent,
        turn_id: &str,
        clock: &TurnCoordinatorClock,
    ) -> SessionEvent {
        self.session_event(
            SessionEventKind::TurnStarted,
            clock,
            json!({
                "turn_id": turn_id,
                "input_event_id": input.event_id,
                "source_kind": input.source_kind,
                "source_id": input.source_id,
                "directive_id": input.directive_id,
                "content_preview": input.content.chars().take(240).collect::<String>()
            }),
        )
    }
    fn session_event(
        &mut self,
        kind: SessionEventKind,
        clock: &TurnCoordinatorClock,
        payload: serde_json::Value,
    ) -> SessionEvent {
        session_event_with_index(
            &self.evidence_context,
            kind,
            clock,
            payload,
            &mut self.next_event_index,
        )
    }

    fn next_turn_id(&mut self, clock: &TurnCoordinatorClock) -> String {
        let id = format!("{}_{}", clock.turn_id_prefix, self.next_turn_index);
        self.next_turn_index += 1;
        id
    }

    fn write_evidence(&self, event: &SessionEvent) -> Result<(), String> {
        append_session_event(&self.session_jsonl_path, event)
    }
}

fn run_turn_worker(
    mut provider_adapter: Box<dyn ProviderAdapter>,
    mut provider_tool_call_executor: Box<dyn ProviderToolCallExecutor>,
    evidence_context: SessionEvidenceContext,
    session_jsonl_path: PathBuf,
    input: InputEvent,
    turn_id: String,
    clock: TurnCoordinatorClock,
    mut next_event_index: u64,
    cancellation: ProviderCancellationToken,
) -> TurnWorkerResult {
    let result = run_turn_worker_inner(
        provider_adapter.as_mut(),
        provider_tool_call_executor.as_mut(),
        &evidence_context,
        &session_jsonl_path,
        input,
        turn_id,
        &clock,
        &mut next_event_index,
        &cancellation,
    );
    TurnWorkerResult {
        provider_adapter,
        provider_tool_call_executor,
        next_event_index,
        result,
    }
}
fn run_turn_worker_inner(
    provider_adapter: &mut dyn ProviderAdapter,
    provider_tool_call_executor: &mut dyn ProviderToolCallExecutor,
    evidence_context: &SessionEvidenceContext,
    session_jsonl_path: &Path,
    input: InputEvent,
    turn_id: String,
    clock: &TurnCoordinatorClock,
    next_event_index: &mut u64,
    cancellation: &ProviderCancellationToken,
) -> Result<CompletedTurn, String> {
    const MAX_PROVIDER_FOLLOW_UP_ROUNDS: usize = 8;
    let mut current_input = input.clone();
    let mut output_evidence_count = 0usize;
    let mut final_provider_record: Option<ProviderDispatchRecord> = None;

    for round in 0..=MAX_PROVIDER_FOLLOW_UP_ROUNDS {
        let provider_request_written = if let Some(start_record) =
            provider_adapter.dispatch_start_record(&current_input, &turn_id)
        {
            let provider_request = session_event_with_index(
                evidence_context,
                SessionEventKind::ProviderRequestRecorded,
                clock,
                start_record.payload.clone(),
                next_event_index,
            );
            append_session_event(session_jsonl_path, &provider_request)?;
            true
        } else {
            false
        };

        let mut streaming_sink = TurnProviderOutputSink {
            evidence_context,
            session_jsonl_path,
            clock,
            next_event_index,
            provider_tool_call_executor,
            evidence_written: 0,
            follow_up_texts: Vec::new(),
        };
        let provider_record = provider_adapter.dispatch_request_streaming(
            &current_input,
            &turn_id,
            cancellation,
            &mut streaming_sink,
        );
        output_evidence_count += streaming_sink.evidence_written;
        let mut follow_up_texts = std::mem::take(&mut streaming_sink.follow_up_texts);
        drop(streaming_sink);

        if !provider_request_written {
            let provider_request = session_event_with_index(
                evidence_context,
                SessionEventKind::ProviderRequestRecorded,
                clock,
                provider_record.payload.clone(),
                next_event_index,
            );
            append_session_event(session_jsonl_path, &provider_request)?;
        }
        if round > 0 {
            output_evidence_count += 1;
        }
        for output in &provider_record.outputs {
            let output_event = session_event_with_index(
                evidence_context,
                output.kind.session_event_kind(),
                clock,
                output.payload.clone(),
                next_event_index,
            );
            append_session_event(session_jsonl_path, &output_event)?;
            output_evidence_count += 1;
            let tool_execution = provider_tool_call_executor.handle_provider_output(
                output,
                evidence_context,
                session_jsonl_path,
                clock,
            )?;
            output_evidence_count += tool_execution.evidence_written;
            if let Some(text) = tool_execution.follow_up_text {
                follow_up_texts.push(text);
            }
        }

        let has_follow_up = !follow_up_texts.is_empty()
            && matches!(provider_record.status, ProviderDispatchStatus::Completed);
        final_provider_record = Some(provider_record);
        if has_follow_up && round == MAX_PROVIDER_FOLLOW_UP_ROUNDS {
            return Err(format!(
                "provider_follow_up_round_limit_exceeded:{MAX_PROVIDER_FOLLOW_UP_ROUNDS}"
            ));
        }
        if !has_follow_up {
            break;
        }
        current_input = provider_follow_up_input(&input, round + 1, &follow_up_texts);
    }

    let provider_record = final_provider_record
        .ok_or_else(|| "turn_provider_follow_up_record_missing".to_string())?;
    let (terminal_kind, terminal_status) =
        provider_status_to_turn_terminal(&provider_record.status);
    let error_summary =
        provider_terminal_error_summary(&terminal_kind, terminal_status, &provider_record);
    let terminal = session_event_with_index(
        evidence_context,
        terminal_kind,
        clock,
        create_turn_terminal_payload(
            &turn_id,
            Some(&input.event_id),
            provider_record.status.as_str(),
            terminal_status,
            provider_record.provider_execution_enabled,
            error_summary.as_deref(),
        ),
        next_event_index,
    );
    append_session_event(session_jsonl_path, &terminal)?;
    Ok(CompletedTurn {
        turn_id,
        input_event_id: input.event_id,
        evidence_written: 3 + output_evidence_count,
    })
}

fn provider_follow_up_input(
    original: &InputEvent,
    round: usize,
    follow_up_texts: &[String],
) -> InputEvent {
    let tool_results = follow_up_texts.join("\n\n");
    let mut follow_up = original.clone();
    follow_up.event_id = format!("{}_provider_follow_up_{round}", original.event_id);
    follow_up.content = format!(
        "Original operator input:\n{}\n\nMCP tool result(s):\n{}\n\nContinue the same turn. Use the tool result(s) to answer the operator. Do not repeat the same tool call unless another tool call is required by the result.",
        original.content.trim(),
        tool_results.trim()
    );
    follow_up
}

struct TurnProviderOutputSink<'a> {
    evidence_context: &'a SessionEvidenceContext,
    session_jsonl_path: &'a Path,
    clock: &'a TurnCoordinatorClock,
    next_event_index: &'a mut u64,
    provider_tool_call_executor: &'a mut dyn ProviderToolCallExecutor,
    evidence_written: usize,
    follow_up_texts: Vec<String>,
}

impl ProviderOutputSink for TurnProviderOutputSink<'_> {
    fn emit_provider_output(&mut self, output: ProviderOutputRecord) -> Result<(), String> {
        let output_event = session_event_with_index(
            self.evidence_context,
            output.kind.session_event_kind(),
            self.clock,
            output.payload.clone(),
            self.next_event_index,
        );
        append_session_event(self.session_jsonl_path, &output_event)?;
        self.evidence_written += 1;
        let tool_execution = self.provider_tool_call_executor.handle_provider_output(
            &output,
            self.evidence_context,
            self.session_jsonl_path,
            self.clock,
        )?;
        self.evidence_written += tool_execution.evidence_written;
        if let Some(text) = tool_execution.follow_up_text {
            self.follow_up_texts.push(text);
        }
        Ok(())
    }
}

fn session_event_with_index(
    context: &SessionEvidenceContext,
    kind: SessionEventKind,
    clock: &TurnCoordinatorClock,
    payload: serde_json::Value,
    next_event_index: &mut u64,
) -> SessionEvent {
    let event_id = format!("{}_{}", clock.event_id_prefix, *next_event_index);
    *next_event_index += 1;
    SessionEvent {
        schema: SESSION_EVENT_SCHEMA.to_string(),
        event_kind: kind,
        event_id,
        occurred_at: clock.occurred_at.clone(),
        carrier_session_id: context.carrier_session_id.clone(),
        agent_id: context.agent_id.clone(),
        site_id: context.site_id.clone(),
        site_root: context.site_root.clone(),
        payload,
    }
}

fn provider_status_to_turn_terminal(
    status: &ProviderDispatchStatus,
) -> (SessionEventKind, &'static str) {
    match status {
        ProviderDispatchStatus::RecordedNotDispatched => (
            SessionEventKind::TurnCompleted,
            "completed_without_provider",
        ),
        ProviderDispatchStatus::Dispatched => {
            (SessionEventKind::TurnCompleted, "completed_after_dispatch")
        }
        ProviderDispatchStatus::Completed => (SessionEventKind::TurnCompleted, "completed"),
        ProviderDispatchStatus::Failed => (SessionEventKind::TurnFailed, "failed"),
        ProviderDispatchStatus::Interrupted => (SessionEventKind::TurnInterrupted, "interrupted"),
    }
}

fn provider_terminal_error_summary(
    terminal_kind: &SessionEventKind,
    terminal_status: &str,
    record: &ProviderDispatchRecord,
) -> Option<String> {
    match terminal_kind {
        SessionEventKind::TurnFailed | SessionEventKind::TurnInterrupted => record
            .payload
            .get("error_summary")
            .and_then(|value| value.as_str())
            .filter(|summary| !summary.trim().is_empty())
            .map(str::to_string)
            .or_else(|| {
                (*terminal_kind == SessionEventKind::TurnFailed)
                    .then(|| terminal_status.to_string())
            }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{
        DeliveryMode, TURN_TERMINAL_PAYLOAD_SCHEMA, create_provider_request_payload,
        parse_input_event, parse_session_event,
    };
    use crate::input_queue::TurnState;
    use crate::provider_adapter_admission::ProviderAdapterKind;
    use crate::provider_adapter_contract::provider_adapter_contract;
    use crate::provider_dispatch::{
        ProviderDispatchRecord, ProviderOutputRecord, ProviderOutputSink, ScriptedProviderAdapter,
    };
    use crate::provider_runtime_config::ProviderRuntimeConfig;
    use serde_json::json;
    use std::fs::{read_to_string, remove_file};
    use std::sync::{Mutex, mpsc};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    const INPUT_FIXTURE: &str = include_str!("../carrier-protocol/fixtures/input-event.json");

    fn temp_session_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock works")
            .as_nanos();
        std::env::temp_dir().join(format!("narada-agent-tui-turn-{unique}.jsonl"))
    }

    fn context() -> SessionEvidenceContext {
        SessionEvidenceContext {
            carrier_session_id: "carrier_fixture_1".to_string(),
            agent_id: "sonar.resident".to_string(),
            site_id: "narada-sonar".to_string(),
            site_root: "D:/code/narada.sonar".to_string(),
        }
    }

    fn clock() -> TurnCoordinatorClock {
        TurnCoordinatorClock {
            occurred_at: "2026-05-30T00:00:04.000Z".to_string(),
            event_id_prefix: "session_event_turn".to_string(),
            turn_id_prefix: "turn".to_string(),
        }
    }

    struct RecordingProviderAdapter;

    impl ProviderAdapter for RecordingProviderAdapter {
        fn dispatch_request(
            &self,
            input: &InputEvent,
            turn_id: &str,
            _cancellation: &ProviderCancellationToken,
        ) -> ProviderDispatchRecord {
            ProviderDispatchRecord {
                status: ProviderDispatchStatus::RecordedNotDispatched,
                provider_execution_enabled: false,
                payload: create_provider_request_payload(
                    turn_id,
                    &input.event_id,
                    "test_adapter_recorded",
                    false,
                    "configured",
                    "configured_without_adapter",
                    None,
                    None,
                    None,
                    None,
                    false,
                    "not_requested",
                    Some("test_adapter_recorded".to_string()),
                    &input.content,
                ),
                outputs: Vec::new(),
            }
        }
    }

    struct AlwaysToolProviderAdapter;

    impl ProviderAdapter for AlwaysToolProviderAdapter {
        fn dispatch_request(
            &self,
            input: &InputEvent,
            turn_id: &str,
            _cancellation: &ProviderCancellationToken,
        ) -> ProviderDispatchRecord {
            ProviderDispatchRecord {
                status: ProviderDispatchStatus::Completed,
                provider_execution_enabled: true,
                payload: create_provider_request_payload(
                    turn_id,
                    &input.event_id,
                    "completed",
                    true,
                    "configured",
                    "admitted",
                    Some("always_tool_adapter".to_string()),
                    None,
                    None,
                    None,
                    true,
                    "single_provider_output_batch",
                    None,
                    &input.content,
                ),
                outputs: vec![ProviderOutputRecord::tool_call_request(
                    turn_id,
                    "site_loop_run_once",
                    "{}",
                    1,
                )],
            }
        }
    }

    struct BlockingProviderAdapter {
        release: mpsc::Receiver<()>,
    }

    impl ProviderAdapter for BlockingProviderAdapter {
        fn dispatch_request(
            &self,
            input: &InputEvent,
            turn_id: &str,
            cancellation: &ProviderCancellationToken,
        ) -> ProviderDispatchRecord {
            while !cancellation.is_cancelled() {
                if self.release.try_recv().is_ok() {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            if cancellation.is_cancelled() {
                let mut payload = create_provider_request_payload(
                    turn_id,
                    &input.event_id,
                    "interrupted",
                    true,
                    "configured",
                    "admitted",
                    Some("test_blocking_adapter".to_string()),
                    None,
                    None,
                    None,
                    false,
                    "single_provider_output_batch",
                    None,
                    &input.content,
                );
                payload["error_summary"] = json!("provider_cancelled");
                return ProviderDispatchRecord {
                    status: ProviderDispatchStatus::Interrupted,
                    provider_execution_enabled: true,
                    payload,
                    outputs: Vec::new(),
                };
            }
            ProviderDispatchRecord {
                status: ProviderDispatchStatus::RecordedNotDispatched,
                provider_execution_enabled: false,
                payload: create_provider_request_payload(
                    turn_id,
                    &input.event_id,
                    "test_blocking_adapter_recorded",
                    false,
                    "configured",
                    "configured_without_adapter",
                    None,
                    None,
                    None,
                    None,
                    false,
                    "not_requested",
                    Some("test_blocking_adapter_recorded".to_string()),
                    &input.content,
                ),
                outputs: Vec::new(),
            }
        }
    }

    struct StreamingBlockingProviderAdapter {
        first_emitted: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }

    impl ProviderAdapter for StreamingBlockingProviderAdapter {
        fn dispatch_start_record(
            &self,
            input: &InputEvent,
            turn_id: &str,
        ) -> Option<ProviderDispatchRecord> {
            let mut record = completed_streaming_record(input, turn_id);
            record.status = ProviderDispatchStatus::Dispatched;
            record.payload["provider_request_status"] = json!("dispatched");
            Some(record)
        }

        fn dispatch_request(
            &self,
            input: &InputEvent,
            turn_id: &str,
            _cancellation: &ProviderCancellationToken,
        ) -> ProviderDispatchRecord {
            completed_streaming_record(input, turn_id)
        }

        fn dispatch_request_streaming(
            &self,
            input: &InputEvent,
            turn_id: &str,
            _cancellation: &ProviderCancellationToken,
            sink: &mut dyn ProviderOutputSink,
        ) -> ProviderDispatchRecord {
            sink.emit_provider_output(ProviderOutputRecord::text_delta(turn_id, "hello", 1))
                .expect("first streaming delta writes");
            self.first_emitted
                .send(())
                .expect("streaming test observes first delta");
            self.release
                .recv()
                .expect("streaming test releases provider");
            sink.emit_provider_output(ProviderOutputRecord::text_delta(turn_id, " world", 2))
                .expect("second streaming delta writes");
            completed_streaming_record(input, turn_id)
        }
    }

    fn completed_streaming_record(input: &InputEvent, turn_id: &str) -> ProviderDispatchRecord {
        ProviderDispatchRecord {
            status: ProviderDispatchStatus::Completed,
            provider_execution_enabled: true,
            payload: create_provider_request_payload(
                turn_id,
                &input.event_id,
                "completed",
                true,
                "configured",
                "admitted",
                Some("test_streaming_adapter".to_string()),
                None,
                None,
                None,
                true,
                "streaming_text_delta_events",
                None,
                &input.content,
            ),
            outputs: Vec::new(),
        }
    }

    struct WritingProviderToolCallExecutor;

    impl ProviderToolCallExecutor for WritingProviderToolCallExecutor {
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
                event_id: format!("{}_tool_request", clock.event_id_prefix),
                occurred_at: clock.occurred_at.clone(),
                carrier_session_id: context.carrier_session_id.clone(),
                agent_id: context.agent_id.clone(),
                site_id: context.site_id.clone(),
                site_root: context.site_root.clone(),
                payload: json!({
                    "tool_name": output.payload["tool_name"],
                    "arguments_summary": output.payload["arguments_summary"],
                    "requesting_agent_id": context.agent_id
                }),
            };
            let result = SessionEvent {
                schema: SESSION_EVENT_SCHEMA.to_string(),
                event_kind: SessionEventKind::ToolResultReceived,
                event_id: format!("{}_tool_result", clock.event_id_prefix),
                occurred_at: clock.occurred_at.clone(),
                carrier_session_id: context.carrier_session_id.clone(),
                agent_id: context.agent_id.clone(),
                site_id: context.site_id.clone(),
                site_root: context.site_root.clone(),
                payload: json!({
                    "tool_name": output.payload["tool_name"],
                    "status": "ok",
                    "duration_ms": 0,
                    "result_summary": "ok"
                }),
            };
            append_session_event(session_jsonl_path, &request)?;
            append_session_event(session_jsonl_path, &result)?;
            Ok(ProviderToolCallExecution {
                evidence_written: 2,
                follow_up_text: Some(
                    "Tool result for site_loop_run_once from test: ok.\nok".to_string(),
                ),
            })
        }
    }
    fn scripted_output_provider_adapter() -> ScriptedProviderAdapter {
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
        ScriptedProviderAdapter::try_new(
            runtime_config,
            ProviderAdapterKind::Scripted,
            vec![
                ProviderOutputRecord::text_delta("turn_1", "hello", 1),
                ProviderOutputRecord::tool_call_request("turn_1", "site_loop_run_once", "{}", 2),
            ],
        )
        .expect("scripted provider adapter admits configured runtime")
    }

    struct ToolThenFinalProviderAdapter {
        calls: Mutex<usize>,
    }

    impl ToolThenFinalProviderAdapter {
        fn new() -> Self {
            Self {
                calls: Mutex::new(0),
            }
        }
    }

    impl ProviderAdapter for ToolThenFinalProviderAdapter {
        fn dispatch_request(
            &self,
            input: &InputEvent,
            turn_id: &str,
            _cancellation: &ProviderCancellationToken,
        ) -> ProviderDispatchRecord {
            let mut calls = self.calls.lock().expect("call count lock works");
            *calls += 1;
            let payload = create_provider_request_payload(
                turn_id,
                &input.event_id,
                "completed",
                true,
                "configured",
                "admitted",
                Some("test_tool_then_final_adapter".to_string()),
                None,
                None,
                None,
                true,
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
                        "site_loop_run_once",
                        "{}",
                        1,
                    )],
                };
            }
            assert!(
                input.content.contains("MCP tool result(s):")
                    && input
                        .content
                        .contains("Tool result for site_loop_run_once from test: ok."),
                "follow-up provider input must include MCP tool result text"
            );
            ProviderDispatchRecord {
                status: ProviderDispatchStatus::Completed,
                provider_execution_enabled: true,
                payload,
                outputs: vec![ProviderOutputRecord::text_delta(
                    turn_id,
                    "Final answer from tool result.",
                    1,
                )],
            }
        }
    }

    #[test]
    fn maps_provider_status_to_turn_terminal_event() {
        assert_eq!(
            provider_status_to_turn_terminal(&ProviderDispatchStatus::RecordedNotDispatched),
            (
                SessionEventKind::TurnCompleted,
                "completed_without_provider"
            )
        );
        assert_eq!(
            provider_status_to_turn_terminal(&ProviderDispatchStatus::Dispatched),
            (SessionEventKind::TurnCompleted, "completed_after_dispatch")
        );
        assert_eq!(
            provider_status_to_turn_terminal(&ProviderDispatchStatus::Completed),
            (SessionEventKind::TurnCompleted, "completed")
        );
        assert_eq!(
            provider_status_to_turn_terminal(&ProviderDispatchStatus::Failed),
            (SessionEventKind::TurnFailed, "failed")
        );
        assert_eq!(
            provider_status_to_turn_terminal(&ProviderDispatchStatus::Interrupted),
            (SessionEventKind::TurnInterrupted, "interrupted")
        );
    }

    #[test]
    fn returns_none_when_no_ready_input_exists() {
        let path = temp_session_path();
        let mut queue = InputQueue::new();
        let mut coordinator = TurnCoordinator::new(&path, context());

        let result = coordinator
            .run_one_ready_turn(&mut queue, &clock())
            .expect("turn check succeeds");
        assert!(result.is_none());
        assert_eq!(queue.turn_state(), TurnState::Idle);

        remove_file(path).ok();
    }

    #[test]
    fn errors_when_provider_follow_up_round_limit_is_exceeded() {
        let path = temp_session_path();
        let mut input = parse_input_event(INPUT_FIXTURE).expect("input parses");
        input.delivery_mode = DeliveryMode::AdmitAfterActiveTurn;
        let mut queue = InputQueue::new();
        queue.admit_input_event(input, false);
        let mut coordinator = TurnCoordinator::with_provider_adapter_and_tool_executor(
            &path,
            context(),
            Box::new(AlwaysToolProviderAdapter),
            Box::new(WritingProviderToolCallExecutor),
        );

        let error = coordinator
            .run_one_ready_turn(&mut queue, &clock())
            .expect_err("follow-up loop limit should fail explicitly");

        assert_eq!(error, "provider_follow_up_round_limit_exceeded:8");
        remove_file(path).ok();
    }

    #[test]
    fn accepts_injected_provider_adapter() {
        let path = temp_session_path();
        let mut input = parse_input_event(INPUT_FIXTURE).expect("input parses");
        input.delivery_mode = DeliveryMode::AdmitAfterActiveTurn;
        let mut queue = InputQueue::new();
        queue.admit_input_event(input, false);
        let mut coordinator = TurnCoordinator::with_provider_adapter(
            &path,
            context(),
            Box::new(RecordingProviderAdapter),
        );

        coordinator
            .run_one_ready_turn(&mut queue, &clock())
            .expect("turn run succeeds")
            .expect("turn completed");

        let session_jsonl = read_to_string(&path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        let provider_request = parse_session_event(lines[1]).expect("provider request parses");
        assert_eq!(
            provider_request.payload["provider_request_status"],
            "test_adapter_recorded"
        );

        remove_file(path).ok();
    }

    #[test]
    fn background_turn_keeps_queue_active_without_blocking_operator_admission() {
        let path = temp_session_path();
        let mut first = parse_input_event(INPUT_FIXTURE).expect("input parses");
        first.delivery_mode = DeliveryMode::AdmitAfterActiveTurn;
        let mut queue = InputQueue::new();
        queue.admit_input_event(first, false);
        let (release_sender, release_receiver) = mpsc::channel();
        let mut coordinator = TurnCoordinator::with_provider_adapter(
            &path,
            context(),
            Box::new(BlockingProviderAdapter {
                release: release_receiver,
            }),
        );

        let first_tick = coordinator
            .run_one_ready_turn_background_tick(&mut queue, &clock())
            .expect("background turn starts");
        assert!(first_tick.is_none());
        assert_eq!(queue.turn_state(), TurnState::Active);

        let mut second = parse_input_event(INPUT_FIXTURE).expect("input parses");
        second.event_id = "input_operator_queued_while_worker_active".to_string();
        let decision = queue.admit_input_event(second, false);
        assert!(matches!(
            decision,
            crate::input_queue::AdmissionDecision::QueueForTurnBoundary { .. }
        ));
        assert_eq!(queue.queued_count(), 1);

        release_sender.send(()).expect("release worker");
        let mut completed = None;
        for _ in 0..100 {
            completed = coordinator
                .run_one_ready_turn_background_tick(&mut queue, &clock())
                .expect("background turn polls");
            if completed.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert!(completed.is_some());
        assert_eq!(queue.turn_state(), TurnState::Idle);
        assert_eq!(queue.queued_count(), 1);

        remove_file(path).ok();
    }

    #[test]
    fn cancelling_background_turn_writes_interrupted_terminal_and_releases_queue() {
        let path = temp_session_path();
        let mut first = parse_input_event(INPUT_FIXTURE).expect("input parses");
        first.delivery_mode = DeliveryMode::AdmitAfterActiveTurn;
        let mut queue = InputQueue::new();
        queue.admit_input_event(first, false);
        let (_release_sender, release_receiver) = mpsc::channel();
        let mut coordinator = TurnCoordinator::with_provider_adapter(
            &path,
            context(),
            Box::new(BlockingProviderAdapter {
                release: release_receiver,
            }),
        );

        coordinator
            .run_one_ready_turn_background_tick(&mut queue, &clock())
            .expect("background turn starts");
        assert_eq!(queue.turn_state(), TurnState::Active);
        assert!(coordinator.request_active_turn_cancel());

        let mut completed = None;
        for _ in 0..100 {
            completed = coordinator
                .run_one_ready_turn_background_tick(&mut queue, &clock())
                .expect("background turn polls");
            if completed.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert!(completed.is_some());
        assert_eq!(queue.turn_state(), TurnState::Idle);

        let session_jsonl = read_to_string(&path).expect("session jsonl exists");
        let events = session_jsonl
            .lines()
            .map(|line| parse_session_event(line).expect("event parses"))
            .collect::<Vec<_>>();
        assert_eq!(
            events.last().expect("terminal event exists").event_kind,
            SessionEventKind::TurnInterrupted
        );
        assert_eq!(
            events.last().expect("terminal event exists").payload["terminal_status"],
            "interrupted"
        );
        assert_eq!(
            events.last().expect("terminal event exists").payload["error_summary"],
            "provider_cancelled"
        );
        assert!(!session_jsonl.contains("provider dispatch interrupted: provider_cancelled"));

        remove_file(path).ok();
    }

    #[test]
    fn background_turn_writes_streaming_provider_delta_before_turn_completion() {
        let path = temp_session_path();
        let mut first = parse_input_event(INPUT_FIXTURE).expect("input parses");
        first.delivery_mode = DeliveryMode::AdmitAfterActiveTurn;
        let mut queue = InputQueue::new();
        queue.admit_input_event(first, false);
        let (first_emitted_sender, first_emitted_receiver) = mpsc::channel();
        let (release_sender, release_receiver) = mpsc::channel();
        let mut coordinator = TurnCoordinator::with_provider_adapter(
            &path,
            context(),
            Box::new(StreamingBlockingProviderAdapter {
                first_emitted: first_emitted_sender,
                release: release_receiver,
            }),
        );

        coordinator
            .run_one_ready_turn_background_tick(&mut queue, &clock())
            .expect("background streaming turn starts");
        assert_eq!(queue.turn_state(), TurnState::Active);
        first_emitted_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("first streaming delta emitted before release");

        let partial_session_jsonl = read_to_string(&path).expect("session jsonl exists");
        assert!(partial_session_jsonl.contains("\"text_delta\":\"hello\""));
        assert!(partial_session_jsonl.contains("\"provider_request_status\":\"dispatched\""));
        assert!(
            partial_session_jsonl
                .find("\"provider_request_status\":\"dispatched\"")
                .expect("provider request recorded before streaming output")
                < partial_session_jsonl
                    .find("\"text_delta\":\"hello\"")
                    .expect("streaming output recorded")
        );
        assert!(!partial_session_jsonl.contains("\"terminal_status\":\"completed\""));

        release_sender.send(()).expect("release provider");
        let mut completed = None;
        for _ in 0..100 {
            completed = coordinator
                .run_one_ready_turn_background_tick(&mut queue, &clock())
                .expect("background streaming turn polls");
            if completed.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        assert!(completed.is_some());
        assert_eq!(queue.turn_state(), TurnState::Idle);
        let session_jsonl = read_to_string(&path).expect("session jsonl exists");
        assert!(session_jsonl.contains("\"text_delta\":\"hello\""));
        assert!(session_jsonl.contains("\"text_delta\":\" world\""));
        assert!(session_jsonl.contains("\"terminal_status\":\"completed\""));

        remove_file(path).ok();
    }

    #[test]
    fn writes_buffered_provider_outputs_before_terminal_event() {
        let path = temp_session_path();
        let mut input = parse_input_event(INPUT_FIXTURE).expect("input parses");
        input.delivery_mode = DeliveryMode::AdmitAfterActiveTurn;
        let mut queue = InputQueue::new();
        queue.admit_input_event(input, false);
        let mut coordinator = TurnCoordinator::with_provider_adapter(
            &path,
            context(),
            Box::new(scripted_output_provider_adapter()),
        );

        let completed = coordinator
            .run_one_ready_turn(&mut queue, &clock())
            .expect("turn run succeeds")
            .expect("turn completed");
        assert_eq!(completed.evidence_written, 5);

        let session_jsonl = read_to_string(&path).expect("session jsonl exists");
        let events = session_jsonl
            .lines()
            .map(|line| parse_session_event(line).expect("event parses"))
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 5);
        let provider_request = &events[1];
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
            provider_request.payload["provider_adapter_refusal_reason"],
            serde_json::Value::Null
        );
        assert_eq!(
            events[2].event_kind,
            SessionEventKind::ProviderTextDeltaRecorded
        );
        assert_eq!(
            events[3].event_kind,
            SessionEventKind::ProviderToolCallRequested
        );
        assert_eq!(events[4].event_kind, SessionEventKind::TurnCompleted);
        assert_eq!(events[4].payload["schema"], TURN_TERMINAL_PAYLOAD_SCHEMA);
        assert_eq!(events[4].payload["provider_execution_enabled"], true);
        assert_eq!(events[4].payload["terminal_status"], "completed");

        remove_file(path).ok();
    }

    #[test]
    fn provider_session_settings_apply_to_next_provider_request() {
        let path = temp_session_path();
        let mut input = parse_input_event(INPUT_FIXTURE).expect("input parses");
        input.delivery_mode = DeliveryMode::AdmitAfterActiveTurn;
        let mut queue = InputQueue::new();
        queue.admit_input_event(input, false);
        let mut coordinator = TurnCoordinator::with_provider_adapter(
            &path,
            context(),
            Box::new(scripted_output_provider_adapter()),
        );
        coordinator.set_provider_model(Some("gpt-5.5-mini".to_string()));
        coordinator.set_provider_thinking(Some("high".to_string()));

        coordinator
            .run_one_ready_turn(&mut queue, &clock())
            .expect("turn run succeeds")
            .expect("turn completed");

        let session_jsonl = read_to_string(&path).expect("session jsonl exists");
        let events = session_jsonl
            .lines()
            .map(|line| parse_session_event(line).expect("event parses"))
            .collect::<Vec<_>>();
        let provider_request = &events[1];
        assert_eq!(provider_request.payload["model"], "gpt-5.5-mini");
        assert_eq!(provider_request.payload["thinking"], "high");

        remove_file(path).ok();
    }

    #[test]
    fn injected_tool_bridge_runs_after_provider_tool_output() {
        let path = temp_session_path();
        let mut input = parse_input_event(INPUT_FIXTURE).expect("input parses");
        input.delivery_mode = DeliveryMode::AdmitAfterActiveTurn;
        let mut queue = InputQueue::new();
        queue.admit_input_event(input, false);
        let mut coordinator = TurnCoordinator::with_provider_adapter_and_tool_executor(
            &path,
            context(),
            Box::new(ToolThenFinalProviderAdapter::new()),
            Box::new(WritingProviderToolCallExecutor),
        );

        let completed = coordinator
            .run_one_ready_turn(&mut queue, &clock())
            .expect("turn run succeeds")
            .expect("turn completed");
        assert_eq!(completed.evidence_written, 8);

        let session_jsonl = read_to_string(&path).expect("session jsonl exists");
        let events = session_jsonl
            .lines()
            .map(|line| parse_session_event(line).expect("event parses"))
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 8);
        assert_eq!(
            events[2].event_kind,
            SessionEventKind::ProviderToolCallRequested
        );
        assert_eq!(events[3].event_kind, SessionEventKind::ToolCallRequested);
        assert_eq!(events[4].event_kind, SessionEventKind::ToolResultReceived);
        assert_eq!(
            events[5].event_kind,
            SessionEventKind::ProviderRequestRecorded
        );
        assert!(
            events[5].payload["content_preview"]
                .as_str()
                .expect("follow-up preview is text")
                .contains("MCP tool result(s):")
        );
        assert_eq!(
            events[6].event_kind,
            SessionEventKind::ProviderTextDeltaRecorded
        );
        assert_eq!(
            events[6].payload["text_delta"],
            "Final answer from tool result."
        );
        assert_eq!(events[7].event_kind, SessionEventKind::TurnCompleted);

        remove_file(path).ok();
    }
    #[test]
    fn drains_ready_input_and_writes_turn_evidence() {
        let path = temp_session_path();
        let mut input = parse_input_event(INPUT_FIXTURE).expect("input parses");
        input.delivery_mode = DeliveryMode::AdmitAfterActiveTurn;
        let mut queue = InputQueue::new();
        queue.admit_input_event(input, false);
        let mut coordinator = TurnCoordinator::new(&path, context());

        let completed = coordinator
            .run_one_ready_turn(&mut queue, &clock())
            .expect("turn run succeeds")
            .expect("turn completed");
        assert_eq!(completed.turn_id, "turn_1");
        assert_eq!(completed.evidence_written, 3);
        assert_eq!(queue.turn_state(), TurnState::Idle);
        assert_eq!(queue.queued_count(), 0);

        let session_jsonl = read_to_string(&path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 3);
        let started = parse_session_event(lines[0]).expect("started parses");
        let provider_request = parse_session_event(lines[1]).expect("provider request parses");
        let completed_event = parse_session_event(lines[2]).expect("completed parses");
        assert_eq!(started.event_kind, SessionEventKind::TurnStarted);
        assert_eq!(
            provider_request.event_kind,
            SessionEventKind::ProviderRequestRecorded
        );
        assert_eq!(
            provider_request.payload["provider_request_status"],
            "recorded_not_dispatched"
        );
        assert_eq!(completed_event.event_kind, SessionEventKind::TurnCompleted);
        assert_eq!(
            completed_event.payload["schema"],
            TURN_TERMINAL_PAYLOAD_SCHEMA
        );
        assert_eq!(
            completed_event.payload["terminal_status"],
            "completed_without_provider"
        );

        remove_file(path).ok();
    }
}
