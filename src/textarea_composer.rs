use crate::composer_draft::{ComposerDraftEffect, ComposerDraftState};
use crate::terminal_input::TerminalInputIntent;
use crate::ui_theme;
use ratatui::style::Style;
use tui_textarea::{CursorMove, TextArea};

#[derive(Debug, Clone)]
pub struct TextareaComposer {
    textarea: TextArea<'static>,
    text: String,
    cursor: usize,
}

impl Default for TextareaComposer {
    fn default() -> Self {
        Self::from_draft(&ComposerDraftState::default())
    }
}

impl TextareaComposer {
    pub fn from_draft(state: &ComposerDraftState) -> Self {
        let cursor = state.text.chars().count();
        Self {
            textarea: textarea_from_text(&state.text, cursor),
            text: state.text.clone(),
            cursor,
        }
    }

    pub fn textarea(&self) -> &TextArea<'static> {
        &self.textarea
    }

    pub fn with_draft_style(&self, style: Style) -> Self {
        let mut next = self.clone();
        next.textarea.set_style(style);
        next
    }

    pub fn with_display_width(&self, width: usize) -> Self {
        let width = width.max(1);
        let cursor = raw_cursor_row_col(&self.text, self.cursor);
        let mut display_lines = Vec::new();
        let mut display_cursor = (0usize, 0usize);

        for (row, line) in composer_text_lines(&self.text).iter().enumerate() {
            let display_line = sanitize_composer_display_text(line);
            let segments = wrap_composer_display_line(&display_line, width);
            if row == cursor.0 {
                let display_cursor_col = composer_display_col_for_raw_col(line, cursor.1);
                display_cursor = cursor_position_in_wrapped_segments(&segments, display_cursor_col);
                display_cursor.0 += display_lines.len();
            }
            display_lines.extend(segments.into_iter().map(|segment| segment.text));
        }

        let mut textarea = styled_textarea(TextArea::from(display_lines));
        textarea.move_cursor(CursorMove::Jump(
            display_cursor.0.min(u16::MAX as usize) as u16,
            display_cursor.1.min(u16::MAX as usize) as u16,
        ));
        Self {
            textarea,
            text: self.text.clone(),
            cursor: self.cursor,
        }
    }

    pub fn draft_state(&self) -> ComposerDraftState {
        ComposerDraftState { text: self.text() }
    }

    pub fn text(&self) -> String {
        self.text.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    pub fn apply_intent(&mut self, intent: TerminalInputIntent) -> ComposerDraftEffect {
        let effect = match intent {
            TerminalInputIntent::InsertChar(value) => {
                insert_at_char_cursor(&mut self.text, self.cursor, &value.to_string());
                self.cursor += 1;
                ComposerDraftEffect::DraftChanged
            }
            TerminalInputIntent::InsertText(value) => {
                if value.is_empty() {
                    ComposerDraftEffect::None
                } else {
                    let inserted = value.chars().count();
                    insert_at_char_cursor(&mut self.text, self.cursor, &value);
                    self.cursor += inserted;
                    ComposerDraftEffect::DraftChanged
                }
            }
            TerminalInputIntent::Backspace => {
                if self.cursor == 0 {
                    ComposerDraftEffect::None
                } else {
                    remove_char_range(&mut self.text, self.cursor - 1, self.cursor);
                    self.cursor -= 1;
                    ComposerDraftEffect::DraftChanged
                }
            }
            TerminalInputIntent::Delete => {
                if self.cursor >= self.text.chars().count() {
                    ComposerDraftEffect::None
                } else {
                    remove_char_range(&mut self.text, self.cursor, self.cursor + 1);
                    ComposerDraftEffect::DraftChanged
                }
            }
            TerminalInputIntent::MoveLeft => {
                self.cursor = self.cursor.saturating_sub(1);
                ComposerDraftEffect::DraftChanged
            }
            TerminalInputIntent::MoveRight => {
                self.cursor = (self.cursor + 1).min(self.text.chars().count());
                ComposerDraftEffect::DraftChanged
            }
            TerminalInputIntent::MoveHome => {
                self.cursor = line_start_cursor(&self.text, self.cursor);
                ComposerDraftEffect::DraftChanged
            }
            TerminalInputIntent::MoveEnd => {
                self.cursor = line_end_cursor(&self.text, self.cursor);
                ComposerDraftEffect::DraftChanged
            }
            TerminalInputIntent::Submit => {
                if self.text.trim().is_empty() {
                    ComposerDraftEffect::None
                } else {
                    let text = self.text.clone();
                    self.clear();
                    return ComposerDraftEffect::SubmitRequested { text };
                }
            }
            TerminalInputIntent::InterruptOrClear => ComposerDraftEffect::ClearOrInterruptRequested,
            TerminalInputIntent::ScrollTranscriptUp | TerminalInputIntent::ScrollTranscriptDown => {
                ComposerDraftEffect::None
            }
            TerminalInputIntent::Exit => ComposerDraftEffect::ExitRequested,
            TerminalInputIntent::Ignored => ComposerDraftEffect::None,
        };
        if matches!(effect, ComposerDraftEffect::DraftChanged) {
            self.rebuild_textarea();
        }
        effect
    }

    fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
        self.rebuild_textarea();
    }

    fn rebuild_textarea(&mut self) {
        self.textarea = textarea_from_text(&self.text, self.cursor);
    }
}

fn textarea_from_text(text: &str, cursor: usize) -> TextArea<'static> {
    let mut textarea = if text.is_empty() {
        TextArea::default()
    } else {
        TextArea::from(composer_text_lines(text))
    };
    let (row, col) = raw_cursor_row_col(text, cursor);
    textarea.move_cursor(CursorMove::Jump(
        row.min(u16::MAX as usize) as u16,
        col.min(u16::MAX as usize) as u16,
    ));
    styled_textarea(textarea)
}

fn composer_text_lines(text: &str) -> Vec<String> {
    text.split('\n').map(ToString::to_string).collect()
}

fn insert_at_char_cursor(text: &mut String, cursor: usize, inserted: &str) {
    let byte_index = byte_index_for_char_cursor(text, cursor);
    text.insert_str(byte_index, inserted);
}

fn remove_char_range(text: &mut String, start: usize, end: usize) {
    let start_byte = byte_index_for_char_cursor(text, start);
    let end_byte = byte_index_for_char_cursor(text, end);
    text.replace_range(start_byte..end_byte, "");
}

fn byte_index_for_char_cursor(text: &str, cursor: usize) -> usize {
    text.char_indices()
        .map(|(byte_index, _)| byte_index)
        .nth(cursor)
        .unwrap_or(text.len())
}

fn raw_cursor_row_col(text: &str, cursor: usize) -> (usize, usize) {
    let mut row = 0usize;
    let mut col = 0usize;
    for (index, character) in text.chars().enumerate() {
        if index >= cursor {
            break;
        }
        if character == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (row, col)
}

fn line_start_cursor(text: &str, cursor: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut index = cursor.min(chars.len());
    while index > 0 && chars[index - 1] != '\n' {
        index -= 1;
    }
    index
}

fn line_end_cursor(text: &str, cursor: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut index = cursor.min(chars.len());
    while index < chars.len() && chars[index] != '\n' {
        index += 1;
    }
    index
}

fn sanitize_composer_display_text(text: &str) -> String {
    let mut sanitized = String::new();
    for character in text.chars() {
        match character {
            '\t' => sanitized.push_str("    "),
            '\r' => sanitized.push(' '),
            character if character.is_control() => sanitized.push(' '),
            character => sanitized.push(character),
        }
    }
    sanitized
}

fn composer_display_col_for_raw_col(line: &str, raw_col: usize) -> usize {
    line.chars()
        .take(raw_col)
        .map(composer_display_char_width)
        .sum()
}

fn composer_display_char_width(character: char) -> usize {
    match character {
        '\t' => 4,
        _ => 1,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DisplaySegment {
    text: String,
    start_col: usize,
    end_col: usize,
}

fn wrap_composer_display_line(line: &str, width: usize) -> Vec<DisplaySegment> {
    let width = width.max(1);
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return vec![DisplaySegment {
            text: String::new(),
            start_col: 0,
            end_col: 0,
        }];
    }

    let mut segments = Vec::new();
    let mut start_col = 0usize;
    while start_col < chars.len() {
        let hard_end = (start_col + width).min(chars.len());
        let (end_col, next_start_col) = if hard_end < chars.len() {
            let split = preferred_composer_display_split(&chars, start_col, hard_end);
            let next = if chars.get(split) == Some(&' ') {
                split.saturating_add(1)
            } else {
                split
            };
            (split, next)
        } else {
            (hard_end, hard_end)
        };
        let end_col = end_col.max(start_col + 1).min(chars.len());
        segments.push(DisplaySegment {
            text: chars[start_col..end_col].iter().collect(),
            start_col,
            end_col,
        });
        start_col = next_start_col.max(end_col).min(chars.len());
        while start_col < chars.len() && chars[start_col] == ' ' {
            start_col += 1;
        }
    }
    segments
}

fn preferred_composer_display_split(chars: &[char], start_col: usize, hard_end: usize) -> usize {
    let minimum_useful_split = start_col + ((hard_end - start_col) / 2).max(1);
    (start_col + 1..hard_end)
        .rev()
        .find(|index| chars[*index] == ' ' && *index >= minimum_useful_split)
        .unwrap_or(hard_end)
}

fn cursor_position_in_wrapped_segments(
    segments: &[DisplaySegment],
    source_col: usize,
) -> (usize, usize) {
    for (row, segment) in segments.iter().enumerate() {
        let is_last = row + 1 == segments.len();
        let inside_segment = source_col >= segment.start_col
            && if is_last {
                source_col <= segment.end_col
            } else {
                source_col < segment.end_col
            };
        if inside_segment {
            return (row, source_col.saturating_sub(segment.start_col));
        }
    }
    segments
        .last()
        .map(|segment| {
            (
                segments.len().saturating_sub(1),
                segment.end_col.saturating_sub(segment.start_col),
            )
        })
        .unwrap_or((0, 0))
}

fn styled_textarea(mut textarea: TextArea<'static>) -> TextArea<'static> {
    textarea.set_style(ui_theme::positive());
    textarea.set_cursor_line_style(ui_theme::neutral_cursor_line());
    textarea.set_cursor_style(ui_theme::composer_cursor());
    textarea
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn exposes_textarea_for_widget_rendering() {
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "run startup sequence".to_string(),
        });

        assert_eq!(composer.textarea().lines(), ["run startup sequence"]);
    }

    #[test]
    fn styles_operator_draft_and_cursor() {
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "run startup sequence".to_string(),
        });

        assert_eq!(composer.textarea().style().fg, Some(Color::Green));
        assert_eq!(composer.textarea().cursor_style().fg, Some(Color::Black));
        assert_eq!(composer.textarea().cursor_style().bg, Some(Color::Green));
        assert_eq!(
            composer.textarea().cursor_line_style().bg,
            Some(Color::Reset)
        );
    }

    #[test]
    fn clones_with_alternate_draft_style_without_mutating_source() {
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "steering note".to_string(),
        });
        let note_composer = composer.with_draft_style(ui_theme::warning_count());

        assert_eq!(composer.textarea().style().fg, Some(Color::Green));
        assert_eq!(note_composer.textarea().style().fg, Some(Color::Magenta));
        assert!(
            note_composer
                .textarea()
                .style()
                .add_modifier
                .contains(ratatui::style::Modifier::BOLD)
        );
    }

    #[test]
    fn round_trips_existing_draft_text() {
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "run startup sequence".to_string(),
        });

        assert_eq!(composer.text(), "run startup sequence");
        assert_eq!(composer.draft_state().text, "run startup sequence");
    }

    #[test]
    fn preserves_initial_draft_trailing_newline_as_empty_visual_line() {
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "first line\n".to_string(),
        });

        assert_eq!(composer.text(), "first line\n");
        assert_eq!(composer.textarea().lines(), ["first line", ""]);
        assert_eq!(composer.textarea().cursor(), (1, 0));
    }

    #[test]
    fn preserves_initial_draft_multiple_explicit_empty_lines() {
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "first line\n\nthird line\n".to_string(),
        });

        assert_eq!(composer.text(), "first line\n\nthird line\n");
        assert_eq!(
            composer.textarea().lines(),
            ["first line", "", "third line", ""]
        );
    }

    #[test]
    fn display_width_wraps_long_draft_without_changing_source_text() {
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "run startup sequence now".to_string(),
        });

        let display = composer.with_display_width(10);

        assert_eq!(composer.text(), "run startup sequence now");
        assert_eq!(
            display.textarea().lines(),
            ["run startu", "p sequence", "now"]
        );
        assert_eq!(display.textarea().cursor(), (2, 3));
    }

    #[test]
    fn display_width_maps_cursor_into_wrapped_draft() {
        let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "abcdefghij12345".to_string(),
        });
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::MoveHome),
            ComposerDraftEffect::DraftChanged
        );
        for _ in 0..12 {
            assert_eq!(
                composer.apply_intent(TerminalInputIntent::MoveRight),
                ComposerDraftEffect::DraftChanged
            );
        }

        let display = composer.with_display_width(10);

        assert_eq!(composer.text(), "abcdefghij12345");
        assert_eq!(display.textarea().lines(), ["abcdefghij", "12345"]);
        assert_eq!(display.textarea().cursor(), (1, 2));
    }

    #[test]
    fn display_width_maps_boundary_cursor_to_next_visual_row() {
        let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "abcdefghij12345".to_string(),
        });
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::MoveHome),
            ComposerDraftEffect::DraftChanged
        );
        for _ in 0..10 {
            assert_eq!(
                composer.apply_intent(TerminalInputIntent::MoveRight),
                ComposerDraftEffect::DraftChanged
            );
        }

        let display = composer.with_display_width(10);

        assert_eq!(display.textarea().lines(), ["abcdefghij", "12345"]);
        assert_eq!(display.textarea().cursor(), (1, 0));
    }

    #[test]
    fn display_width_maps_cursor_after_tab_using_sanitized_display_width() {
        let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "a\tb".to_string(),
        });
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::MoveHome),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::MoveRight),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::MoveRight),
            ComposerDraftEffect::DraftChanged
        );

        let display = composer.with_display_width(80);

        assert_eq!(composer.text(), "a\tb");
        assert_eq!(display.textarea().lines(), ["a    b"]);
        assert_eq!(display.textarea().cursor(), (0, 5));
    }

    #[test]
    fn display_width_wraps_cursor_after_tab_using_sanitized_display_width() {
        let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "aaaa\tbbbb".to_string(),
        });
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::MoveHome),
            ComposerDraftEffect::DraftChanged
        );
        for _ in 0..5 {
            assert_eq!(
                composer.apply_intent(TerminalInputIntent::MoveRight),
                ComposerDraftEffect::DraftChanged
            );
        }

        let display = composer.with_display_width(6);

        assert_eq!(display.textarea().lines(), ["aaaa ", "bbbb"]);
        assert_eq!(display.textarea().cursor(), (1, 0));
    }

    #[test]
    fn applies_multiline_text_insert_through_textarea() {
        let mut composer = TextareaComposer::default();

        assert_eq!(
            composer.apply_intent(TerminalInputIntent::InsertText(
                "first line\nsecond line".to_string()
            )),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(composer.text(), "first line\nsecond line");
    }

    #[test]
    fn preserves_initial_draft_exactly_while_sanitizing_display_clone() {
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "run\tstartup\u{0007}\r\nsequence".to_string(),
        });

        let display = composer.with_display_width(80);

        assert_eq!(composer.text(), "run\tstartup\u{0007}\r\nsequence");
        assert_eq!(
            composer.textarea().lines(),
            ["run\tstartup\u{0007}\r", "sequence"]
        );
        assert_eq!(display.textarea().lines(), ["run    startup  ", "sequence"]);
    }

    #[test]
    fn preserves_inserted_text_exactly_while_sanitizing_display_clone() {
        let mut composer = TextareaComposer::default();

        assert_eq!(
            composer.apply_intent(TerminalInputIntent::InsertText(
                "run\tstartup\u{0007}\r\nsequence".to_string()
            )),
            ComposerDraftEffect::DraftChanged
        );
        let display = composer.with_display_width(80);
        assert_eq!(composer.text(), "run\tstartup\u{0007}\r\nsequence");
        assert_eq!(display.textarea().lines(), ["run    startup  ", "sequence"]);
    }

    #[test]
    fn preserves_inserted_control_characters_while_sanitizing_display_clone() {
        let mut composer = TextareaComposer::default();

        assert_eq!(
            composer.apply_intent(TerminalInputIntent::InsertChar('\t')),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::InsertChar('\u{0007}')),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::InsertChar('\r')),
            ComposerDraftEffect::DraftChanged
        );
        let display = composer.with_display_width(80);
        assert_eq!(composer.text(), "\t\u{0007}\r");
        assert_eq!(display.textarea().lines(), ["      "]);
    }

    #[test]
    fn applies_navigation_and_delete_through_textarea() {
        let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "abcd".to_string(),
        });

        assert_eq!(
            composer.apply_intent(TerminalInputIntent::MoveLeft),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::MoveLeft),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::InsertChar('X')),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(composer.text(), "abXcd");
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::MoveLeft),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::Delete),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(composer.text(), "abcd");
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::MoveHome),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::InsertChar('>')),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::MoveEnd),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::InsertChar('<')),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(composer.text(), ">abcd<");
    }

    #[test]
    fn applies_insert_and_backspace_through_textarea() {
        let mut composer = TextareaComposer::default();

        assert_eq!(
            composer.apply_intent(TerminalInputIntent::InsertChar('x')),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(composer.text(), "x");
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::Backspace),
            ComposerDraftEffect::DraftChanged
        );
        assert_eq!(composer.text(), "");
        assert_eq!(
            composer.apply_intent(TerminalInputIntent::Backspace),
            ComposerDraftEffect::None
        );
    }

    #[test]
    fn submit_preserves_exact_nonempty_text_and_clears_textarea() {
        let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "  run startup sequence  ".to_string(),
        });

        assert_eq!(
            composer.apply_intent(TerminalInputIntent::Submit),
            ComposerDraftEffect::SubmitRequested {
                text: "  run startup sequence  ".to_string()
            }
        );
        assert_eq!(composer.text(), "");
    }

    #[test]
    fn submit_preserves_exact_multiline_control_text() {
        let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "run\tstartup\u{0007}\r\nsequence".to_string(),
        });

        assert_eq!(
            composer.apply_intent(TerminalInputIntent::Submit),
            ComposerDraftEffect::SubmitRequested {
                text: "run\tstartup\u{0007}\r\nsequence".to_string()
            }
        );
        assert_eq!(composer.text(), "");
    }

    #[test]
    fn empty_submit_preserves_whitespace_draft() {
        let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "   ".to_string(),
        });

        assert_eq!(
            composer.apply_intent(TerminalInputIntent::Submit),
            ComposerDraftEffect::None
        );
        assert_eq!(composer.text(), "   ");
    }

    #[test]
    fn interrupt_request_preserves_draft_text() {
        let mut composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "draft".to_string(),
        });

        assert_eq!(
            composer.apply_intent(TerminalInputIntent::InterruptOrClear),
            ComposerDraftEffect::ClearOrInterruptRequested
        );
        assert_eq!(composer.text(), "draft");
    }
}
