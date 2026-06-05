use crate::app_view_model::AppViewModel;
use crate::composer_draft::{ComposerDraftEffect, ComposerDraftState};
use crate::input_queue::{QueuedInputSummary, elapsed_label_between};
use crate::interactive_runtime::{AgentTuiInteractiveRuntime, InteractiveStepClock};
use crate::layout_model::TerminalSize;
use crate::runtime_clock::RuntimeClock;
use crate::runtime_coordinator::{RuntimeCoordinatorClock, RuntimeOperatorSubmitResult};
use crate::terminal_input_tick::{
    TerminalInputReader, TerminalInputTickOutcome, run_textarea_composer_input_tick_with_wait,
};
use crate::textarea_composer::TextareaComposer;
use crate::transcript_projection::{TranscriptActor, TranscriptItem, TranscriptItemKind};
use crate::transcript_view_model::build_transcript_rows;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct AgentTuiLoopState {
    pub composer: TextareaComposer,
    pub should_exit: bool,
    pub last_error: Option<String>,
    pub local_transcript_items: Vec<TranscriptItem>,
    pub transcript_scroll_offset: usize,
}

impl AgentTuiLoopState {
    pub fn draft_state(&self) -> ComposerDraftState {
        self.composer.draft_state()
    }

    pub fn draft_text(&self) -> String {
        self.composer.text()
    }

    pub fn composer_has_draft(&self) -> bool {
        !self.composer.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderLoopAction {
    None,
    Redraw,
    Exit,
}

pub trait ComposerAdmissionBridge {
    fn submit_operator_text(&mut self, text: String) -> Result<Option<TranscriptItem>, String>;
    fn clear_transcript_projection(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn request_interrupt(&mut self) -> Result<(), String>;
}

pub struct RuntimeCoordinatorComposerBridge<'a> {
    pub runtime: &'a mut AgentTuiInteractiveRuntime,
    pub clock: RuntimeCoordinatorClock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InteractiveLoopRunSummary {
    pub steps_attempted: u64,
    pub exited_by_input: bool,
    pub final_drawn: bool,
}

pub trait InteractiveTerminalFrame {
    fn terminal_size(&mut self) -> Result<TerminalSize, String>;
    fn draw_frame(
        &mut self,
        model: &AppViewModel,
        composer: &TextareaComposer,
    ) -> Result<(), String>;
}

pub trait InteractiveInputSource {
    fn read_tick(&mut self, composer: &mut TextareaComposer) -> TerminalInputTickOutcome;
}

pub trait InteractiveClockSource {
    fn next_interactive_step_clock(&mut self) -> InteractiveStepClock;
}

pub struct TerminalInputTickSource<'a, R: TerminalInputReader> {
    pub reader: &'a mut R,
    pub wait: Duration,
}

impl<R: TerminalInputReader> InteractiveInputSource for TerminalInputTickSource<'_, R> {
    fn read_tick(&mut self, composer: &mut TextareaComposer) -> TerminalInputTickOutcome {
        run_textarea_composer_input_tick_with_wait(self.reader, composer, self.wait)
    }
}

pub struct RuntimeClockInteractiveSource<'a> {
    pub clock: &'a mut RuntimeClock,
}

impl InteractiveClockSource for RuntimeClockInteractiveSource<'_> {
    fn next_interactive_step_clock(&mut self) -> InteractiveStepClock {
        InteractiveStepClock::from(self.clock.next_step_clock())
    }
}

impl ComposerAdmissionBridge for RuntimeCoordinatorComposerBridge<'_> {
    fn submit_operator_text(&mut self, text: String) -> Result<Option<TranscriptItem>, String> {
        let result = self
            .runtime
            .coordinator_mut()
            .handle_operator_submit(text, &self.clock)?;
        self.runtime.apply_operator_submit_result(&result)?;
        Ok(queue_command_feedback_item(
            &result,
            &self.clock.occurred_at,
        ))
    }

    fn clear_transcript_projection(&mut self) -> Result<(), String> {
        self.runtime.clear_transcript_projection();
        Ok(())
    }

    fn request_interrupt(&mut self) -> Result<(), String> {
        self.runtime
            .record_interrupt_and_cancel_active_turn(&self.clock)
            .map(|_| ())
    }
}

pub fn run_injected_interactive_loop<T, I, C>(
    runtime: &mut AgentTuiInteractiveRuntime,
    loop_state: &mut AgentTuiLoopState,
    terminal: &mut T,
    input: &mut I,
    clock: &mut C,
    max_steps: u64,
) -> Result<InteractiveLoopRunSummary, String>
where
    T: InteractiveTerminalFrame,
    I: InteractiveInputSource,
    C: InteractiveClockSource,
{
    let mut steps_attempted = 0;
    let mut exited_by_input = false;

    for _ in 0..max_steps {
        steps_attempted += 1;
        let step_clock = clock.next_interactive_step_clock();
        let step_result = runtime.run_background_step(&loop_state.draft_state(), &step_clock)?;
        apply_step_result_to_loop_state(loop_state, &step_result);

        let model = build_loop_view(
            runtime,
            loop_state,
            terminal.terminal_size()?,
            Some(step_clock.input.occurred_at.as_str()),
        );
        terminal.draw_frame(&model, &loop_state.composer)?;

        let outcome = input.read_tick(&mut loop_state.composer);
        let mut bridge = RuntimeCoordinatorComposerBridge {
            runtime,
            clock: step_clock.input.clone(),
        };
        let action = apply_input_tick_outcome(loop_state, &mut bridge, outcome);
        if action == RenderLoopAction::Exit || loop_state.should_exit {
            exited_by_input = true;
            break;
        }
        if action == RenderLoopAction::Redraw {
            runtime.ingest_transcript()?;
            let model = build_loop_view(
                runtime,
                loop_state,
                terminal.terminal_size()?,
                Some(step_clock.input.occurred_at.as_str()),
            );
            terminal.draw_frame(&model, &loop_state.composer)?;
        }

        let step_result = runtime.run_background_step(&loop_state.draft_state(), &step_clock)?;
        apply_step_result_to_loop_state(loop_state, &step_result);
    }

    let step_clock = clock.next_interactive_step_clock();
    let step_result = runtime.run_background_step(&loop_state.draft_state(), &step_clock)?;
    apply_step_result_to_loop_state(loop_state, &step_result);
    runtime.ingest_transcript()?;
    let model = build_loop_view(
        runtime,
        loop_state,
        terminal.terminal_size()?,
        Some(step_clock.input.occurred_at.as_str()),
    );
    terminal.draw_frame(&model, &loop_state.composer)?;

    Ok(InteractiveLoopRunSummary {
        steps_attempted,
        exited_by_input,
        final_drawn: true,
    })
}

fn apply_step_result_to_loop_state(
    loop_state: &mut AgentTuiLoopState,
    step_result: &crate::interactive_runtime::InteractiveStepResult,
) {
    if step_result.parse_errors > 0 {
        loop_state.last_error = Some(format!(
            "control_jsonl_parse_errors:{}",
            step_result.parse_errors
        ));
    }
}

pub fn apply_input_tick_outcome<B: ComposerAdmissionBridge>(
    state: &mut AgentTuiLoopState,
    bridge: &mut B,
    outcome: TerminalInputTickOutcome,
) -> RenderLoopAction {
    match outcome {
        TerminalInputTickOutcome::NoInput | TerminalInputTickOutcome::NonKeyEventIgnored => {
            RenderLoopAction::None
        }
        TerminalInputTickOutcome::ScrollTranscriptUp => {
            state.transcript_scroll_offset = state.transcript_scroll_offset.saturating_add(8);
            RenderLoopAction::Redraw
        }
        TerminalInputTickOutcome::ScrollTranscriptDown => {
            state.transcript_scroll_offset = state.transcript_scroll_offset.saturating_sub(8);
            RenderLoopAction::Redraw
        }
        TerminalInputTickOutcome::ReadFailed(error) => {
            state.last_error = Some(error);
            RenderLoopAction::Redraw
        }
        TerminalInputTickOutcome::DraftEffect(effect) => {
            let action = apply_draft_effect(state, bridge, effect);
            if action == RenderLoopAction::Redraw {
                state.transcript_scroll_offset = 0;
            }
            action
        }
    }
}

fn apply_draft_effect<B: ComposerAdmissionBridge>(
    state: &mut AgentTuiLoopState,
    bridge: &mut B,
    effect: ComposerDraftEffect,
) -> RenderLoopAction {
    match effect {
        ComposerDraftEffect::None => RenderLoopAction::None,
        ComposerDraftEffect::DraftChanged => RenderLoopAction::Redraw,
        ComposerDraftEffect::SubmitRequested { text } => {
            let trimmed = text.trim();
            if trimmed.eq_ignore_ascii_case("/exit") || trimmed.eq_ignore_ascii_case("/quit") {
                state.should_exit = true;
                return RenderLoopAction::Exit;
            }
            if trimmed.eq_ignore_ascii_case("/clear") {
                match bridge.clear_transcript_projection() {
                    Ok(()) => {
                        state.local_transcript_items.clear();
                        state.transcript_scroll_offset = 0;
                    }
                    Err(error) => state.last_error = Some(error),
                }
                return RenderLoopAction::Redraw;
            }
            match bridge.submit_operator_text(text) {
                Ok(Some(item)) => state.local_transcript_items.push(item),
                Ok(None) => {}
                Err(error) => state.last_error = Some(error),
            }
            RenderLoopAction::Redraw
        }
        ComposerDraftEffect::ClearOrInterruptRequested => {
            if let Err(error) = bridge.request_interrupt() {
                state.last_error = Some(error);
            }
            RenderLoopAction::Redraw
        }
        ComposerDraftEffect::ExitRequested => {
            state.should_exit = true;
            RenderLoopAction::Exit
        }
    }
}

fn build_loop_view(
    runtime: &AgentTuiInteractiveRuntime,
    state: &AgentTuiLoopState,
    terminal_size: TerminalSize,
    now: Option<&str>,
) -> AppViewModel {
    let mut model = runtime.build_view_at(
        terminal_size,
        &state.draft_state(),
        state.last_error.clone(),
        now,
    );
    if !state.local_transcript_items.is_empty() {
        model
            .transcript_rows
            .extend(build_transcript_rows(&state.local_transcript_items));
    }
    model.set_transcript_scroll_offset(state.transcript_scroll_offset);
    model
}

fn queue_command_feedback_item(
    result: &RuntimeOperatorSubmitResult,
    occurred_at: &str,
) -> Option<TranscriptItem> {
    let text = match result {
        RuntimeOperatorSubmitResult::HelpShown => help_text(),
        RuntimeOperatorSubmitResult::StatusShown {
            identity,
            session,
            model,
            thinking,
            queued,
            held,
            turn_state,
        } => format!(
            "Identity     {identity}\nSession      {session}\nModel        {}\nThinking     {}\nTurn         {turn_state}\nQueued       {queued}\nHeld         {held}",
            model.clone().unwrap_or_else(|| "unset".to_string()),
            thinking.clone().unwrap_or_else(|| "unset".to_string())
        ),
        RuntimeOperatorSubmitResult::StatsShown { output } => output.clone(),
        RuntimeOperatorSubmitResult::ModelShown { value } => {
            format!(
                "Current model: {}",
                value.clone().unwrap_or_else(|| "unset".to_string())
            )
        }
        RuntimeOperatorSubmitResult::ModelChanged { value } => format!("Model set to {value}"),
        RuntimeOperatorSubmitResult::ThinkingShown { value } => {
            format!(
                "Current thinking: {}",
                value.clone().unwrap_or_else(|| "unset".to_string())
            )
        }
        RuntimeOperatorSubmitResult::ThinkingChanged { value } => {
            format!("Thinking set to {value}")
        }
        RuntimeOperatorSubmitResult::ThinkingRejected { value: _ } => {
            "Usage: /thinking none|low|medium|high".to_string()
        }
        RuntimeOperatorSubmitResult::ToolOutputShown { shown }
        | RuntimeOperatorSubmitResult::ToolOutputChanged { shown } => format!(
            "Tool call outputs are {} in the displayed transcript.",
            if *shown { "shown" } else { "hidden" }
        ),
        RuntimeOperatorSubmitResult::ToolOutputRejected { value: _ } => {
            "Usage: /tool-output [on|off|toggle|status]".to_string()
        }
        RuntimeOperatorSubmitResult::ClearDisplay => "cleared".to_string(),
        RuntimeOperatorSubmitResult::Exit => "exiting".to_string(),
        RuntimeOperatorSubmitResult::UnknownCommand { command } => {
            format!("Unknown command: {command}. Type /help.")
        }
        RuntimeOperatorSubmitResult::QueueShown { queued } => {
            queue_shown_text(&queued, occurred_at)
        }
        RuntimeOperatorSubmitResult::QueueCleared { dropped } => {
            format!("queue cleared {dropped} {}", item_label(*dropped))
        }
        RuntimeOperatorSubmitResult::QueueDrop {
            index,
            dropped_input_event_id,
        } => match dropped_input_event_id {
            Some(input_event_id) => format!("queue dropped {index}: {input_event_id}"),
            None => format!("queue drop {index}: not found"),
        },
        RuntimeOperatorSubmitResult::Empty | RuntimeOperatorSubmitResult::AgentInput(_) => {
            return None;
        }
    };
    Some(TranscriptItem {
        kind: TranscriptItemKind::TurnTerminalStatus,
        actor: TranscriptActor::AgentTui,
        turn_id: String::new(),
        text,
        sequence: None,
        projection_key: None,
        occurred_at: Some(occurred_at.to_string()),
    })
}

fn help_text() -> String {
    [
        "Commands",
        "",
        "/help                 Show commands",
        "/status               Show session state",
        "/stats [args]         Show local Codex transcript statistics",
        "/model <name>         Set model for later turns",
        "/thinking <level>     none, low, medium, high",
        "/tool-output [state]  Toggle displayed tool call outputs (on, off, toggle)",
        "/queue                Show queued carrier input",
        "/queue clear          Clear queued operator steering",
        "/queue drop <index>   Drop one queued operator steering item",
        "/clear                Clear terminal display",
        "/exit                 Save and quit",
    ]
    .join("\n")
}

fn queue_shown_text(queued: &[QueuedInputSummary], occurred_at: &str) -> String {
    if queued.is_empty() {
        return "queue empty".to_string();
    }
    let mut lines = vec![format!(
        "queue: {} {}",
        queued.len(),
        item_label(queued.len())
    )];
    for item in queued {
        let age = elapsed_label_between(&item.created_at, occurred_at)
            .unwrap_or_else(|| "age unknown".to_string());
        lines.push(format!(
            "{}. {} · {} · {}",
            item.index,
            source_kind_label(&item.source_kind),
            age,
            item.content_preview
        ));
    }
    lines.join("\n")
}

fn source_kind_label(source_kind: &crate::carrier_protocol::SourceKind) -> &'static str {
    match source_kind {
        crate::carrier_protocol::SourceKind::Operator => "operator",
        crate::carrier_protocol::SourceKind::System => "system",
        crate::carrier_protocol::SourceKind::Agent => "agent",
        crate::carrier_protocol::SourceKind::External => "external",
    }
}

fn item_label(count: usize) -> &'static str {
    if count == 1 { "item" } else { "items" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{
        InputEvent, SessionEventKind, SourceKind, create_provider_request_payload,
        parse_session_event,
    };
    use crate::input_queue::{SessionEvidenceContext, TurnState};
    use crate::provider_dispatch::{
        ProviderAdapter, ProviderCancellationToken, ProviderDispatchRecord, ProviderDispatchStatus,
    };
    use crate::terminal_input_tick::TerminalInputReader;
    use crate::test_env_lock::ENV_LOCK;
    use crate::transcript_store::TranscriptStore;
    use crate::turn_coordinator::TurnCoordinatorClock;
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use serde_json::json;
    use std::collections::VecDeque;
    use std::fs::{OpenOptions, read_to_string, remove_file};
    use std::io;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    const CONTROL_FIXTURE: &str =
        include_str!("../carrier-protocol/fixtures/control-input-event.json");

    #[derive(Debug, Default)]
    struct FakeBridge {
        submitted: Vec<String>,
        interrupts: usize,
        clears: usize,
        fail_submit: bool,
        fail_interrupt: bool,
    }

    impl ComposerAdmissionBridge for FakeBridge {
        fn submit_operator_text(&mut self, text: String) -> Result<Option<TranscriptItem>, String> {
            if self.fail_submit {
                return Err("submit failed".to_string());
            }
            self.submitted.push(text);
            Ok(None)
        }

        fn clear_transcript_projection(&mut self) -> Result<(), String> {
            self.clears += 1;
            Ok(())
        }

        fn request_interrupt(&mut self) -> Result<(), String> {
            if self.fail_interrupt {
                return Err("interrupt failed".to_string());
            }
            self.interrupts += 1;
            Ok(())
        }
    }

    struct CancellableProviderAdapter {
        release: mpsc::Receiver<()>,
    }

    impl ProviderAdapter for CancellableProviderAdapter {
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
                    Some("test_cancellable_adapter".to_string()),
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
                status: ProviderDispatchStatus::Completed,
                provider_execution_enabled: true,
                payload: create_provider_request_payload(
                    turn_id,
                    &input.event_id,
                    "completed",
                    true,
                    "configured",
                    "admitted",
                    Some("test_cancellable_adapter".to_string()),
                    None,
                    None,
                    None,
                    false,
                    "single_provider_output_batch",
                    None,
                    &input.content,
                ),
                outputs: Vec::new(),
            }
        }
    }

    #[derive(Debug)]
    struct FakeTerminalFrame {
        size: TerminalSize,
        size_sequence: VecDeque<TerminalSize>,
        draw_count: usize,
        prompt_labels: Vec<String>,
        composer_texts: Vec<String>,
        status_lines: Vec<String>,
        transcript_counts: Vec<usize>,
        transcript_texts: Vec<String>,
        transcript_scroll_offsets: Vec<usize>,
        layout_snapshots: Vec<(u16, u16, u16, u16)>,
    }

    impl FakeTerminalFrame {
        fn new() -> Self {
            Self {
                size: TerminalSize {
                    width: 80,
                    height: 12,
                },
                size_sequence: VecDeque::new(),
                draw_count: 0,
                prompt_labels: Vec::new(),
                composer_texts: Vec::new(),
                status_lines: Vec::new(),
                transcript_counts: Vec::new(),
                transcript_texts: Vec::new(),
                transcript_scroll_offsets: Vec::new(),
                layout_snapshots: Vec::new(),
            }
        }

        fn resizing(sizes: impl IntoIterator<Item = TerminalSize>) -> Self {
            Self {
                size_sequence: VecDeque::from_iter(sizes),
                ..Self::new()
            }
        }
    }

    impl InteractiveTerminalFrame for FakeTerminalFrame {
        fn terminal_size(&mut self) -> Result<TerminalSize, String> {
            if let Some(size) = self.size_sequence.pop_front() {
                self.size = size;
            }
            Ok(self.size)
        }

        fn draw_frame(
            &mut self,
            model: &AppViewModel,
            composer: &TextareaComposer,
        ) -> Result<(), String> {
            self.draw_count += 1;
            self.prompt_labels.push(model.composer.prompt_label.clone());
            self.composer_texts.push(composer.text());
            self.status_lines.push(model.status.compact_line.clone());
            self.transcript_counts.push(model.transcript_rows.len());
            self.transcript_scroll_offsets
                .push(model.transcript_scroll_offset);
            self.transcript_texts.push(
                model
                    .transcript_rows
                    .iter()
                    .map(|row| row.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            self.layout_snapshots.push((
                model.layout.transcript.height,
                model.layout.status.y,
                model.layout.composer.y,
                model.layout.composer.height,
            ));
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct FakeInputSource {
        outcomes: VecDeque<TerminalInputTickOutcome>,
    }

    impl InteractiveInputSource for FakeInputSource {
        fn read_tick(&mut self, _composer: &mut TextareaComposer) -> TerminalInputTickOutcome {
            self.outcomes
                .pop_front()
                .unwrap_or(TerminalInputTickOutcome::NoInput)
        }
    }

    #[derive(Debug)]
    struct AppendingInputSource {
        outcomes: VecDeque<TerminalInputTickOutcome>,
        control_path: PathBuf,
        appended: bool,
    }

    impl InteractiveInputSource for AppendingInputSource {
        fn read_tick(&mut self, _composer: &mut TextareaComposer) -> TerminalInputTickOutcome {
            if !self.appended && self.outcomes.len() == 2 {
                append(&self.control_path, CONTROL_FIXTURE.trim_end());
                append(&self.control_path, "\n");
                self.appended = true;
            }
            self.outcomes
                .pop_front()
                .unwrap_or(TerminalInputTickOutcome::NoInput)
        }
    }

    #[derive(Debug, Default)]
    struct FakeTerminalInputReader {
        events: VecDeque<Event>,
    }

    impl TerminalInputReader for FakeTerminalInputReader {
        fn poll_input(&mut self, _wait: Duration) -> io::Result<bool> {
            Ok(!self.events.is_empty())
        }

        fn read_input(&mut self) -> io::Result<Event> {
            self.events
                .pop_front()
                .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "no event"))
        }
    }

    struct FakeClockSource {
        index: u64,
    }

    impl InteractiveClockSource for FakeClockSource {
        fn next_interactive_step_clock(&mut self) -> InteractiveStepClock {
            self.index += 1;
            InteractiveStepClock {
                input: RuntimeCoordinatorClock {
                    occurred_at: format!("2026-05-30T00:00:0{}.000Z", self.index),
                    event_id_prefix: format!("session_event_input_{}", self.index),
                },
                turn: TurnCoordinatorClock {
                    occurred_at: format!("2026-05-30T00:00:0{}.000Z", self.index),
                    event_id_prefix: format!("session_event_turn_{}", self.index),
                    turn_id_prefix: format!("turn_{}", self.index),
                },
            }
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock works")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "narada-agent-tui-render-loop-{name}-{unique}.jsonl"
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

    fn runtime_for_paths(control_path: &Path, session_path: &Path) -> AgentTuiInteractiveRuntime {
        AgentTuiInteractiveRuntime::new(
            "sonar.resident",
            "carrier_fixture_1",
            control_path,
            session_path,
            context(),
        )
    }

    fn input_source(
        outcomes: impl IntoIterator<Item = TerminalInputTickOutcome>,
    ) -> FakeInputSource {
        FakeInputSource {
            outcomes: VecDeque::from_iter(outcomes),
        }
    }

    fn appending_input_source(
        control_path: PathBuf,
        outcomes: impl IntoIterator<Item = TerminalInputTickOutcome>,
    ) -> AppendingInputSource {
        AppendingInputSource {
            outcomes: VecDeque::from_iter(outcomes),
            control_path,
            appended: false,
        }
    }

    fn terminal_reader(events: impl IntoIterator<Item = Event>) -> FakeTerminalInputReader {
        FakeTerminalInputReader {
            events: VecDeque::from_iter(events),
        }
    }

    fn key_event(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn clock_source() -> FakeClockSource {
        FakeClockSource { index: 0 }
    }

    #[test]
    fn loop_state_exposes_composer_draft_snapshot() {
        let state = AgentTuiLoopState {
            composer: TextareaComposer::from_draft(&ComposerDraftState {
                text: "draft".to_string(),
            }),
            ..AgentTuiLoopState::default()
        };

        assert_eq!(state.draft_text(), "draft");
        assert_eq!(state.draft_state().text, "draft");
        assert!(state.composer_has_draft());
    }

    #[test]
    fn ignores_empty_tick_outcomes() {
        let mut state = AgentTuiLoopState::default();
        let mut bridge = FakeBridge::default();

        assert_eq!(
            apply_input_tick_outcome(&mut state, &mut bridge, TerminalInputTickOutcome::NoInput),
            RenderLoopAction::None
        );
        assert_eq!(
            apply_input_tick_outcome(
                &mut state,
                &mut bridge,
                TerminalInputTickOutcome::NonKeyEventIgnored
            ),
            RenderLoopAction::None
        );
        assert!(bridge.submitted.is_empty());
        assert_eq!(bridge.interrupts, 0);
    }

    #[test]
    fn scrolls_transcript_without_mutating_composer_or_bridge() {
        let mut state = AgentTuiLoopState::default();
        let mut bridge = FakeBridge::default();

        assert_eq!(
            apply_input_tick_outcome(
                &mut state,
                &mut bridge,
                TerminalInputTickOutcome::ScrollTranscriptUp
            ),
            RenderLoopAction::Redraw
        );
        assert_eq!(state.transcript_scroll_offset, 8);
        assert_eq!(state.draft_text(), "");
        assert!(bridge.submitted.is_empty());
        assert_eq!(bridge.interrupts, 0);

        assert_eq!(
            apply_input_tick_outcome(
                &mut state,
                &mut bridge,
                TerminalInputTickOutcome::ScrollTranscriptDown
            ),
            RenderLoopAction::Redraw
        );
        assert_eq!(state.transcript_scroll_offset, 0);
    }

    #[test]
    fn redraws_for_draft_changes_and_read_errors() {
        let mut state = AgentTuiLoopState {
            transcript_scroll_offset: 8,
            ..AgentTuiLoopState::default()
        };
        let mut bridge = FakeBridge::default();

        assert_eq!(
            apply_input_tick_outcome(
                &mut state,
                &mut bridge,
                TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::DraftChanged)
            ),
            RenderLoopAction::Redraw
        );
        assert_eq!(state.transcript_scroll_offset, 0);
        assert_eq!(
            apply_input_tick_outcome(
                &mut state,
                &mut bridge,
                TerminalInputTickOutcome::ReadFailed("read failed".to_string())
            ),
            RenderLoopAction::Redraw
        );
        assert_eq!(state.last_error.as_deref(), Some("read failed"));
    }

    #[test]
    fn submits_operator_text_through_bridge() {
        let mut state = AgentTuiLoopState::default();
        let mut bridge = FakeBridge::default();

        assert_eq!(
            apply_input_tick_outcome(
                &mut state,
                &mut bridge,
                TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::SubmitRequested {
                    text: "run startup sequence".to_string()
                })
            ),
            RenderLoopAction::Redraw
        );
        assert_eq!(bridge.submitted, vec!["run startup sequence".to_string()]);
        assert!(state.last_error.is_none());
    }

    #[test]
    fn stats_feedback_renders_command_output() {
        let item = queue_command_feedback_item(
            &RuntimeOperatorSubmitResult::StatsShown {
                output: "Codex transcript stats\nScanned: 1 rollout files".to_string(),
            },
            "2026-05-30T18:39:10.000Z",
        )
        .expect("stats command renders local feedback");

        assert_eq!(item.actor, TranscriptActor::AgentTui);
        assert!(item.text.contains("Codex transcript stats"));
        assert!(item.text.contains("Scanned: 1 rollout files"));
    }

    #[test]
    fn status_feedback_includes_model_and_thinking() {
        let item = queue_command_feedback_item(
            &RuntimeOperatorSubmitResult::StatusShown {
                identity: "sonar.resident".to_string(),
                session: "carrier_fixture_1".to_string(),
                model: Some("gpt-5.5-mini".to_string()),
                thinking: Some("high".to_string()),
                queued: 0,
                held: 0,
                turn_state: "idle".to_string(),
            },
            "2026-05-30T18:39:10.000Z",
        )
        .expect("status command renders local feedback");

        assert!(item.text.contains("Model        gpt-5.5-mini"));
        assert!(item.text.contains("Thinking     high"));
    }

    #[test]
    fn queue_feedback_formats_index_source_age_and_preview() {
        let item = queue_command_feedback_item(
            &RuntimeOperatorSubmitResult::QueueShown {
                queued: vec![QueuedInputSummary {
                    index: 1,
                    input_event_id: "input_fixture_1".to_string(),
                    source_kind: SourceKind::Operator,
                    created_at: "2026-05-30T18:38:00.000Z".to_string(),
                    content_preview: "queued note".to_string(),
                }],
            },
            "2026-05-30T18:39:10.000Z",
        )
        .expect("queue command renders local feedback");

        assert_eq!(item.actor, TranscriptActor::AgentTui);
        assert_eq!(item.kind, TranscriptItemKind::TurnTerminalStatus);
        assert_eq!(
            item.occurred_at.as_deref(),
            Some("2026-05-30T18:39:10.000Z")
        );
        assert_eq!(
            item.text,
            "queue: 1 item\n1. operator · 1m 10s · queued note"
        );
    }

    #[test]
    fn submit_pushes_local_queue_feedback_when_bridge_returns_item() {
        struct QueueFeedbackBridge;

        impl ComposerAdmissionBridge for QueueFeedbackBridge {
            fn submit_operator_text(
                &mut self,
                _text: String,
            ) -> Result<Option<TranscriptItem>, String> {
                Ok(Some(TranscriptItem {
                    kind: TranscriptItemKind::TurnTerminalStatus,
                    actor: TranscriptActor::AgentTui,
                    turn_id: String::new(),
                    text: "queue empty".to_string(),
                    sequence: None,
                    projection_key: None,
                    occurred_at: Some("2026-05-30T18:39:10.000Z".to_string()),
                }))
            }

            fn request_interrupt(&mut self) -> Result<(), String> {
                Ok(())
            }
        }

        let mut state = AgentTuiLoopState::default();
        let mut bridge = QueueFeedbackBridge;

        assert_eq!(
            apply_input_tick_outcome(
                &mut state,
                &mut bridge,
                TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::SubmitRequested {
                    text: "/queue".to_string()
                })
            ),
            RenderLoopAction::Redraw
        );

        assert_eq!(state.local_transcript_items.len(), 1);
        assert_eq!(state.local_transcript_items[0].text, "queue empty");
    }

    #[test]
    fn requests_interrupt_through_bridge() {
        let mut state = AgentTuiLoopState::default();
        let mut bridge = FakeBridge::default();

        assert_eq!(
            apply_input_tick_outcome(
                &mut state,
                &mut bridge,
                TerminalInputTickOutcome::DraftEffect(
                    ComposerDraftEffect::ClearOrInterruptRequested
                )
            ),
            RenderLoopAction::Redraw
        );
        assert_eq!(bridge.interrupts, 1);
    }

    #[test]
    fn slash_clear_clears_projected_and_local_transcript() {
        let mut state = AgentTuiLoopState::default();
        state.transcript_scroll_offset = 3;
        state.local_transcript_items.push(TranscriptItem {
            kind: TranscriptItemKind::TurnTerminalStatus,
            actor: TranscriptActor::AgentTui,
            turn_id: String::new(),
            text: "status".to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: None,
        });
        let mut bridge = FakeBridge::default();

        assert_eq!(
            apply_input_tick_outcome(
                &mut state,
                &mut bridge,
                TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::SubmitRequested {
                    text: "/clear".to_string()
                })
            ),
            RenderLoopAction::Redraw
        );
        assert_eq!(bridge.clears, 1);
        assert!(state.local_transcript_items.is_empty());
        assert_eq!(state.transcript_scroll_offset, 0);
    }

    #[test]
    fn records_bridge_failures_without_exiting() {
        let mut state = AgentTuiLoopState::default();
        let mut bridge = FakeBridge {
            fail_submit: true,
            ..FakeBridge::default()
        };

        assert_eq!(
            apply_input_tick_outcome(
                &mut state,
                &mut bridge,
                TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::SubmitRequested {
                    text: "run startup sequence".to_string()
                })
            ),
            RenderLoopAction::Redraw
        );
        assert_eq!(state.last_error.as_deref(), Some("submit failed"));
    }

    #[test]
    fn exit_effect_marks_loop_exit() {
        let mut state = AgentTuiLoopState::default();
        let mut bridge = FakeBridge::default();

        assert_eq!(
            apply_input_tick_outcome(
                &mut state,
                &mut bridge,
                TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::ExitRequested)
            ),
            RenderLoopAction::Exit
        );
        assert!(state.should_exit);
    }

    #[test]
    fn injected_interactive_loop_draws_polls_input_and_final_frame() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        append(&control_path, CONTROL_FIXTURE.trim_end());
        append(&control_path, "\n");
        let mut runtime = runtime_for_paths(&control_path, &session_path);
        let mut state = AgentTuiLoopState {
            composer: TextareaComposer::from_draft(&ComposerDraftState {
                text: "live note".to_string(),
            }),
            ..AgentTuiLoopState::default()
        };
        let mut terminal = FakeTerminalFrame::new();
        let mut input = input_source([TerminalInputTickOutcome::NoInput]);
        let mut clock = clock_source();

        let summary = run_injected_interactive_loop(
            &mut runtime,
            &mut state,
            &mut terminal,
            &mut input,
            &mut clock,
            1,
        )
        .expect("interactive loop succeeds");

        assert_eq!(summary.steps_attempted, 1);
        assert!(!summary.exited_by_input);
        assert!(summary.final_drawn);
        assert_eq!(terminal.draw_count, 2);
        assert_eq!(terminal.composer_texts, vec!["live note", "live note"]);
        assert!(
            terminal
                .prompt_labels
                .iter()
                .all(|label| label == "operator -> sonar.resident>")
        );
        assert!(terminal.transcript_counts[0] >= 1);
        assert!(terminal.transcript_counts[1] >= 1);

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn injected_interactive_loop_preserves_draft_and_recomputes_layout_across_resize() {
        let control_path = temp_path("control-resize");
        let session_path = temp_path("session-resize");
        append(&control_path, "");
        let mut runtime = runtime_for_paths(&control_path, &session_path);
        let mut state = AgentTuiLoopState {
            composer: TextareaComposer::from_draft(&ComposerDraftState {
                text: "resize draft".to_string(),
            }),
            ..AgentTuiLoopState::default()
        };
        let mut terminal = FakeTerminalFrame::resizing([
            TerminalSize {
                width: 100,
                height: 24,
            },
            TerminalSize {
                width: 60,
                height: 8,
            },
        ]);
        let mut input = input_source([TerminalInputTickOutcome::NoInput]);
        let mut clock = clock_source();

        let summary = run_injected_interactive_loop(
            &mut runtime,
            &mut state,
            &mut terminal,
            &mut input,
            &mut clock,
            1,
        )
        .expect("interactive loop survives resize");

        assert_eq!(summary.steps_attempted, 1);
        assert_eq!(terminal.draw_count, 2);
        assert_eq!(
            terminal.composer_texts,
            vec!["resize draft", "resize draft"]
        );
        assert_eq!(terminal.layout_snapshots[0], (19, 19, 20, 4));
        assert_eq!(terminal.layout_snapshots[1], (3, 3, 4, 4));

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn injected_interactive_loop_queues_submit_during_active_turn() {
        let control_path = temp_path("control-submit");
        let session_path = temp_path("session-submit");
        append(&control_path, "");
        let mut runtime = runtime_for_paths(&control_path, &session_path);
        runtime
            .coordinator_mut()
            .queue_mut()
            .set_turn_state(TurnState::Active);
        let mut state = AgentTuiLoopState::default();
        let mut terminal = FakeTerminalFrame::new();
        let mut input = input_source([TerminalInputTickOutcome::DraftEffect(
            ComposerDraftEffect::SubmitRequested {
                text: "queued operator note".to_string(),
            },
        )]);
        let mut clock = clock_source();

        let summary = run_injected_interactive_loop(
            &mut runtime,
            &mut state,
            &mut terminal,
            &mut input,
            &mut clock,
            1,
        )
        .expect("interactive loop queues submit");

        assert_eq!(summary.steps_attempted, 1);
        assert!(!summary.exited_by_input);
        assert_eq!(terminal.draw_count, 3);
        assert_eq!(
            terminal.prompt_labels,
            vec![
                "operator note -> sonar.resident>".to_string(),
                "operator note -> sonar.resident>".to_string(),
                "operator note -> sonar.resident>".to_string()
            ]
        );
        assert!(
            terminal
                .status_lines
                .last()
                .expect("final status exists")
                .contains("queued operator steering 1")
        );
        assert!(
            !terminal
                .transcript_texts
                .iter()
                .any(|text| text.contains("queued operator note"))
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn injected_interactive_loop_queues_operator_steering_while_provider_worker_is_active() {
        let control_path = temp_path("control-active-provider-submit");
        let session_path = temp_path("session-active-provider-submit");
        append(&control_path, CONTROL_FIXTURE.trim_end());
        append(&control_path, "\n");
        let (release_sender, release_receiver) = mpsc::channel();
        let mut runtime = AgentTuiInteractiveRuntime::with_provider_adapter(
            "sonar.resident",
            "carrier_fixture_1",
            &control_path,
            &session_path,
            context(),
            Box::new(CancellableProviderAdapter {
                release: release_receiver,
            }),
        );
        let mut state = AgentTuiLoopState::default();
        let mut terminal = FakeTerminalFrame::new();
        let mut input = input_source([TerminalInputTickOutcome::DraftEffect(
            ComposerDraftEffect::SubmitRequested {
                text: "check mailbox after this turn".to_string(),
            },
        )]);
        let mut clock = clock_source();

        let summary = run_injected_interactive_loop(
            &mut runtime,
            &mut state,
            &mut terminal,
            &mut input,
            &mut clock,
            1,
        )
        .expect("interactive loop queues submit while provider is active");

        assert_eq!(summary.steps_attempted, 1);
        assert!(!summary.exited_by_input);
        assert_eq!(terminal.draw_count, 3);
        assert!(
            terminal
                .prompt_labels
                .iter()
                .all(|label| label == "operator note -> sonar.resident>")
        );
        assert!(
            terminal
                .status_lines
                .iter()
                .all(|line| { !line.contains("thinking") && !line.contains("provider working") })
        );
        assert!(terminal.status_lines.iter().any(|line| {
            line.contains("queued operator steering 1") && !line.contains("provider working")
        }));
        assert!(
            !terminal
                .transcript_texts
                .iter()
                .any(|text| text.contains("check mailbox after this turn"))
        );
        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        assert!(session_jsonl.contains("\"queue_state\":\"queued_for_turn_boundary\""));
        assert!(session_jsonl.contains("\"delivery_mode\":\"admit_for_current_turn\""));
        assert!(session_jsonl.contains("\"content_preview\":\"check mailbox after this turn\""));

        release_sender.send(()).expect("release active provider");
        for _ in 0..100 {
            if runtime
                .run_background_step(&state.draft_state(), &clock.next_interactive_step_clock())
                .expect("background step polls released provider")
                .completed_turn
                .is_some()
            {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn interrupt_request_preserves_composer_draft() {
        let control_path = temp_path("control-interrupt-preserve-draft");
        let session_path = temp_path("session-interrupt-preserve-draft");
        append(&control_path, "");
        let mut runtime = runtime_for_paths(&control_path, &session_path);
        runtime
            .coordinator_mut()
            .queue_mut()
            .set_turn_state(TurnState::Active);
        let mut state = AgentTuiLoopState {
            composer: TextareaComposer::from_draft(&ComposerDraftState {
                text: "keep this note".to_string(),
            }),
            ..AgentTuiLoopState::default()
        };
        let mut terminal = FakeTerminalFrame::new();
        let mut input = input_source([TerminalInputTickOutcome::DraftEffect(
            ComposerDraftEffect::ClearOrInterruptRequested,
        )]);
        let mut clock = clock_source();

        let summary = run_injected_interactive_loop(
            &mut runtime,
            &mut state,
            &mut terminal,
            &mut input,
            &mut clock,
            1,
        )
        .expect("interactive loop preserves draft on interrupt");

        assert_eq!(summary.steps_attempted, 1);
        assert_eq!(state.draft_text(), "keep this note");
        assert!(
            terminal
                .composer_texts
                .iter()
                .any(|text| text == "keep this note")
        );
        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        assert!(session_jsonl.contains("\"event_kind\":\"interrupt_requested\""));

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn injected_interactive_loop_interrupt_cancels_active_provider_turn() {
        let control_path = temp_path("control-interrupt-provider");
        let session_path = temp_path("session-interrupt-provider");
        append(&control_path, CONTROL_FIXTURE.trim_end());
        append(&control_path, "\n");
        let (_release_sender, release_receiver) = mpsc::channel();
        let mut runtime = AgentTuiInteractiveRuntime::with_provider_adapter(
            "sonar.resident",
            "carrier_fixture_1",
            &control_path,
            &session_path,
            context(),
            Box::new(CancellableProviderAdapter {
                release: release_receiver,
            }),
        );
        let mut state = AgentTuiLoopState::default();
        let mut terminal = FakeTerminalFrame::new();
        let mut input = input_source([
            TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::ClearOrInterruptRequested),
            TerminalInputTickOutcome::NoInput,
            TerminalInputTickOutcome::NoInput,
            TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::ExitRequested),
        ]);
        let mut clock = clock_source();

        let summary = run_injected_interactive_loop(
            &mut runtime,
            &mut state,
            &mut terminal,
            &mut input,
            &mut clock,
            10,
        )
        .expect("interactive loop cancels active provider");

        assert!(summary.exited_by_input);
        let mut session_jsonl = String::new();
        for _ in 0..100 {
            session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
            if session_jsonl.contains("\"event_kind\":\"turn_interrupted\"") {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let events = session_jsonl
            .lines()
            .map(|line| parse_session_event(line).expect("event parses"))
            .collect::<Vec<_>>();
        let interrupted = events
            .iter()
            .find(|event| event.event_kind == SessionEventKind::TurnInterrupted)
            .expect("interrupted terminal event exists");
        assert_eq!(interrupted.payload["terminal_status"], "interrupted");
        assert_eq!(interrupted.payload["error_summary"], "provider_cancelled");
        assert!(!session_jsonl.contains("provider dispatch interrupted: provider_cancelled"));

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn injected_interactive_loop_accepts_real_terminal_key_events() {
        let _guard = ENV_LOCK.lock().expect("test env lock");
        let control_path = temp_path("control-key-events");
        let session_path = temp_path("session-key-events");
        append(&control_path, "");
        let mut runtime = runtime_for_paths(&control_path, &session_path);
        let mut state = AgentTuiLoopState::default();
        let mut terminal = FakeTerminalFrame::new();
        let mut reader = terminal_reader([
            key_event(KeyCode::Char('h'), KeyModifiers::NONE),
            key_event(KeyCode::Char('i'), KeyModifiers::NONE),
            key_event(KeyCode::Enter, KeyModifiers::NONE),
            key_event(KeyCode::Char('c'), KeyModifiers::CONTROL),
        ]);
        let mut input = TerminalInputTickSource {
            reader: &mut reader,
            wait: Duration::from_millis(0),
        };
        let mut clock = clock_source();

        let summary = run_injected_interactive_loop(
            &mut runtime,
            &mut state,
            &mut terminal,
            &mut input,
            &mut clock,
            10,
        )
        .expect("interactive loop accepts terminal key events");

        assert_eq!(summary.steps_attempted, 4);
        assert!(summary.exited_by_input);
        assert!(summary.final_drawn);
        assert_eq!(state.draft_text(), "");

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        assert!(session_jsonl.contains("\"source_kind\":\"operator\""));
        assert!(session_jsonl.contains("\"transport\":\"interactive_terminal\""));
        assert!(session_jsonl.contains("hi"));
        assert!(session_jsonl.contains("\"provider_request_status\":\"recorded_not_dispatched\""));

        let mut transcript = TranscriptStore::new();
        transcript
            .ingest_jsonl_file_summary(&session_path)
            .expect("session transcript ingests");
        assert_eq!(transcript.items()[0].actor.as_str(), "operator");
        assert_eq!(transcript.items()[0].text, "hi");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn injected_interactive_loop_accepts_operator_submit_and_later_control_input() {
        let _guard = ENV_LOCK.lock().expect("test env lock");
        let control_path = temp_path("control-live-input");
        let session_path = temp_path("session-live-input");
        append(&control_path, "");
        let mut runtime = runtime_for_paths(&control_path, &session_path);
        let mut state = AgentTuiLoopState::default();
        let mut terminal = FakeTerminalFrame::new();
        let mut input = appending_input_source(
            control_path.clone(),
            [
                TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::SubmitRequested {
                    text: "hello from live tui".to_string(),
                }),
                TerminalInputTickOutcome::NoInput,
                TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::ExitRequested),
            ],
        );
        let mut clock = clock_source();

        let summary = run_injected_interactive_loop(
            &mut runtime,
            &mut state,
            &mut terminal,
            &mut input,
            &mut clock,
            10,
        )
        .expect("interactive loop admits terminal and control inputs");

        assert_eq!(summary.steps_attempted, 3);
        assert!(summary.exited_by_input);
        assert!(summary.final_drawn);
        assert!(input.appended);

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        assert!(session_jsonl.contains("\"source_kind\":\"operator\""));
        assert!(session_jsonl.contains("\"transport\":\"interactive_terminal\""));
        assert!(session_jsonl.contains("hello from live tui"));
        assert!(session_jsonl.contains("\"source_kind\":\"system\""));
        assert!(session_jsonl.contains("run startup sequence"));
        assert!(session_jsonl.contains("\"provider_request_status\":\"recorded_not_dispatched\""));

        let events = session_jsonl
            .lines()
            .map(|line| parse_session_event(line).expect("session event parses"))
            .collect::<Vec<_>>();
        assert!(!events.iter().any(|event| matches!(
            event.event_kind,
            SessionEventKind::ProviderToolCallRequested
                | SessionEventKind::ToolCallRequested
                | SessionEventKind::ToolResultReceived
        )));

        let mut transcript = TranscriptStore::new();
        transcript
            .ingest_jsonl_file_summary(&session_path)
            .expect("session transcript ingests");
        let projected = transcript
            .items()
            .iter()
            .map(|item| (item.actor.as_str().to_string(), item.text.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            projected[0],
            ("operator".to_string(), "hello from live tui".to_string())
        );
        let operator_index = projected
            .iter()
            .position(|item| item == &("operator".to_string(), "hello from live tui".to_string()))
            .expect("operator input projected");
        let system_index = projected
            .iter()
            .position(|item| item == &("system".to_string(), "run startup sequence".to_string()))
            .expect("system input projected");
        assert!(operator_index < system_index);

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn injected_interactive_loop_exits_after_exit_input_and_still_final_draws() {
        let control_path = temp_path("control-exit");
        let session_path = temp_path("session-exit");
        append(&control_path, "");
        let mut runtime = runtime_for_paths(&control_path, &session_path);
        let mut state = AgentTuiLoopState::default();
        let mut terminal = FakeTerminalFrame::new();
        let mut input = input_source([TerminalInputTickOutcome::DraftEffect(
            ComposerDraftEffect::ExitRequested,
        )]);
        let mut clock = clock_source();

        let summary = run_injected_interactive_loop(
            &mut runtime,
            &mut state,
            &mut terminal,
            &mut input,
            &mut clock,
            5,
        )
        .expect("interactive loop exits cleanly");

        assert_eq!(summary.steps_attempted, 1);
        assert!(summary.exited_by_input);
        assert!(summary.final_drawn);
        assert!(state.should_exit);
        assert_eq!(terminal.draw_count, 2);

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn injected_interactive_loop_surfaces_input_read_failure_without_stopping() {
        let control_path = temp_path("control-read-failure");
        let session_path = temp_path("session-read-failure");
        append(&control_path, "");
        let mut runtime = runtime_for_paths(&control_path, &session_path);
        let mut state = AgentTuiLoopState::default();
        let mut terminal = FakeTerminalFrame::new();
        let mut input = input_source([TerminalInputTickOutcome::ReadFailed(
            "read failed".to_string(),
        )]);
        let mut clock = clock_source();

        let summary = run_injected_interactive_loop(
            &mut runtime,
            &mut state,
            &mut terminal,
            &mut input,
            &mut clock,
            1,
        )
        .expect("interactive loop records read failure");

        assert_eq!(summary.steps_attempted, 1);
        assert!(!summary.exited_by_input);
        assert_eq!(state.last_error.as_deref(), Some("read failed"));
        assert_eq!(terminal.draw_count, 3);
        assert!(
            terminal
                .status_lines
                .last()
                .expect("final status exists")
                .contains("error read failed")
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn injected_interactive_loop_surfaces_malformed_control_jsonl_without_stopping() {
        let control_path = temp_path("control-malformed");
        let session_path = temp_path("session-malformed");
        append(&control_path, "{not valid json}\n");
        let mut runtime = runtime_for_paths(&control_path, &session_path);
        let mut state = AgentTuiLoopState::default();
        let mut terminal = FakeTerminalFrame::new();
        let mut input = input_source([TerminalInputTickOutcome::NoInput]);
        let mut clock = clock_source();

        let summary = run_injected_interactive_loop(
            &mut runtime,
            &mut state,
            &mut terminal,
            &mut input,
            &mut clock,
            1,
        )
        .expect("interactive loop records malformed control jsonl");

        assert_eq!(summary.steps_attempted, 1);
        assert!(!summary.exited_by_input);
        assert_eq!(
            state.last_error.as_deref(),
            Some("control_jsonl_parse_errors:1")
        );
        assert_eq!(terminal.draw_count, 2);
        assert!(
            terminal
                .status_lines
                .last()
                .expect("final status exists")
                .contains("error control jsonl parse errors:1")
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn injected_interactive_loop_recovers_from_malformed_control_jsonl() {
        let _guard = ENV_LOCK.lock().expect("test env lock");
        let control_path = temp_path("control-malformed-recovery");
        let session_path = temp_path("session-malformed-recovery");
        append(&control_path, "{not valid json}\n");
        let mut runtime = runtime_for_paths(&control_path, &session_path);
        let mut state = AgentTuiLoopState::default();
        let mut terminal = FakeTerminalFrame::new();
        let mut input = appending_input_source(
            control_path.clone(),
            [
                TerminalInputTickOutcome::NoInput,
                TerminalInputTickOutcome::NoInput,
                TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::ExitRequested),
            ],
        );
        let mut clock = clock_source();

        let summary = run_injected_interactive_loop(
            &mut runtime,
            &mut state,
            &mut terminal,
            &mut input,
            &mut clock,
            10,
        )
        .expect("interactive loop recovers from malformed control jsonl");

        assert_eq!(summary.steps_attempted, 3);
        assert!(summary.exited_by_input);
        assert!(input.appended);
        assert_eq!(
            state.last_error.as_deref(),
            Some("control_jsonl_parse_errors:1")
        );
        assert!(
            terminal
                .status_lines
                .iter()
                .any(|line| line.contains("error control jsonl parse errors:1"))
        );

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        assert!(session_jsonl.contains("\"source_kind\":\"system\""));
        assert!(session_jsonl.contains("run startup sequence"));
        assert!(session_jsonl.contains("\"provider_request_status\":\"recorded_not_dispatched\""));

        let mut transcript = TranscriptStore::new();
        transcript
            .ingest_jsonl_file_summary(&session_path)
            .expect("session transcript ingests");
        assert!(
            transcript
                .items()
                .iter()
                .any(|item| item.actor.as_str() == "system" && item.text == "run startup sequence")
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }
}
