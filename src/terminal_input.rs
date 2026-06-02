use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalInputIntent {
    InsertChar(char),
    InsertText(String),
    Submit,
    InterruptOrClear,
    Backspace,
    Delete,
    MoveLeft,
    MoveRight,
    MoveHome,
    MoveEnd,
    ScrollTranscriptUp,
    ScrollTranscriptDown,
    Exit,
    Ignored,
}

pub fn decode_key_event(event: KeyEvent) -> TerminalInputIntent {
    if event.kind != KeyEventKind::Press {
        return TerminalInputIntent::Ignored;
    }
    match (event.code, event.modifiers) {
        (KeyCode::Char('c') | KeyCode::Char('C'), KeyModifiers::CONTROL) => {
            TerminalInputIntent::Exit
        }
        (KeyCode::Char(value), modifiers)
            if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
        {
            TerminalInputIntent::InsertChar(value)
        }
        (KeyCode::Enter, _) => TerminalInputIntent::Submit,
        (KeyCode::Esc, _) => TerminalInputIntent::InterruptOrClear,
        (KeyCode::Backspace, _) => TerminalInputIntent::Backspace,
        (KeyCode::Delete, _) => TerminalInputIntent::Delete,
        (KeyCode::Left, _) => TerminalInputIntent::MoveLeft,
        (KeyCode::Right, _) => TerminalInputIntent::MoveRight,
        (KeyCode::Home, _) => TerminalInputIntent::MoveHome,
        (KeyCode::End, _) => TerminalInputIntent::MoveEnd,
        (KeyCode::PageUp, _) => TerminalInputIntent::ScrollTranscriptUp,
        (KeyCode::PageDown, _) => TerminalInputIntent::ScrollTranscriptDown,
        _ => TerminalInputIntent::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn decodes_printable_characters() {
        assert_eq!(
            decode_key_event(key(KeyCode::Char('a'), KeyModifiers::NONE)),
            TerminalInputIntent::InsertChar('a')
        );
        assert_eq!(
            decode_key_event(key(KeyCode::Char('A'), KeyModifiers::SHIFT)),
            TerminalInputIntent::InsertChar('A')
        );
    }

    #[test]
    fn decodes_submit_interrupt_edit_navigation_and_exit() {
        assert_eq!(
            decode_key_event(key(KeyCode::Enter, KeyModifiers::NONE)),
            TerminalInputIntent::Submit
        );
        assert_eq!(
            decode_key_event(key(KeyCode::Esc, KeyModifiers::NONE)),
            TerminalInputIntent::InterruptOrClear
        );
        assert_eq!(
            decode_key_event(key(KeyCode::Backspace, KeyModifiers::NONE)),
            TerminalInputIntent::Backspace
        );
        assert_eq!(
            decode_key_event(key(KeyCode::Delete, KeyModifiers::NONE)),
            TerminalInputIntent::Delete
        );
        assert_eq!(
            decode_key_event(key(KeyCode::Left, KeyModifiers::NONE)),
            TerminalInputIntent::MoveLeft
        );
        assert_eq!(
            decode_key_event(key(KeyCode::Right, KeyModifiers::NONE)),
            TerminalInputIntent::MoveRight
        );
        assert_eq!(
            decode_key_event(key(KeyCode::Home, KeyModifiers::NONE)),
            TerminalInputIntent::MoveHome
        );
        assert_eq!(
            decode_key_event(key(KeyCode::End, KeyModifiers::NONE)),
            TerminalInputIntent::MoveEnd
        );
        assert_eq!(
            decode_key_event(key(KeyCode::PageUp, KeyModifiers::NONE)),
            TerminalInputIntent::ScrollTranscriptUp
        );
        assert_eq!(
            decode_key_event(key(KeyCode::PageDown, KeyModifiers::NONE)),
            TerminalInputIntent::ScrollTranscriptDown
        );
        assert_eq!(
            decode_key_event(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            TerminalInputIntent::Exit
        );
    }

    #[test]
    fn ignores_navigation_and_control_modified_characters() {
        assert_eq!(
            decode_key_event(key(KeyCode::Up, KeyModifiers::NONE)),
            TerminalInputIntent::Ignored
        );
        assert_eq!(
            decode_key_event(key(KeyCode::Down, KeyModifiers::NONE)),
            TerminalInputIntent::Ignored
        );
        assert_eq!(
            decode_key_event(key(KeyCode::Char('x'), KeyModifiers::ALT)),
            TerminalInputIntent::Ignored
        );
    }

    #[test]
    fn ignores_non_press_key_events() {
        let release = KeyEvent {
            code: KeyCode::Char('r'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        };
        let repeat = KeyEvent {
            code: KeyCode::Char('r'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Repeat,
            state: KeyEventState::NONE,
        };

        assert_eq!(decode_key_event(release), TerminalInputIntent::Ignored);
        assert_eq!(decode_key_event(repeat), TerminalInputIntent::Ignored);
    }
}
