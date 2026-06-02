use ratatui::style::{Color, Modifier, Style};

pub fn operator_label() -> Style {
    Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD)
}

pub fn system_label() -> Style {
    Style::default()
        .fg(Color::LightMagenta)
        .add_modifier(Modifier::BOLD)
}

pub fn system_body() -> Style {
    Style::default().fg(Color::LightMagenta)
}

pub fn operator_directive_label() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

pub fn operator_directive_body() -> Style {
    Style::default().fg(Color::Yellow)
}

pub fn agent_label() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

pub fn agent_tui_label() -> Style {
    Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD)
}

pub fn provider_label() -> Style {
    Style::default()
        .fg(Color::LightBlue)
        .add_modifier(Modifier::BOLD)
}

pub fn provider_body() -> Style {
    Style::default().fg(Color::LightBlue)
}

pub fn status_key() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

pub fn body() -> Style {
    Style::default().fg(Color::White)
}

pub fn body_heading() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

pub fn muted() -> Style {
    Style::default().fg(Color::DarkGray)
}

pub fn code() -> Style {
    Style::default().fg(Color::Gray)
}

pub fn positive() -> Style {
    Style::default().fg(Color::Green)
}

pub fn warning_count() -> Style {
    Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD)
}

pub fn negative() -> Style {
    Style::default().fg(Color::Red)
}

pub fn negative_strong() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}

pub fn composer_cursor() -> Style {
    Style::default().fg(Color::Black).bg(Color::Green)
}

pub fn neutral_cursor_line() -> Style {
    Style::default().bg(Color::Reset)
}
