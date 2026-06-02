use crate::composer_draft::{ComposerDraftEffect, ComposerDraftState};
use crate::terminal_input::{TerminalInputIntent, decode_key_event};
use crate::textarea_composer::TextareaComposer;
use crossterm::event::{Event, poll, read};
use std::io;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalInputTickOutcome {
    NoInput,
    DraftEffect(ComposerDraftEffect),
    ScrollTranscriptUp,
    ScrollTranscriptDown,
    NonKeyEventIgnored,
    ReadFailed(String),
}

pub trait TerminalInputReader {
    fn poll_input(&mut self, wait: Duration) -> io::Result<bool>;
    fn read_input(&mut self) -> io::Result<Event>;
}

#[derive(Debug, Default)]
pub struct CrosstermTerminalInputReader;

impl TerminalInputReader for CrosstermTerminalInputReader {
    fn poll_input(&mut self, wait: Duration) -> io::Result<bool> {
        poll(wait)
    }

    fn read_input(&mut self) -> io::Result<Event> {
        read()
    }
}

pub fn run_terminal_input_tick<R: TerminalInputReader>(
    reader: &mut R,
    draft: &mut ComposerDraftState,
) -> TerminalInputTickOutcome {
    run_terminal_input_tick_with_wait(reader, draft, Duration::from_millis(0))
}

pub fn run_terminal_input_tick_with_wait<R: TerminalInputReader>(
    reader: &mut R,
    draft: &mut ComposerDraftState,
    wait: Duration,
) -> TerminalInputTickOutcome {
    let mut composer = TextareaComposer::from_draft(draft);
    let outcome = run_textarea_composer_input_tick_with_wait(reader, &mut composer, wait);
    *draft = composer.draft_state();
    outcome
}

pub fn run_textarea_composer_input_tick<R: TerminalInputReader>(
    reader: &mut R,
    composer: &mut TextareaComposer,
) -> TerminalInputTickOutcome {
    run_textarea_composer_input_tick_with_wait(reader, composer, Duration::from_millis(0))
}

pub fn run_textarea_composer_input_tick_with_wait<R: TerminalInputReader>(
    reader: &mut R,
    composer: &mut TextareaComposer,
    wait: Duration,
) -> TerminalInputTickOutcome {
    match reader.poll_input(wait) {
        Ok(false) => TerminalInputTickOutcome::NoInput,
        Err(error) => TerminalInputTickOutcome::ReadFailed(error.to_string()),
        Ok(true) => match reader.read_input() {
            Ok(Event::Key(key_event)) => match decode_key_event(key_event) {
                TerminalInputIntent::ScrollTranscriptUp => {
                    TerminalInputTickOutcome::ScrollTranscriptUp
                }
                TerminalInputIntent::ScrollTranscriptDown => {
                    TerminalInputTickOutcome::ScrollTranscriptDown
                }
                intent => TerminalInputTickOutcome::DraftEffect(composer.apply_intent(intent)),
            },
            Ok(Event::Paste(text)) => TerminalInputTickOutcome::DraftEffect(
                composer.apply_intent(TerminalInputIntent::InsertText(text)),
            ),
            Ok(_) => TerminalInputTickOutcome::NonKeyEventIgnored,
            Err(error) => TerminalInputTickOutcome::ReadFailed(error.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{
        KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseEvent, MouseEventKind,
    };

    struct FakeReader {
        poll_result: io::Result<bool>,
        read_result: Option<io::Result<Event>>,
        observed_wait: Option<Duration>,
    }

    impl TerminalInputReader for FakeReader {
        fn poll_input(&mut self, wait: Duration) -> io::Result<bool> {
            self.observed_wait = Some(wait);
            match &self.poll_result {
                Ok(value) => Ok(*value),
                Err(error) => Err(io::Error::new(error.kind(), error.to_string())),
            }
        }

        fn read_input(&mut self) -> io::Result<Event> {
            self.read_result
                .take()
                .unwrap_or_else(|| Err(io::Error::new(io::ErrorKind::UnexpectedEof, "no event")))
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

    #[test]
    fn reports_no_input_without_changing_draft() {
        let mut reader = FakeReader {
            poll_result: Ok(false),
            read_result: None,
            observed_wait: None,
        };
        let mut draft = ComposerDraftState {
            text: "draft".to_string(),
        };

        assert_eq!(
            run_terminal_input_tick(&mut reader, &mut draft),
            TerminalInputTickOutcome::NoInput
        );
        assert_eq!(draft.text, "draft");
    }

    #[test]
    fn accepts_explicit_wait_interval() {
        let mut reader = FakeReader {
            poll_result: Ok(false),
            read_result: None,
            observed_wait: None,
        };
        let mut draft = ComposerDraftState::default();

        assert_eq!(
            run_terminal_input_tick_with_wait(&mut reader, &mut draft, Duration::from_millis(25)),
            TerminalInputTickOutcome::NoInput
        );
        assert_eq!(reader.observed_wait, Some(Duration::from_millis(25)));
    }

    #[test]
    fn applies_key_event_to_draft_through_textarea_composer() {
        let mut reader = FakeReader {
            poll_result: Ok(true),
            read_result: Some(Ok(key_event(KeyCode::Char('x'), KeyModifiers::NONE))),
            observed_wait: None,
        };
        let mut draft = ComposerDraftState::default();

        assert_eq!(
            run_terminal_input_tick(&mut reader, &mut draft),
            TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::DraftChanged)
        );
        assert_eq!(draft.text, "x");
    }

    #[test]
    fn reports_transcript_scroll_keys_without_mutating_composer() {
        let mut reader = FakeReader {
            poll_result: Ok(true),
            read_result: Some(Ok(key_event(KeyCode::PageUp, KeyModifiers::NONE))),
            observed_wait: None,
        };
        let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "draft".to_string(),
        });

        assert_eq!(
            run_textarea_composer_input_tick(&mut reader, &mut composer),
            TerminalInputTickOutcome::ScrollTranscriptUp
        );
        assert_eq!(composer.text(), "draft");

        let mut reader = FakeReader {
            poll_result: Ok(true),
            read_result: Some(Ok(key_event(KeyCode::PageDown, KeyModifiers::NONE))),
            observed_wait: None,
        };

        assert_eq!(
            run_textarea_composer_input_tick(&mut reader, &mut composer),
            TerminalInputTickOutcome::ScrollTranscriptDown
        );
        assert_eq!(composer.text(), "draft");
    }

    #[test]
    fn applies_paste_event_to_textarea_composer() {
        let mut reader = FakeReader {
            poll_result: Ok(true),
            read_result: Some(Ok(Event::Paste("first line\nsecond line".to_string()))),
            observed_wait: None,
        };
        let mut composer = TextareaComposer::default();

        assert_eq!(
            run_textarea_composer_input_tick(&mut reader, &mut composer),
            TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::DraftChanged)
        );
        assert_eq!(composer.text(), "first line\nsecond line");
    }

    #[test]
    fn textarea_composer_tick_preserves_composer_state_directly() {
        let mut reader = FakeReader {
            poll_result: Ok(true),
            read_result: Some(Ok(key_event(KeyCode::Char('z'), KeyModifiers::NONE))),
            observed_wait: None,
        };
        let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "draft".to_string(),
        });

        assert_eq!(
            run_textarea_composer_input_tick(&mut reader, &mut composer),
            TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::DraftChanged)
        );
        assert_eq!(composer.text(), "draftz");
    }

    #[test]
    fn returns_submit_effect_without_queueing() {
        let mut reader = FakeReader {
            poll_result: Ok(true),
            read_result: Some(Ok(key_event(KeyCode::Enter, KeyModifiers::NONE))),
            observed_wait: None,
        };
        let mut draft = ComposerDraftState {
            text: " run startup sequence ".to_string(),
        };

        assert_eq!(
            run_terminal_input_tick(&mut reader, &mut draft),
            TerminalInputTickOutcome::DraftEffect(ComposerDraftEffect::SubmitRequested {
                text: "run startup sequence".to_string()
            })
        );
        assert_eq!(draft.text, "");
    }

    #[test]
    fn ignores_non_key_events() {
        let mut reader = FakeReader {
            poll_result: Ok(true),
            read_result: Some(Ok(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Moved,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            }))),
            observed_wait: None,
        };
        let mut draft = ComposerDraftState::default();

        assert_eq!(
            run_terminal_input_tick(&mut reader, &mut draft),
            TerminalInputTickOutcome::NonKeyEventIgnored
        );
    }

    #[test]
    fn reports_poll_and_read_failures() {
        let mut poll_reader = FakeReader {
            poll_result: Err(io::Error::new(io::ErrorKind::Other, "poll failed")),
            read_result: None,
            observed_wait: None,
        };
        let mut read_reader = FakeReader {
            poll_result: Ok(true),
            read_result: Some(Err(io::Error::new(io::ErrorKind::Other, "read failed"))),
            observed_wait: None,
        };
        let mut draft = ComposerDraftState::default();

        assert_eq!(
            run_terminal_input_tick(&mut poll_reader, &mut draft),
            TerminalInputTickOutcome::ReadFailed("poll failed".to_string())
        );
        assert_eq!(
            run_terminal_input_tick(&mut read_reader, &mut draft),
            TerminalInputTickOutcome::ReadFailed("read failed".to_string())
        );
    }
}
