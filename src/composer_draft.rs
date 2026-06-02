use crate::terminal_input::TerminalInputIntent;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ComposerDraftState {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposerDraftEffect {
    None,
    DraftChanged,
    SubmitRequested { text: String },
    ClearOrInterruptRequested,
    ExitRequested,
}

pub fn reduce_composer_draft(
    state: &mut ComposerDraftState,
    intent: TerminalInputIntent,
) -> ComposerDraftEffect {
    match intent {
        TerminalInputIntent::InsertChar(value) => {
            state.text.push(value);
            ComposerDraftEffect::DraftChanged
        }
        TerminalInputIntent::InsertText(value) => {
            if value.is_empty() {
                ComposerDraftEffect::None
            } else {
                state.text.push_str(&value);
                ComposerDraftEffect::DraftChanged
            }
        }
        TerminalInputIntent::Backspace => {
            if state.text.pop().is_some() {
                ComposerDraftEffect::DraftChanged
            } else {
                ComposerDraftEffect::None
            }
        }
        TerminalInputIntent::Delete
        | TerminalInputIntent::MoveLeft
        | TerminalInputIntent::MoveRight
        | TerminalInputIntent::MoveHome
        | TerminalInputIntent::MoveEnd
        | TerminalInputIntent::ScrollTranscriptUp
        | TerminalInputIntent::ScrollTranscriptDown => ComposerDraftEffect::None,
        TerminalInputIntent::Submit => {
            let text = state.text.trim().to_string();
            if text.is_empty() {
                ComposerDraftEffect::None
            } else {
                state.text.clear();
                ComposerDraftEffect::SubmitRequested { text }
            }
        }
        TerminalInputIntent::InterruptOrClear => ComposerDraftEffect::ClearOrInterruptRequested,
        TerminalInputIntent::Exit => ComposerDraftEffect::ExitRequested,
        TerminalInputIntent::Ignored => ComposerDraftEffect::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_and_backspaces_text() {
        let mut state = ComposerDraftState::default();

        assert_eq!(
            reduce_composer_draft(&mut state, TerminalInputIntent::InsertChar('a')),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(state.text, "a");
        assert_eq!(
            reduce_composer_draft(&mut state, TerminalInputIntent::Backspace),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(state.text, "");
        assert_eq!(
            reduce_composer_draft(&mut state, TerminalInputIntent::Backspace),
            ComposerDraftEffect::None
        );
    }

    #[test]
    fn inserts_multiline_text_exactly() {
        let mut state = ComposerDraftState {
            text: "prefix ".to_string(),
        };

        assert_eq!(
            reduce_composer_draft(
                &mut state,
                TerminalInputIntent::InsertText("first line\nsecond line".to_string())
            ),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(state.text, "prefix first line\nsecond line");
        assert_eq!(
            reduce_composer_draft(&mut state, TerminalInputIntent::InsertText(String::new())),
            ComposerDraftEffect::None
        );
        assert_eq!(state.text, "prefix first line\nsecond line");
    }

    #[test]
    fn submit_trims_and_clears_nonempty_draft() {
        let mut state = ComposerDraftState {
            text: "  run startup sequence  ".to_string(),
        };

        assert_eq!(
            reduce_composer_draft(&mut state, TerminalInputIntent::Submit),
            ComposerDraftEffect::SubmitRequested {
                text: "run startup sequence".to_string()
            }
        );
        assert_eq!(state.text, "");
    }

    #[test]
    fn submit_ignores_empty_draft() {
        let mut state = ComposerDraftState {
            text: "  \t  ".to_string(),
        };

        assert_eq!(
            reduce_composer_draft(&mut state, TerminalInputIntent::Submit),
            ComposerDraftEffect::None
        );
        assert_eq!(state.text, "  \t  ");
    }

    #[test]
    fn escape_requests_interrupt_without_clearing_draft() {
        let mut state = ComposerDraftState {
            text: "draft".to_string(),
        };

        assert_eq!(
            reduce_composer_draft(&mut state, TerminalInputIntent::InterruptOrClear),
            ComposerDraftEffect::ClearOrInterruptRequested
        );
        assert_eq!(state.text, "draft");
    }

    #[test]
    fn maps_exit_and_ignored_intents() {
        let mut state = ComposerDraftState::default();

        assert_eq!(
            reduce_composer_draft(&mut state, TerminalInputIntent::Exit),
            ComposerDraftEffect::ExitRequested
        );
        assert_eq!(
            reduce_composer_draft(&mut state, TerminalInputIntent::Ignored),
            ComposerDraftEffect::None
        );
    }
}
