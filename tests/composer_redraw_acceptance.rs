use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use narada_agent_tui::app_view_model::{AppViewInput, build_app_view};
use narada_agent_tui::composer_view_model::ComposerViewInput;
use narada_agent_tui::input_queue::TurnState;
use narada_agent_tui::layout_model::{LayoutConfig, TerminalSize};
use narada_agent_tui::ratatui_renderer::render_app_to_buffer;
use narada_agent_tui::status_view_model::{RuntimePostureState, StatusViewInput};
use narada_agent_tui::terminal_input_tick::{
    TerminalInputReader, run_textarea_composer_input_tick,
};
use narada_agent_tui::textarea_composer::TextareaComposer;
use narada_agent_tui::transcript_projection::TranscriptItem;
use narada_agent_tui::tui_render_loop::{
    AgentTuiLoopState, ComposerAdmissionBridge, RenderLoopAction, apply_input_tick_outcome,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect as TuiRect;
use std::collections::VecDeque;
use std::io;
use std::time::Duration;

#[derive(Debug, Default)]
struct FakeBridge {
    submitted: Vec<String>,
    interrupts: usize,
}

impl ComposerAdmissionBridge for FakeBridge {
    fn submit_operator_text(&mut self, text: String) -> Result<Option<TranscriptItem>, String> {
        self.submitted.push(text);
        Ok(None)
    }

    fn request_interrupt(&mut self) -> Result<(), String> {
        self.interrupts += 1;
        Ok(())
    }
}

struct FakeReader {
    events: VecDeque<Event>,
}

impl TerminalInputReader for FakeReader {
    fn poll_input(&mut self, _wait: Duration) -> io::Result<bool> {
        Ok(!self.events.is_empty())
    }

    fn read_input(&mut self) -> io::Result<Event> {
        self.events
            .pop_front()
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "no event"))
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

fn buffer_text(buffer: &Buffer) -> String {
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

fn render_draft_text(draft_text: String) -> String {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 80,
            height: 10,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: Vec::new(),
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 0,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text,
            turn_state: TurnState::Idle,
            queued_operator_notes: 0,
            held_system_directives: 0,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 10));
    render_app_to_buffer(&model, &mut buffer);
    buffer_text(&buffer)
}

#[test]
fn composer_redraw_acceptance_preserves_draft_across_key_ticks() {
    let mut state = AgentTuiLoopState::default();
    let mut bridge = FakeBridge::default();
    let mut reader = FakeReader {
        events: VecDeque::from(vec![
            key_event(KeyCode::Char('r'), KeyModifiers::NONE),
            key_event(KeyCode::Char('u'), KeyModifiers::NONE),
            key_event(KeyCode::Char('n'), KeyModifiers::NONE),
        ]),
    };

    for expected in ["r", "ru", "run"] {
        let outcome = run_textarea_composer_input_tick(&mut reader, &mut state.composer);
        let action = apply_input_tick_outcome(&mut state, &mut bridge, outcome);
        assert_eq!(action, RenderLoopAction::Redraw);
        assert_eq!(state.draft_text(), expected);
        assert!(render_draft_text(state.draft_text()).contains(expected));
    }

    assert!(bridge.submitted.is_empty());
    assert_eq!(bridge.interrupts, 0);
    assert!(!state.should_exit);
}

#[test]
fn composer_redraw_acceptance_backspace_updates_rendered_draft_without_submit() {
    let mut state = AgentTuiLoopState::default();
    state.composer =
        TextareaComposer::from_draft(&narada_agent_tui::composer_draft::ComposerDraftState {
            text: "runx".to_string(),
        });
    let mut bridge = FakeBridge::default();
    let mut reader = FakeReader {
        events: VecDeque::from(vec![key_event(KeyCode::Backspace, KeyModifiers::NONE)]),
    };

    let outcome = run_textarea_composer_input_tick(&mut reader, &mut state.composer);
    let action = apply_input_tick_outcome(&mut state, &mut bridge, outcome);

    assert_eq!(action, RenderLoopAction::Redraw);
    assert_eq!(state.draft_text(), "run");
    assert!(render_draft_text(state.draft_text()).contains("run"));
    assert!(bridge.submitted.is_empty());
}

#[test]
fn composer_redraw_acceptance_submit_clears_draft_and_uses_bridge_once() {
    let mut state = AgentTuiLoopState::default();
    state.composer =
        TextareaComposer::from_draft(&narada_agent_tui::composer_draft::ComposerDraftState {
            text: " run startup sequence ".to_string(),
        });
    let mut bridge = FakeBridge::default();
    let mut reader = FakeReader {
        events: VecDeque::from(vec![key_event(KeyCode::Enter, KeyModifiers::NONE)]),
    };

    let outcome = run_textarea_composer_input_tick(&mut reader, &mut state.composer);
    let action = apply_input_tick_outcome(&mut state, &mut bridge, outcome);

    assert_eq!(action, RenderLoopAction::Redraw);
    assert_eq!(state.draft_text(), "");
    assert_eq!(bridge.submitted, vec![" run startup sequence ".to_string()]);
    assert!(render_draft_text(state.draft_text()).contains("operator -> sonar.resident>"));
}
