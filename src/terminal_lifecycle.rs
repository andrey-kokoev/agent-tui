use crate::app_view_model::AppViewModel;
use crate::ratatui_renderer::{render_app_to_frame, render_app_to_frame_with_composer};
use crate::textarea_composer::TextareaComposer;
use crate::tui_render_loop::InteractiveTerminalFrame;
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::{Backend, CrosstermBackend};
use std::io::{Stdout, stdout};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalLifecycleState {
    Normal,
    TuiActive,
}

#[derive(Debug)]
pub struct TerminalLifecycle {
    state: TerminalLifecycleState,
}

impl TerminalLifecycle {
    pub fn new() -> Self {
        Self {
            state: TerminalLifecycleState::Normal,
        }
    }

    pub fn state(&self) -> TerminalLifecycleState {
        self.state
    }

    pub fn mark_entered(&mut self) -> Result<(), String> {
        if self.state == TerminalLifecycleState::TuiActive {
            return Err("terminal_lifecycle_already_active".to_string());
        }
        self.state = TerminalLifecycleState::TuiActive;
        Ok(())
    }

    pub fn mark_left(&mut self) -> Result<(), String> {
        if self.state == TerminalLifecycleState::Normal {
            return Err("terminal_lifecycle_not_active".to_string());
        }
        self.state = TerminalLifecycleState::Normal;
        Ok(())
    }

    pub fn run_guarded<T>(
        &mut self,
        operation: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        self.mark_entered()?;
        let operation_result = operation();
        let leave_result = self.mark_left();

        match (operation_result, leave_result) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), Ok(())) => Err(error),
            (Ok(_), Err(leave_error)) => Err(leave_error),
            (Err(error), Err(leave_error)) => Err(format!(
                "{error}; terminal_lifecycle_leave_failed:{leave_error}"
            )),
        }
    }
}

impl Default for TerminalLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

pub fn draw_model_to_terminal<B: Backend>(
    terminal: &mut Terminal<B>,
    model: &AppViewModel,
) -> Result<(), String> {
    terminal
        .draw(|frame| render_app_to_frame(model, frame))
        .map(|_| ())
        .map_err(|error| format!("terminal_draw_failed:{error}"))
}

pub fn draw_model_with_composer_to_terminal<B: Backend>(
    terminal: &mut Terminal<B>,
    model: &AppViewModel,
    composer: &TextareaComposer,
) -> Result<(), String> {
    terminal
        .draw(|frame| render_app_to_frame_with_composer(model, composer, frame))
        .map(|_| ())
        .map_err(|error| format!("terminal_draw_failed:{error}"))
}

pub struct TerminalLifecycleHarness<B: Backend> {
    terminal: Terminal<B>,
    lifecycle: TerminalLifecycle,
}

impl<B: Backend> TerminalLifecycleHarness<B> {
    pub fn enter_from_terminal(terminal: Terminal<B>) -> Result<Self, String> {
        let mut lifecycle = TerminalLifecycle::new();
        lifecycle.mark_entered()?;
        Ok(Self {
            terminal,
            lifecycle,
        })
    }

    pub fn state(&self) -> TerminalLifecycleState {
        self.lifecycle.state()
    }

    pub fn backend(&self) -> &B {
        self.terminal.backend()
    }

    pub fn draw_once(&mut self, model: &AppViewModel) -> Result<(), String> {
        draw_model_to_terminal(&mut self.terminal, model)
    }

    pub fn draw_once_with_composer(
        &mut self,
        model: &AppViewModel,
        composer: &TextareaComposer,
    ) -> Result<(), String> {
        draw_model_with_composer_to_terminal(&mut self.terminal, model, composer)
    }

    pub fn leave(mut self) -> Result<(), String> {
        self.leave_inner()
    }

    fn leave_inner(&mut self) -> Result<(), String> {
        if self.lifecycle.state() == TerminalLifecycleState::Normal {
            return Ok(());
        }
        self.lifecycle.mark_left()
    }
}

impl<B: Backend> InteractiveTerminalFrame for TerminalLifecycleHarness<B> {
    fn terminal_size(&mut self) -> Result<crate::layout_model::TerminalSize, String> {
        let area = self
            .terminal
            .size()
            .map_err(|error| format!("terminal_size_read_failed:{error}"))?;
        Ok(crate::layout_model::TerminalSize {
            width: area.width,
            height: area.height,
        })
    }

    fn draw_frame(
        &mut self,
        model: &AppViewModel,
        composer: &TextareaComposer,
    ) -> Result<(), String> {
        self.draw_once_with_composer(model, composer)
    }
}

impl<B: Backend> Drop for TerminalLifecycleHarness<B> {
    fn drop(&mut self) {
        let _ = self.leave_inner();
    }
}

pub struct TerminalSession {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    lifecycle: TerminalLifecycle,
}

impl TerminalSession {
    pub fn enter() -> Result<Self, String> {
        let mut stdout = stdout();
        enable_raw_mode().map_err(|error| format!("terminal_raw_mode_enable_failed:{error}"))?;
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(format!("terminal_alternate_screen_enter_failed:{error}"));
        }
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)
            .map_err(|error| format!("terminal_backend_create_failed:{error}"))?;
        let mut lifecycle = TerminalLifecycle::new();
        lifecycle.mark_entered()?;
        Ok(Self {
            terminal,
            lifecycle,
        })
    }

    pub fn state(&self) -> TerminalLifecycleState {
        self.lifecycle.state()
    }

    pub fn draw_once(&mut self, model: &AppViewModel) -> Result<(), String> {
        draw_model_to_terminal(&mut self.terminal, model)
    }

    pub fn draw_once_with_composer(
        &mut self,
        model: &AppViewModel,
        composer: &TextareaComposer,
    ) -> Result<(), String> {
        draw_model_with_composer_to_terminal(&mut self.terminal, model, composer)
    }

    pub fn leave(mut self) -> Result<(), String> {
        self.leave_inner()
    }

    fn leave_inner(&mut self) -> Result<(), String> {
        if self.lifecycle.state() == TerminalLifecycleState::Normal {
            return Ok(());
        }
        execute!(self.terminal.backend_mut(), LeaveAlternateScreen)
            .map_err(|error| format!("terminal_alternate_screen_leave_failed:{error}"))?;
        disable_raw_mode().map_err(|error| format!("terminal_raw_mode_disable_failed:{error}"))?;
        self.lifecycle.mark_left()
    }
}

impl InteractiveTerminalFrame for TerminalSession {
    fn terminal_size(&mut self) -> Result<crate::layout_model::TerminalSize, String> {
        let area = self
            .terminal
            .size()
            .map_err(|error| format!("terminal_size_read_failed:{error}"))?;
        Ok(crate::layout_model::TerminalSize {
            width: area.width,
            height: area.height,
        })
    }

    fn draw_frame(
        &mut self,
        model: &AppViewModel,
        composer: &TextareaComposer,
    ) -> Result<(), String> {
        self.draw_once_with_composer(model, composer)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.leave_inner();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_view_model::{AppViewInput, build_app_view};
    use crate::composer_draft::ComposerDraftState;
    use crate::composer_view_model::ComposerViewInput;
    use crate::input_queue::TurnState;
    use crate::layout_model::{LayoutConfig, TerminalSize};
    use crate::status_view_model::{ProviderRuntimeState, RuntimePostureState, StatusViewInput};
    use crate::transcript_projection::{TranscriptActor, TranscriptItem, TranscriptItemKind};
    use ratatui::backend::TestBackend;
    use ratatui::style::{Color, Modifier};

    fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
        let area = buffer.area;
        let mut output = String::new();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                output.push_str(buffer[(x, y)].symbol());
            }
            output.push('\n');
        }
        output
    }

    fn find_text_position(buffer: &ratatui::buffer::Buffer, needle: &str) -> Option<(u16, u16)> {
        let area = buffer.area;
        for y in area.y..area.y + area.height {
            let mut line = String::new();
            for x in area.x..area.x + area.width {
                line.push_str(buffer[(x, y)].symbol());
            }
            if let Some(index) = line.find(needle) {
                let column = line[..index].chars().count() as u16;
                return Some((area.x + column, y));
            }
        }
        None
    }

    fn assert_cell_style(
        buffer: &ratatui::buffer::Buffer,
        x: u16,
        y: u16,
        fg: Color,
        modifier: Modifier,
    ) {
        let cell = &buffer[(x, y)];
        assert_eq!(cell.fg, fg);
        assert!(cell.modifier.contains(modifier));
    }

    #[test]
    fn lifecycle_starts_normal() {
        let lifecycle = TerminalLifecycle::new();

        assert_eq!(lifecycle.state(), TerminalLifecycleState::Normal);
    }

    #[test]
    fn lifecycle_marks_enter_and_leave() {
        let mut lifecycle = TerminalLifecycle::new();

        lifecycle.mark_entered().expect("enter succeeds");
        assert_eq!(lifecycle.state(), TerminalLifecycleState::TuiActive);
        lifecycle.mark_left().expect("leave succeeds");
        assert_eq!(lifecycle.state(), TerminalLifecycleState::Normal);
    }

    #[test]
    fn lifecycle_rejects_double_enter() {
        let mut lifecycle = TerminalLifecycle::new();

        lifecycle.mark_entered().expect("enter succeeds");
        let error = lifecycle.mark_entered().expect_err("double enter rejected");
        assert_eq!(error, "terminal_lifecycle_already_active");
    }

    #[test]
    fn lifecycle_rejects_leave_when_not_active() {
        let mut lifecycle = TerminalLifecycle::new();

        let error = lifecycle.mark_left().expect_err("inactive leave rejected");
        assert_eq!(error, "terminal_lifecycle_not_active");
    }

    #[test]
    fn guarded_lifecycle_leaves_after_success() {
        let mut lifecycle = TerminalLifecycle::new();

        let value = lifecycle
            .run_guarded(|| Ok("rendered".to_string()))
            .expect("guarded operation succeeds");

        assert_eq!(value, "rendered");
        assert_eq!(lifecycle.state(), TerminalLifecycleState::Normal);
    }

    #[test]
    fn guarded_lifecycle_leaves_after_error() {
        let mut lifecycle = TerminalLifecycle::new();

        let error = lifecycle
            .run_guarded(|| Err::<(), String>("render_loop_failed".to_string()))
            .expect_err("guarded operation returns error");

        assert_eq!(error, "render_loop_failed");
        assert_eq!(lifecycle.state(), TerminalLifecycleState::Normal);
    }

    fn active_model() -> AppViewModel {
        build_app_view(&AppViewInput {
            terminal_size: TerminalSize {
                width: 80,
                height: 12,
            },
            layout_config: LayoutConfig::default(),
            transcript_items: vec![TranscriptItem {
                kind: TranscriptItemKind::ProviderTextDelta,
                actor: TranscriptActor::Agent,
                turn_id: "turn_1".to_string(),
                text: "working".to_string(),
                sequence: None,
                projection_key: None,
                occurred_at: Some("2026-05-30T00:00:00.000Z".to_string()),
            }],
            status: StatusViewInput {
                identity: "sonar.resident".to_string(),
                session: "carrier_1".to_string(),
                turn_state: TurnState::Active,
                active_phase: None,
                active_turn_age: Some("1m 12s".to_string()),
                queued_inputs: 1,
                held_system_directives: 0,
                oldest_held_age: None,
                transcript_items: 1,
                runtime_posture: RuntimePostureState {
                    provider_state: ProviderRuntimeState::Working,
                    ..RuntimePostureState::disabled()
                },
                last_error: None,
            },
            composer: ComposerViewInput {
                identity: "sonar.resident".to_string(),
                draft_text: "stale snapshot".to_string(),
                turn_state: TurnState::Active,
                queued_operator_notes: 1,
                held_system_directives: 0,
            },
        })
    }

    #[test]
    fn generic_terminal_draw_renders_live_composer_frame() {
        let model = active_model();
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "live steering note".to_string(),
        });
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).expect("test terminal creates");

        draw_model_with_composer_to_terminal(&mut terminal, &model, &composer)
            .expect("test terminal draws");

        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Transcript"));
        assert!(text.contains("operator note -> sonar.resident>"));
        assert!(!text.contains("Composer:"));
        assert!(text.contains("live steering note"));
        assert!(text.contains("thinking"));
        assert!(!text.contains("stale snapshot"));
    }

    #[test]
    fn lifecycle_harness_preserves_polished_transcript_styles_through_terminal_draw() {
        let model = build_app_view(&AppViewInput {
            terminal_size: TerminalSize {
                width: 120,
                height: 16,
            },
            layout_config: LayoutConfig::default(),
            transcript_items: vec![
                TranscriptItem {
                    kind: TranscriptItemKind::InputAdmitted,
                    actor: TranscriptActor::Operator,
                    turn_id: "turn_1".to_string(),
                    text: "run startup sequence".to_string(),
                    sequence: None,
                    projection_key: None,
                    occurred_at: Some("2026-05-30T00:00:00.000Z".to_string()),
                },
                TranscriptItem {
                    kind: TranscriptItemKind::ToolResultReceived,
                    actor: TranscriptActor::AgentTui,
                    turn_id: "turn_1".to_string(),
                    text: "ok site_loop_run_once in 12ms".to_string(),
                    sequence: None,
                    projection_key: None,
                    occurred_at: Some("2026-05-30T00:00:01.000Z".to_string()),
                },
            ],
            status: StatusViewInput {
                identity: "sonar.resident".to_string(),
                session: "carrier_1".to_string(),
                turn_state: TurnState::Idle,
                active_phase: None,
                active_turn_age: None,
                queued_inputs: 0,
                held_system_directives: 0,
                oldest_held_age: None,
                transcript_items: 2,
                runtime_posture: RuntimePostureState::disabled(),
                last_error: None,
            },
            composer: ComposerViewInput {
                identity: "sonar.resident".to_string(),
                draft_text: String::new(),
                turn_state: TurnState::Idle,
                queued_operator_notes: 0,
                held_system_directives: 0,
            },
        });
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "operator draft".to_string(),
        });
        let backend = TestBackend::new(120, 16);
        let terminal = Terminal::new(backend).expect("test terminal creates");
        let mut harness = TerminalLifecycleHarness::enter_from_terminal(terminal)
            .expect("harness enters terminal lifecycle");

        harness
            .draw_once_with_composer(&model, &composer)
            .expect("harness draws polished frame");

        let buffer = harness.backend().buffer();
        let text = buffer_text(buffer);
        assert!(text.contains("operator -> sonar.resident: run startup sequence"));
        assert!(!text.contains("operator -> sonar.resident:\n  run startup sequence"));
        assert!(text.contains(
            "agent-tui -> sonar.resident: ok site_loop_run_once in 12ms  2026-05-30Z00:00"
        ));
        assert!(text.contains("operator -> sonar.resident>"));
        assert!(text.contains("operator draft"));

        let (operator_x, operator_y) = find_text_position(buffer, "operator -> sonar.resident:")
            .expect("operator transcript label is visible");
        let (agent_tui_x, agent_tui_y) = find_text_position(buffer, "agent-tui -> sonar.resident:")
            .expect("tool result label is visible");
        let (result_x, _) = find_text_position(buffer, "ok site_loop_run_once in 12ms")
            .expect("tool result body is visible");
        let tool_line_prefix = "agent-tui -> sonar.resident: ok site_loop_run_once in 12ms  ";
        let (tool_line_x, timestamp_y) = find_text_position(
            buffer,
            "agent-tui -> sonar.resident: ok site_loop_run_once in 12ms  2026-05-30Z00:00",
        )
        .expect("inline tool timestamp is visible on tool result row");
        let timestamp_x = tool_line_x + tool_line_prefix.chars().count() as u16;
        let (composer_x, composer_y) = find_text_position(buffer, "operator -> sonar.resident>")
            .expect("composer prompt is visible");
        let agent_target_x = operator_x + "operator -> ".chars().count() as u16;
        let tool_target_x = agent_tui_x + "agent-tui -> ".chars().count() as u16;
        let timestamp_time_x = timestamp_x + "2026-05-30Z".chars().count() as u16;

        assert_cell_style(buffer, operator_x, operator_y, Color::Green, Modifier::BOLD);
        assert_cell_style(
            buffer,
            agent_target_x,
            operator_y,
            Color::Cyan,
            Modifier::BOLD,
        );
        assert_cell_style(
            buffer,
            agent_tui_x,
            agent_tui_y,
            Color::Magenta,
            Modifier::BOLD,
        );
        assert_cell_style(
            buffer,
            tool_target_x,
            agent_tui_y,
            Color::Cyan,
            Modifier::BOLD,
        );
        assert_eq!(buffer[(result_x, agent_tui_y)].fg, Color::Green);
        assert_eq!(timestamp_y, agent_tui_y);
        assert_eq!(buffer[(timestamp_x, timestamp_y)].fg, Color::DarkGray);
        assert_eq!(buffer[(timestamp_time_x, timestamp_y)].fg, Color::Gray);
        assert_cell_style(buffer, composer_x, composer_y, Color::Green, Modifier::BOLD);

        harness.leave().expect("harness leaves cleanly");
    }

    fn draw_through_terminal_frame_contract<T: InteractiveTerminalFrame>(
        terminal: &mut T,
        model: &AppViewModel,
        composer: &TextareaComposer,
    ) -> Result<TerminalSize, String> {
        let size = terminal.terminal_size()?;
        terminal.draw_frame(model, composer)?;
        Ok(size)
    }

    fn assert_interactive_terminal_frame<T: InteractiveTerminalFrame>() {}

    #[test]
    fn lifecycle_harness_records_enter_draw_and_leave() {
        let model = active_model();
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "live harness note".to_string(),
        });
        let backend = TestBackend::new(80, 12);
        let terminal = Terminal::new(backend).expect("test terminal creates");
        let mut harness = TerminalLifecycleHarness::enter_from_terminal(terminal)
            .expect("harness enters terminal lifecycle");

        assert_eq!(harness.state(), TerminalLifecycleState::TuiActive);
        harness
            .draw_once_with_composer(&model, &composer)
            .expect("harness draws frame");

        let text = buffer_text(harness.backend().buffer());
        assert!(text.contains("live harness note"));
        assert!(text.contains("operator note -> sonar.resident>"));
        assert!(!text.contains("Composer:"));

        harness.leave().expect("harness leaves cleanly");
    }

    #[test]
    fn terminal_session_and_harness_share_interactive_frame_contract() {
        assert_interactive_terminal_frame::<TerminalSession>();
        assert_interactive_terminal_frame::<TerminalLifecycleHarness<TestBackend>>();

        let model = active_model();
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "contract note".to_string(),
        });
        let backend = TestBackend::new(80, 12);
        let terminal = Terminal::new(backend).expect("test terminal creates");
        let mut harness = TerminalLifecycleHarness::enter_from_terminal(terminal)
            .expect("harness enters terminal lifecycle");

        let size = draw_through_terminal_frame_contract(&mut harness, &model, &composer)
            .expect("contract draw succeeds");

        assert_eq!(
            size,
            TerminalSize {
                width: 80,
                height: 12
            }
        );
        let text = buffer_text(harness.backend().buffer());
        assert!(text.contains("contract note"));
        assert!(text.contains("operator note -> sonar.resident>"));
        assert!(!text.contains("Composer:"));

        harness.leave().expect("harness leaves cleanly");
    }
}
