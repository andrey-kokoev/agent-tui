use narada_agent_tui::app_view_model::{AppViewInput, build_app_view};
use narada_agent_tui::composer_view_model::ComposerViewInput;
use narada_agent_tui::input_queue::TurnState;
use narada_agent_tui::layout_model::{LayoutConfig, Rect, TerminalSize};
use narada_agent_tui::ratatui_renderer::render_app_to_buffer;
use narada_agent_tui::status_view_model::{
    McpRuntimeState, ProviderAdapterState, ProviderRuntimeState, RuntimePostureState,
    StatusViewInput, TerminalRuntimeState,
};
use narada_agent_tui::transcript_projection::{
    TranscriptActor, TranscriptItem, TranscriptItemKind,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect as TuiRect;
use ratatui::style::{Color, Modifier};

fn acceptance_view(width: u16, height: u16) -> narada_agent_tui::app_view_model::AppViewModel {
    build_app_view(&AppViewInput {
        terminal_size: TerminalSize { width, height },
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
                kind: TranscriptItemKind::ProviderToolCallRequest,
                actor: TranscriptActor::AgentTui,
                turn_id: "turn_1".to_string(),
                text: "site_loop_run_once({})".to_string(),
                sequence: Some(2),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:00:01.000Z".to_string()),
            },
            TranscriptItem {
                kind: TranscriptItemKind::ToolResultReceived,
                actor: TranscriptActor::AgentTui,
                turn_id: "turn_1".to_string(),
                text: "ok site_loop_run_once in 12ms · success".to_string(),
                sequence: None,
                projection_key: None,
                occurred_at: Some("2026-05-30T00:00:02.000Z".to_string()),
            },
            TranscriptItem {
                kind: TranscriptItemKind::TurnTerminalStatus,
                actor: TranscriptActor::AgentTui,
                turn_id: "turn_1".to_string(),
                text: "completed_without_provider".to_string(),
                sequence: None,
                projection_key: None,
                occurred_at: Some("2026-05-30T00:00:03.000Z".to_string()),
            },
        ],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 4,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: "operator draft".to_string(),
            turn_state: TurnState::Idle,
            queued_operator_notes: 0,
            held_system_directives: 0,
        },
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

fn nonblank_cells(buffer: &Buffer) -> usize {
    let area = buffer.area;
    let mut count = 0;
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if buffer[(x, y)].symbol() != " " {
                count += 1;
            }
        }
    }
    count
}

fn find_text_position(buffer: &Buffer, needle: &str) -> Option<(u16, u16)> {
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

fn first_nonblank_content_x(buffer: &Buffer, y: u16) -> Option<u16> {
    let area = buffer.area;
    for x in area.x + 1..area.x + area.width {
        if buffer[(x, y)].symbol() != " " {
            return Some(x);
        }
    }
    None
}

fn find_text_position_in_row(buffer: &Buffer, y: u16, needle: &str) -> Option<(u16, u16)> {
    let area = buffer.area;
    let mut line = String::new();
    for x in area.x..area.x + area.width {
        line.push_str(buffer[(x, y)].symbol());
    }
    line.find(needle)
        .map(|index| (area.x + line[..index].chars().count() as u16, y))
}

fn content_row_is_blank(buffer: &Buffer, y: u16) -> bool {
    let area = buffer.area;
    (area.x + 1..area.x + area.width.saturating_sub(1)).all(|x| buffer[(x, y)].symbol() == " ")
}

fn assert_cell_style(buffer: &Buffer, x: u16, y: u16, fg: Color, modifier: Modifier) {
    let cell = &buffer[(x, y)];
    assert_eq!(cell.fg, fg);
    assert!(cell.modifier.contains(modifier));
}

#[test]
fn renderer_acceptance_frame_is_nonblank_and_contains_core_regions() {
    let model = acceptance_view(100, 24);
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 100, 24));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(nonblank_cells(&buffer) > 100);
    assert!(text.contains("Transcript"));
    assert!(text.contains("operator -> sonar.resident: run startup sequence"));
    assert!(!text.contains("operator -> sonar.resident:\n  run startup sequence"));
    assert!(text.contains("  2026-05-30Z00:00"));
    assert!(text.contains("sonar.resident -> agent-tui: site_loop_run_once({})"));
    assert!(!text.contains("sonar.resident -> agent-tui:\n  site_loop_run_once({})"));
    assert!(text.contains("agent-tui -> sonar.resident: ok site_loop_run_once in 12ms"));
    assert!(!text.contains("agent-tui -> sonar.resident:\n  ok site_loop_run_once in 12ms"));
    assert!(text.contains("agent-tui:"));
    assert!(text.contains("  completed without provider"));
    assert!(!text.contains("completed_without_provider"));
    assert!(text.contains("sonar.resident"));
    assert!(!text.contains("agent="));
    assert!(!text.contains("Composer"));
    assert_eq!(buffer[(0, 20)].fg, Color::DarkGray);
    let (title_x, title_y) = find_text_position(&buffer, "Transcript").expect("title is visible");
    assert_cell_style(&buffer, title_x, title_y, Color::Cyan, Modifier::BOLD);

    let (operator_x, operator_y) = find_text_position(&buffer, "operator -> sonar.resident:")
        .expect("operator label is visible");
    assert_cell_style(
        &buffer,
        operator_x,
        operator_y,
        Color::Green,
        Modifier::BOLD,
    );
    assert_eq!(
        buffer[(operator_x + "operator".chars().count() as u16, operator_y)].fg,
        Color::DarkGray
    );
    assert_cell_style(
        &buffer,
        operator_x + "operator -> ".chars().count() as u16,
        operator_y,
        Color::Cyan,
        Modifier::BOLD,
    );
    let body_x = operator_x + "operator -> sonar.resident: ".chars().count() as u16;
    let body_y = operator_y;
    assert_eq!(buffer[(body_x, body_y)].fg, Color::White);
    let (timestamp_x, timestamp_y) =
        find_text_position(&buffer, "2026-05-30Z00:00").expect("timestamp is visible");
    assert_eq!(timestamp_x, operator_x + 2);
    assert_eq!(buffer[(timestamp_x, timestamp_y)].fg, Color::DarkGray);

    let (tool_call_x, tool_call_y) = find_text_position(&buffer, "sonar.resident -> agent-tui:")
        .expect("tool-call label is visible");
    assert_cell_style(
        &buffer,
        tool_call_x,
        tool_call_y,
        Color::Cyan,
        Modifier::BOLD,
    );
    assert_eq!(
        buffer[(
            tool_call_x + "sonar.resident".chars().count() as u16,
            tool_call_y
        )]
            .fg,
        Color::DarkGray
    );
    assert_eq!(
        buffer[(
            tool_call_x + "sonar.resident -> ".chars().count() as u16,
            tool_call_y
        )]
            .fg,
        Color::Magenta
    );

    let (tool_result_x, tool_result_y) =
        find_text_position(&buffer, "agent-tui -> sonar.resident:")
            .expect("tool-result label is visible");
    assert_eq!(buffer[(tool_result_x, tool_result_y)].fg, Color::Magenta);
    assert_cell_style(
        &buffer,
        tool_result_x + "agent-tui -> ".chars().count() as u16,
        tool_result_y,
        Color::Cyan,
        Modifier::BOLD,
    );
    let (result_payload_x, _) =
        find_text_position_in_row(&buffer, tool_result_y, "ok site_loop_run_once in 12ms")
            .expect("tool-result payload is visible on the result row");
    let result_tool_x = result_payload_x + "ok ".chars().count() as u16;
    let result_duration_x = result_tool_x + "site_loop_run_once in ".chars().count() as u16;
    let result_success_x = result_duration_x + "12ms · ".chars().count() as u16;
    assert_eq!(buffer[(result_payload_x, tool_result_y)].fg, Color::Green);
    assert_eq!(buffer[(result_tool_x, tool_result_y)].fg, Color::Gray);
    assert_eq!(buffer[(result_duration_x, tool_result_y)].fg, Color::Gray);
    assert_eq!(buffer[(result_success_x, tool_result_y)].fg, Color::Green);
    let (terminal_status_x, terminal_status_y) =
        find_text_position(&buffer, "completed without provider")
            .expect("terminal status body is visible");
    assert_eq!(
        buffer[(terminal_status_x, terminal_status_y)].fg,
        Color::Green
    );

    let status_y = model.layout.status.y;
    assert_eq!(
        buffer[(model.layout.status.x, status_y)].fg,
        Color::DarkGray
    );
    let (draft_label_x, draft_label_y) = find_text_position_in_row(&buffer, status_y, "draft")
        .expect("draft status label is visible");
    assert_cell_style(
        &buffer,
        draft_label_x,
        draft_label_y,
        Color::Yellow,
        Modifier::BOLD,
    );

    let composer_y = model.layout.composer.y;
    let (composer_operator_x, composer_operator_y) =
        find_text_position_in_row(&buffer, composer_y, "operator -> sonar.resident>")
            .expect("composer prompt is visible");
    assert_cell_style(
        &buffer,
        composer_operator_x,
        composer_operator_y,
        Color::Green,
        Modifier::BOLD,
    );
    assert_cell_style(
        &buffer,
        composer_operator_x + "operator -> ".chars().count() as u16,
        composer_operator_y,
        Color::Cyan,
        Modifier::BOLD,
    );
    let (draft_x, draft_y) =
        find_text_position(&buffer, "operator draft").expect("composer draft is visible");
    assert_eq!(buffer[(draft_x, draft_y)].fg, Color::Green);
}

#[test]
fn renderer_acceptance_splits_directive_transcript_labels_into_identity_and_mode() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 120,
            height: 16,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![
            TranscriptItem {
                kind: TranscriptItemKind::InputAdmitted,
                actor: TranscriptActor::OperatorDirective,
                turn_id: "turn_1".to_string(),
                text: "Prefer concise startup reports.".to_string(),
                sequence: None,
                projection_key: None,
                occurred_at: Some("2026-05-30T00:16:00.000Z".to_string()),
            },
            TranscriptItem {
                kind: TranscriptItemKind::SystemDirectiveHeld,
                actor: TranscriptActor::System,
                turn_id: String::new(),
                text: "held input_system_1".to_string(),
                sequence: None,
                projection_key: None,
                occurred_at: Some("2026-05-30T00:17:00.000Z".to_string()),
            },
        ],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 1,
            oldest_held_age: Some("22s".to_string()),
            transcript_items: 2,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: String::new(),
            turn_state: TurnState::Idle,
            queued_operator_notes: 0,
            held_system_directives: 1,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 120, 16));

    render_app_to_buffer(&model, &mut buffer);

    let (operator_x, operator_y) =
        find_text_position(&buffer, "operator directive -> sonar.resident:")
            .expect("operator directive label is visible");
    let operator_mode_x = operator_x + "operator ".chars().count() as u16;
    let operator_target_x = operator_x + "operator directive -> ".chars().count() as u16;
    let (operator_body_x, operator_body_y) =
        find_text_position(&buffer, "Prefer concise startup reports.")
            .expect("operator directive body is visible");
    let (operator_timestamp_x, operator_timestamp_y) =
        find_text_position(&buffer, "2026-05-30Z00:16")
            .expect("operator directive timestamp is visible");

    assert_cell_style(
        &buffer,
        operator_x,
        operator_y,
        Color::Green,
        Modifier::BOLD,
    );
    assert_cell_style(
        &buffer,
        operator_mode_x,
        operator_y,
        Color::Yellow,
        Modifier::BOLD,
    );
    assert_cell_style(
        &buffer,
        operator_target_x,
        operator_y,
        Color::Cyan,
        Modifier::BOLD,
    );
    assert_eq!(
        operator_body_x,
        operator_x + "operator directive -> sonar.resident: ".chars().count() as u16
    );
    assert_eq!(operator_body_y, operator_y);
    assert_eq!(operator_timestamp_x, operator_x + 2);
    assert_eq!(operator_timestamp_y, operator_y + 1);
    assert_eq!(buffer[(operator_body_x, operator_body_y)].fg, Color::Yellow);

    let (system_x, system_y) = find_text_position(&buffer, "system directive:")
        .expect("system directive label is visible");
    let system_mode_x = system_x + "system ".chars().count() as u16;
    let (held_x, held_y) =
        find_text_position(&buffer, "held input_system_1").expect("held directive body is visible");
    let input_id_x = held_x + "held ".chars().count() as u16;

    assert_cell_style(
        &buffer,
        system_x,
        system_y,
        Color::LightMagenta,
        Modifier::BOLD,
    );
    assert_cell_style(
        &buffer,
        system_mode_x,
        system_y,
        Color::Magenta,
        Modifier::BOLD,
    );
    assert_eq!(held_x, system_x + 2);
    assert_cell_style(&buffer, held_x, held_y, Color::Magenta, Modifier::BOLD);
    assert_eq!(buffer[(input_id_x, held_y)].fg, Color::Gray);
}

#[test]
fn renderer_acceptance_renders_admitted_system_input_as_system_directive() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 100,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::InputAdmitted,
            actor: TranscriptActor::System,
            turn_id: "turn_1".to_string(),
            text: "run startup sequence".to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: Some("2026-05-30T00:18:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 100, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("system directive:"));
    assert!(!text.contains("system -> sonar.resident:"));
    let (system_x, system_y) = find_text_position(&buffer, "system directive:")
        .expect("system directive label is visible");
    let directive_x = system_x + "system ".chars().count() as u16;
    let (body_x, body_y) = find_text_position(&buffer, "run startup sequence")
        .expect("system directive body is visible");
    let (timestamp_x, timestamp_y) =
        find_text_position(&buffer, "2026-05-30Z00:18").expect("timestamp is visible");

    assert_cell_style(
        &buffer,
        system_x,
        system_y,
        Color::LightMagenta,
        Modifier::BOLD,
    );
    assert_cell_style(
        &buffer,
        directive_x,
        system_y,
        Color::Magenta,
        Modifier::BOLD,
    );
    assert_eq!(body_x, system_x + 2);
    assert_eq!(timestamp_x, system_x + 2);
    assert_eq!(body_y, system_y + 1);
    assert_eq!(timestamp_y, system_y + 2);
    assert_eq!(buffer[(body_x, body_y)].fg, Color::LightMagenta);
    assert_eq!(buffer[(timestamp_x, timestamp_y)].fg, Color::DarkGray);
}

#[test]
fn renderer_acceptance_renders_operator_steering_as_distinct_operator_mode() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 110,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::InputAdmitted,
            actor: TranscriptActor::OperatorSteering,
            turn_id: "turn_1".to_string(),
            text: "check the mailbox after this turn".to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: Some("2026-05-30T00:19:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 110, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(
        text.contains("operator steering -> sonar.resident: check the mailbox after this turn")
    );
    assert!(
        !text.contains("operator steering -> sonar.resident:\n  check the mailbox after this turn")
    );
    assert!(!text.contains("operator directive -> sonar.resident:"));
    let (operator_x, operator_y) =
        find_text_position(&buffer, "operator steering -> sonar.resident:")
            .expect("operator steering label is visible");
    let steering_x = operator_x + "operator ".chars().count() as u16;
    let target_x = operator_x + "operator steering -> ".chars().count() as u16;
    let (body_x, body_y) = find_text_position(&buffer, "check the mailbox after this turn")
        .expect("operator steering body is visible");
    let (timestamp_x, timestamp_y) =
        find_text_position(&buffer, "2026-05-30Z00:19").expect("timestamp is visible");

    assert_cell_style(
        &buffer,
        operator_x,
        operator_y,
        Color::Green,
        Modifier::BOLD,
    );
    assert_cell_style(
        &buffer,
        steering_x,
        operator_y,
        Color::Magenta,
        Modifier::BOLD,
    );
    assert_cell_style(&buffer, target_x, operator_y, Color::Cyan, Modifier::BOLD);
    assert_eq!(
        body_x,
        operator_x + "operator steering -> sonar.resident: ".chars().count() as u16
    );
    assert_eq!(body_y, operator_y);
    assert_eq!(timestamp_x, operator_x + 2);
    assert_eq!(timestamp_y, operator_y + 1);
    assert_cell_style(&buffer, body_x, body_y, Color::Magenta, Modifier::BOLD);
}

#[test]
fn renderer_acceptance_distinguishes_active_composer_note_mode_from_operator_identity() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 100,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: "working".to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:16:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Active,
            active_phase: Some("thinking 9s".to_string()),
            active_turn_age: Some("9s".to_string()),
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: "steering note".to_string(),
            turn_state: TurnState::Active,
            queued_operator_notes: 0,
            held_system_directives: 0,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 100, 12));

    render_app_to_buffer(&model, &mut buffer);

    let composer_y = model.layout.composer.y;
    let (operator_x, operator_y) =
        find_text_position_in_row(&buffer, composer_y, "operator note -> sonar.resident>")
            .expect("active composer prompt is visible");
    let note_x = operator_x + "operator ".chars().count() as u16;
    let agent_x = operator_x + "operator note -> ".chars().count() as u16;

    assert_cell_style(
        &buffer,
        operator_x,
        operator_y,
        Color::Green,
        Modifier::BOLD,
    );
    assert_cell_style(&buffer, note_x, operator_y, Color::Magenta, Modifier::BOLD);
    assert_cell_style(&buffer, agent_x, operator_y, Color::Cyan, Modifier::BOLD);
    assert!(find_text_position(&buffer, "queued notes:").is_none());
    assert!(find_text_position(&buffer, "held system directives:").is_none());
    let (draft_x, draft_y) =
        find_text_position(&buffer, "steering note").expect("active note draft is visible");
    assert_cell_style(&buffer, draft_x, draft_y, Color::Magenta, Modifier::BOLD);
}

#[test]
fn renderer_acceptance_composer_title_shows_nonzero_queue_and_held_counts() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 180,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Active,
            active_phase: Some("thinking 9s".to_string()),
            active_turn_age: Some("9s".to_string()),
            queued_inputs: 2,
            held_system_directives: 1,
            oldest_held_age: Some("22s".to_string()),
            transcript_items: 0,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: "steering note".to_string(),
            turn_state: TurnState::Active,
            queued_operator_notes: 2,
            held_system_directives: 1,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 180, 12));

    render_app_to_buffer(&model, &mut buffer);

    let composer_y = model.layout.composer.y;
    let (queued_x, queued_y) =
        find_text_position_in_row(&buffer, composer_y, "queued operator notes: 2")
            .expect("queued note affordance is visible in composer title");
    let queued_operator_x = queued_x + "queued ".chars().count() as u16;
    let queued_mode_x = queued_x + "queued operator ".chars().count() as u16;
    let (held_x, held_y) =
        find_text_position_in_row(&buffer, composer_y, "held system directives: 1")
            .expect("held directive affordance is visible in composer title");
    let held_system_x = held_x + "held ".chars().count() as u16;
    let held_mode_x = held_x + "held system ".chars().count() as u16;

    assert_eq!(buffer[(queued_x, queued_y)].fg, Color::Green);
    assert_cell_style(
        &buffer,
        queued_operator_x,
        queued_y,
        Color::Green,
        Modifier::BOLD,
    );
    assert_eq!(buffer[(queued_mode_x, queued_y)].fg, Color::Magenta);
    assert_eq!(buffer[(held_x, held_y)].fg, Color::Green);
    assert_cell_style(
        &buffer,
        held_system_x,
        held_y,
        Color::LightMagenta,
        Modifier::BOLD,
    );
    assert_eq!(buffer[(held_mode_x, held_y)].fg, Color::Magenta);
}

#[test]
fn renderer_acceptance_colors_failed_tool_result_prefix_as_negative_state() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 140,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ToolResultReceived,
            actor: TranscriptActor::AgentTui,
            turn_id: "turn_1".to_string(),
            text: "failed site_loop_run_once in 2s".to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:15:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 140, 12));

    render_app_to_buffer(&model, &mut buffer);

    let (failed_x, failed_y) = find_text_position(&buffer, "failed site_loop_run_once")
        .expect("failed tool result is visible");
    let (tool_x, tool_y) = find_text_position_in_row(&buffer, failed_y, "site_loop_run_once")
        .expect("failed tool payload is visible");
    let (duration_x, duration_y) =
        find_text_position_in_row(&buffer, failed_y, "2s").expect("failed duration is visible");

    assert_cell_style(&buffer, failed_x, failed_y, Color::Red, Modifier::BOLD);
    assert_eq!(tool_y, failed_y);
    assert_eq!(duration_y, failed_y);
    assert_eq!(buffer[(tool_x, tool_y)].fg, Color::Gray);
    assert_eq!(buffer[(duration_x, duration_y)].fg, Color::Gray);
}

#[test]
fn renderer_acceptance_does_not_color_tool_call_argument_text_as_result_summary() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 150,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderToolCallRequest,
            actor: TranscriptActor::AgentTui,
            turn_id: "turn_1".to_string(),
            text: "mail_lookup({ subject: ISSA · success })".to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:15:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 150, 12));

    render_app_to_buffer(&model, &mut buffer);

    let (success_x, success_y) =
        find_text_position(&buffer, "success").expect("argument text is visible");
    assert_ne!(buffer[(success_x, success_y)].fg, Color::Green);
}

#[test]
fn renderer_acceptance_colors_tool_result_semantic_summaries() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 150,
            height: 14,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![
            TranscriptItem {
                kind: TranscriptItemKind::ToolResultReceived,
                actor: TranscriptActor::AgentTui,
                turn_id: "turn_1".to_string(),
                text: "ok site_loop_run_once in 12ms · success".to_string(),
                sequence: Some(1),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:15:00.000Z".to_string()),
            },
            TranscriptItem {
                kind: TranscriptItemKind::ToolResultReceived,
                actor: TranscriptActor::AgentTui,
                turn_id: "turn_1".to_string(),
                text: "failed site_loop_run_once in 2s · error".to_string(),
                sequence: Some(2),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:16:00.000Z".to_string()),
            },
        ],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 150, 14));

    render_app_to_buffer(&model, &mut buffer);

    let (success_x, success_y) =
        find_text_position(&buffer, "success").expect("success summary is visible");
    let (error_x, error_y) =
        find_text_position(&buffer, "error").expect("error summary is visible");

    assert_eq!(buffer[(success_x, success_y)].fg, Color::Green);
    assert_cell_style(&buffer, error_x, error_y, Color::Red, Modifier::BOLD);
}

#[test]
fn renderer_acceptance_tool_result_without_summary_has_no_dangling_separator() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 80,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ToolResultReceived,
            actor: TranscriptActor::AgentTui,
            turn_id: "turn_1".to_string(),
            text: "ok site_loop_run_once in 12ms".to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: Some("2026-05-30T00:15:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("agent-tui -> sonar.resident: ok site_loop_run_once in 12ms"));
    assert!(!text.contains("12ms ·"));
    assert!(text.contains("12ms  2026-05-30Z00:15"));
    let (_, result_y) = find_text_position(&buffer, "ok site_loop_run_once in 12ms")
        .expect("tool result remains visible");
    let (timestamp_x, timestamp_y) =
        find_text_position(&buffer, "2026-05-30Z00:15").expect("inline tool timestamp is visible");
    assert_eq!(timestamp_y, result_y);
    assert_eq!(buffer[(timestamp_x, timestamp_y)].fg, Color::DarkGray);
    assert_eq!(
        buffer[(
            timestamp_x + "2026-05-30Z".chars().count() as u16,
            timestamp_y
        )]
            .fg,
        Color::Gray
    );
}

#[test]
fn renderer_acceptance_tool_call_request_uses_inline_timestamp_when_it_fits() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 96,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderToolCallRequest,
            actor: TranscriptActor::AgentTui,
            turn_id: "turn_1".to_string(),
            text: "site_loop_run_once({})".to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:15:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 96, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("sonar.resident -> agent-tui: site_loop_run_once({})  2026-05-30Z00:15"));
    let (_, payload_y) = find_text_position(&buffer, "site_loop_run_once({})")
        .expect("tool call payload remains visible");
    let (timestamp_x, timestamp_y) =
        find_text_position(&buffer, "2026-05-30Z00:15").expect("inline tool timestamp is visible");
    assert_eq!(timestamp_y, payload_y);
    assert_eq!(buffer[(timestamp_x, timestamp_y)].fg, Color::DarkGray);
    assert_eq!(
        buffer[(
            timestamp_x + "2026-05-30Z".chars().count() as u16,
            timestamp_y
        )]
            .fg,
        Color::Gray
    );
}

#[test]
fn renderer_acceptance_aligns_wrapped_tool_continuations_under_tool_payload() {
    let long_tool_text =
        "site_loop_run_once({ mode: full, include_mailbox: true, include_tasks: true })";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 72,
            height: 14,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderToolCallRequest,
            actor: TranscriptActor::AgentTui,
            turn_id: "turn_1".to_string(),
            text: long_tool_text.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:15:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 72, 14));

    render_app_to_buffer(&model, &mut buffer);

    let (_, label_y) =
        find_text_position(&buffer, "sonar.resident -> agent-tui:").expect("tool label is visible");
    let (payload_x, payload_y) =
        find_text_position(&buffer, "site_loop_run_once").expect("first tool payload is visible");
    let (_, continuation_y) = find_text_position(&buffer, "include_mailbox")
        .expect("wrapped tool continuation is visible");

    let (timestamp_x, timestamp_y) =
        find_text_position(&buffer, "2026-05-30Z00:15").expect("tool timestamp is visible");

    assert_eq!(payload_y, label_y);
    assert_eq!(continuation_y, label_y + 1);
    assert_eq!(timestamp_y, continuation_y + 1);
    assert_eq!(
        first_nonblank_content_x(&buffer, continuation_y),
        Some(payload_x)
    );
    assert_eq!(
        first_nonblank_content_x(&buffer, timestamp_y),
        Some(payload_x)
    );
    assert_eq!(buffer[(payload_x - 1, continuation_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(payload_x, continuation_y)].fg, Color::Gray);
    assert_eq!(buffer[(timestamp_x, timestamp_y)].fg, Color::DarkGray);
    assert_eq!(
        buffer[(
            timestamp_x + "2026-05-30Z".chars().count() as u16,
            timestamp_y
        )]
            .fg,
        Color::Gray
    );
}

#[test]
fn renderer_acceptance_wrapped_tool_uses_inline_timestamp_on_final_line_when_it_fits() {
    let long_tool_text = "site_loop_run_once({ mode: full, include_mailbox: true })";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 74,
            height: 14,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderToolCallRequest,
            actor: TranscriptActor::AgentTui,
            turn_id: "turn_1".to_string(),
            text: long_tool_text.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:15:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 74, 14));

    render_app_to_buffer(&model, &mut buffer);

    let (_, continuation_y) = find_text_position(&buffer, "include_mailbox")
        .expect("wrapped tool continuation is visible");
    let (timestamp_x, timestamp_y) =
        find_text_position(&buffer, "2026-05-30Z00:15").expect("inline tool timestamp is visible");

    assert_eq!(timestamp_y, continuation_y);
    assert_eq!(buffer[(timestamp_x, timestamp_y)].fg, Color::DarkGray);
    assert_eq!(
        buffer[(
            timestamp_x + "2026-05-30Z".chars().count() as u16,
            timestamp_y
        )]
            .fg,
        Color::Gray
    );
}

#[test]
fn renderer_acceptance_narrow_tool_rows_keep_payload_visible_below_label() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 34,
            height: 14,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderToolCallRequest,
            actor: TranscriptActor::AgentTui,
            turn_id: "turn_1".to_string(),
            text: "site_loop_run_once({})".to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:15:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 34, 14));

    render_app_to_buffer(&model, &mut buffer);

    let (_, label_y) =
        find_text_position(&buffer, "sonar.resident -> agent-tui:").expect("tool label is visible");
    let (payload_x, payload_y) =
        find_text_position(&buffer, "site_loop_run_once").expect("tool payload remains visible");
    let (timestamp_x, timestamp_y) =
        find_text_position(&buffer, "2026-05-30Z00:15").expect("tool timestamp remains visible");

    assert_eq!(payload_y, label_y + 1);
    assert_eq!(payload_x, 3);
    assert_eq!(timestamp_y, payload_y + 1);
    assert_eq!(timestamp_x, payload_x);
    assert_eq!(buffer[(payload_x - 1, payload_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(payload_x, payload_y)].fg, Color::Gray);
    assert_eq!(buffer[(timestamp_x, timestamp_y)].fg, Color::DarkGray);
}

#[test]
fn renderer_acceptance_applies_transcript_scroll_offset_without_touching_composer() {
    let transcript_items = (1..=6)
        .map(|index| TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: format!("turn_{index}"),
            text: format!("agent message {index}"),
            sequence: Some(index),
            projection_key: None,
            occurred_at: Some(format!("2026-05-30T00:0{index}:00.000Z")),
        })
        .collect::<Vec<_>>();
    let mut model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 140,
            height: 14,
        },
        layout_config: LayoutConfig::default(),
        transcript_items,
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 6,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: "operator draft".to_string(),
            turn_state: TurnState::Idle,
            queued_operator_notes: 0,
            held_system_directives: 0,
        },
    });
    let mut tailed_buffer = Buffer::empty(TuiRect::new(0, 0, 140, 14));

    render_app_to_buffer(&model, &mut tailed_buffer);
    let tailed_text = buffer_text(&tailed_buffer);

    assert!(tailed_text.contains("agent message 6"));
    assert!(!tailed_text.contains("agent message 1"));
    assert!(tailed_text.contains("operator draft"));

    model.transcript_scroll_offset = 16;
    let mut scrolled_buffer = Buffer::empty(TuiRect::new(0, 0, 140, 14));

    render_app_to_buffer(&model, &mut scrolled_buffer);
    let scrolled_text = buffer_text(&scrolled_buffer);

    assert!(scrolled_text.contains("agent message 1"));
    assert!(!scrolled_text.contains("agent message 6"));
    assert!(scrolled_text.contains("operator draft"));
    assert!(scrolled_text.contains("scroll 16 lines"));
    let first_content_y = model.layout.transcript.y + 1;
    assert!(!content_row_is_blank(&scrolled_buffer, first_content_y));
    let status_y = model.layout.status.y;
    let (scroll_x, scroll_y) =
        find_text_position_in_row(&scrolled_buffer, status_y, "scroll 16 lines")
            .expect("scroll status is visible");
    let scroll_value_x = scroll_x + "scroll ".chars().count() as u16;
    assert_cell_style(
        &scrolled_buffer,
        scroll_x,
        scroll_y,
        Color::Yellow,
        Modifier::BOLD,
    );
    assert_cell_style(
        &scrolled_buffer,
        scroll_value_x,
        scroll_y,
        Color::Magenta,
        Modifier::BOLD,
    );
}

#[test]
fn renderer_acceptance_uses_single_blank_line_between_transcript_blocks() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 80,
            height: 16,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![
            TranscriptItem {
                kind: TranscriptItemKind::InputAdmitted,
                actor: TranscriptActor::Operator,
                turn_id: "turn_1".to_string(),
                text: "first message".to_string(),
                sequence: None,
                projection_key: None,
                occurred_at: Some("2026-05-30T00:00:00.000Z".to_string()),
            },
            TranscriptItem {
                kind: TranscriptItemKind::ProviderTextDelta,
                actor: TranscriptActor::Agent,
                turn_id: "turn_1".to_string(),
                text: "second message".to_string(),
                sequence: Some(1),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:00:01.000Z".to_string()),
            },
        ],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 16));

    render_app_to_buffer(&model, &mut buffer);

    let (_, first_label_y) = find_text_position(&buffer, "operator -> sonar.resident:")
        .expect("first block label is visible");
    let (_, first_body_y) =
        find_text_position(&buffer, "first message").expect("first block body is visible");
    let (_, first_timestamp_y) =
        find_text_position(&buffer, "2026-05-30Z00:00").expect("first block timestamp is visible");
    let (_, second_body_y) =
        find_text_position(&buffer, "second message").expect("second block body is visible");
    let second_label_y = second_body_y - 1;

    assert_eq!(first_body_y, first_label_y);
    assert_eq!(first_timestamp_y, first_body_y + 1);
    assert!(content_row_is_blank(&buffer, first_timestamp_y + 1));
    assert_eq!(second_label_y, first_timestamp_y + 2);
    assert!(!content_row_is_blank(&buffer, second_label_y));
    assert_eq!(first_nonblank_content_x(&buffer, second_label_y), Some(1));
}

#[test]
fn renderer_acceptance_renders_carrier_local_queue_feedback_as_agent_tui_block() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 96,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::TurnTerminalStatus,
            actor: TranscriptActor::AgentTui,
            turn_id: String::new(),
            text: "queue: 1 item\n1. operator · 1m 10s · queued note".to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: Some("2026-05-30T18:39:10.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Active,
            active_phase: Some("thinking 1m 10s".to_string()),
            active_turn_age: Some("1m 10s".to_string()),
            queued_inputs: 1,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: String::new(),
            turn_state: TurnState::Active,
            queued_operator_notes: 1,
            held_system_directives: 0,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 96, 18));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("agent-tui:"));
    assert!(text.contains("  queue: 1 item"));
    assert!(text.contains("  1. operator · 1m 10s · queued note"));
    assert!(text.contains("  2026-05-30Z18:39"));
    assert!(text.contains("queued operator steering 1"));
    assert!(text.contains("operator note -> sonar.resident>"));
    assert_cell_style(&buffer, 1, 1, Color::Magenta, Modifier::BOLD);
    assert_cell_style(&buffer, 3, 2, Color::Magenta, Modifier::BOLD);
    assert_eq!(buffer[(3, 3)].fg, Color::DarkGray);
    let (operator_x, operator_y) =
        find_text_position(&buffer, "operator · 1m 10s").expect("queued participant is visible");
    let duration_x = operator_x + "operator · ".chars().count() as u16;
    let queued_note_x = duration_x + "1m 10s · ".chars().count() as u16;
    assert_cell_style(
        &buffer,
        operator_x,
        operator_y,
        Color::Green,
        Modifier::BOLD,
    );
    assert_eq!(buffer[(duration_x, operator_y)].fg, Color::Gray);
    let note_mode_x = queued_note_x + "queued".chars().count() as u16;
    assert_eq!(buffer[(queued_note_x, operator_y)].fg, Color::Green);
    assert_eq!(buffer[(note_mode_x, operator_y)].fg, Color::Magenta);
    assert_eq!(buffer[(3, 4)].fg, Color::DarkGray);
}

#[test]
fn renderer_acceptance_keeps_wrapped_queue_detail_colors() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 34,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::TurnTerminalStatus,
            actor: TranscriptActor::AgentTui,
            turn_id: String::new(),
            text: "queue: operator · 1m 10s · queued note".to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: Some("2026-05-30T18:39:10.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Active,
            active_phase: Some("thinking 1m 10s".to_string()),
            active_turn_age: Some("1m 10s".to_string()),
            queued_inputs: 1,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: String::new(),
            turn_state: TurnState::Active,
            queued_operator_notes: 1,
            held_system_directives: 0,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 34, 18));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  queue: operator · 1m 10s"));
    assert!(text.contains("         queued note"));
    let (operator_x, operator_y) =
        find_text_position(&buffer, "operator · 1m 10s").expect("operator queue detail is visible");
    let (queued_x, queued_y) =
        find_text_position(&buffer, "queued note").expect("wrapped queued note is visible");
    assert_cell_style(
        &buffer,
        operator_x,
        operator_y,
        Color::Green,
        Modifier::BOLD,
    );
    let note_mode_x = queued_x + "queued".chars().count() as u16;
    assert_eq!(buffer[(3, queued_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(queued_x, queued_y)].fg, Color::Green);
    assert_eq!(buffer[(note_mode_x, queued_y)].fg, Color::Magenta);
}

#[test]
fn renderer_acceptance_aligns_wrapped_directive_status_detail_under_detail_column() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 34,
            height: 16,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::SystemDirectiveHeld,
            actor: TranscriptActor::System,
            turn_id: String::new(),
            text: "held input_held because operator draft is not empty".to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: Some("2026-05-30T00:06:02.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 1,
            oldest_held_age: None,
            transcript_items: 1,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: String::new(),
            turn_state: TurnState::Idle,
            queued_operator_notes: 0,
            held_system_directives: 1,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 34, 16));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  held input_held because"));
    assert!(text.contains("       operator draft is not"));
    let (state_x, state_y) =
        find_text_position(&buffer, "held input_held").expect("directive state is visible");
    let detail_x = state_x + "held ".chars().count() as u16;
    let (continuation_x, continuation_y) =
        find_text_position(&buffer, "operator draft").expect("directive continuation is visible");
    assert_eq!(continuation_y, state_y + 1);
    assert_eq!(continuation_x, detail_x);
    assert_cell_style(&buffer, state_x, state_y, Color::Magenta, Modifier::BOLD);
    assert_eq!(buffer[(detail_x, state_y)].fg, Color::Gray);
    assert_eq!(buffer[(3, continuation_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(continuation_x, continuation_y)].fg, Color::White);
}

#[test]
fn renderer_acceptance_distinguishes_system_inputs_operator_directives_and_directive_status() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 96,
            height: 20,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![
            TranscriptItem {
                kind: TranscriptItemKind::InputAdmitted,
                actor: TranscriptActor::System,
                turn_id: "turn_system_1".to_string(),
                text: "run startup sequence".to_string(),
                sequence: None,
                projection_key: None,
                occurred_at: Some("2026-05-30T00:06:00.000Z".to_string()),
            },
            TranscriptItem {
                kind: TranscriptItemKind::InputAdmitted,
                actor: TranscriptActor::OperatorDirective,
                turn_id: "turn_operator_directive_1".to_string(),
                text: "Always include active directives.".to_string(),
                sequence: None,
                projection_key: None,
                occurred_at: Some("2026-05-30T00:06:01.000Z".to_string()),
            },
            TranscriptItem {
                kind: TranscriptItemKind::SystemDirectiveHeld,
                actor: TranscriptActor::System,
                turn_id: String::new(),
                text: "held input_held".to_string(),
                sequence: None,
                projection_key: None,
                occurred_at: Some("2026-05-30T00:06:02.000Z".to_string()),
            },
        ],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 1,
            oldest_held_age: None,
            transcript_items: 3,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: String::new(),
            turn_state: TurnState::Idle,
            queued_operator_notes: 0,
            held_system_directives: 1,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 96, 20));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("system directive:"));
    assert!(!text.contains("system -> sonar.resident:"));
    assert!(text.contains("operator directive -> sonar.resident:"));
    assert!(text.contains("system directive:"));
    assert!(text.contains("  held input_held"));
    assert!(!text.contains("system directive held input_held"));
    let (system_label_x, system_label_y) = find_text_position(&buffer, "system directive:")
        .expect("system directive label is visible");
    let system_mode_x = system_label_x + "system ".chars().count() as u16;
    let (system_body_x, system_body_y) =
        find_text_position(&buffer, "run startup sequence").expect("system input body is visible");
    let (operator_label_x, operator_label_y) =
        find_text_position(&buffer, "operator directive -> sonar.resident:")
            .expect("operator directive label is visible");
    let operator_mode_x = operator_label_x + "operator ".chars().count() as u16;
    let operator_target_x = operator_label_x + "operator directive -> ".chars().count() as u16;
    let (operator_directive_body_x, operator_directive_body_y) =
        find_text_position(&buffer, "Always include active directives.")
            .expect("operator directive body is visible");

    assert_cell_style(
        &buffer,
        system_label_x,
        system_label_y,
        Color::LightMagenta,
        Modifier::BOLD,
    );
    assert_cell_style(
        &buffer,
        system_mode_x,
        system_label_y,
        Color::Magenta,
        Modifier::BOLD,
    );
    assert_cell_style(
        &buffer,
        operator_label_x,
        operator_label_y,
        Color::Green,
        Modifier::BOLD,
    );
    assert_cell_style(
        &buffer,
        operator_mode_x,
        operator_label_y,
        Color::Yellow,
        Modifier::BOLD,
    );
    assert_cell_style(
        &buffer,
        operator_target_x,
        operator_label_y,
        Color::Cyan,
        Modifier::BOLD,
    );
    assert_eq!(
        buffer[(system_body_x, system_body_y)].fg,
        Color::LightMagenta
    );
    assert_eq!(
        buffer[(operator_directive_body_x, operator_directive_body_y)].fg,
        Color::Yellow
    );
    let (held_x, held_y) =
        find_text_position(&buffer, "held input_held").expect("held directive body is visible");
    let id_x = held_x + "held ".chars().count() as u16;

    let (held_label_x, held_label_y) = find_text_position(&buffer, "system directive:")
        .expect("held system directive label is visible");
    assert_cell_style(
        &buffer,
        held_label_x,
        held_label_y,
        Color::LightMagenta,
        Modifier::BOLD,
    );
    assert_cell_style(&buffer, held_x, held_y, Color::Magenta, Modifier::BOLD);
    assert_eq!(buffer[(id_x, held_y)].fg, Color::Gray);
}

#[test]
fn renderer_acceptance_keeps_latest_transcript_lines_visible_when_overflowing() {
    let mut transcript_items = Vec::new();
    for index in 0..8 {
        transcript_items.push(TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: format!("turn_{index}"),
            text: format!("old message {index}"),
            sequence: Some(index),
            projection_key: None,
            occurred_at: Some(format!("2026-05-30T00:0{index}:00.000Z")),
        });
    }
    transcript_items.push(TranscriptItem {
        kind: TranscriptItemKind::ProviderTextDelta,
        actor: TranscriptActor::Agent,
        turn_id: "turn_latest".to_string(),
        text: "latest visible answer".to_string(),
        sequence: Some(99),
        projection_key: None,
        occurred_at: Some("2026-05-30T00:09:00.000Z".to_string()),
    });
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 80,
            height: 10,
        },
        layout_config: LayoutConfig::default(),
        transcript_items,
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 9,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 10));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(!text.contains("old message 0"));
    assert!(text.contains("latest visible answer"));
    assert!(text.contains("2026-05-30Z00:09"));
}

#[test]
fn renderer_acceptance_renders_streamed_agent_deltas_as_one_block() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 96,
            height: 14,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![
            TranscriptItem {
                kind: TranscriptItemKind::ProviderTextDelta,
                actor: TranscriptActor::Agent,
                turn_id: "turn_stream".to_string(),
                text: "Startup ".to_string(),
                sequence: Some(1),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:08:00.000Z".to_string()),
            },
            TranscriptItem {
                kind: TranscriptItemKind::ProviderTextDelta,
                actor: TranscriptActor::Agent,
                turn_id: "turn_stream".to_string(),
                text: "sequence ".to_string(),
                sequence: Some(2),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:08:01.000Z".to_string()),
            },
            TranscriptItem {
                kind: TranscriptItemKind::ProviderTextDelta,
                actor: TranscriptActor::Agent,
                turn_id: "turn_stream".to_string(),
                text: "completed.".to_string(),
                sequence: Some(3),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:08:02.000Z".to_string()),
            },
        ],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 3,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 96, 14));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert_eq!(text.matches("sonar.resident:").count(), 1);
    assert!(text.contains("  Startup sequence completed."));
    assert!(text.contains("2026-05-30Z00:08"));
}

#[test]
fn renderer_acceptance_tails_by_whole_transcript_blocks() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 80,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![
            TranscriptItem {
                kind: TranscriptItemKind::ProviderTextDelta,
                actor: TranscriptActor::Agent,
                turn_id: "turn_old".to_string(),
                text: "old line one\nold line two\nold line three".to_string(),
                sequence: Some(1),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:01:00.000Z".to_string()),
            },
            TranscriptItem {
                kind: TranscriptItemKind::ProviderTextDelta,
                actor: TranscriptActor::Agent,
                turn_id: "turn_latest".to_string(),
                text: "latest block line".to_string(),
                sequence: Some(2),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:02:00.000Z".to_string()),
            },
        ],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(!text.contains("old line one"));
    assert!(!text.contains("old line two"));
    assert!(!text.contains("old line three"));
    assert!(text.contains("sonar.resident:"));
    assert!(text.contains("latest block line"));
    assert!(text.contains("2026-05-30Z00:02"));
}

#[test]
fn renderer_acceptance_status_and_composer_are_adjacent_without_blank_gap() {
    let model = acceptance_view(100, 24);
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 100, 24));

    render_app_to_buffer(&model, &mut buffer);
    let status_y = model.layout.status.y;
    let composer_y = model.layout.composer.y;

    assert_eq!(composer_y, status_y + model.layout.status.height);
    assert!(find_text_position_in_row(&buffer, status_y, "draft").is_some());
    assert!(
        find_text_position_in_row(&buffer, composer_y, "operator -> sonar.resident>").is_some()
    );
    assert!(!content_row_is_blank(&buffer, status_y));
    assert!(!content_row_is_blank(&buffer, composer_y));
}

#[test]
fn renderer_acceptance_layout_rectangles_are_stable() {
    let model = acceptance_view(100, 20);

    assert_eq!(
        model.layout.transcript,
        Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 15,
        }
    );
    assert_eq!(
        model.layout.status,
        Rect {
            x: 0,
            y: 15,
            width: 100,
            height: 1,
        }
    );
    assert_eq!(
        model.layout.composer,
        Rect {
            x: 0,
            y: 16,
            width: 100,
            height: 4,
        }
    );
    assert_eq!(model.layout.composer.y + model.layout.composer.height, 20);
}

#[test]
fn renderer_acceptance_truncates_long_participant_labels_with_colored_ellipsis() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 36,
            height: 14,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::InputAdmitted,
            actor: TranscriptActor::Operator,
            turn_id: "turn_1".to_string(),
            text: "check status".to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: Some("2026-05-30T00:02:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "narada-timour-marketing-agent.builder2".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "narada-timour-marketing-agent.builder2".to_string(),
            draft_text: String::new(),
            turn_state: TurnState::Idle,
            queued_operator_notes: 0,
            held_system_directives: 0,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 36, 14));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("operator -> narada-timour-marke..."));
    assert!(text.contains("  check status"));
    let (operator_x, label_y) =
        find_text_position(&buffer, "operator").expect("operator label is visible");
    let (agent_x, _) =
        find_text_position(&buffer, "narada-timour-").expect("truncated agent label is visible");
    let (ellipsis_x, _) = find_text_position(&buffer, "...").expect("ellipsis is visible");
    assert_eq!(buffer[(operator_x, label_y)].fg, Color::Green);
    assert_eq!(buffer[(agent_x, label_y)].fg, Color::Cyan);
    assert_eq!(buffer[(ellipsis_x, label_y)].fg, Color::DarkGray);
}

#[test]
fn renderer_acceptance_truncates_narrow_timestamp_rows_with_muted_ellipsis() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 18,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::InputAdmitted,
            actor: TranscriptActor::Operator,
            turn_id: "turn_1".to_string(),
            text: "go".to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: Some("2026-05-30T00:02:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "a".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "a".to_string(),
            draft_text: String::new(),
            turn_state: TurnState::Idle,
            queued_operator_notes: 0,
            held_system_directives: 0,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 18, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  2026-05-30Z..."));
    assert!(!text.contains("2026-05-30Z00:02"));
    let (timestamp_x, timestamp_y) =
        find_text_position(&buffer, "2026-05-30").expect("truncated timestamp date is visible");
    let (ellipsis_x, _) =
        find_text_position(&buffer, "...").expect("timestamp ellipsis is visible");
    assert_eq!(timestamp_x, 3);
    assert_eq!(buffer[(timestamp_x, timestamp_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(ellipsis_x, timestamp_y)].fg, Color::DarkGray);
}

#[test]
fn renderer_acceptance_wraps_long_transcript_rows_instead_of_clipping() {
    let long_agent_text = "I’ll locate the site’s startup procedure and run it through the repo’s admitted surfaces where available. First I’m checking the local scripts/docs so I don’t invent the sequence.";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 72,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: long_agent_text.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:01:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 72, 18));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("sonar.resident:"));
    assert!(text.contains("  I’ll locate the site’s startup procedure"));
    assert!(text.contains("First I’m checking"));
    assert!(text.contains("scripts/docs"));
    assert!(text.contains("I don’t"));
    assert!(text.contains("invent the sequence."));
    assert!(text.contains("sonar.resident"));
    assert!(!text.contains("agent="));

    let (first_x, first_y) = find_text_position(&buffer, "I’ll locate the site’s startup")
        .expect("first wrapped body line is visible");
    let (_, second_y) = find_text_position(&buffer, "available. First I’m checking")
        .expect("second wrapped body line is visible");
    let (_, third_y) = find_text_position(&buffer, "so I don’t invent the sequence.")
        .expect("third wrapped body line is visible");
    assert!(first_x >= 3);
    assert_eq!(first_nonblank_content_x(&buffer, first_y), Some(3));
    assert_eq!(first_nonblank_content_x(&buffer, second_y), Some(3));
    assert_eq!(first_nonblank_content_x(&buffer, third_y), Some(3));
    assert_eq!(second_y, first_y + 1);
    assert_eq!(third_y, second_y + 1);
}

#[test]
fn renderer_acceptance_sanitizes_control_characters_before_terminal_rendering() {
    let body = "First line\r\nSecond\tline with bell\u{0007} marker";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 72,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:01:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 72, 18));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  First line"));
    assert!(text.contains("  Second    line with bell  marker"));
    assert!(!text.contains('\r'));
    assert!(!text.contains('\t'));
    assert!(!text.contains('\u{0007}'));
}

#[test]
fn renderer_acceptance_wraps_long_operator_rows_instead_of_clipping() {
    let long_operator_text = "find where Re: ISSA - Instructor Visiting Markets received 4pm Chicago time on Friday 5/29/2026 is and report the exact local evidence path";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 68,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::InputAdmitted,
            actor: TranscriptActor::Operator,
            turn_id: "turn_1".to_string(),
            text: long_operator_text.to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: Some("2026-05-30T00:02:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 68, 18));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("operator -> sonar.resident:"));
    assert!(text.contains("  find where Re: ISSA"));
    assert!(text.contains("Friday 5/29/2026"));
}

#[test]
fn renderer_acceptance_mutes_wrapped_continuation_indent_without_muting_content() {
    let body = "- investigate the inbox projection and the site operating loop admission path before changing runtime behavior";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 54,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:03:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 54, 18));

    render_app_to_buffer(&model, &mut buffer);

    let (marker_x, marker_y) = find_text_position(&buffer, "-").expect("bullet marker is visible");
    let (continuation_x, continuation_y) = find_text_position(&buffer, "path before changing")
        .expect("wrapped continuation content is visible");
    assert_eq!(buffer[(marker_x, marker_y)].fg, Color::DarkGray);
    assert_eq!(continuation_y, marker_y + 1);
    assert_eq!(first_nonblank_content_x(&buffer, continuation_y), Some(5));
    assert_eq!(buffer[(3, continuation_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(4, continuation_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(continuation_x, continuation_y)].fg, Color::White);
}

#[test]
fn renderer_acceptance_aligns_wrapped_key_value_continuation_under_value() {
    let body = "Authority locus: narada_proper with additional scoped detail for startup context";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 36,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:03:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 36, 18));

    render_app_to_buffer(&model, &mut buffer);

    let (key_x, key_y) =
        find_text_position(&buffer, "Authority locus").expect("key/value first line is visible");
    let (value_x, _) =
        find_text_position(&buffer, "narada_proper").expect("key/value value is visible");
    let (continuation_x, continuation_y) =
        find_text_position(&buffer, "with additional").expect("key/value continuation is visible");
    assert_eq!(continuation_y, key_y + 1);
    assert_eq!(continuation_x, value_x);
    assert_eq!(buffer[(key_x, key_y)].fg, Color::Yellow);
    assert_eq!(buffer[(value_x, key_y)].fg, Color::Gray);
    assert_eq!(buffer[(3, continuation_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(continuation_x, continuation_y)].fg, Color::White);
}

#[test]
fn renderer_acceptance_aligns_wrapped_diff_continuation_under_payload() {
    let body = "+added line with enough words to wrap in the transcript pane";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 30,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:03:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 30, 18));

    render_app_to_buffer(&model, &mut buffer);

    let (marker_x, marker_y) = find_text_position(&buffer, "+").expect("diff marker is visible");
    let (payload_x, _) =
        find_text_position(&buffer, "added line").expect("diff payload is visible");
    let (continuation_x, continuation_y) =
        find_text_position(&buffer, "words to wrap").expect("diff continuation is visible");
    assert_eq!(continuation_y, marker_y + 1);
    assert_eq!(continuation_x, payload_x);
    assert_eq!(buffer[(marker_x, marker_y)].fg, Color::Green);
    assert_eq!(buffer[(3, continuation_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(continuation_x, continuation_y)].fg, Color::White);
}

#[test]
fn renderer_acceptance_aligns_wrapped_powershell_continuation_under_command() {
    let body = "PS D:\\code\\narada> narada-proper-mcp --site-root D:\\code\\narada --reconcile-mcp-policy --apply";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 58,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:03:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 58, 18));

    render_app_to_buffer(&model, &mut buffer);

    let (prompt_x, prompt_y) =
        find_text_position(&buffer, "PS D:\\code\\narada>").expect("PowerShell prompt is visible");
    let (command_x, _) =
        find_text_position(&buffer, "narada-proper-mcp").expect("PowerShell command is visible");
    let (continuation_x, continuation_y) = find_text_position(&buffer, "--reconcile-mcp-policy")
        .expect("PowerShell continuation is visible");
    assert!(continuation_y > prompt_y);
    assert_eq!(continuation_x, command_x);
    assert_eq!(buffer[(prompt_x, prompt_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(command_x, prompt_y)].fg, Color::Gray);
    assert_eq!(buffer[(3, continuation_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(continuation_x, continuation_y)].fg, Color::Gray);
}

#[test]
fn renderer_acceptance_keeps_blockquote_marker_on_wrapped_continuation() {
    let body = "> quoted advisory context should keep visible quote structure after wrapping";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 36,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:03:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 36, 18));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  > quoted advisory context"));
    assert!(text.contains("  > keep visible quote"));
    let (first_marker_x, first_marker_y) = find_text_position(&buffer, "> quoted")
        .map(|(x, y)| (x, y))
        .expect("first blockquote marker is visible");
    let (continuation_marker_x, continuation_marker_y) =
        find_text_position(&buffer, "> keep").expect("continuation blockquote marker is visible");
    assert_eq!(continuation_marker_y, first_marker_y + 1);
    assert_eq!(continuation_marker_x, first_marker_x);
    assert_eq!(buffer[(first_marker_x, first_marker_y)].fg, Color::DarkGray);
    assert_eq!(
        buffer[(continuation_marker_x, continuation_marker_y)].fg,
        Color::DarkGray
    );
    assert_eq!(
        buffer[(continuation_marker_x + 2, continuation_marker_y)].fg,
        Color::White
    );
}

#[test]
fn renderer_acceptance_preserves_explicit_body_line_structure() {
    let structured_text =
        "First paragraph.\n\n- bullet item\n    let value = site_loop_run_once({});";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 80,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: structured_text.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:03:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 18));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  First paragraph."));
    assert!(!text.contains("First paragraph. - bullet item"));
    assert!(text.contains("  - bullet item"));
    assert!(text.contains("      let value = site_loop_run_once({});"));
    assert!(text.contains("  2026-05-30Z00:03"));
    let (_, bullet_y) =
        find_text_position(&buffer, "- bullet item").expect("bullet line is visible");
    let bullet_x = first_nonblank_content_x(&buffer, bullet_y).expect("bullet marker is visible");
    assert_eq!(buffer[(bullet_x, bullet_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(bullet_x + 2, bullet_y)].fg, Color::White);
}

#[test]
fn renderer_acceptance_keeps_wrapped_markdown_table_pipes_muted() {
    let body = "| Field | Value with enough words to wrap around the transcript pane |";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 44,
            height: 16,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:16:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 44, 16));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  | Field | Value with enough words to"));
    assert!(text.contains("  wrap around the transcript pane |"));
    let (first_pipe_x, first_pipe_y) =
        find_text_position(&buffer, "|").expect("first wrapped table pipe is visible");
    let (last_pipe_x, last_pipe_y) = find_text_position(&buffer, "pane |")
        .map(|(x, y)| (x + "pane ".chars().count() as u16, y))
        .expect("continuation table pipe is visible");
    assert_eq!(buffer[(first_pipe_x, first_pipe_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(first_pipe_x + 2, first_pipe_y)].fg, Color::White);
    assert_eq!(buffer[(last_pipe_x, last_pipe_y)].fg, Color::DarkGray);
}

#[test]
fn renderer_acceptance_keeps_wrapped_markdown_heading_continuations_styled() {
    let body = "## Current scoped Site posture and directive context for startup sequence";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 42,
            height: 16,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:16:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 42, 16));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  ## Current scoped Site posture and"));
    assert!(text.contains("     directive context for startup"));
    assert!(!text.contains("  ## directive context"));
    let (marker_x, marker_y) =
        find_text_position(&buffer, "##").expect("markdown heading marker is visible");
    let (continuation_x, continuation_y) =
        find_text_position(&buffer, "directive context").expect("heading continuation is visible");
    assert_eq!(buffer[(marker_x, marker_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(marker_x + 3, marker_y)].fg, Color::Cyan);
    assert_eq!(continuation_y, marker_y + 1);
    assert_eq!(continuation_x, marker_x + 3);
    assert_eq!(buffer[(continuation_x, continuation_y)].fg, Color::Cyan);
    assert!(
        buffer[(continuation_x, continuation_y)]
            .modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn renderer_acceptance_styles_short_colon_lines_as_body_headings() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 88,
            height: 16,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: "Current context:\n- Site: narada-proper".to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:16:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 88, 16));

    render_app_to_buffer(&model, &mut buffer);

    let (heading_x, heading_y) =
        find_text_position(&buffer, "Current context:").expect("heading is visible");
    let (_, site_marker_y) =
        find_text_position(&buffer, "- Site: narada-proper").expect("list item is visible");
    let site_marker_x =
        first_nonblank_content_x(&buffer, site_marker_y).expect("list marker is visible");

    assert_cell_style(&buffer, heading_x, heading_y, Color::Cyan, Modifier::BOLD);
    assert_eq!(buffer[(site_marker_x, site_marker_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(site_marker_x + 2, site_marker_y)].fg, Color::White);
}

#[test]
fn renderer_acceptance_does_not_render_blank_provider_text_blocks() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 140,
            height: 14,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![
            TranscriptItem {
                kind: TranscriptItemKind::ProviderTextDelta,
                actor: TranscriptActor::Agent,
                turn_id: "turn_1".to_string(),
                text: "  \n  ".to_string(),
                sequence: Some(1),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:13:00.000Z".to_string()),
            },
            TranscriptItem {
                kind: TranscriptItemKind::ProviderTextDelta,
                actor: TranscriptActor::Agent,
                turn_id: "turn_1".to_string(),
                text: "Visible response.".to_string(),
                sequence: Some(2),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:14:00.000Z".to_string()),
            },
        ],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 140, 14));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("sonar.resident:"));
    assert!(text.contains("Visible response."));
    assert!(text.contains("2026-05-30Z00:14"));
    assert!(text.contains("transcript 1"));
    assert!(!text.contains("2026-05-30Z00:13"));
    assert!(!text.contains("transcript 2"));
}

#[test]
fn renderer_acceptance_trims_boundary_blank_body_lines_without_collapsing_internal_paragraphs() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 80,
            height: 16,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: "\n\nFirst paragraph.\n\nSecond paragraph.\n\n".to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:14:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 16));

    render_app_to_buffer(&model, &mut buffer);

    let (_, label_y) =
        find_text_position(&buffer, "sonar.resident:").expect("agent label is visible");
    let (_, first_y) =
        find_text_position(&buffer, "First paragraph.").expect("first paragraph is visible");
    let (_, second_y) =
        find_text_position(&buffer, "Second paragraph.").expect("second paragraph is visible");
    let (_, timestamp_y) =
        find_text_position(&buffer, "2026-05-30Z00:14").expect("timestamp is visible");

    assert_eq!(first_y, label_y + 1);
    assert!(content_row_is_blank(&buffer, first_y + 1));
    assert_eq!(second_y, first_y + 2);
    assert_eq!(timestamp_y, second_y + 1);
}

#[test]
fn renderer_acceptance_styles_inline_code_spans_in_body_text() {
    let body = "Current Site is `narada-proper` at `D:\\code\\narada`.";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 96,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:04:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 96, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  Current Site is narada-proper at D:\\code\\narada."));
    assert!(!text.contains("`narada-proper`"));
    let (site_x, site_y) =
        find_text_position(&buffer, "narada-proper").expect("site inline code is visible");
    assert!(find_text_position(&buffer, "D:\\code\\narada").is_some());
    assert_eq!(buffer[(site_x, site_y)].fg, Color::Gray);
    assert_eq!(buffer[(3, 2)].fg, Color::White);
}

#[test]
fn renderer_acceptance_keeps_wrapped_inline_code_styled_without_raw_markers() {
    let body = "Use `narada-directive-context` now.";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 28,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:19:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 28, 18));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(!text.contains('`'));
    let (first_x, first_y) = find_text_position(&buffer, "narada-directive-contex")
        .expect("first code chunk is visible");
    let (last_x, last_y) =
        find_text_position(&buffer, "t now.").expect("last code chunk is visible");
    assert_eq!(buffer[(first_x, first_y)].fg, Color::Gray);
    assert_eq!(buffer[(last_x, last_y)].fg, Color::Gray);
}

#[test]
fn renderer_acceptance_styles_bold_emphasis_without_raw_markers() {
    let body = "This is **important context** for startup.";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 96,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:17:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 96, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  This is important context for startup."));
    assert!(!text.contains("**important context**"));
    let (emphasis_x, emphasis_y) =
        find_text_position(&buffer, "important context").expect("emphasis text is visible");
    assert_eq!(buffer[(emphasis_x, emphasis_y)].fg, Color::White);
    assert!(
        buffer[(emphasis_x, emphasis_y)]
            .modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn renderer_acceptance_styles_italic_emphasis_without_raw_markers() {
    let body = "This is *gentle emphasis* for startup.";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 96,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:18:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 96, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  This is gentle emphasis for startup."));
    assert!(!text.contains("*gentle emphasis*"));
    let (emphasis_x, emphasis_y) =
        find_text_position(&buffer, "gentle emphasis").expect("emphasis text is visible");
    assert_eq!(buffer[(emphasis_x, emphasis_y)].fg, Color::White);
    assert!(
        buffer[(emphasis_x, emphasis_y)]
            .modifier
            .contains(Modifier::ITALIC)
    );
}

#[test]
fn renderer_acceptance_styles_fenced_code_blocks() {
    let body = "Before code\n```powershell\npwsh -File .\\narada.ps1 agent-start\n```\nAfter code";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 96,
            height: 16,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:05:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 96, 16));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  Before code"));
    assert!(text.contains("  code: powershell"));
    assert!(text.contains("  pwsh -File .\\narada.ps1 agent-start"));
    assert!(!text.contains("  ```powershell"));
    assert!(!text.contains("  ```"));
    assert!(text.contains("  After code"));
    assert_eq!(buffer[(3, 2)].fg, Color::White);
    assert_eq!(buffer[(3, 3)].fg, Color::DarkGray);
    assert_eq!(buffer[(3, 4)].fg, Color::Gray);
    assert_eq!(buffer[(3, 5)].fg, Color::White);
}

#[test]
fn renderer_acceptance_wraps_fenced_code_without_markdown_marker_balancing() {
    let body = "```text\necho `literal marker with long content`\n```";
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 34,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: body.to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:05:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 34, 18));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  code: text"));
    assert!(text.contains("echo `literal"));
    assert!(text.contains("echo `literal"));
    assert!(!text.contains("`literal marker with`"));
    assert!(!text.contains("`literal marker with long`"));
    let (first_x, first_y) =
        find_text_position(&buffer, "echo `literal").expect("wrapped fenced-code line is visible");
    assert_eq!(first_x, 3);
    assert_eq!(buffer[(first_x, first_y)].fg, Color::Gray);
}

#[test]
fn renderer_acceptance_prioritizes_status_segments_in_narrow_width() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 104,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: "status check".to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:07:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_with_a_long_identifier".to_string(),
            turn_state: TurnState::Active,
            active_phase: None,
            active_turn_age: Some("1m 12s".to_string()),
            queued_inputs: 2,
            held_system_directives: 1,
            oldest_held_age: Some("1m 14s".to_string()),
            transcript_items: 42,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: String::new(),
            turn_state: TurnState::Active,
            queued_operator_notes: 2,
            held_system_directives: 1,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 104, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("sonar.resident"));
    assert!(text.contains("thinking 1m 12s"));
    assert!(text.contains("queued operator steering 2"));
    assert!(text.contains("held system directives 1"));
    assert!(text.contains("oldest 1m 14s"));
    assert!(text.contains("Esc interrupt"));
    assert!(!text.contains("carrier_fixture_with_a_long_identifier"));
}

#[test]
fn renderer_acceptance_does_not_render_orphan_status_separator_or_dot_when_tight() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 28,
            height: 18,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: "Use `narada-directive-context` now.".to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:19:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 28, 18));

    render_app_to_buffer(&model, &mut buffer);
    let status_line = buffer_text(&buffer)
        .lines()
        .nth(model.layout.status.y as usize)
        .expect("status row is present")
        .to_string();

    assert!(status_line.contains("idle"));
    assert!(status_line.contains("provider disabled"));
    assert!(!status_line.contains("| ."));
    assert!(!status_line.trim_end().ends_with('|'));
}

#[test]
fn renderer_acceptance_truncates_overlong_priority_status_segment_in_place() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 54,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Active,
            active_phase: Some(
                "calling site_loop_run_once_with_mailbox_and_task_projection 1m 22s".to_string(),
            ),
            active_turn_age: Some("1m 22s".to_string()),
            queued_inputs: 2,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 0,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: String::new(),
            turn_state: TurnState::Active,
            queued_operator_notes: 2,
            held_system_directives: 0,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 54, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);
    let status_y = model.layout.status.y;

    assert!(text.contains("calling site_loop"));
    assert!(text.contains("..."));
    assert!(!text.contains("task_projection"));
    assert!(!text.contains("queued operator steering 2"));
    let (ellipsis_x, ellipsis_y) = find_text_position_in_row(&buffer, status_y, "...")
        .expect("in-place status truncation marker is visible");
    let (calling_x, calling_y) = find_text_position_in_row(&buffer, status_y, "calling")
        .expect("truncated calling phase is visible");
    let tool_x = calling_x + "calling ".chars().count() as u16;

    assert_eq!(buffer[(calling_x, calling_y)].fg, Color::Green);
    assert_eq!(buffer[(tool_x, calling_y)].fg, Color::Gray);
    assert_eq!(buffer[(ellipsis_x, ellipsis_y)].fg, Color::DarkGray);
}

#[test]
fn renderer_acceptance_colors_negative_status_values_by_semantic_state() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 180,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: "provider failed".to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:10:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Active,
            active_phase: Some("interrupted".to_string()),
            active_turn_age: Some("9s".to_string()),
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
            runtime_posture: RuntimePostureState {
                provider_state: ProviderRuntimeState::Failed,
                ..RuntimePostureState::disabled()
            },
            last_error: Some("provider_cancelled".to_string()),
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: String::new(),
            turn_state: TurnState::Active,
            queued_operator_notes: 0,
            held_system_directives: 0,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 180, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("interrupted"));
    assert!(text.contains("provider failed"));
    assert!(text.contains("error provider cancelled"));
    assert!(!text.contains("provider_cancelled"));
    let status_y = model.layout.status.y;
    let (interrupted_x, interrupted_y) =
        find_text_position_in_row(&buffer, status_y, "interrupted")
            .expect("interrupted phase is visible");
    let (provider_label_x, provider_y) =
        find_text_position_in_row(&buffer, status_y, "provider failed")
            .expect("failed provider state is visible");
    let provider_x = provider_label_x + "provider ".chars().count() as u16;
    let (error_label_x, error_y) =
        find_text_position_in_row(&buffer, status_y, "error provider cancelled")
            .expect("error state is visible");
    let error_value_x = error_label_x + "error ".chars().count() as u16;

    assert_eq!(buffer[(interrupted_x, interrupted_y)].fg, Color::Red);
    assert_eq!(buffer[(provider_x, provider_y)].fg, Color::Red);
    assert!(
        buffer[(provider_x, provider_y)]
            .modifier
            .contains(Modifier::BOLD)
    );
    assert_cell_style(
        &buffer,
        error_label_x,
        error_y,
        Color::Yellow,
        Modifier::BOLD,
    );
    assert_eq!(buffer[(error_value_x, error_y)].fg, Color::Red);
    assert!(
        buffer[(error_value_x, error_y)]
            .modifier
            .contains(Modifier::BOLD)
    );
}

#[test]
fn renderer_acceptance_styles_provider_participant_label_distinctly() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 96,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::TurnTerminalStatus,
            actor: TranscriptActor::Provider,
            turn_id: "turn_1".to_string(),
            text: "provider output gpt-5.5 mediated".to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: Some("2026-05-30T00:13:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 96, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("provider:"));
    assert!(text.contains("  provider output gpt-5.5 mediated"));
    let (provider_x, provider_y) =
        find_text_position(&buffer, "provider:").expect("provider label is visible");
    let (body_x, body_y) =
        find_text_position(&buffer, "provider output").expect("provider body is visible");
    let (model_x, model_y) =
        find_text_position(&buffer, "gpt-5.5").expect("provider technical token is visible");
    assert_cell_style(
        &buffer,
        provider_x,
        provider_y,
        Color::LightBlue,
        Modifier::BOLD,
    );
    assert_eq!(buffer[(body_x, body_y)].fg, Color::LightBlue);
    assert!(!buffer[(body_x, body_y)].modifier.contains(Modifier::BOLD));
    assert_eq!(buffer[(model_x, model_y)].fg, Color::Gray);
}

#[test]
fn renderer_acceptance_renders_mediated_diagnostics_without_raw_stderr() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 100,
            height: 14,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::TurnTerminalStatus,
            actor: TranscriptActor::AgentTui,
            turn_id: "turn_1".to_string(),
            text: "diagnostic warn provider stderr · mediated".to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: Some("2026-05-30T00:12:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 100, 14));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("agent-tui:"));
    assert!(text.contains("  warn provider stderr · mediated"));
    assert!(!text.contains("  diagnostic warn provider stderr"));
    assert!(text.contains("  2026-05-30Z00:12"));
    assert!(!text.contains("raw provider stderr"));
    let (label_x, label_y) =
        find_text_position(&buffer, "agent-tui:").expect("diagnostic carrier label is visible");
    let (warn_x, diagnostic_y) =
        find_text_position(&buffer, "warn provider stderr").expect("diagnostic body is visible");
    let detail_x = warn_x + "warn ".chars().count() as u16;

    assert_cell_style(&buffer, label_x, label_y, Color::Magenta, Modifier::BOLD);
    assert_cell_style(
        &buffer,
        warn_x,
        diagnostic_y,
        Color::Magenta,
        Modifier::BOLD,
    );
    assert_eq!(buffer[(detail_x, diagnostic_y)].fg, Color::DarkGray);
}

#[test]
fn renderer_acceptance_aligns_wrapped_diagnostic_detail_under_detail_column() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 42,
            height: 14,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::TurnTerminalStatus,
            actor: TranscriptActor::AgentTui,
            turn_id: "turn_1".to_string(),
            text:
                "diagnostic warn provider stderr transport emitted enough mediated context to wrap"
                    .to_string(),
            sequence: None,
            projection_key: None,
            occurred_at: Some("2026-05-30T00:12:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 42, 14));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("  warn provider stderr transport"));
    assert!(text.contains("       enough mediated context"));
    assert!(!text.contains("diagnostic warn"));
    let (warn_x, warn_y) =
        find_text_position(&buffer, "warn provider").expect("diagnostic first line is visible");
    let detail_x = warn_x + "warn ".chars().count() as u16;
    let (continuation_x, continuation_y) =
        find_text_position(&buffer, "enough mediated").expect("diagnostic continuation is visible");
    assert_eq!(continuation_y, warn_y + 1);
    assert_eq!(continuation_x, detail_x);
    assert_cell_style(&buffer, warn_x, warn_y, Color::Magenta, Modifier::BOLD);
    assert_eq!(buffer[(detail_x, warn_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(3, continuation_y)].fg, Color::DarkGray);
    assert_eq!(buffer[(continuation_x, continuation_y)].fg, Color::DarkGray);
}

#[test]
fn renderer_acceptance_colors_status_values_by_semantic_state() {
    let mut model = acceptance_view(140, 20);
    for segment in &mut model.status.segments {
        match segment.key.as_str() {
            "queued_inputs" => segment.value = "2".to_string(),
            "held_system_directives" => segment.value = "1".to_string(),
            _ => {}
        }
    }
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 140, 20));

    render_app_to_buffer(&model, &mut buffer);
    let status_y = model.layout.status.y;
    let (draft_x, draft_y) = find_text_position_in_row(&buffer, status_y, "draft 14 chars")
        .expect("draft status is visible");
    let draft_value_x = draft_x + "draft ".chars().count() as u16;
    let (queued_x, queued_y) =
        find_text_position_in_row(&buffer, status_y, "queued operator steering 2")
            .expect("queued operator steering status is visible");
    let queued_operator_x = queued_x + "queued ".chars().count() as u16;
    let queued_mode_x = queued_x + "queued operator ".chars().count() as u16;
    let queued_value_x = queued_x + "queued operator steering ".chars().count() as u16;
    let (held_x, held_y) = find_text_position_in_row(&buffer, status_y, "held system directives 1")
        .expect("held system directives status is visible");
    let held_system_x = held_x + "held ".chars().count() as u16;
    let held_mode_x = held_x + "held system ".chars().count() as u16;
    let held_value_x = held_x + "held system directives ".chars().count() as u16;
    let (provider_x, provider_y) =
        find_text_position_in_row(&buffer, status_y, "provider disabled")
            .expect("provider status is visible");

    assert_cell_style(&buffer, draft_x, draft_y, Color::Yellow, Modifier::BOLD);
    assert_eq!(buffer[(draft_value_x, draft_y)].fg, Color::Magenta);
    assert_eq!(buffer[(queued_x, queued_y)].fg, Color::Green);
    assert_cell_style(
        &buffer,
        queued_operator_x,
        queued_y,
        Color::Green,
        Modifier::BOLD,
    );
    assert_eq!(buffer[(queued_mode_x, queued_y)].fg, Color::Magenta);
    assert_eq!(buffer[(queued_value_x, queued_y)].fg, Color::Magenta);
    assert_eq!(buffer[(held_x, held_y)].fg, Color::Green);
    assert_cell_style(
        &buffer,
        held_system_x,
        held_y,
        Color::LightMagenta,
        Modifier::BOLD,
    );
    assert_eq!(buffer[(held_mode_x, held_y)].fg, Color::Magenta);
    assert_eq!(buffer[(held_value_x, held_y)].fg, Color::Magenta);
    assert_cell_style(
        &buffer,
        provider_x,
        provider_y,
        Color::Yellow,
        Modifier::BOLD,
    );
}

#[test]
fn renderer_acceptance_hides_non_actionable_status_noise() {
    let model = acceptance_view(180, 20);
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 180, 20));

    render_app_to_buffer(&model, &mut buffer);
    let status_y = model.layout.status.y;

    assert!(find_text_position_in_row(&buffer, status_y, "draft 14 chars").is_some());
    assert!(find_text_position_in_row(&buffer, status_y, "queued operator steering 0").is_none());
    assert!(find_text_position_in_row(&buffer, status_y, "held system directives 0").is_none());
    assert!(find_text_position_in_row(&buffer, status_y, "error none").is_none());
}

#[test]
fn renderer_acceptance_styles_session_and_transcript_status_as_neutral_scan_data() {
    let model = acceptance_view(260, 24);
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 260, 24));

    render_app_to_buffer(&model, &mut buffer);
    let status_y = model.layout.status.y;

    let (session_x, session_y) =
        find_text_position_in_row(&buffer, status_y, "session carrier_fixture_1")
            .expect("session status is visible");
    let session_value_x = session_x + "session ".chars().count() as u16;
    let (transcript_x, transcript_y) = find_text_position_in_row(&buffer, status_y, "transcript 4")
        .expect("transcript status is visible");
    let transcript_value_x = transcript_x + "transcript ".chars().count() as u16;

    assert_cell_style(&buffer, session_x, session_y, Color::Yellow, Modifier::BOLD);
    assert_eq!(buffer[(session_value_x, session_y)].fg, Color::Gray);
    assert_cell_style(
        &buffer,
        transcript_x,
        transcript_y,
        Color::Yellow,
        Modifier::BOLD,
    );
    assert_eq!(buffer[(transcript_value_x, transcript_y)].fg, Color::Gray);
}

#[test]
fn renderer_acceptance_humanizes_and_colors_runtime_status_values() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 150,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![],
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
            runtime_posture: RuntimePostureState {
                provider_state: ProviderRuntimeState::Configured,
                provider_adapter_state: ProviderAdapterState::ConfiguredWithoutAdapter,
                mcp_state: McpRuntimeState::Refused,
                terminal_state: TerminalRuntimeState::Configured,
            },
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
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 150, 12));

    render_app_to_buffer(&model, &mut buffer);
    let status_y = model.layout.status.y;
    let text = buffer_text(&buffer);

    assert!(text.contains("provider configured"));
    assert!(text.contains("provider adapter configured without adapter"));
    assert!(text.contains("mcp refused"));
    assert!(!text.contains("configured_without_adapter"));

    let (adapter_x, adapter_y) = find_text_position_in_row(
        &buffer,
        status_y,
        "provider adapter configured without adapter",
    )
    .expect("provider adapter status is visible");
    let adapter_value_x = adapter_x + "provider adapter ".chars().count() as u16;
    assert_cell_style(&buffer, adapter_x, adapter_y, Color::Yellow, Modifier::BOLD);
    assert_cell_style(
        &buffer,
        adapter_value_x,
        adapter_y,
        Color::Magenta,
        Modifier::BOLD,
    );

    let (mcp_x, mcp_y) =
        find_text_position_in_row(&buffer, status_y, "mcp refused").expect("mcp status is visible");
    let mcp_value_x = mcp_x + "mcp ".chars().count() as u16;
    assert_cell_style(&buffer, mcp_x, mcp_y, Color::Yellow, Modifier::BOLD);
    assert_cell_style(&buffer, mcp_value_x, mcp_y, Color::Red, Modifier::BOLD);
}

#[test]
fn renderer_acceptance_wraps_long_composer_draft_inside_composer_region() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 48,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![],
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
            draft_text: "run startup sequence and then inspect directive context".to_string(),
            turn_state: TurnState::Idle,
            queued_operator_notes: 0,
            held_system_directives: 0,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 48, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("run startup sequence and then inspect"));
    assert!(text.contains("directive context"));
    let (first_x, first_y) = find_text_position(&buffer, "run startup sequence")
        .expect("first composer draft line is visible");
    let (second_x, second_y) = find_text_position(&buffer, "directive context")
        .expect("wrapped composer draft continuation is visible");
    assert_eq!(first_x, model.layout.composer.x + 1);
    assert_eq!(second_x, model.layout.composer.x + 1);
    assert_eq!(second_y, first_y + 1);
    assert_eq!(buffer[(first_x, first_y)].fg, Color::Green);
    assert_eq!(buffer[(second_x, second_y)].fg, Color::Green);
    assert!(first_y >= model.layout.composer.y);
    assert!(second_y < model.layout.composer.y + model.layout.composer.height);
}

#[test]
fn renderer_acceptance_renders_multiline_composer_draft_inside_composer_region() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 80,
            height: 12,
        },
        layout_config: LayoutConfig::default(),
        transcript_items: vec![TranscriptItem {
            kind: TranscriptItemKind::ProviderTextDelta,
            actor: TranscriptActor::Agent,
            turn_id: "turn_1".to_string(),
            text: "waiting".to_string(),
            sequence: Some(1),
            projection_key: None,
            occurred_at: Some("2026-05-30T00:11:00.000Z".to_string()),
        }],
        status: StatusViewInput {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 1,
            runtime_posture: RuntimePostureState::disabled(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: "first pasted line\nsecond pasted line".to_string(),
            turn_state: TurnState::Idle,
            queued_operator_notes: 0,
            held_system_directives: 0,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 12));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(text.contains("first pasted line"));
    assert!(text.contains("second pasted line"));
    let (first_x, first_y) =
        find_text_position(&buffer, "first pasted line").expect("first pasted line is visible");
    let (second_x, second_y) =
        find_text_position(&buffer, "second pasted line").expect("second pasted line is visible");
    assert_eq!(buffer[(first_x, first_y)].fg, Color::Green);
    assert_eq!(buffer[(second_x, second_y)].fg, Color::Green);
    assert!(first_y >= model.layout.composer.y);
    assert!(second_y >= model.layout.composer.y);
    assert!(first_y < model.layout.composer.y + model.layout.composer.height);
    assert!(second_y < model.layout.composer.y + model.layout.composer.height);
}

#[test]
fn renderer_acceptance_preserves_explicit_empty_composer_draft_lines() {
    let model = build_app_view(&AppViewInput {
        terminal_size: TerminalSize {
            width: 80,
            height: 13,
        },
        layout_config: LayoutConfig {
            min_transcript_height: 1,
            status_height: 1,
            composer_height: 5,
        },
        transcript_items: vec![],
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
            draft_text: "first pasted line\n\nthird pasted line".to_string(),
            turn_state: TurnState::Idle,
            queued_operator_notes: 0,
            held_system_directives: 0,
        },
    });
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 12));

    render_app_to_buffer(&model, &mut buffer);

    let (first_x, first_y) =
        find_text_position(&buffer, "first pasted line").expect("first pasted line is visible");
    let (third_x, third_y) =
        find_text_position(&buffer, "third pasted line").expect("third pasted line is visible");
    assert_eq!(first_x, model.layout.composer.x + 1);
    assert_eq!(third_x, model.layout.composer.x + 1);
    assert_eq!(third_y, first_y + 2);
    assert!(content_row_is_blank(&buffer, first_y + 1));
    assert_eq!(buffer[(first_x, first_y)].fg, Color::Green);
    assert_eq!(buffer[(third_x, third_y)].fg, Color::Green);
    assert!(third_y < model.layout.composer.y + model.layout.composer.height);
}

#[test]
fn renderer_acceptance_preserves_composer_draft_in_compact_frame() {
    let model = acceptance_view(60, 8);
    let mut buffer = Buffer::empty(TuiRect::new(0, 0, 60, 8));

    render_app_to_buffer(&model, &mut buffer);
    let text = buffer_text(&buffer);

    assert!(nonblank_cells(&buffer) > 30);
    assert_eq!(model.layout.transcript.height, 3);
    assert_eq!(model.layout.status.y, 3);
    assert_eq!(model.layout.composer.y, 4);
    assert_eq!(model.layout.composer.y + model.layout.composer.height, 8);
    assert!(text.contains("operator draft"));
}
