use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use narada_agent_tui::app_view_model::{AppViewInput, build_app_view};
use narada_agent_tui::composer_draft::{ComposerDraftEffect, ComposerDraftState};
use narada_agent_tui::composer_view_model::ComposerViewInput;
use narada_agent_tui::layout_model::{LayoutConfig, TerminalSize};
use narada_agent_tui::projection_state::TurnState;
use narada_agent_tui::ratatui_renderer::render_app_to_buffer;
use narada_agent_tui::status_view_model::{RuntimePostureState, StatusViewInput};
use narada_agent_tui::terminal_input_tick::{
    TerminalInputReader, TerminalInputTickOutcome, run_textarea_composer_input_tick,
};
use narada_agent_tui::textarea_composer::TextareaComposer;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect as TuiRect;
use std::collections::VecDeque;
use std::io;
use std::time::Duration;

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
fn composer_redraw_preserves_draft_across_key_ticks() {
    let mut composer = TextareaComposer::default();
    let mut reader = FakeReader {
        events: VecDeque::from(vec![
            key_event(KeyCode::Char('r'), KeyModifiers::NONE),
            key_event(KeyCode::Char('u'), KeyModifiers::NONE),
            key_event(KeyCode::Char('n'), KeyModifiers::NONE),
        ]),
    };

    for expected in ["r", "ru", "run"] {
        assert_eq!(
            run_textarea_composer_input_tick(&mut reader, &mut composer),
            TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::DraftChanged)
        );
        assert_eq!(composer.text(), expected);
        assert!(render_draft_text(composer.text()).contains(expected));
    }
}

#[test]
fn composer_redraw_backspace_updates_rendered_draft_without_submit() {
    let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
        text: "runx".to_string(),
    });
    let mut reader = FakeReader {
        events: VecDeque::from(vec![key_event(KeyCode::Backspace, KeyModifiers::NONE)]),
    };

    assert_eq!(
        run_textarea_composer_input_tick(&mut reader, &mut composer),
        TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::DraftChanged)
    );
    assert_eq!(composer.text(), "run");
    assert!(render_draft_text(composer.text()).contains("run"));
}

#[test]
fn composer_submit_returns_text_and_clears_local_draft() {
    let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
        text: " run startup sequence ".to_string(),
    });
    let mut reader = FakeReader {
        events: VecDeque::from(vec![key_event(KeyCode::Enter, KeyModifiers::NONE)]),
    };

    assert_eq!(
        run_textarea_composer_input_tick(&mut reader, &mut composer),
        TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::SubmitRequested {
            text: " run startup sequence ".to_string(),
        })
    );
    assert!(composer.is_empty());
    assert!(render_draft_text(composer.text()).contains("operator -> sonar.resident>"));
}
