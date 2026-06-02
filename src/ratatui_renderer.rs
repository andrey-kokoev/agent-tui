use crate::app_view_model::AppViewModel;
use crate::composer_draft::ComposerDraftState;
use crate::layout_model::Rect;
use crate::status_view_model::{
    StatusSegment, status_segment_compact_text, status_segment_is_visible, turn_state_display_value,
};
use crate::textarea_composer::TextareaComposer;
use crate::transcript_projection::{TranscriptActor, TranscriptItemKind};
use crate::transcript_view_model::TranscriptRow;
use crate::ui_theme;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect as TuiRect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

pub fn render_app_to_buffer(model: &AppViewModel, buffer: &mut Buffer) {
    let composer = TextareaComposer::from_draft(&ComposerDraftState {
        text: model.composer.draft_text.clone(),
    });
    render_app_to_buffer_with_composer(model, &composer, buffer);
}

pub fn render_app_to_buffer_with_composer(
    model: &AppViewModel,
    composer: &TextareaComposer,
    buffer: &mut Buffer,
) {
    render_transcript_to_buffer(model, buffer);
    render_status_to_buffer(model, buffer);
    render_textarea_composer_to_buffer(model, composer, buffer);
}

pub fn render_app_to_frame(model: &AppViewModel, frame: &mut ratatui::Frame<'_>) {
    let composer = TextareaComposer::from_draft(&ComposerDraftState {
        text: model.composer.draft_text.clone(),
    });
    render_app_to_frame_with_composer(model, &composer, frame);
}

pub fn render_app_to_frame_with_composer(
    model: &AppViewModel,
    composer: &TextareaComposer,
    frame: &mut ratatui::Frame<'_>,
) {
    render_transcript_to_frame(model, frame);
    render_status_to_frame(model, frame);
    render_textarea_composer_to_frame(model, composer, frame);
}

fn render_transcript_to_buffer(model: &AppViewModel, buffer: &mut Buffer) {
    Widget::render(
        transcript_paragraph(model),
        to_tui_rect(model.layout.transcript),
        buffer,
    );
}

fn render_status_to_buffer(model: &AppViewModel, buffer: &mut Buffer) {
    Widget::render(
        status_paragraph(model),
        to_tui_rect(model.layout.status),
        buffer,
    );
}

fn render_textarea_composer_to_buffer(
    model: &AppViewModel,
    composer: &TextareaComposer,
    buffer: &mut Buffer,
) {
    let area = to_tui_rect(model.layout.composer);
    let block = composer_block(model);
    let inner = block.inner(area);
    Widget::render(block, area, buffer);
    let display_composer = composer.with_display_width(inner.width as usize);
    let styled_composer = display_composer.with_draft_style(composer_draft_style(model));
    Widget::render(styled_composer.textarea(), inner, buffer);
}

fn render_transcript_to_frame(model: &AppViewModel, frame: &mut ratatui::Frame<'_>) {
    frame.render_widget(
        transcript_paragraph(model),
        to_tui_rect(model.layout.transcript),
    );
}

fn render_status_to_frame(model: &AppViewModel, frame: &mut ratatui::Frame<'_>) {
    frame.render_widget(status_paragraph(model), to_tui_rect(model.layout.status));
}

fn render_textarea_composer_to_frame(
    model: &AppViewModel,
    composer: &TextareaComposer,
    frame: &mut ratatui::Frame<'_>,
) {
    let area = to_tui_rect(model.layout.composer);
    let block = composer_block(model);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let display_composer = composer.with_display_width(inner.width as usize);
    let styled_composer = display_composer.with_draft_style(composer_draft_style(model));
    frame.render_widget(styled_composer.textarea(), inner);
}

fn transcript_paragraph(model: &AppViewModel) -> Paragraph<'static> {
    let inner_width = model.layout.transcript.width.saturating_sub(2).max(1) as usize;
    let inner_height = model.layout.transcript.height.saturating_sub(2).max(1) as usize;
    let agent_identity = transcript_agent_identity(model);
    let lines = visible_transcript_blocks(
        transcript_blocks(&model.transcript_rows, &agent_identity, inner_width),
        inner_height,
        model.transcript_scroll_offset,
    );
    Paragraph::new(lines).block(
        Block::default()
            .title(transcript_title())
            .borders(Borders::ALL)
            .border_style(frame_style()),
    )
}

fn transcript_title() -> Line<'static> {
    Line::from(Span::styled("Transcript", ui_theme::agent_label()))
}

fn visible_transcript_blocks(
    blocks: Vec<Vec<Line<'static>>>,
    max_lines: usize,
    scroll_offset: usize,
) -> Vec<Line<'static>> {
    if scroll_offset == 0 {
        return visible_tail_blocks(blocks, max_lines);
    }
    visible_scrolled_lines(flatten_transcript_blocks(blocks), max_lines, scroll_offset)
}

fn visible_tail_blocks(blocks: Vec<Vec<Line<'static>>>, max_lines: usize) -> Vec<Line<'static>> {
    let mut selected: Vec<Vec<Line<'static>>> = Vec::new();
    let mut used = 0usize;
    for block in blocks.into_iter().rev() {
        let separator = if selected.is_empty() { 0 } else { 1 };
        let required = block.len() + separator;
        if used + required <= max_lines {
            used += required;
            selected.push(block);
        } else if selected.is_empty() {
            selected.push(visible_tail_lines(block, max_lines));
            break;
        } else {
            break;
        }
    }

    let mut lines = Vec::new();
    for block in selected.into_iter().rev() {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.extend(block);
    }
    lines
}

fn flatten_transcript_blocks(blocks: Vec<Vec<Line<'static>>>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for block in blocks {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.extend(block);
    }
    lines
}

fn visible_scrolled_lines(
    lines: Vec<Line<'static>>,
    max_lines: usize,
    scroll_offset: usize,
) -> Vec<Line<'static>> {
    if lines.len() <= max_lines {
        return lines;
    }
    let end = lines.len().saturating_sub(scroll_offset).max(max_lines);
    let start = end.saturating_sub(max_lines);
    let visible =
        trim_boundary_blank_lines(lines.into_iter().skip(start).take(end - start).collect());
    if visible.is_empty() && max_lines > 0 {
        return vec![truncated_context_marker_line()];
    }
    visible
}

fn trim_boundary_blank_lines(mut lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    while lines.first().is_some_and(line_is_blank) {
        lines.remove(0);
    }
    while lines.last().is_some_and(line_is_blank) {
        lines.pop();
    }
    lines
}

fn line_is_blank(line: &Line<'static>) -> bool {
    line.spans.iter().all(|span| span.content.trim().is_empty())
}

fn truncated_context_marker_line() -> Line<'static> {
    Line::from(vec![
        Span::styled("  ".to_string(), muted_style()),
        Span::styled("...".to_string(), muted_style()),
    ])
}

fn visible_tail_lines(lines: Vec<Line<'static>>, max_lines: usize) -> Vec<Line<'static>> {
    let line_count = lines.len();
    if line_count <= max_lines {
        return lines;
    }
    if max_lines == 0 {
        return Vec::new();
    }
    if max_lines == 1 {
        return lines.into_iter().take(1).collect();
    }

    let tail_count = max_lines.saturating_sub(2);
    let mut visible = Vec::with_capacity(max_lines);
    let mut iter = lines.into_iter();
    if let Some(label) = iter.next() {
        visible.push(label);
    }
    visible.push(truncated_context_marker_line());
    let remaining: Vec<Line<'static>> = iter.collect();
    let start = remaining.len().saturating_sub(tail_count);
    visible.extend(remaining.into_iter().skip(start));
    visible
}

fn transcript_blocks(
    rows: &[TranscriptRow],
    agent_identity: &str,
    width: usize,
) -> Vec<Vec<Line<'static>>> {
    rows.iter()
        .map(|row| transcript_block(row, agent_identity, width))
        .collect()
}

fn transcript_block(row: &TranscriptRow, agent_identity: &str, width: usize) -> Vec<Line<'static>> {
    if is_tool_transcript_row(row) {
        return tool_transcript_block(row, agent_identity, width);
    }

    if let Some(lines) = inline_short_input_transcript_block(row, agent_identity, width) {
        return lines;
    }

    let mut lines = vec![transcript_label_line(row, agent_identity, width)];
    let mut in_code_block = false;
    for source_line in display_source_lines(&row.text) {
        if is_code_fence_line(&source_line) {
            if in_code_block {
                in_code_block = false;
            } else {
                lines.push(Line::from(code_fence_header_spans(&source_line)));
                in_code_block = true;
            }
            continue;
        }
        if !in_code_block {
            if let Some(directive_lines) = directive_status_body_lines(row, &source_line, width) {
                lines.extend(directive_lines);
                continue;
            }
            if let Some(diagnostic_lines) = diagnostic_body_lines(row, &source_line, width) {
                lines.extend(diagnostic_lines);
                continue;
            }
            if let Some(heading_lines) = markdown_heading_body_lines(&source_line, width) {
                lines.extend(heading_lines);
                continue;
            }
        }
        let wrapped_lines = if in_code_block {
            wrap_source_line_without_inline_balance(&source_line, width.saturating_sub(2).max(1))
        } else {
            wrap_source_line(&source_line, width.saturating_sub(2).max(1))
        };
        for wrapped in wrapped_lines {
            lines.push(Line::from(body_line_spans(row, &wrapped, in_code_block)));
        }
    }
    append_timestamp_line(&mut lines, row, width);
    lines
}

fn is_tool_transcript_row(row: &TranscriptRow) -> bool {
    matches!(
        row.kind,
        TranscriptItemKind::ProviderToolCallRequest | TranscriptItemKind::ToolResultReceived
    )
}

fn inline_short_input_transcript_block(
    row: &TranscriptRow,
    agent_identity: &str,
    width: usize,
) -> Option<Vec<Line<'static>>> {
    if !matches!(
        row.actor,
        TranscriptActor::Operator
            | TranscriptActor::OperatorSteering
            | TranscriptActor::OperatorDirective
    ) {
        return None;
    }
    let source_lines = display_source_lines(&row.text);
    if source_lines.len() != 1 || source_lines[0].trim().is_empty() {
        return None;
    }
    let body_spans = body_content_spans(row, &source_lines[0], false);
    let mut first_line_spans = transcript_label_spans(row, agent_identity);
    first_line_spans.push(Span::styled(": ".to_string(), muted_style()));
    first_line_spans.extend(body_spans);
    if span_text_len(&first_line_spans) > width {
        return None;
    }
    let mut lines = vec![Line::from(first_line_spans)];
    append_timestamp_line(&mut lines, row, width);
    Some(lines)
}

fn tool_transcript_block(
    row: &TranscriptRow,
    agent_identity: &str,
    width: usize,
) -> Vec<Line<'static>> {
    let label_width = tool_label_width(row, agent_identity) + 2;
    if label_width + MIN_INLINE_TOOL_PAYLOAD_WIDTH > width {
        return narrow_tool_transcript_block(row, agent_identity, width);
    }

    let body_width = width.saturating_sub(label_width).max(1);
    let mut wrapped = wrap_text(&row.text, body_width);
    let first = wrapped.first().cloned().unwrap_or_default();
    let mut first_line_spans = transcript_label_spans(row, agent_identity);
    first_line_spans.push(Span::styled(": ", muted_style()));
    first_line_spans.extend(tool_body_spans(row, &first));
    let mut lines = vec![Line::from(first_line_spans)];
    for continuation in wrapped.drain(1..) {
        lines.push(Line::from(tool_continuation_line_spans(
            row,
            &continuation,
            label_width,
        )));
    }
    append_tool_timestamp(&mut lines, row, label_width, width);
    lines
}

const MIN_INLINE_TOOL_PAYLOAD_WIDTH: usize = 8;

fn narrow_tool_transcript_block(
    row: &TranscriptRow,
    agent_identity: &str,
    width: usize,
) -> Vec<Line<'static>> {
    let mut label_spans = transcript_label_spans(row, agent_identity);
    label_spans.push(Span::styled(":", muted_style()));
    let mut lines = vec![Line::from(truncate_spans_to_width(label_spans, width))];
    for wrapped in wrap_text(&row.text, width.saturating_sub(2).max(1)) {
        lines.push(Line::from(tool_body_line_spans(row, &wrapped, 2)));
    }
    append_timestamp_line(&mut lines, row, width);
    lines
}

fn tool_body_spans(row: &TranscriptRow, text: &str) -> Vec<Span<'static>> {
    let payload_style = ui_theme::body();
    if row.kind != TranscriptItemKind::ToolResultReceived {
        return inline_code_spans(text, payload_style);
    }
    for (prefix, style) in [
        ("ok", ui_theme::positive()),
        ("success", ui_theme::positive()),
        ("failed", ui_theme::negative_strong()),
        ("error", ui_theme::negative_strong()),
        ("refused", ui_theme::negative_strong()),
    ] {
        if text == prefix {
            return vec![Span::styled(prefix.to_string(), style)];
        }
        if let Some(rest) = text.strip_prefix(&format!("{prefix} ")) {
            let mut spans = vec![Span::styled(format!("{prefix} "), style)];
            spans.extend(tool_result_detail_spans(rest, payload_style));
            return spans;
        }
    }
    tool_result_detail_spans(text, payload_style)
}

fn tool_result_detail_spans(text: &str, normal_style: Style) -> Vec<Span<'static>> {
    let Some((before_summary, summary)) = text.rsplit_once(" · ") else {
        return inline_code_spans(text, normal_style);
    };
    let mut spans = inline_code_spans(before_summary, normal_style);
    spans.push(Span::styled(" · ".to_string(), muted_style()));
    spans.extend(tool_result_summary_spans(summary, normal_style));
    spans
}

fn tool_result_summary_spans(summary: &str, normal_style: Style) -> Vec<Span<'static>> {
    let trimmed = summary.trim();
    let style = match trimmed {
        "ok" | "success" | "completed" => Some(ui_theme::positive()),
        "failed" | "failure" | "error" | "refused" | "interrupted" => {
            Some(ui_theme::negative_strong())
        }
        _ => None,
    };
    style
        .map(|style| vec![Span::styled(summary.to_string(), style)])
        .unwrap_or_else(|| inline_code_spans(summary, normal_style))
}

fn tool_label_width(row: &TranscriptRow, agent_identity: &str) -> usize {
    match row.kind {
        TranscriptItemKind::ProviderToolCallRequest => {
            agent_identity.chars().count() + 4 + "agent-tui".chars().count()
        }
        TranscriptItemKind::ToolResultReceived => {
            "agent-tui".chars().count() + 4 + agent_identity.chars().count()
        }
        _ => 0,
    }
}

fn append_timestamp_line(lines: &mut Vec<Line<'static>>, row: &TranscriptRow, width: usize) {
    append_timestamp_line_with_indent(lines, row, 2, width);
}

fn append_timestamp_line_with_indent(
    lines: &mut Vec<Line<'static>>,
    row: &TranscriptRow,
    indent_width: usize,
    width: usize,
) {
    if let Some(timestamp) = row.occurred_at.as_deref().and_then(format_timestamp) {
        lines.push(Line::from(truncate_spans_to_width(
            timestamp_line_spans(&timestamp, indent_width),
            width,
        )));
    }
}

fn append_tool_timestamp(
    lines: &mut Vec<Line<'static>>,
    row: &TranscriptRow,
    indent_width: usize,
    width: usize,
) {
    let Some(timestamp) = row.occurred_at.as_deref().and_then(format_timestamp) else {
        return;
    };
    let suffix = inline_timestamp_spans(&timestamp);
    if let Some(last) = lines.last_mut() {
        if span_text_len(&last.spans) + span_text_len(&suffix) <= width {
            last.spans.extend(suffix);
            return;
        }
    }
    lines.push(Line::from(truncate_spans_to_width(
        timestamp_line_spans(&timestamp, indent_width),
        width,
    )));
}

fn timestamp_line_spans(timestamp: &str, indent_width: usize) -> Vec<Span<'static>> {
    let indent = " ".repeat(indent_width);
    let Some(date) = timestamp.get(0..10) else {
        return vec![Span::styled(indent + timestamp, timestamp_style())];
    };
    let Some(separator) = timestamp.get(10..11) else {
        return vec![Span::styled(indent + timestamp, timestamp_style())];
    };
    let Some(time) = timestamp.get(11..) else {
        return vec![Span::styled(indent + timestamp, timestamp_style())];
    };
    vec![
        Span::styled(indent, muted_style()),
        Span::styled(date.to_string(), timestamp_style()),
        Span::styled(separator.to_string(), muted_style()),
        Span::styled(time.to_string(), timestamp_style()),
    ]
}

fn inline_timestamp_spans(timestamp: &str) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled("  ".to_string(), muted_style())];
    spans.extend(timestamp_line_spans(timestamp, 0));
    spans
}

fn transcript_label_line(row: &TranscriptRow, agent_identity: &str, width: usize) -> Line<'static> {
    let mut spans = transcript_label_spans(row, agent_identity);
    spans.push(Span::styled(":", muted_style()));
    Line::from(truncate_spans_to_width(spans, width))
}

fn transcript_label_spans(row: &TranscriptRow, agent_identity: &str) -> Vec<Span<'static>> {
    match row.kind {
        TranscriptItemKind::ProviderToolCallRequest => directional_label_spans(
            agent_identity.to_string(),
            agent_label_style(),
            "agent-tui".to_string(),
            agent_tui_label_style(),
        ),
        TranscriptItemKind::ToolResultReceived => directional_label_spans(
            "agent-tui".to_string(),
            agent_tui_label_style(),
            agent_identity.to_string(),
            agent_label_style(),
        ),
        _ => match row.actor {
            TranscriptActor::Operator => directional_label_spans(
                "operator".to_string(),
                operator_label_style(),
                agent_identity.to_string(),
                agent_label_style(),
            ),
            TranscriptActor::OperatorSteering => directional_label_with_source_spans(
                composite_operator_steering_label_spans(),
                agent_identity.to_string(),
                agent_label_style(),
            ),
            TranscriptActor::OperatorDirective => directional_label_with_source_spans(
                composite_operator_directive_label_spans(),
                agent_identity.to_string(),
                agent_label_style(),
            ),
            TranscriptActor::System => composite_system_directive_label_spans(),
            TranscriptActor::Agent => vec![Span::styled(
                agent_identity.to_string(),
                agent_label_style(),
            )],
            TranscriptActor::AgentTui => vec![Span::styled("agent-tui", agent_tui_label_style())],
            TranscriptActor::Provider => vec![Span::styled("provider", provider_label_style())],
        },
    }
}

fn directional_label_spans(
    source: String,
    source_style: Style,
    target: String,
    target_style: Style,
) -> Vec<Span<'static>> {
    directional_label_with_source_spans(
        vec![Span::styled(source, source_style)],
        target,
        target_style,
    )
}

fn directional_label_with_source_spans(
    mut source_spans: Vec<Span<'static>>,
    target: String,
    target_style: Style,
) -> Vec<Span<'static>> {
    source_spans.push(Span::styled(" -> ", muted_style()));
    source_spans.push(Span::styled(target, target_style));
    source_spans
}

fn composite_operator_directive_label_spans() -> Vec<Span<'static>> {
    vec![
        Span::styled("operator".to_string(), operator_label_style()),
        Span::styled(" directive".to_string(), operator_directive_label_style()),
    ]
}

fn composite_operator_steering_label_spans() -> Vec<Span<'static>> {
    vec![
        Span::styled("operator".to_string(), operator_label_style()),
        Span::styled(" steering".to_string(), ui_theme::warning_count()),
    ]
}

fn composite_system_directive_label_spans() -> Vec<Span<'static>> {
    vec![
        Span::styled("system".to_string(), system_label_style()),
        Span::styled(" directive".to_string(), ui_theme::warning_count()),
    ]
}

fn transcript_agent_identity(model: &AppViewModel) -> String {
    model
        .status
        .segments
        .iter()
        .find(|segment| segment.key == "identity")
        .map(|segment| segment.value.clone())
        .unwrap_or_else(|| "agent".to_string())
}

fn operator_label_style() -> Style {
    ui_theme::operator_label()
}

fn system_label_style() -> Style {
    ui_theme::system_label()
}

fn operator_directive_label_style() -> Style {
    ui_theme::operator_directive_label()
}

fn agent_label_style() -> Style {
    ui_theme::agent_label()
}

fn agent_tui_label_style() -> Style {
    ui_theme::agent_tui_label()
}

fn provider_label_style() -> Style {
    ui_theme::provider_label()
}

fn body_style(row: &TranscriptRow) -> Style {
    match row.actor {
        TranscriptActor::AgentTui => muted_style(),
        TranscriptActor::Provider => ui_theme::provider_body(),
        TranscriptActor::System => ui_theme::system_body(),
        TranscriptActor::OperatorSteering => ui_theme::warning_count(),
        TranscriptActor::OperatorDirective => ui_theme::operator_directive_body(),
        _ => ui_theme::body(),
    }
}

fn body_line_spans(row: &TranscriptRow, text: &str, in_code_block: bool) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled("  ", muted_style())];
    spans.extend(body_content_spans(row, text, in_code_block));
    spans
}

fn body_content_spans(row: &TranscriptRow, text: &str, in_code_block: bool) -> Vec<Span<'static>> {
    if in_code_block {
        vec![Span::styled(text.to_string(), code_style())]
    } else if let Some(diagnostic_spans) = diagnostic_body_spans(row, text) {
        diagnostic_spans
    } else if let Some(directive_status_spans) = directive_status_body_spans(row, text) {
        directive_status_spans
    } else if let Some(carrier_status_spans) = carrier_status_body_spans(row, text) {
        carrier_status_spans
    } else if let Some(carrier_queue_spans) = carrier_queue_body_spans(row, text) {
        carrier_queue_spans
    } else {
        structured_body_spans(text, body_style(row))
    }
}

fn directive_status_body_lines(
    row: &TranscriptRow,
    source_line: &str,
    width: usize,
) -> Option<Vec<Line<'static>>> {
    let state_style = directive_state_style(row)?;
    let (state, detail) = source_line.split_once(' ').unwrap_or((source_line, ""));
    let body_width = width.saturating_sub(2).max(1);
    if detail.is_empty() {
        return Some(vec![Line::from(vec![
            Span::styled("  ".to_string(), muted_style()),
            Span::styled(state.to_string(), state_style),
        ])]);
    }
    let state_width = state.chars().count() + 1;
    let wrapped = wrap_source_line(detail, body_width.saturating_sub(state_width).max(1));
    let mut lines = Vec::new();
    for (index, text) in wrapped.into_iter().enumerate() {
        let mut spans = vec![Span::styled("  ".to_string(), muted_style())];
        if index == 0 {
            spans.push(Span::styled(state.to_string(), state_style));
            spans.push(Span::styled(" ".to_string(), muted_style()));
        } else {
            spans.push(Span::styled(" ".repeat(state_width), muted_style()));
        }
        spans.extend(inline_code_spans(&text, ui_theme::body()));
        lines.push(Line::from(spans));
    }
    Some(lines)
}

fn diagnostic_body_lines(
    row: &TranscriptRow,
    source_line: &str,
    width: usize,
) -> Option<Vec<Line<'static>>> {
    if row.actor != TranscriptActor::AgentTui {
        return None;
    }
    let rest = source_line.strip_prefix("diagnostic ")?;
    let (severity, detail) = rest.split_once(' ').unwrap_or((rest, ""));
    let severity_style = diagnostic_severity_style(severity);
    let body_width = width.saturating_sub(2).max(1);
    let severity_width = if detail.is_empty() {
        0
    } else {
        severity.chars().count() + 1
    };
    let wrapped = if detail.is_empty() {
        vec![severity.to_string()]
    } else {
        wrap_source_line(detail, body_width.saturating_sub(severity_width).max(1))
    };
    let mut lines = Vec::new();
    if detail.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("  ".to_string(), muted_style()),
            Span::styled(severity.to_string(), severity_style),
        ]));
        return Some(lines);
    }
    for (index, text) in wrapped.into_iter().enumerate() {
        let mut spans = vec![Span::styled("  ".to_string(), muted_style())];
        if index == 0 {
            spans.push(Span::styled(severity.to_string(), severity_style));
            spans.push(Span::styled(" ".to_string(), muted_style()));
        } else {
            spans.push(Span::styled(" ".repeat(severity_width), muted_style()));
        }
        spans.extend(inline_code_spans(&text, muted_style()));
        lines.push(Line::from(spans));
    }
    Some(lines)
}

fn markdown_heading_body_lines(source_line: &str, width: usize) -> Option<Vec<Line<'static>>> {
    let (prefix, marker, heading_text) = markdown_heading_parts(source_line)?;
    let body_width = width.saturating_sub(2).max(1);
    let marker_width = prefix.chars().count() + marker.chars().count();
    let heading_width = body_width.saturating_sub(marker_width).max(1);
    let wrapped = wrap_source_line(heading_text, heading_width);
    let mut lines = Vec::new();
    for (index, text) in wrapped.into_iter().enumerate() {
        let mut spans = vec![Span::styled("  ".to_string(), muted_style())];
        if index == 0 {
            if !prefix.is_empty() {
                spans.push(Span::styled(prefix.to_string(), muted_style()));
            }
            spans.push(Span::styled(marker.to_string(), muted_style()));
        } else if marker_width > 0 {
            spans.push(Span::styled(" ".repeat(marker_width), muted_style()));
        }
        spans.push(Span::styled(text, ui_theme::body_heading()));
        lines.push(Line::from(spans));
    }
    Some(lines)
}

fn carrier_status_body_spans(row: &TranscriptRow, text: &str) -> Option<Vec<Span<'static>>> {
    if row.actor != TranscriptActor::AgentTui || row.kind != TranscriptItemKind::TurnTerminalStatus
    {
        return None;
    }
    if matches!(text, "completed" | "completed_without_provider") {
        return Some(vec![Span::styled(
            humanize_protocol_token(text),
            ui_theme::positive(),
        )]);
    }
    if is_inline_technical_token(text) {
        return Some(vec![Span::styled(text.to_string(), code_style())]);
    }
    None
}

fn humanize_protocol_token(value: &str) -> String {
    value.replace('_', " ")
}

fn carrier_queue_body_spans(row: &TranscriptRow, text: &str) -> Option<Vec<Span<'static>>> {
    if row.actor != TranscriptActor::AgentTui || row.kind != TranscriptItemKind::TurnTerminalStatus
    {
        return None;
    }
    if let Some(rest) = text.strip_prefix("queue: ") {
        let mut spans = vec![
            Span::styled("queue".to_string(), agent_tui_label_style()),
            Span::styled(": ".to_string(), muted_style()),
        ];
        spans.extend(queue_detail_spans(row, rest));
        return Some(spans);
    }
    if text.starts_with(' ') {
        let indent_len = text.len() - text.trim_start_matches(' ').len();
        let (indent, detail) = text.split_at(indent_len);
        if is_queue_detail_fragment(detail) {
            let mut spans = vec![Span::styled(indent.to_string(), muted_style())];
            spans.extend(queue_detail_spans(row, detail));
            return Some(spans);
        }
    }
    let Some((marker, detail)) = numbered_queue_line_parts(text) else {
        return None;
    };
    let mut spans = vec![Span::styled(marker.to_string(), muted_style())];
    spans.extend(queue_detail_spans(row, detail));
    Some(spans)
}

fn is_queue_detail_fragment(detail: &str) -> bool {
    detail.contains(" · ")
        || matches!(
            detail,
            "operator" | "system" | "agent" | "queued note" | "queued turn" | "held directive"
        )
        || is_duration_phrase(detail)
}

fn queue_detail_spans(row: &TranscriptRow, detail: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (index, part) in detail.split(" · ").enumerate() {
        if index > 0 {
            spans.push(Span::styled(" · ".to_string(), muted_style()));
        }
        spans.extend(queue_detail_part_spans(row, part));
    }
    spans
}

fn queue_detail_part_spans(row: &TranscriptRow, part: &str) -> Vec<Span<'static>> {
    match part {
        "operator" => vec![Span::styled(part.to_string(), ui_theme::operator_label())],
        "system" => vec![Span::styled(part.to_string(), ui_theme::system_label())],
        "agent" => vec![Span::styled(part.to_string(), ui_theme::agent_label())],
        "queued note" => vec![
            Span::styled("queued".to_string(), ui_theme::positive()),
            Span::styled(" note".to_string(), ui_theme::warning_count()),
        ],
        "queued turn" => vec![
            Span::styled("queued".to_string(), ui_theme::positive()),
            Span::styled(" turn".to_string(), ui_theme::warning_count()),
        ],
        "held directive" => vec![
            Span::styled("held".to_string(), ui_theme::positive()),
            Span::styled(" directive".to_string(), ui_theme::warning_count()),
        ],
        value if is_duration_phrase(value) => vec![Span::styled(part.to_string(), code_style())],
        _ => vec![Span::styled(part.to_string(), body_style(row))],
    }
}

fn numbered_queue_line_parts(text: &str) -> Option<(&str, &str)> {
    let marker_end = numbered_list_marker_end(text)?;
    Some((&text[..marker_end], &text[marker_end..]))
}

fn is_duration_phrase(value: &str) -> bool {
    let mut parts = value.split_whitespace().peekable();
    if parts.peek().is_none() {
        return false;
    }
    parts.all(is_duration_token)
}

fn directive_state_style(row: &TranscriptRow) -> Option<Style> {
    match row.kind {
        TranscriptItemKind::SystemDirectiveHeld => Some(ui_theme::warning_count()),
        TranscriptItemKind::SystemDirectiveReleased => Some(ui_theme::positive()),
        _ => None,
    }
}

fn directive_status_body_spans(row: &TranscriptRow, text: &str) -> Option<Vec<Span<'static>>> {
    let state_style = directive_state_style(row)?;
    let (state, detail) = text.split_once(' ').unwrap_or((text, ""));
    let mut spans = vec![Span::styled(state.to_string(), state_style)];
    if !detail.is_empty() {
        spans.push(Span::styled(" ".to_string(), muted_style()));
        spans.extend(inline_code_spans(detail, ui_theme::body()));
    }
    Some(spans)
}

fn diagnostic_severity_style(severity: &str) -> Style {
    match severity {
        "warn" | "warning" => ui_theme::warning_count(),
        "error" | "failed" | "failure" => ui_theme::negative_strong(),
        "ok" | "success" => ui_theme::positive(),
        _ => muted_style(),
    }
}

fn diagnostic_body_spans(row: &TranscriptRow, text: &str) -> Option<Vec<Span<'static>>> {
    if row.actor != TranscriptActor::AgentTui {
        return None;
    }
    let rest = text.strip_prefix("diagnostic ")?;
    let (severity, detail) = rest.split_once(' ').unwrap_or((rest, ""));
    let severity_style = diagnostic_severity_style(severity);
    let mut spans = vec![Span::styled(severity.to_string(), severity_style)];
    if !detail.is_empty() {
        spans.push(Span::styled(" ".to_string(), muted_style()));
        spans.extend(inline_code_spans(detail, muted_style()));
    }
    Some(spans)
}

fn tool_continuation_line_spans(
    row: &TranscriptRow,
    text: &str,
    indent_width: usize,
) -> Vec<Span<'static>> {
    tool_body_line_spans(row, text, indent_width)
}

fn tool_body_line_spans(
    row: &TranscriptRow,
    text: &str,
    indent_width: usize,
) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(" ".repeat(indent_width), muted_style())];
    spans.extend(tool_body_spans(row, text));
    spans
}

fn is_code_fence_line(text: &str) -> bool {
    text.trim_start().starts_with("```")
}

fn code_fence_header_spans(text: &str) -> Vec<Span<'static>> {
    let language = text.trim_start().trim_start_matches("```").trim();
    let label = if language.is_empty() {
        "code".to_string()
    } else {
        format!("code: {language}")
    };
    vec![
        Span::styled("  ", muted_style()),
        Span::styled(label, muted_style()),
    ]
}

fn structured_body_spans(text: &str, normal_style: Style) -> Vec<Span<'static>> {
    if is_markdown_rule_line(text) {
        return vec![Span::styled(text.to_string(), muted_style())];
    }
    if let Some((prefix, marker, rest)) = blockquote_marker_parts(text) {
        return marked_body_spans(prefix, marker, rest, normal_style);
    }
    if let Some((prefix, marker, rest)) = list_marker_parts(text) {
        return marked_body_spans(prefix, marker, rest, normal_style);
    }
    if let Some((marker, rest, style)) = diff_line_parts(text) {
        let mut spans = vec![Span::styled(marker.to_string(), style)];
        spans.extend(inline_code_spans(rest, normal_style));
        return spans;
    }
    if let Some((prefix, marker, rest)) = markdown_heading_parts(text) {
        let mut spans = Vec::new();
        if !prefix.is_empty() {
            spans.push(Span::styled(prefix.to_string(), muted_style()));
        }
        spans.push(Span::styled(marker.to_string(), muted_style()));
        spans.push(Span::styled(rest.to_string(), ui_theme::body_heading()));
        return spans;
    }
    if is_section_heading_line(text) {
        return vec![Span::styled(text.to_string(), ui_theme::body_heading())];
    }
    if is_markdown_table_row(text) || is_markdown_table_fragment(text) {
        return markdown_table_row_spans(text, normal_style);
    }
    if let Some((prompt, command)) = powershell_prompt_parts(text) {
        let mut spans = Vec::new();
        spans.push(Span::styled(prompt.to_string(), muted_style()));
        spans.extend(inline_code_spans(command, code_style()));
        return spans;
    }
    if normal_style.fg != muted_style().fg {
        if let Some((key, value)) = key_value_line_parts(text) {
            let mut spans = Vec::new();
            spans.push(Span::styled(key.to_string(), ui_theme::status_key()));
            spans.push(Span::styled(": ", muted_style()));
            spans.extend(key_value_value_spans(value, normal_style));
            return spans;
        }
    }
    indented_inline_body_spans(text, normal_style)
}

fn indented_inline_body_spans(text: &str, normal_style: Style) -> Vec<Span<'static>> {
    let indent_len = text.len() - text.trim_start_matches(' ').len();
    if indent_len == 0 || indent_len == text.len() {
        return inline_code_spans(text, normal_style);
    }
    let (indent, rest) = text.split_at(indent_len);
    let mut spans = vec![Span::styled(indent.to_string(), muted_style())];
    spans.extend(inline_code_spans(rest, normal_style));
    spans
}

fn is_markdown_rule_line(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.len() >= 3
        && (trimmed.chars().all(|character| character == '-')
            || trimmed.chars().all(|character| character == '*')
            || trimmed.chars().all(|character| character == '_'))
}

fn diff_line_parts(text: &str) -> Option<(&str, &str, Style)> {
    if let Some(rest) = text.strip_prefix('+') {
        if !rest.starts_with(' ') && !rest.starts_with('+') && !rest.is_empty() {
            return Some(("+", rest, ui_theme::positive()));
        }
    }
    if let Some(rest) = text.strip_prefix('-') {
        if !rest.starts_with(' ') && !rest.starts_with('-') && !rest.is_empty() {
            return Some(("-", rest, ui_theme::negative()));
        }
    }
    None
}

fn is_markdown_table_row(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.matches('|').count() >= 2
}

fn is_markdown_table_fragment(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.contains('|')
        && (trimmed.starts_with('|') || trimmed.ends_with('|'))
        && !trimmed.chars().all(|character| character == '|')
}

fn markdown_table_row_spans(text: &str, normal_style: Style) -> Vec<Span<'static>> {
    let cell_style = if is_markdown_table_separator_row(text) {
        muted_style()
    } else {
        normal_style
    };
    let mut spans = Vec::new();
    let mut cell = String::new();
    for character in text.chars() {
        if character == '|' {
            if !cell.is_empty() {
                spans.extend(inline_code_spans(&cell, cell_style));
                cell.clear();
            }
            spans.push(Span::styled("|".to_string(), muted_style()));
        } else {
            cell.push(character);
        }
    }
    if !cell.is_empty() {
        spans.extend(inline_code_spans(&cell, cell_style));
    }
    spans
}

fn is_markdown_table_separator_row(text: &str) -> bool {
    let trimmed = text.trim().trim_matches('|');
    trimmed.split('|').all(|cell| {
        let cell = cell.trim();
        cell.len() >= 3
            && cell
                .chars()
                .all(|character| matches!(character, '-' | ':' | ' '))
            && cell.chars().any(|character| character == '-')
    })
}

fn marked_body_spans(
    prefix: &str,
    marker: &str,
    rest: &str,
    normal_style: Style,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if !prefix.is_empty() {
        spans.push(Span::styled(prefix.to_string(), muted_style()));
    }
    spans.push(Span::styled(marker.to_string(), muted_style()));
    if let Some((checkbox, after_checkbox)) = task_checkbox_parts(rest) {
        spans.push(Span::styled(
            checkbox.to_string(),
            task_checkbox_style(checkbox),
        ));
        spans.extend(inline_code_spans(after_checkbox, normal_style));
    } else if let Some(after_recommended_marker) = rest.strip_prefix("(*) ") {
        spans.push(Span::styled("(*) ".to_string(), ui_theme::warning_count()));
        spans.extend(inline_code_spans(after_recommended_marker, normal_style));
    } else {
        spans.extend(inline_code_spans(rest, normal_style));
    }
    spans
}

fn task_checkbox_parts(text: &str) -> Option<(&str, &str)> {
    for checkbox in ["[ ] ", "[x] ", "[X] "] {
        if let Some(after_checkbox) = text.strip_prefix(checkbox) {
            return Some((checkbox, after_checkbox));
        }
    }
    None
}

fn task_checkbox_style(checkbox: &str) -> Style {
    if checkbox.eq_ignore_ascii_case("[x] ") {
        ui_theme::positive()
    } else {
        muted_style()
    }
}

fn blockquote_marker_parts(text: &str) -> Option<(&str, &str, &str)> {
    let prefix_len = text.len() - text.trim_start_matches(' ').len();
    let (prefix, rest) = text.split_at(prefix_len);
    for marker in ["> ", ">"] {
        if let Some(after_marker) = rest.strip_prefix(marker) {
            return Some((prefix, marker, after_marker));
        }
    }
    None
}

fn markdown_heading_parts(text: &str) -> Option<(&str, &str, &str)> {
    let prefix_len = text.len() - text.trim_start_matches(' ').len();
    let (prefix, rest) = text.split_at(prefix_len);
    let marker_len = rest
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if marker_len == 0 || marker_len > 6 || rest.get(marker_len..marker_len + 1) != Some(" ") {
        return None;
    }
    let marker_end = marker_len + 1;
    let heading_text = &rest[marker_end..];
    if heading_text.trim().is_empty() {
        return None;
    }
    Some((prefix, &rest[..marker_end], heading_text))
}

fn is_section_heading_line(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty()
        && trimmed.ends_with(':')
        && !trimmed.contains('`')
        && !trimmed.contains("\\")
        && trimmed.chars().count() <= 48
}

fn powershell_prompt_parts(text: &str) -> Option<(&str, &str)> {
    let after_prefix = text.strip_prefix("PS ")?;
    let prompt_end = after_prefix.find("> ")? + "PS ".len() + "> ".len();
    let prompt = &text[..prompt_end];
    let command = &text[prompt_end..];
    if command.trim().is_empty() {
        return None;
    }
    Some((prompt, command))
}

fn key_value_line_parts(text: &str) -> Option<(&str, &str)> {
    if text.starts_with(' ') || text.starts_with('\t') {
        return None;
    }
    let (key, value) = text.split_once(": ")?;
    let key_len = key.chars().count();
    if key_len == 0 || key_len > 32 || value.trim().is_empty() {
        return None;
    }
    if !key
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, ' ' | '_' | '-'))
    {
        return None;
    }
    if !key.chars().any(|character| character.is_ascii_alphabetic()) {
        return None;
    }
    Some((key, value))
}

fn key_value_value_spans(value: &str, normal_style: Style) -> Vec<Span<'static>> {
    if value.contains('`') {
        return inline_code_spans(value, normal_style);
    }
    if let Some(style) = semantic_status_value_style(value) {
        return vec![Span::styled(value.to_string(), style)];
    }
    if let Some(spans) = comma_separated_identifier_list_spans(value) {
        return spans;
    }
    if is_technical_value(value) {
        return vec![Span::styled(value.to_string(), code_style())];
    }
    inline_code_spans(value, normal_style)
}

fn comma_separated_identifier_list_spans(value: &str) -> Option<Vec<Span<'static>>> {
    if !value.contains(',') {
        return None;
    }
    let parts: Vec<&str> = value.split(',').collect();
    if parts.len() < 2 {
        return None;
    }
    let mut spans = Vec::new();
    for (index, raw_part) in parts.iter().enumerate() {
        let leading = raw_part.len() - raw_part.trim_start().len();
        let trailing = raw_part.trim_end().len();
        let item = raw_part.trim();
        if item.is_empty() || !is_identifier_list_item(item) {
            return None;
        }
        if index > 0 {
            spans.push(Span::styled(",".to_string(), muted_style()));
            if leading > 0 {
                spans.push(Span::styled(" ".repeat(leading), muted_style()));
            }
        }
        spans.push(Span::styled(item.to_string(), code_style()));
        if trailing < raw_part.len() {
            spans.push(Span::styled(
                raw_part[trailing..].to_string(),
                muted_style(),
            ));
        }
    }
    Some(spans)
}

fn is_identifier_list_item(value: &str) -> bool {
    value == "..."
        || (value
            .chars()
            .any(|character| character.is_ascii_alphabetic())
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/')
            }))
}

fn semantic_status_value_style(value: &str) -> Option<Style> {
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "ok" | "success" | "succeeded" | "passed" | "ready" | "aligned" | "true" | "yes" => {
            Some(ui_theme::positive())
        }
        "failed" | "failure" | "error" | "refused" | "missing" => Some(ui_theme::negative_strong()),
        "warning" | "warn" | "blocked" | "partial" | "stale" | "pending" => {
            Some(ui_theme::warning_count())
        }
        "false" | "no" | "none" | "null" => Some(muted_style()),
        _ => None,
    }
}

fn is_technical_value(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    if is_windows_path(trimmed) {
        return true;
    }
    !trimmed.chars().any(char::is_whitespace)
        && trimmed
            .chars()
            .any(|character| matches!(character, '_' | '-' | '.' | '/' | '\\' | ':'))
}

fn is_windows_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
        && bytes[0].is_ascii_alphabetic()
}

fn list_marker_parts(text: &str) -> Option<(&str, &str, &str)> {
    let prefix_len = text.len() - text.trim_start_matches(' ').len();
    let (prefix, rest) = text.split_at(prefix_len);
    for marker in ["- ", "* ", "+ ", "• "] {
        if let Some(after_marker) = rest.strip_prefix(marker) {
            return Some((prefix, marker, after_marker));
        }
    }
    if let Some(marker_end) = numbered_list_marker_end(rest) {
        return Some((prefix, &rest[..marker_end], &rest[marker_end..]));
    }
    let marker_end = lettered_list_marker_end(rest)?;
    Some((prefix, &rest[..marker_end], &rest[marker_end..]))
}

fn numbered_list_marker_end(text: &str) -> Option<usize> {
    let mut digits_end = 0usize;
    for (index, character) in text.char_indices() {
        if character.is_ascii_digit() {
            digits_end = index + character.len_utf8();
        } else {
            break;
        }
    }
    if digits_end == 0 || text.get(digits_end..digits_end + 2) != Some(". ") {
        return None;
    }
    Some(digits_end + 2)
}

fn lettered_list_marker_end(text: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    let (_, letter) = chars.next()?;
    let (dot_index, dot) = chars.next()?;
    let (space_index, space) = chars.next()?;
    if !letter.is_ascii_uppercase() || dot != '.' || space != ' ' {
        return None;
    }
    Some(space_index + space.len_utf8()).filter(|_| dot_index == letter.len_utf8())
}

fn inline_code_spans(text: &str, normal_style: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = text;
    let mut in_code = false;
    let mut in_bold = false;
    let mut in_italic = false;
    while let Some((index, marker)) = next_inline_marker(remaining, in_code) {
        let (before, after_marker) = remaining.split_at(index);
        if !before.is_empty() {
            append_inline_text_spans(
                &mut spans,
                before,
                inline_style(normal_style, in_code, in_bold, in_italic),
                in_code,
            );
        }
        if marker == "`" {
            remaining = &after_marker[1..];
            in_code = !in_code;
        } else if marker == "**" {
            remaining = &after_marker[2..];
            in_bold = !in_bold;
        } else {
            remaining = &after_marker[1..];
            in_italic = !in_italic;
        }
    }
    if !remaining.is_empty() {
        append_inline_text_spans(
            &mut spans,
            remaining,
            inline_style(normal_style, in_code, in_bold, in_italic),
            in_code,
        );
    }
    spans
}

fn append_inline_text_spans(
    spans: &mut Vec<Span<'static>>,
    text: &str,
    style: Style,
    in_code: bool,
) {
    if in_code || style.fg == muted_style().fg {
        spans.push(Span::styled(text.to_string(), style));
        return;
    }
    if next_markdown_link_parts(text).is_none() {
        append_plain_inline_text_spans(spans, text, style);
        return;
    }
    for (segment, segment_style) in split_markdown_link_segments(text, style) {
        if segment_style.fg == code_style().fg || segment_style.fg == muted_style().fg {
            spans.push(Span::styled(segment, segment_style));
        } else {
            append_plain_inline_text_spans(spans, &segment, segment_style);
        }
    }
}

fn split_markdown_link_segments(text: &str, normal_style: Style) -> Vec<(String, Style)> {
    let mut segments = Vec::new();
    let mut remaining = text;
    while let Some((prefix, image_marker, label, url, suffix)) = next_markdown_link_parts(remaining)
    {
        if !prefix.is_empty() {
            segments.push((prefix.to_string(), normal_style));
        }
        if image_marker {
            segments.push(("!".to_string(), muted_style()));
        }
        segments.push(("[".to_string(), muted_style()));
        segments.push((label.to_string(), normal_style));
        segments.push(("](".to_string(), muted_style()));
        segments.push((url.to_string(), code_style()));
        segments.push((")".to_string(), muted_style()));
        remaining = suffix;
    }
    if !remaining.is_empty() {
        segments.push((remaining.to_string(), normal_style));
    }
    segments
}

fn next_markdown_link_parts(text: &str) -> Option<(&str, bool, &str, &str, &str)> {
    let open = text.find('[')?;
    let image_marker = open > 0 && text.as_bytes().get(open - 1) == Some(&b'!');
    let prefix_end = if image_marker { open - 1 } else { open };
    let label_start = open + 1;
    let label_end = text[label_start..].find("](")? + label_start;
    let url_start = label_end + 2;
    let url_end = text[url_start..].find(')')? + url_start;
    let label = &text[label_start..label_end];
    let url = &text[url_start..url_end];
    if label.is_empty() || url.is_empty() || !is_link_url(url) {
        return None;
    }
    Some((
        &text[..prefix_end],
        image_marker,
        label,
        url,
        &text[url_end + 1..],
    ))
}

fn is_link_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn append_plain_inline_text_spans(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
    for (segment, segment_style) in split_windows_path_segments(text, style) {
        if segment_style.fg == code_style().fg {
            spans.push(Span::styled(segment, segment_style));
        } else {
            for (inner_segment, inner_style) in
                split_inline_technical_token_segments(&segment, segment_style)
            {
                spans.push(Span::styled(inner_segment, inner_style));
            }
        }
    }
}

fn split_windows_path_segments(text: &str, normal_style: Style) -> Vec<(String, Style)> {
    let mut segments = Vec::new();
    let mut remaining = text;
    while let Some(start) = windows_path_start(remaining) {
        let (before, path_and_after) = remaining.split_at(start);
        if !before.is_empty() {
            segments.push((before.to_string(), normal_style));
        }
        let path_len = windows_path_len(path_and_after);
        let (path, after) = path_and_after.split_at(path_len);
        segments.push((path.to_string(), code_style()));
        remaining = after;
    }
    if !remaining.is_empty() {
        segments.push((remaining.to_string(), normal_style));
    }
    segments
}

fn split_inline_technical_token_segments(text: &str, normal_style: Style) -> Vec<(String, Style)> {
    let mut segments = Vec::new();
    let mut remaining = text;
    while let Some((token_start, token_end)) = next_non_whitespace_span(remaining) {
        let (before, token_and_after) = remaining.split_at(token_start);
        if !before.is_empty() {
            push_inline_segment(&mut segments, before.to_string(), normal_style);
        }
        let (token, after) = token_and_after.split_at(token_end - token_start);
        append_token_with_optional_code_style(&mut segments, token, normal_style);
        remaining = after;
    }
    if !remaining.is_empty() {
        push_inline_segment(&mut segments, remaining.to_string(), normal_style);
    }
    segments
}

fn next_non_whitespace_span(text: &str) -> Option<(usize, usize)> {
    let start = text
        .char_indices()
        .find_map(|(index, character)| (!character.is_whitespace()).then_some(index))?;
    let end = text[start..]
        .char_indices()
        .find_map(|(index, character)| character.is_whitespace().then_some(start + index))
        .unwrap_or(text.len());
    Some((start, end))
}

fn append_token_with_optional_code_style(
    segments: &mut Vec<(String, Style)>,
    token: &str,
    normal_style: Style,
) {
    let leading_len = token
        .char_indices()
        .find_map(|(index, character)| is_token_core_character(character).then_some(index))
        .unwrap_or(token.len());
    let mut trailing_start = token.len();
    while trailing_start > leading_len {
        let candidate_core = &token[leading_len..trailing_start];
        let Some(character) = candidate_core.chars().next_back() else {
            break;
        };
        if is_trailing_token_punctuation(candidate_core, character) {
            trailing_start -= character.len_utf8();
        } else {
            break;
        }
    }
    let leading = &token[..leading_len];
    let core = &token[leading_len..trailing_start];
    let trailing = &token[trailing_start..];
    if !leading.is_empty() {
        push_inline_segment(segments, leading.to_string(), normal_style);
    }
    if !core.is_empty() {
        let style = if is_inline_technical_token(core) {
            code_style()
        } else {
            normal_style
        };
        push_inline_segment(segments, core.to_string(), style);
    }
    if !trailing.is_empty() {
        push_inline_segment(segments, trailing.to_string(), normal_style);
    }
}

fn is_trailing_token_punctuation(candidate_core: &str, character: char) -> bool {
    if character == ')' && candidate_core.contains('(') {
        return false;
    }
    matches!(character, '.' | ',' | ';' | ')')
        || (character == ':'
            && !candidate_core.ends_with("http:")
            && !candidate_core.ends_with("https:"))
}

fn push_inline_segment(segments: &mut Vec<(String, Style)>, text: String, style: Style) {
    if text.is_empty() {
        return;
    }
    if let Some((last_text, last_style)) = segments.last_mut() {
        if *last_style == style {
            last_text.push_str(&text);
            return;
        }
    }
    segments.push((text, style));
}

fn is_token_core_character(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(
            character,
            '_' | '-' | '.' | '/' | '\\' | ':' | '?' | '&' | '=' | '%' | '@' | '+'
        )
}

fn is_inline_technical_token(token: &str) -> bool {
    token.starts_with("https://")
        || token.starts_with("http://")
        || token.starts_with("--")
        || token.starts_with('/')
        || is_powershell_parameter_token(token)
        || is_email_token(token)
        || token.contains('_')
        || token.starts_with("narada-")
        || token.starts_with("carrier-")
        || token.starts_with("carrier_")
        || token.starts_with("checkpoint-")
        || token.starts_with("checkpoint_")
        || token.starts_with("agent_start_")
        || token.starts_with("evt-")
        || token.starts_with("dir_")
        || token.starts_with("auth_")
        || is_timestamp_token(token)
        || is_duration_token(token)
        || is_dotted_technical_token(token)
}

fn is_powershell_parameter_token(token: &str) -> bool {
    let Some(rest) = token.strip_prefix('-') else {
        return false;
    };
    !rest.is_empty()
        && rest
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_uppercase())
        && rest
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
}

fn is_timestamp_token(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() >= 16
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && (bytes.get(10) == Some(&b'T') || bytes.get(10) == Some(&b'Z'))
        && bytes.get(13) == Some(&b':')
        && token
            .chars()
            .take(4)
            .all(|character| character.is_ascii_digit())
}

fn is_duration_token(token: &str) -> bool {
    let Some(number) = token
        .strip_suffix("ms")
        .or_else(|| token.strip_suffix('s'))
        .or_else(|| token.strip_suffix('m'))
        .or_else(|| token.strip_suffix('h'))
    else {
        return false;
    };
    if number.is_empty() {
        return false;
    }
    let mut dot_count = 0usize;
    let mut digit_count = 0usize;
    for character in number.chars() {
        if character.is_ascii_digit() {
            digit_count += 1;
        } else if character == '.' {
            dot_count += 1;
            if dot_count > 1 {
                return false;
            }
        } else {
            return false;
        }
    }
    digit_count > 0
}

fn is_email_token(token: &str) -> bool {
    let Some((local, domain)) = token.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && domain
            .chars()
            .any(|character| character.is_ascii_alphabetic())
        && token.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '+' | '@')
        })
}

fn is_dotted_technical_token(token: &str) -> bool {
    token.contains('.')
        && token
            .chars()
            .any(|character| character.is_ascii_alphabetic())
        && token.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_')
        })
}

fn windows_path_start(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    bytes.windows(3).enumerate().find_map(|(index, window)| {
        let left_boundary = index == 0
            || bytes
                .get(index - 1)
                .is_some_and(|character| !character.is_ascii_alphanumeric());
        (left_boundary
            && window[0].is_ascii_alphabetic()
            && window[1] == b':'
            && (window[2] == b'\\' || window[2] == b'/'))
            .then_some(index)
    })
}

fn windows_path_len(text: &str) -> usize {
    let mut end = text.len();
    for (index, character) in text.char_indices() {
        if character.is_whitespace() {
            end = index;
            break;
        }
    }
    let mut trimmed_end = end;
    while trimmed_end > 0 {
        let Some(character) = text[..trimmed_end].chars().next_back() else {
            break;
        };
        if matches!(character, '.' | ',' | ';' | ')') {
            trimmed_end -= character.len_utf8();
        } else {
            break;
        }
    }
    trimmed_end.max(3)
}

fn next_inline_marker(text: &str, in_code: bool) -> Option<(usize, &'static str)> {
    let mut candidates = Vec::new();
    if let Some(index) = text.find('`') {
        candidates.push((index, "`"));
    }
    if !in_code {
        if let Some(index) = text.find("**") {
            candidates.push((index, "**"));
        }
        if let Some(index) = single_asterisk_marker_index(text) {
            candidates.push((index, "*"));
        }
    }
    candidates.into_iter().min_by_key(|candidate| candidate.0)
}

fn single_asterisk_marker_index(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'*'
            && bytes.get(index + 1) != Some(&b'*')
            && (index == 0 || bytes.get(index - 1) != Some(&b'*'))
        {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn inline_style(normal_style: Style, in_code: bool, in_bold: bool, in_italic: bool) -> Style {
    if in_code {
        code_style()
    } else {
        let mut style = normal_style;
        if in_bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if in_italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        style
    }
}

fn muted_style() -> Style {
    ui_theme::muted()
}

fn frame_style() -> Style {
    ui_theme::muted()
}

fn code_style() -> Style {
    ui_theme::code()
}

fn timestamp_style() -> Style {
    ui_theme::muted()
}

fn format_timestamp(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    if bytes.len() < 20
        || bytes.get(4) != Some(&b'-')
        || bytes.get(7) != Some(&b'-')
        || bytes.get(10) != Some(&b'T')
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
        || bytes.get(19) != Some(&b'.')
        || !value.ends_with('Z')
        || !is_ascii_digit_range(value, 0..4)
        || !is_ascii_digit_range(value, 5..7)
        || !is_ascii_digit_range(value, 8..10)
        || !is_ascii_digit_range(value, 11..13)
        || !is_ascii_digit_range(value, 14..16)
    {
        return None;
    }
    let date = value.get(0..10)?;
    let hour = value.get(11..13)?;
    let minute = value.get(14..16)?;
    Some(format!("{date}Z{hour}:{minute}"))
}

fn is_ascii_digit_range(value: &str, range: std::ops::Range<usize>) -> bool {
    value
        .as_bytes()
        .get(range)
        .is_some_and(|bytes| bytes.iter().all(u8::is_ascii_digit))
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for source_line in display_source_lines(text) {
        lines.extend(wrap_source_line(&source_line, width));
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn display_source_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text.lines().map(sanitize_display_source_line).collect();
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines
}

fn sanitize_display_source_line(line: &str) -> String {
    let mut sanitized = String::new();
    for character in line.chars() {
        match character {
            '\t' => sanitized.push_str("    "),
            character if character.is_control() => sanitized.push(' '),
            character => sanitized.push(character),
        }
    }
    sanitized
}

fn wrap_source_line(source_line: &str, width: usize) -> Vec<String> {
    wrap_source_line_with_inline_balance(source_line, width, true)
}

fn wrap_source_line_without_inline_balance(source_line: &str, width: usize) -> Vec<String> {
    wrap_source_line_with_inline_balance(source_line, width, false)
}

fn wrap_source_line_with_inline_balance(
    source_line: &str,
    width: usize,
    balance_inline_markers: bool,
) -> Vec<String> {
    if source_line.is_empty() {
        return vec![String::new()];
    }
    if source_line.chars().count() <= width {
        return vec![source_line.to_string()];
    }

    let continuation_prefix = wrapping_continuation_prefix(source_line, width);
    let mut remaining = source_line.to_string();
    let mut lines = Vec::new();
    while remaining.chars().count() > width {
        let split_at = preferred_split_byte_index(&remaining, width);
        let (head, tail) = remaining.split_at(split_at);
        lines.push(head.trim_end_matches(' ').to_string());
        remaining = tail.trim_start_matches(' ').to_string();
        if !remaining.is_empty() && !continuation_prefix.is_empty() {
            remaining = format!("{continuation_prefix}{remaining}");
        }
    }
    lines.push(remaining);
    if balance_inline_markers {
        balance_inline_markers_across_wrapped_lines(lines)
    } else {
        lines
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct InlineMarkerState {
    in_code: bool,
    in_bold: bool,
    in_italic: bool,
}

fn balance_inline_markers_across_wrapped_lines(lines: Vec<String>) -> Vec<String> {
    let mut state = InlineMarkerState::default();
    lines
        .into_iter()
        .map(|line| {
            let opening = inline_marker_opening_prefix(state);
            let mut next_state = state;
            update_inline_marker_state(&mut next_state, &line);
            let closing = inline_marker_closing_suffix(next_state);
            state = next_state;
            format!("{opening}{line}{closing}")
        })
        .collect()
}

fn inline_marker_opening_prefix(state: InlineMarkerState) -> String {
    let mut prefix = String::new();
    if state.in_bold {
        prefix.push_str("**");
    }
    if state.in_italic {
        prefix.push('*');
    }
    if state.in_code {
        prefix.push('`');
    }
    prefix
}

fn inline_marker_closing_suffix(state: InlineMarkerState) -> String {
    let mut suffix = String::new();
    if state.in_code {
        suffix.push('`');
    }
    if state.in_italic {
        suffix.push('*');
    }
    if state.in_bold {
        suffix.push_str("**");
    }
    suffix
}

fn update_inline_marker_state(state: &mut InlineMarkerState, line: &str) {
    let mut remaining = line;
    while let Some((index, marker)) = next_inline_marker(remaining, state.in_code) {
        let after_marker = &remaining[index..];
        if marker == "`" {
            state.in_code = !state.in_code;
            remaining = &after_marker[1..];
        } else if marker == "**" {
            state.in_bold = !state.in_bold;
            remaining = &after_marker[2..];
        } else {
            state.in_italic = !state.in_italic;
            remaining = &after_marker[1..];
        }
    }
}

fn wrapping_continuation_prefix(source_line: &str, width: usize) -> String {
    let key_value_prefix = key_value_line_parts(source_line)
        .map(|(key, _)| " ".repeat(key.chars().count() + ": ".chars().count()));
    let diff_prefix =
        diff_line_parts(source_line).map(|(marker, _, _)| " ".repeat(marker.chars().count()));
    let powershell_prefix =
        powershell_prompt_parts(source_line).map(|(prompt, _)| " ".repeat(prompt.chars().count()));
    let blockquote_prefix =
        blockquote_marker_parts(source_line).map(|(prefix, marker, _)| format!("{prefix}{marker}"));
    let list_prefix = list_marker_parts(source_line).map(|(prefix, marker, rest)| {
        let task_checkbox_width = task_checkbox_parts(rest)
            .map(|(checkbox, _)| checkbox.chars().count())
            .unwrap_or(0);
        format!(
            "{prefix}{}",
            " ".repeat(marker.chars().count() + task_checkbox_width)
        )
    });
    let prefix = key_value_prefix
        .or(diff_prefix)
        .or(powershell_prefix)
        .or(blockquote_prefix)
        .or(list_prefix)
        .unwrap_or_else(|| leading_whitespace(source_line));
    if prefix.chars().count() < width {
        prefix
    } else {
        String::new()
    }
}

fn leading_whitespace(value: &str) -> String {
    value
        .chars()
        .take_while(|character| character.is_whitespace() && *character != '\n')
        .collect()
}

fn preferred_split_byte_index(value: &str, width: usize) -> usize {
    let mut last_space = None;
    let mut fallback = value.len();
    for (char_index, (byte_index, character)) in value.char_indices().enumerate() {
        if char_index == width {
            fallback = byte_index;
            if character == ' ' {
                last_space = Some(byte_index);
            }
            break;
        }
        if character == ' ' && char_index > 0 {
            last_space = Some(byte_index);
        }
    }
    last_space.unwrap_or(fallback)
}

fn status_paragraph(model: &AppViewModel) -> Paragraph<'_> {
    let width = model.layout.status.width as usize;
    let segments = status_segments_with_scroll(model);
    Paragraph::new(Line::from(status_spans(&segments, width)))
}

fn status_segments_with_scroll(model: &AppViewModel) -> Vec<StatusSegment> {
    let mut segments = model.status.segments.clone();
    if model.transcript_scroll_offset > 0
        && !segments
            .iter()
            .any(|segment| segment.key == "transcript_scroll_offset")
    {
        segments.push(StatusSegment {
            key: "transcript_scroll_offset".to_string(),
            label: "scroll".to_string(),
            value: model.transcript_scroll_offset.to_string(),
        });
    }
    segments
}

fn status_spans(segments: &[StatusSegment], width: usize) -> Vec<Span<'static>> {
    let ordered = prioritized_status_segments(segments);
    let mut spans = Vec::new();
    let mut used = 0usize;
    let mut omitted = false;
    for segment in ordered {
        let segment_len = status_segment_len(segment);
        let separator_len = if spans.is_empty() { 0 } else { 3 };
        if used + separator_len + segment_len > width {
            let available = width.saturating_sub(used + separator_len);
            if available >= 4 {
                if !spans.is_empty() {
                    spans.push(Span::styled(" | ", muted_style()));
                }
                spans.extend(truncated_status_segment_spans(segment, available));
                used = width;
                omitted = false;
                break;
            }
            omitted = true;
            continue;
        }
        if !spans.is_empty() {
            spans.push(Span::styled(" | ", muted_style()));
            used += separator_len;
        }
        spans.extend(status_segment_spans(segment));
        used += segment_len;
    }
    let ellipsis_len = if spans.is_empty() { 3 } else { 6 };
    if omitted && used + ellipsis_len <= width {
        if !spans.is_empty() {
            spans.push(Span::styled(" | ".to_string(), muted_style()));
        }
        spans.push(Span::styled("...", muted_style()));
    } else if omitted && spans.is_empty() && width > 0 {
        spans.push(Span::styled(".".repeat(width), muted_style()));
    }
    spans
}

fn span_text_len(spans: &[Span<'static>]) -> usize {
    spans.iter().map(|span| span.content.chars().count()).sum()
}

fn prioritized_status_segments(segments: &[StatusSegment]) -> Vec<&StatusSegment> {
    const PRIORITY: &[&str] = &[
        "turn_state",
        "draft_chars",
        "queued_inputs",
        "held_system_directives",
        "oldest_held_age",
        "esc_action",
        "transcript_scroll_offset",
        "last_error",
        "provider_state",
        "provider_adapter_state",
        "mcp_state",
        "terminal_state",
        "session",
        "transcript_items",
    ];
    let mut ordered = Vec::new();
    for key in PRIORITY {
        if let Some(segment) = segments
            .iter()
            .find(|segment| segment.key == *key && status_segment_is_visible(segment))
        {
            ordered.push(segment);
        }
    }
    ordered
}

fn status_segment_len(segment: &StatusSegment) -> usize {
    status_segment_text(segment).chars().count()
}

fn status_segment_spans(segment: &StatusSegment) -> Vec<Span<'static>> {
    if segment.key == "identity" {
        return vec![Span::styled(segment.value.clone(), ui_theme::agent_label())];
    }
    if segment.key == "queued_inputs" {
        return queued_operator_steering_status_spans(&segment.value);
    }
    if segment.key == "held_system_directives" {
        return held_system_directives_status_spans(&segment.value);
    }
    let label = status_segment_display_label(segment);
    let value = status_segment_display_value(segment);
    let value_spans = status_value_spans(segment.key.as_str(), segment.value.as_str(), &value);
    if label.is_empty() {
        return value_spans;
    }
    let mut spans = vec![
        Span::styled(label, ui_theme::status_key()),
        Span::styled(" ".to_string(), muted_style()),
    ];
    spans.extend(value_spans);
    spans
}

fn status_value_spans(key: &str, raw_value: &str, display_value: &str) -> Vec<Span<'static>> {
    if key == "turn_state" {
        if let Some(spans) = turn_state_value_spans(display_value) {
            return spans;
        }
    }
    if key == "oldest_held_age" && is_duration_phrase(display_value) {
        return vec![Span::styled(display_value.to_string(), code_style())];
    }
    vec![Span::styled(
        display_value.to_string(),
        status_value_style(key, raw_value),
    )]
}

fn turn_state_value_spans(display_value: &str) -> Option<Vec<Span<'static>>> {
    let parts: Vec<&str> = display_value.split_whitespace().collect();
    match parts.as_slice() {
        ["thinking", durations @ ..] if !durations.is_empty() && all_duration_tokens(durations) => {
            Some(phase_with_duration_spans("thinking", durations))
        }
        ["calling", tool, durations @ ..]
            if !durations.is_empty() && all_duration_tokens(durations) =>
        {
            let mut spans = vec![
                Span::styled("calling".to_string(), ui_theme::positive()),
                Span::styled(" ".to_string(), muted_style()),
                Span::styled((*tool).to_string(), code_style()),
            ];
            append_duration_spans(&mut spans, durations);
            Some(spans)
        }
        ["typing", "operator", mode, count]
            if operator_activity_mode_style(mode).is_some() && count_is_scan_data(count) =>
        {
            Some(operator_activity_status_spans("typing", mode, count))
        }
        ["queued", "operator", mode, count]
            if operator_activity_mode_style(mode).is_some() && count_is_scan_data(count) =>
        {
            Some(operator_activity_status_spans("queued", mode, count))
        }
        _ => None,
    }
}

fn operator_activity_status_spans(action: &str, mode: &str, count: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(action.to_string(), ui_theme::positive()),
        Span::styled(" ".to_string(), muted_style()),
        Span::styled("operator".to_string(), ui_theme::operator_label()),
        Span::styled(" ".to_string(), muted_style()),
        Span::styled(
            mode.to_string(),
            operator_activity_mode_style(mode).unwrap_or_else(ui_theme::warning_count),
        ),
        Span::styled(" ".to_string(), muted_style()),
        Span::styled(count.to_string(), ui_theme::warning_count()),
    ]
}

fn queued_operator_steering_status_spans(count: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled("queued".to_string(), ui_theme::positive()),
        Span::styled(" ".to_string(), muted_style()),
        Span::styled("operator".to_string(), ui_theme::operator_label()),
        Span::styled(" ".to_string(), muted_style()),
        Span::styled("steering".to_string(), ui_theme::warning_count()),
        Span::styled(" ".to_string(), muted_style()),
        Span::styled(count.to_string(), ui_theme::warning_count()),
    ]
}

fn held_system_directives_status_spans(count: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled("held".to_string(), ui_theme::positive()),
        Span::styled(" ".to_string(), muted_style()),
        Span::styled("system".to_string(), system_label_style()),
        Span::styled(" ".to_string(), muted_style()),
        Span::styled("directives".to_string(), ui_theme::warning_count()),
        Span::styled(" ".to_string(), muted_style()),
        Span::styled(count.to_string(), ui_theme::warning_count()),
    ]
}

fn operator_activity_mode_style(mode: &str) -> Option<Style> {
    match mode {
        "directive" | "directives" => Some(ui_theme::operator_directive_label()),
        "note" | "notes" => Some(ui_theme::warning_count()),
        _ => None,
    }
}

fn count_is_scan_data(value: &str) -> bool {
    let trimmed = value
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(value);
    !trimmed.is_empty() && trimmed.chars().all(|ch| ch.is_ascii_digit())
}

fn all_duration_tokens(values: &[&str]) -> bool {
    values.iter().all(|value| is_duration_token(value))
}

fn phase_with_duration_spans(phase: &str, durations: &[&str]) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(phase.to_string(), ui_theme::positive())];
    append_duration_spans(&mut spans, durations);
    spans
}

fn append_duration_spans(spans: &mut Vec<Span<'static>>, durations: &[&str]) {
    for duration in durations {
        spans.push(Span::styled(" ".to_string(), muted_style()));
        spans.push(Span::styled((*duration).to_string(), code_style()));
    }
}

fn status_segment_text(segment: &StatusSegment) -> String {
    status_segment_compact_text(segment)
}

fn truncated_status_segment_spans(segment: &StatusSegment, width: usize) -> Vec<Span<'static>> {
    let label = status_segment_display_label(segment);
    let value = status_segment_display_value(segment);
    if label.is_empty() {
        return truncate_spans_to_width(
            status_value_spans(segment.key.as_str(), segment.value.as_str(), &value),
            width,
        );
    }

    let label_len = label.chars().count();
    if width <= label_len + 1 {
        return vec![Span::styled(
            truncate_text(&label, width),
            ui_theme::status_key(),
        )];
    }

    let mut spans = vec![
        Span::styled(label, ui_theme::status_key()),
        Span::styled(" ".to_string(), muted_style()),
    ];
    spans.extend(truncate_spans_to_width(
        status_value_spans(segment.key.as_str(), segment.value.as_str(), &value),
        width - label_len - 1,
    ));
    spans
}

fn truncate_spans_to_width(spans: Vec<Span<'static>>, width: usize) -> Vec<Span<'static>> {
    if span_text_len(&spans) <= width {
        return spans;
    }
    if width == 0 {
        return Vec::new();
    }
    if width <= 3 {
        return vec![Span::styled(".".repeat(width), muted_style())];
    }
    let mut remaining = width - 3;
    let mut truncated = Vec::new();
    for span in spans {
        if remaining == 0 {
            break;
        }
        let span_len = span.content.chars().count();
        if span_len <= remaining {
            remaining -= span_len;
            truncated.push(span);
        } else {
            let kept = span.content.chars().take(remaining).collect::<String>();
            truncated.push(Span::styled(kept, span.style));
            remaining = 0;
        }
    }
    truncated.push(Span::styled("...", muted_style()));
    truncated
}

fn truncate_text(value: &str, width: usize) -> String {
    let length = value.chars().count();
    if length <= width {
        return value.to_string();
    }
    if width == 0 {
        return String::new();
    }
    if width <= 3 {
        return ".".repeat(width);
    }
    let head: String = value.chars().take(width - 3).collect();
    format!("{head}...")
}

fn status_segment_display_label(segment: &StatusSegment) -> String {
    match segment.key.as_str() {
        "turn_state" => String::new(),
        "draft_chars" => "draft".to_string(),
        "queued_inputs" => "queued operator steering".to_string(),
        "held_system_directives" => "held system directives".to_string(),
        "oldest_held_age" => "oldest".to_string(),
        "esc_action" => "Esc".to_string(),
        "transcript_scroll_offset" => "scroll".to_string(),
        "provider_state" => "provider".to_string(),
        "provider_adapter_state" => "provider adapter".to_string(),
        "mcp_state" => "mcp".to_string(),
        "terminal_state" => "terminal".to_string(),
        "last_error" => "error".to_string(),
        "transcript_items" => "transcript".to_string(),
        "session" => "session".to_string(),
        _ => segment.label.clone(),
    }
}

fn status_segment_display_value(segment: &StatusSegment) -> String {
    match (segment.key.as_str(), segment.value.as_str()) {
        ("turn_state", value) => turn_state_display_value(value),
        ("draft_chars", "1") => "1 char".to_string(),
        ("draft_chars", value) => format!("{value} chars"),
        ("transcript_scroll_offset", "1") => "1 line".to_string(),
        ("transcript_scroll_offset", value) => format!("{value} lines"),
        ("provider_state", value)
        | ("provider_adapter_state", value)
        | ("mcp_state", value)
        | ("terminal_state", value) => human_status_value(value),
        ("last_error", value) => human_status_value(value),
        _ => segment.value.clone(),
    }
}

fn human_status_value(value: &str) -> String {
    value.replace('_', " ")
}

fn status_value_style(key: &str, value: &str) -> Style {
    match key {
        "identity" => ui_theme::agent_label(),
        "turn_state" => {
            if value == "active"
                || value.starts_with("active ")
                || value.starts_with("calling ")
                || value == "working"
            {
                ui_theme::positive()
            } else if value == "failed" || value == "interrupted" {
                ui_theme::negative()
            } else {
                muted_style()
            }
        }
        "draft_chars"
        | "queued_inputs"
        | "held_system_directives"
        | "oldest_held_age"
        | "esc_action"
        | "transcript_scroll_offset" => {
            if value == "0" {
                muted_style()
            } else {
                ui_theme::warning_count()
            }
        }
        "provider_state" | "provider_adapter_state" | "mcp_state" | "terminal_state" => {
            runtime_status_style(value)
        }
        "last_error" => {
            if value == "none" {
                muted_style()
            } else {
                ui_theme::negative_strong()
            }
        }
        "session" => code_style(),
        "transcript_items" => {
            if value == "0" {
                muted_style()
            } else {
                code_style()
            }
        }
        _ => ui_theme::body(),
    }
}

fn runtime_status_style(value: &str) -> Style {
    if matches!(value, "configured" | "admitted" | "idle" | "working") {
        ui_theme::positive()
    } else if value.starts_with("configured_") {
        ui_theme::warning_count()
    } else if value.starts_with("refused")
        || value.starts_with("failed")
        || value.starts_with("error")
    {
        ui_theme::negative_strong()
    } else if matches!(value, "disabled" | "none") {
        muted_style()
    } else {
        ui_theme::body()
    }
}

fn composer_block(model: &AppViewModel) -> Block<'_> {
    Block::default()
        .title(composer_title(
            model,
            model.layout.composer.width.saturating_sub(2) as usize,
        ))
        .borders(Borders::ALL)
        .border_style(frame_style())
}

fn composer_title(model: &AppViewModel, width: usize) -> Line<'static> {
    let (source, identity) = composer_prompt_parts(&model.composer.prompt_label);
    let mut spans = composer_source_spans(&source);
    spans.push(Span::styled(" -> ", muted_style()));
    spans.push(Span::styled(identity, ui_theme::agent_label()));
    spans.push(Span::styled(">", muted_style()));
    append_composer_affordance_spans(&mut spans, &model.composer.queued_note_affordance, width);
    append_composer_affordance_spans(&mut spans, &model.composer.held_directive_affordance, width);
    Line::from(truncate_spans_to_width(spans, width))
}

fn append_composer_affordance_spans(
    spans: &mut Vec<Span<'static>>,
    affordance: &str,
    width: usize,
) {
    let Some((label, count)) = visible_composer_affordance_parts(affordance) else {
        return;
    };
    let affordance_spans = composer_affordance_spans(label, count);
    let current_len = span_text_len(spans);
    let affordance_len = span_text_len(&affordance_spans);
    if current_len + affordance_len <= width {
        spans.extend(affordance_spans);
    } else if current_len + 6 <= width && !composer_title_has_omission(spans) {
        spans.push(Span::styled(" | ".to_string(), muted_style()));
        spans.push(Span::styled("...".to_string(), muted_style()));
    }
}

fn composer_title_has_omission(spans: &[Span<'static>]) -> bool {
    spans.iter().any(|span| span.content.as_ref() == "...")
}

fn visible_composer_affordance_parts(affordance: &str) -> Option<(&str, &str)> {
    let (label, count) = affordance.rsplit_once(": ")?;
    if count == "0" || count.is_empty() {
        return None;
    }
    Some((label, count))
}

fn composer_affordance_spans(label: &str, count: &str) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(" | ".to_string(), muted_style())];
    if label == "queued notes" {
        spans.push(Span::styled("queued ".to_string(), ui_theme::positive()));
        spans.push(Span::styled(
            "operator".to_string(),
            ui_theme::operator_label(),
        ));
        spans.push(Span::styled(
            " notes".to_string(),
            ui_theme::warning_count(),
        ));
    } else if label == "held system directives" {
        spans.push(Span::styled("held ".to_string(), ui_theme::positive()));
        spans.push(Span::styled("system".to_string(), system_label_style()));
        spans.push(Span::styled(
            " directives".to_string(),
            ui_theme::warning_count(),
        ));
    } else {
        spans.push(Span::styled(label.to_string(), ui_theme::status_key()));
    }
    spans.push(Span::styled(
        format!(": {count}"),
        ui_theme::warning_count(),
    ));
    spans
}

fn composer_source_spans(source: &str) -> Vec<Span<'static>> {
    if source == "operator note" {
        return vec![
            Span::styled("operator".to_string(), ui_theme::operator_label()),
            Span::styled(" note".to_string(), ui_theme::warning_count()),
        ];
    }
    vec![Span::styled(source.to_string(), ui_theme::operator_label())]
}

fn composer_draft_style(model: &AppViewModel) -> Style {
    let (source, _) = composer_prompt_parts(&model.composer.prompt_label);
    if source == "operator note" {
        ui_theme::warning_count()
    } else {
        ui_theme::positive()
    }
}

fn composer_prompt_parts(prompt_label: &str) -> (String, String) {
    let trimmed = prompt_label.trim_end_matches('>');
    trimmed
        .rsplit_once(" -> ")
        .map(|(source, identity)| (source.to_string(), identity.to_string()))
        .unwrap_or_else(|| ("operator".to_string(), prompt_label.to_string()))
}

fn to_tui_rect(rect: Rect) -> TuiRect {
    TuiRect::new(rect.x, rect.y, rect.width, rect.height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_view_model::{AppViewInput, build_app_view};
    use crate::composer_view_model::ComposerViewInput;
    use crate::input_queue::TurnState;
    use crate::layout_model::{LayoutConfig, TerminalSize};
    use crate::status_view_model::{
        ProviderRuntimeState, RuntimePostureState, StatusSegment, StatusViewInput,
    };
    use crate::transcript_projection::{TranscriptActor, TranscriptItem, TranscriptItemKind};
    use ratatui::style::{Color, Modifier};

    fn model() -> AppViewModel {
        build_app_view(&AppViewInput {
            terminal_size: TerminalSize {
                width: 80,
                height: 12,
            },
            layout_config: LayoutConfig::default(),
            transcript_items: vec![TranscriptItem {
                kind: TranscriptItemKind::InputAdmitted,
                actor: TranscriptActor::Operator,
                turn_id: "turn_1".to_string(),
                text: "run startup sequence".to_string(),
                sequence: None,
                projection_key: None,
                occurred_at: Some("2026-05-30T00:00:00.000Z".to_string()),
            }],
            status: StatusViewInput {
                identity: "sonar.resident".to_string(),
                session: "carrier_1".to_string(),
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
                draft_text: "hello".to_string(),
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

    fn find_text_position(buffer: &Buffer, needle: &str) -> Option<(u16, u16)> {
        let area = buffer.area;
        for y in area.y..area.y + area.height {
            let line = buffer_line(buffer, y);
            if let Some(index) = line.find(needle) {
                return Some((area.x + index as u16, y));
            }
        }
        None
    }

    fn buffer_line(buffer: &Buffer, y: u16) -> String {
        let area = buffer.area;
        let mut line = String::new();
        for x in area.x..area.x + area.width {
            line.push_str(buffer[(x, y)].symbol());
        }
        line
    }

    #[test]
    fn wrap_source_line_aligns_powershell_continuation_under_command() {
        let lines = wrap_source_line(
            "PS D:\\code\\narada> narada-proper-mcp --site-root D:\\code\\narada --reconcile-mcp-policy --apply",
            54,
        );

        assert_eq!(
            lines,
            vec![
                "PS D:\\code\\narada> narada-proper-mcp --site-root".to_string(),
                "                   D:\\code\\narada".to_string(),
                "                   --reconcile-mcp-policy --apply".to_string()
            ]
        );
    }

    #[test]
    fn wrap_source_line_aligns_diff_continuation_under_payload() {
        let lines = wrap_source_line("+added line with enough words to wrap", 18);

        assert_eq!(
            lines,
            vec![
                "+added line with".to_string(),
                " enough words to".to_string(),
                " wrap".to_string()
            ]
        );
    }

    #[test]
    fn wrap_source_line_aligns_key_value_continuation_under_value() {
        let lines = wrap_source_line(
            "Authority locus: narada_proper with additional scoped detail",
            32,
        );

        assert_eq!(
            lines,
            vec![
                "Authority locus: narada_proper".to_string(),
                "                 with additional".to_string(),
                "                 scoped detail".to_string()
            ]
        );
    }

    #[test]
    fn wrap_source_line_aligns_list_continuation_under_item_text() {
        let lines = wrap_source_line("- alpha beta gamma delta", 14);

        assert_eq!(
            lines,
            vec!["- alpha beta".to_string(), "  gamma delta".to_string()]
        );
    }

    #[test]
    fn wrap_source_line_keeps_blockquote_marker_on_continuation() {
        let lines = wrap_source_line("> alpha beta gamma delta", 14);

        assert_eq!(
            lines,
            vec!["> alpha beta".to_string(), "> gamma delta".to_string()]
        );
    }

    #[test]
    fn wrap_source_line_aligns_task_list_continuation_under_task_text() {
        let lines = wrap_source_line("- [ ] alpha beta gamma delta", 16);

        assert_eq!(
            lines,
            vec![
                "- [ ] alpha beta".to_string(),
                "      gamma".to_string(),
                "      delta".to_string()
            ]
        );
    }

    #[test]
    fn wrap_source_line_balances_inline_code_markers_across_wrapped_lines() {
        let lines = wrap_source_line("Use `narada-directive-render-context` now", 18);

        assert_eq!(
            lines,
            vec![
                "Use".to_string(),
                "`narada-directive-`".to_string(),
                "`render-context`".to_string(),
                "now".to_string()
            ]
        );
    }

    #[test]
    fn wrap_source_line_can_preserve_literal_code_markers() {
        let lines =
            wrap_source_line_without_inline_balance("echo `literal marker with long content`", 17);

        assert_eq!(
            lines,
            vec![
                "echo `literal".to_string(),
                "marker with long".to_string(),
                "content`".to_string()
            ]
        );
    }

    #[test]
    fn renders_app_view_into_buffer_with_textarea_composer() {
        let model = model();
        let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 12));

        render_app_to_buffer(&model, &mut buffer);
        let text = buffer_text(&buffer);

        assert!(text.contains("Transcript"));
        assert!(text.contains("operator -> sonar.resident:"));
        assert!(text.contains("  run startup sequence"));
        assert!(text.contains("  2026-05-30Z00:00"));
        assert!(text.contains("sonar.resident"));
        assert!(!text.contains("agent="));
        assert!(text.contains("operator -> sonar.resident>"));
        assert!(!text.contains("Composer:"));
        assert!(text.contains("hello"));
    }

    #[test]
    fn scrolled_transcript_view_trims_boundary_separator_rows() {
        let lines = vec![
            Line::from("first label"),
            Line::from(""),
            Line::from("second label"),
            Line::from("second body"),
            Line::from(""),
            Line::from("third label"),
        ];

        let visible = visible_scrolled_lines(lines, 3, 1);

        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].spans[0].content.as_ref(), "second label");
        assert_eq!(visible[1].spans[0].content.as_ref(), "second body");
    }

    #[test]
    fn scrolled_transcript_view_uses_marker_when_slice_is_only_separators() {
        let lines = vec![
            Line::from("first label"),
            Line::from(""),
            Line::from(""),
            Line::from("second label"),
        ];

        let visible = visible_scrolled_lines(lines, 1, 1);

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].spans[0].content.as_ref(), "  ");
        assert_eq!(visible[0].spans[0].style.fg, Some(Color::DarkGray));
        assert_eq!(visible[0].spans[1].content.as_ref(), "...");
        assert_eq!(visible[0].spans[1].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn oversized_latest_block_tail_keeps_participant_label() {
        let lines = vec![
            Line::from(Span::styled("sonar.resident:", agent_label_style())),
            Line::from("  first body line"),
            Line::from("  second body line"),
            Line::from("  third body line"),
            Line::from("  2026-05-30Z00:14"),
        ];

        let visible = visible_tail_lines(lines, 4);

        assert_eq!(visible.len(), 4);
        assert_eq!(visible[0].spans[0].content.as_ref(), "sonar.resident:");
        assert_eq!(
            visible[0].spans[0].style.fg,
            Some(ratatui::style::Color::Cyan)
        );
        assert_eq!(visible[1].spans[1].content.as_ref(), "...");
        assert_eq!(visible[2].spans[0].content.as_ref(), "  third body line");
        assert_eq!(visible[3].spans[0].content.as_ref(), "  2026-05-30Z00:14");
    }

    #[test]
    fn transcript_body_and_timestamp_keep_offsets_and_muted_timestamp_style() {
        let model = model();
        let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 12));

        render_app_to_buffer(&model, &mut buffer);

        let (label_x, label_y) = find_text_position(&buffer, "operator -> sonar.resident:")
            .expect("label line is rendered");
        let (body_x, body_y) =
            find_text_position(&buffer, "run startup sequence").expect("body line is rendered");
        let (timestamp_x, timestamp_y) =
            find_text_position(&buffer, "2026-05-30Z00:00").expect("timestamp line is rendered");

        assert_eq!(body_x, label_x + 2);
        assert_eq!(body_y, label_y + 1);
        assert_eq!(timestamp_x, label_x + 2);
        assert_eq!(timestamp_y, body_y + 1);
        assert_eq!(buffer[(timestamp_x, timestamp_y)].fg, Color::DarkGray);
        assert_eq!(buffer[(timestamp_x + 10, timestamp_y)].fg, Color::DarkGray);
        assert_eq!(buffer[(timestamp_x + 15, timestamp_y)].fg, Color::DarkGray);
        assert_eq!(buffer[(body_x, body_y)].fg, Color::White);
    }

    #[test]
    fn compact_timestamp_styles_entire_value_as_muted() {
        let spans = timestamp_line_spans("2026-06-01Z23:54", 2);

        assert_eq!(spans[0].content.as_ref(), "  ");
        assert_eq!(spans[1].content.as_ref(), "2026-06-01");
        assert_eq!(spans[2].content.as_ref(), "Z");
        assert_eq!(spans[3].content.as_ref(), "23:54");
        for span in spans {
            assert_eq!(span.style.fg, Some(Color::DarkGray));
        }
    }

    #[test]
    fn compact_timestamp_requires_utc_rfc3339_shape() {
        assert_eq!(
            format_timestamp("2026-05-30T18:29:02.000Z").as_deref(),
            Some("2026-05-30Z18:29")
        );
        assert_eq!(format_timestamp("2026-05-30 18:29:02.000Z"), None);
        assert_eq!(format_timestamp("2026-05-30T18:29:02.000-05:00"), None);
        assert_eq!(format_timestamp("2026-05-30T18:AA:02.000Z"), None);
    }

    #[test]
    fn transcript_blocks_keep_exactly_one_blank_separator_line() {
        let model = build_app_view(&AppViewInput {
            terminal_size: TerminalSize {
                width: 100,
                height: 18,
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
                    kind: TranscriptItemKind::ProviderTextDelta,
                    actor: TranscriptActor::Agent,
                    turn_id: "turn_1".to_string(),
                    text: "Startup sequence completed.".to_string(),
                    sequence: None,
                    projection_key: None,
                    occurred_at: Some("2026-05-30T00:01:00.000Z".to_string()),
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
        let mut buffer = Buffer::empty(TuiRect::new(0, 0, 100, 18));

        render_app_to_buffer(&model, &mut buffer);

        let (_, first_timestamp_y) = find_text_position(&buffer, "2026-05-30Z00:00")
            .expect("first timestamp line is rendered");
        let (_, second_body_y) = find_text_position(&buffer, "Startup sequence completed.")
            .expect("second body line is rendered");
        let second_label_y = second_body_y - 1;
        let separator_line = buffer_line(&buffer, first_timestamp_y + 1);

        assert_eq!(second_label_y, first_timestamp_y + 2);
        assert!(separator_line.trim_matches(['│', ' ']).is_empty());
    }

    #[test]
    fn participant_labels_keep_distinct_colors() {
        let model = build_app_view(&AppViewInput {
            terminal_size: TerminalSize {
                width: 100,
                height: 18,
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
                    kind: TranscriptItemKind::ProviderTextDelta,
                    actor: TranscriptActor::Agent,
                    turn_id: "turn_1".to_string(),
                    text: "working".to_string(),
                    sequence: Some(1),
                    projection_key: None,
                    occurred_at: Some("2026-05-30T00:00:01.000Z".to_string()),
                },
                TranscriptItem {
                    kind: TranscriptItemKind::TurnTerminalStatus,
                    actor: TranscriptActor::AgentTui,
                    turn_id: String::new(),
                    text: "queue empty".to_string(),
                    sequence: None,
                    projection_key: None,
                    occurred_at: Some("2026-05-30T00:00:02.000Z".to_string()),
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
        let mut buffer = Buffer::empty(TuiRect::new(0, 0, 100, 18));

        render_app_to_buffer(&model, &mut buffer);

        let (operator_x, operator_y) =
            find_text_position(&buffer, "operator").expect("operator label is rendered");
        let (agent_x, agent_y) =
            find_text_position(&buffer, "sonar.resident:").expect("agent label is rendered");
        let (agent_tui_x, agent_tui_y) =
            find_text_position(&buffer, "agent-tui:").expect("agent-tui label is rendered");

        assert_eq!(buffer[(operator_x, operator_y)].fg, Color::Green);
        assert!(
            buffer[(operator_x, operator_y)]
                .modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(buffer[(agent_x, agent_y)].fg, Color::Cyan);
        assert!(buffer[(agent_x, agent_y)].modifier.contains(Modifier::BOLD));
        assert_eq!(buffer[(agent_tui_x, agent_tui_y)].fg, Color::Magenta);
        assert!(
            buffer[(agent_tui_x, agent_tui_y)]
                .modifier
                .contains(Modifier::BOLD)
        );
        assert_ne!(
            buffer[(agent_tui_x, agent_tui_y)].fg,
            buffer[(agent_x, agent_y)].fg
        );
        assert_ne!(
            buffer[(agent_tui_x, agent_tui_y)].fg,
            buffer[(operator_x, operator_y)].fg
        );
    }

    #[test]
    fn carrier_terminal_status_humanizes_protocol_token_for_display() {
        let spans = carrier_status_body_spans(
            &TranscriptRow {
                key: "row_1".to_string(),
                actor: TranscriptActor::AgentTui,
                actor_label: "agent-tui".to_string(),
                kind: TranscriptItemKind::TurnTerminalStatus,
                turn_id: "turn_1".to_string(),
                text: "completed_without_provider".to_string(),
                occurred_at: None,
            },
            "completed_without_provider",
        )
        .expect("carrier status spans exist");

        assert_eq!(spans[0].content.as_ref(), "completed without provider");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
    }

    #[test]
    fn diagnostic_body_omits_redundant_diagnostic_prefix() {
        let spans = diagnostic_body_spans(
            &TranscriptRow {
                key: "row_1".to_string(),
                actor: TranscriptActor::AgentTui,
                actor_label: "agent-tui".to_string(),
                kind: TranscriptItemKind::TurnTerminalStatus,
                turn_id: "turn_1".to_string(),
                text: "diagnostic warn provider stderr · mediated".to_string(),
                occurred_at: None,
            },
            "diagnostic warn provider stderr · mediated",
        )
        .expect("diagnostic spans exist");

        let rendered = spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert_eq!(rendered, "warn provider stderr · mediated");
        assert_eq!(spans[0].style.fg, Some(Color::Magenta));
    }

    #[test]
    fn queue_summary_label_uses_carrier_color() {
        let spans = carrier_queue_body_spans(
            &TranscriptRow {
                key: "row_1".to_string(),
                actor: TranscriptActor::AgentTui,
                actor_label: "agent-tui".to_string(),
                kind: TranscriptItemKind::TurnTerminalStatus,
                turn_id: String::new(),
                text: "queue: 1 item".to_string(),
                occurred_at: None,
            },
            "queue: 1 item",
        )
        .expect("queue spans exist");

        assert_eq!(spans[0].content.as_ref(), "queue");
        assert_eq!(spans[0].style.fg, Some(Color::Magenta));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn queue_detail_modes_split_state_and_mode_colors() {
        let spans = carrier_queue_body_spans(
            &TranscriptRow {
                key: "row_1".to_string(),
                actor: TranscriptActor::AgentTui,
                actor_label: "agent-tui".to_string(),
                kind: TranscriptItemKind::TurnTerminalStatus,
                turn_id: String::new(),
                text: "1. system · held directive · 1m 14s".to_string(),
                occurred_at: None,
            },
            "1. system · held directive · 1m 14s",
        )
        .expect("queue detail spans exist");
        let rendered = spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert_eq!(rendered, "1. system · held directive · 1m 14s");
        assert_eq!(spans[1].content.as_ref(), "system");
        assert_eq!(spans[1].style.fg, Some(Color::LightMagenta));
        assert!(spans[1].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[3].content.as_ref(), "held");
        assert_eq!(spans[3].style.fg, Some(Color::Green));
        assert_eq!(spans[4].content.as_ref(), " directive");
        assert_eq!(spans[4].style.fg, Some(Color::Magenta));
        assert_eq!(spans[6].content.as_ref(), "1m 14s");
        assert_eq!(spans[6].style.fg, Some(Color::Gray));
    }

    #[test]
    fn tool_rows_keep_directional_labels_inline_and_colored() {
        let model = build_app_view(&AppViewInput {
            terminal_size: TerminalSize {
                width: 110,
                height: 16,
            },
            layout_config: LayoutConfig::default(),
            transcript_items: vec![
                TranscriptItem {
                    kind: TranscriptItemKind::ProviderToolCallRequest,
                    actor: TranscriptActor::AgentTui,
                    turn_id: "turn_1".to_string(),
                    text: "site_loop_run_once({})".to_string(),
                    sequence: Some(1),
                    projection_key: None,
                    occurred_at: Some("2026-05-30T18:29:00.000Z".to_string()),
                },
                TranscriptItem {
                    kind: TranscriptItemKind::ToolResultReceived,
                    actor: TranscriptActor::AgentTui,
                    turn_id: "turn_1".to_string(),
                    text: "ok site_loop_run_once in 2s".to_string(),
                    sequence: Some(2),
                    projection_key: None,
                    occurred_at: Some("2026-05-30T18:29:02.000Z".to_string()),
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
        let mut buffer = Buffer::empty(TuiRect::new(0, 0, 110, 16));

        render_app_to_buffer(&model, &mut buffer);
        let text = buffer_text(&buffer);

        assert!(text.contains("sonar.resident -> agent-tui: site_loop_run_once({})"));
        assert!(text.contains("agent-tui -> sonar.resident: ok site_loop_run_once in 2s"));
        let (request_agent_x, request_y) =
            find_text_position(&buffer, "sonar.resident -> agent-tui:")
                .expect("tool request line is rendered");
        let request_agent_tui_x = request_agent_x + "sonar.resident -> ".chars().count() as u16;
        let (result_agent_tui_x, result_y) =
            find_text_position(&buffer, "agent-tui -> sonar.resident:")
                .expect("tool result line is rendered");
        let result_agent_x = result_agent_tui_x + "agent-tui -> ".chars().count() as u16;

        let (request_call_x, request_call_y) =
            find_text_position(&buffer, "site_loop_run_once({})")
                .expect("tool request payload is rendered");
        let result_tool_x =
            result_agent_tui_x + "agent-tui -> sonar.resident: ok ".chars().count() as u16;
        let result_spans =
            tool_body_spans(&model.transcript_rows[1], "ok site_loop_run_once in 2s");

        let duration_span = result_spans
            .iter()
            .find(|span| span.content.as_ref() == "2s")
            .expect("tool result duration is rendered");

        assert_eq!(result_spans[0].content.as_ref(), "ok ");
        assert_eq!(result_spans[0].style.fg, Some(Color::Green));
        assert_eq!(duration_span.style.fg, Some(Color::Gray));
        assert_eq!(buffer[(request_agent_x, request_y)].fg, Color::Cyan);
        assert_eq!(buffer[(request_agent_tui_x, request_y)].fg, Color::Magenta);
        assert_eq!(buffer[(result_agent_tui_x, result_y)].fg, Color::Magenta);
        assert_eq!(buffer[(result_agent_x, result_y)].fg, Color::Cyan);
        assert_eq!(buffer[(request_call_x, request_call_y)].fg, Color::Gray);
        assert_eq!(buffer[(result_tool_x, result_y)].fg, Color::Gray);
    }

    #[test]
    fn wrapped_tool_result_continuations_keep_payload_styling() {
        let model = build_app_view(&AppViewInput {
            terminal_size: TerminalSize {
                width: 72,
                height: 16,
            },
            layout_config: LayoutConfig::default(),
            transcript_items: vec![TranscriptItem {
                kind: TranscriptItemKind::ToolResultReceived,
                actor: TranscriptActor::AgentTui,
                turn_id: "turn_1".to_string(),
                text: "ok site_loop_run_once in 2s with hydrate_current checkpoint_summary directive_context".to_string(),
                sequence: Some(1),
                projection_key: None,
                occurred_at: Some("2026-05-30T18:29:02.000Z".to_string()),
            }],
            status: StatusViewInput {
                identity: "sonar.resident".to_string(),
                session: "carrier_1".to_string(),
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
        let mut buffer = Buffer::empty(TuiRect::new(0, 0, 72, 16));

        render_app_to_buffer(&model, &mut buffer);

        let (continuation_x, continuation_y) = find_text_position(&buffer, "checkpoint_summary")
            .expect("wrapped tool continuation payload is rendered");
        assert_eq!(buffer[(continuation_x, continuation_y)].fg, Color::Gray);
    }

    #[test]
    fn composer_titles_keep_idle_and_active_shapes_and_colors() {
        let mut idle_model = model();
        idle_model.composer.prompt_label = "operator -> sonar.resident>".to_string();
        let mut idle_buffer = Buffer::empty(TuiRect::new(0, 0, 80, 12));

        render_app_to_buffer(&idle_model, &mut idle_buffer);

        let (idle_operator_x, idle_y) =
            find_text_position(&idle_buffer, "operator -> sonar.resident>")
                .expect("idle composer title is rendered");
        let idle_agent_x = idle_operator_x + "operator -> ".chars().count() as u16;
        assert_eq!(idle_buffer[(idle_operator_x, idle_y)].fg, Color::Green);
        assert!(
            idle_buffer[(idle_operator_x, idle_y)]
                .modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(idle_buffer[(idle_agent_x, idle_y)].fg, Color::Cyan);
        assert!(
            idle_buffer[(idle_agent_x, idle_y)]
                .modifier
                .contains(Modifier::BOLD)
        );
        assert!(find_text_position(&idle_buffer, "queued notes:").is_none());
        assert!(find_text_position(&idle_buffer, "held system directives:").is_none());

        let active_model = build_app_view(&AppViewInput {
            terminal_size: TerminalSize {
                width: 180,
                height: 12,
            },
            layout_config: LayoutConfig::default(),
            transcript_items: vec![],
            status: StatusViewInput {
                identity: "sonar.resident".to_string(),
                session: "carrier_1".to_string(),
                turn_state: TurnState::Active,
                active_phase: None,
                active_turn_age: Some("12s".to_string()),
                queued_inputs: 0,
                held_system_directives: 0,
                oldest_held_age: None,
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
        let mut active_buffer = Buffer::empty(TuiRect::new(0, 0, 180, 12));

        render_app_to_buffer(&active_model, &mut active_buffer);

        let (active_operator_x, active_y) =
            find_text_position(&active_buffer, "operator note -> sonar.resident>")
                .expect("active composer title is rendered");
        let active_agent_x = active_operator_x + "operator note -> ".chars().count() as u16;
        assert_eq!(
            active_buffer[(active_operator_x, active_y)].fg,
            Color::Green
        );
        assert!(
            active_buffer[(active_operator_x, active_y)]
                .modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(active_buffer[(active_agent_x, active_y)].fg, Color::Cyan);
        assert!(
            active_buffer[(active_agent_x, active_y)]
                .modifier
                .contains(Modifier::BOLD)
        );
        let (queued_x, queued_y) = find_text_position(&active_buffer, "queued operator notes: 2")
            .expect("queued note affordance is rendered in composer title");
        let queued_operator_x = queued_x + "queued ".chars().count() as u16;
        let queued_mode_x = queued_x + "queued operator ".chars().count() as u16;
        let queued_count_x = queued_x + "queued operator notes".chars().count() as u16;
        let (held_x, held_y) = find_text_position(&active_buffer, "held system directives: 1")
            .expect("held directive affordance is rendered in composer title");
        let held_system_x = held_x + "held ".chars().count() as u16;
        let held_directive_x = held_x + "held system ".chars().count() as u16;
        let held_count_x = held_x + "held system directives".chars().count() as u16;
        assert_eq!(active_buffer[(queued_x, queued_y)].fg, Color::Green);
        assert_eq!(
            active_buffer[(queued_operator_x, queued_y)].fg,
            Color::Green
        );
        assert!(
            active_buffer[(queued_operator_x, queued_y)]
                .modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(active_buffer[(queued_mode_x, queued_y)].fg, Color::Magenta);
        assert_eq!(active_buffer[(queued_count_x, queued_y)].fg, Color::Magenta);
        assert_eq!(active_buffer[(held_x, held_y)].fg, Color::Green);
        assert_eq!(
            active_buffer[(held_system_x, held_y)].fg,
            Color::LightMagenta
        );
        assert!(
            active_buffer[(held_system_x, held_y)]
                .modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(active_buffer[(held_directive_x, held_y)].fg, Color::Magenta);
        assert_eq!(active_buffer[(held_count_x, held_y)].fg, Color::Magenta);

        let title = composer_title(
            &active_model,
            active_model.layout.composer.width.saturating_sub(2) as usize,
        );
        let count_spans: Vec<_> = title
            .spans
            .iter()
            .filter(|span| span.content.as_ref() == ": 2" || span.content.as_ref() == ": 1")
            .collect();
        assert_eq!(count_spans.len(), 2);
        assert!(
            count_spans
                .iter()
                .all(|span| span.style.fg == Some(Color::Magenta))
        );
        assert!(
            count_spans
                .iter()
                .all(|span| span.style.add_modifier.contains(Modifier::BOLD))
        );
    }

    #[test]
    fn composer_title_truncates_long_prompt_without_losing_participant_colors() {
        let active_model = build_app_view(&AppViewInput {
            terminal_size: TerminalSize {
                width: 44,
                height: 12,
            },
            layout_config: LayoutConfig::default(),
            transcript_items: vec![],
            status: StatusViewInput {
                identity: "narada-timour-marketing-agent.builder2".to_string(),
                session: "carrier_1".to_string(),
                turn_state: TurnState::Active,
                active_phase: None,
                active_turn_age: Some("12s".to_string()),
                queued_inputs: 2,
                held_system_directives: 1,
                oldest_held_age: None,
                transcript_items: 0,
                runtime_posture: RuntimePostureState::disabled(),
                last_error: None,
            },
            composer: ComposerViewInput {
                identity: "narada-timour-marketing-agent.builder2".to_string(),
                draft_text: "steering note".to_string(),
                turn_state: TurnState::Active,
                queued_operator_notes: 2,
                held_system_directives: 1,
            },
        });
        let title = composer_title(&active_model, 42);
        let title_text: String = title
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();

        assert_eq!(title_text.chars().count(), 42);
        assert!(title_text.starts_with("operator note -> narada-timour-"));
        assert!(title_text.ends_with("..."));
        assert!(!title_text.contains("queued notes"));
        assert_eq!(title.spans[0].style.fg, Some(Color::Green));
        assert_eq!(title.spans[1].style.fg, Some(Color::Magenta));
        assert!(title.spans.iter().any(|span| {
            span.content.as_ref().contains("narada-timour") && span.style.fg == Some(Color::Cyan)
        }));
        assert_eq!(title.spans.last().unwrap().style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn composer_title_omits_whole_affordances_when_width_is_tight() {
        let active_model = build_app_view(&AppViewInput {
            terminal_size: TerminalSize {
                width: 56,
                height: 12,
            },
            layout_config: LayoutConfig::default(),
            transcript_items: vec![],
            status: StatusViewInput {
                identity: "sonar.resident".to_string(),
                session: "carrier_1".to_string(),
                turn_state: TurnState::Active,
                active_phase: None,
                active_turn_age: Some("12s".to_string()),
                queued_inputs: 2,
                held_system_directives: 1,
                oldest_held_age: None,
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
        let title = composer_title(&active_model, 66);
        let title_text: String = title
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();

        assert!(title_text.contains("operator note -> sonar.resident>"));
        assert!(title_text.contains("queued operator notes: 2"));
        assert!(title_text.contains(" | ..."));
        assert!(!title_text.contains("held system directives"));
        assert!(title.spans.iter().any(|span| {
            span.content.as_ref() == "..." && span.style.fg == Some(Color::DarkGray)
        }));
    }

    #[test]
    fn status_spans_render_muted_fill_when_too_narrow_for_any_segment() {
        let spans = status_spans(
            &[StatusSegment {
                key: "turn_state".to_string(),
                label: "turn".to_string(),
                value: "thinking 12s".to_string(),
            }],
            2,
        );

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.as_ref(), "..");
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn status_line_keeps_operational_colors_and_separators() {
        let model = build_app_view(&AppViewInput {
            terminal_size: TerminalSize {
                width: 120,
                height: 12,
            },
            layout_config: LayoutConfig::default(),
            transcript_items: vec![],
            status: StatusViewInput {
                identity: "sonar.resident".to_string(),
                session: "carrier_1".to_string(),
                turn_state: TurnState::Active,
                active_phase: Some("calling site_loop_run_once 8s".to_string()),
                active_turn_age: Some("12s".to_string()),
                queued_inputs: 2,
                held_system_directives: 1,
                oldest_held_age: Some("22s".to_string()),
                transcript_items: 0,
                runtime_posture: RuntimePostureState {
                    provider_state: ProviderRuntimeState::Working,
                    ..RuntimePostureState::disabled()
                },
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
        let mut buffer = Buffer::empty(TuiRect::new(0, 0, 120, 12));

        render_app_to_buffer(&model, &mut buffer);
        let text = buffer_text(&buffer);

        assert!(text.contains("calling site_loop_run_once 8s"));
        assert!(text.contains("queued operator steering 2"));
        assert!(text.contains("held system directives 1"));
        assert!(text.contains("oldest 22s"));
        assert!(text.contains("Esc interrupt"));
        let (phase_x, phase_y) = find_text_position(&buffer, "calling site_loop_run_once 8s")
            .expect("phase appears in status");
        let phase_tool_x = phase_x + "calling ".chars().count() as u16;
        let phase_duration_x = phase_x + "calling site_loop_run_once ".chars().count() as u16;
        let (queued_label_x, queued_y) = find_text_position(&buffer, "queued operator steering")
            .expect("queued operator steering label appears in status");
        let queued_operator_x = queued_label_x + "queued ".chars().count() as u16;
        let queued_mode_x = queued_label_x + "queued operator ".chars().count() as u16;
        let queued_value_x = queued_label_x + "queued operator steering ".chars().count() as u16;
        let (held_x, held_y) = find_text_position(&buffer, "held system directives 1")
            .expect("held system directive segment appears in status");
        let held_system_x = held_x + "held ".chars().count() as u16;
        let held_directive_x = held_x + "held system ".chars().count() as u16;
        let held_count_x = held_x + "held system directives ".chars().count() as u16;
        let (separator_x, separator_y) = find_text_position(&buffer, " | queued operator steering")
            .expect("separator appears before queued segment");
        let (oldest_label_x, oldest_y) =
            find_text_position(&buffer, "oldest 22s").expect("oldest held age appears in status");
        let oldest_value_x = oldest_label_x + "oldest ".chars().count() as u16;
        let (esc_label_x, esc_y) =
            find_text_position(&buffer, "Esc interrupt").expect("Esc affordance appears in status");
        let esc_value_x = esc_label_x + "Esc ".chars().count() as u16;

        assert_eq!(buffer[(phase_x, phase_y)].fg, Color::Green);
        assert_eq!(buffer[(phase_tool_x, phase_y)].fg, Color::Gray);
        assert_eq!(buffer[(phase_duration_x, phase_y)].fg, Color::Gray);
        assert_eq!(buffer[(queued_label_x, queued_y)].fg, Color::Green);
        assert_eq!(buffer[(queued_operator_x, queued_y)].fg, Color::Green);
        assert!(
            buffer[(queued_operator_x, queued_y)]
                .modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(buffer[(queued_mode_x, queued_y)].fg, Color::Magenta);
        assert_eq!(buffer[(queued_value_x, queued_y)].fg, Color::Magenta);
        assert!(
            buffer[(queued_value_x, queued_y)]
                .modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(buffer[(held_system_x, held_y)].fg, Color::LightMagenta);
        assert!(
            buffer[(held_system_x, held_y)]
                .modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(buffer[(held_directive_x, held_y)].fg, Color::Magenta);
        assert_eq!(buffer[(held_count_x, held_y)].fg, Color::Magenta);
        assert_eq!(buffer[(oldest_label_x, oldest_y)].fg, Color::Yellow);
        assert_eq!(buffer[(oldest_value_x, oldest_y)].fg, Color::Gray);
        assert_eq!(buffer[(separator_x, separator_y)].fg, Color::DarkGray);
        assert_eq!(buffer[(esc_label_x, esc_y)].fg, Color::Yellow);
        assert_eq!(buffer[(esc_value_x, esc_y)].fg, Color::Magenta);
    }

    #[test]
    fn active_operator_directive_status_splits_participant_mode_and_count() {
        let spans = status_segment_spans(&StatusSegment {
            key: "turn_state".to_string(),
            label: "turn".to_string(),
            value: "typing operator directive (86)".to_string(),
        });

        assert_eq!(spans[0].content.as_ref(), "typing");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
        assert_eq!(spans[2].content.as_ref(), "operator");
        assert_eq!(spans[2].style.fg, Some(Color::Green));
        assert!(spans[2].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[4].content.as_ref(), "directive");
        assert_eq!(spans[4].style.fg, Some(Color::Yellow));
        assert!(spans[4].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[6].content.as_ref(), "(86)");
        assert_eq!(spans[6].style.fg, Some(Color::Magenta));
        assert!(spans[6].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn active_operator_note_queue_status_splits_participant_mode_and_count() {
        let spans = status_segment_spans(&StatusSegment {
            key: "turn_state".to_string(),
            label: "turn".to_string(),
            value: "queued operator notes 2".to_string(),
        });

        assert_eq!(spans[0].content.as_ref(), "queued");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
        assert_eq!(spans[2].content.as_ref(), "operator");
        assert_eq!(spans[2].style.fg, Some(Color::Green));
        assert_eq!(spans[4].content.as_ref(), "notes");
        assert_eq!(spans[4].style.fg, Some(Color::Magenta));
        assert_eq!(spans[6].content.as_ref(), "2");
        assert_eq!(spans[6].style.fg, Some(Color::Magenta));
    }

    #[test]
    fn held_system_directive_status_splits_participant_mode_and_count() {
        let spans = status_segment_spans(&StatusSegment {
            key: "held_system_directives".to_string(),
            label: "held system directives".to_string(),
            value: "2".to_string(),
        });

        assert_eq!(spans[0].content.as_ref(), "held");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
        assert_eq!(spans[2].content.as_ref(), "system");
        assert_eq!(spans[2].style.fg, Some(Color::LightMagenta));
        assert!(spans[2].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[4].content.as_ref(), "directives");
        assert_eq!(spans[4].style.fg, Some(Color::Magenta));
        assert_eq!(spans[6].content.as_ref(), "2");
        assert_eq!(spans[6].style.fg, Some(Color::Magenta));
    }

    #[test]
    fn oldest_held_age_status_renders_duration_as_scan_data() {
        let spans = status_segment_spans(&StatusSegment {
            key: "oldest_held_age".to_string(),
            label: "oldest".to_string(),
            value: "1m 14s".to_string(),
        });

        assert_eq!(spans[0].content.as_ref(), "oldest");
        assert_eq!(spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(spans[2].content.as_ref(), "1m 14s");
        assert_eq!(spans[2].style.fg, Some(Color::Gray));
    }

    #[test]
    fn truncated_status_values_preserve_value_span_styles() {
        let spans = truncated_status_segment_spans(
            &StatusSegment {
                key: "turn_state".to_string(),
                label: "turn".to_string(),
                value: "calling site_loop_run_once 8s".to_string(),
            },
            20,
        );

        assert_eq!(spans[0].content.as_ref(), "calling");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
        assert_eq!(spans[1].content.as_ref(), " ");
        assert_eq!(spans[1].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[2].content.as_ref(), "site_loop");
        assert_eq!(spans[2].style.fg, Some(Color::Gray));
        assert_eq!(spans[3].content.as_ref(), "...");
        assert_eq!(spans[3].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn status_segments_style_session_and_transcript_counters_as_neutral_scan_data() {
        let session = status_segment_spans(&StatusSegment {
            key: "session".to_string(),
            label: "session".to_string(),
            value: "carrier_manual_20260601_000134_sonar_resident".to_string(),
        });
        let transcript_zero = status_segment_spans(&StatusSegment {
            key: "transcript_items".to_string(),
            label: "transcript".to_string(),
            value: "0".to_string(),
        });
        let transcript_nonzero = status_segment_spans(&StatusSegment {
            key: "transcript_items".to_string(),
            label: "transcript".to_string(),
            value: "12".to_string(),
        });

        assert_eq!(session[0].content.as_ref(), "session");
        assert_eq!(session[0].style.fg, Some(Color::Yellow));
        assert_eq!(session[2].style.fg, Some(Color::Gray));
        assert_eq!(transcript_zero[2].style.fg, Some(Color::DarkGray));
        assert_eq!(transcript_nonzero[2].style.fg, Some(Color::Gray));
    }

    #[test]
    fn structured_body_spans_style_list_marker_as_muted_structure() {
        let spans = structured_body_spans("- bullet item", ui_theme::body());

        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content.as_ref(), "- ");
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[1].content.as_ref(), "bullet item");
        assert_eq!(spans[1].style.fg, Some(Color::White));
    }

    #[test]
    fn structured_body_spans_style_diff_lines_without_breaking_lists() {
        let added = structured_body_spans("+added line", ui_theme::body());
        let removed = structured_body_spans("-removed line", ui_theme::body());
        let list = structured_body_spans("- bullet item", ui_theme::body());

        assert_eq!(added[0].content.as_ref(), "+");
        assert_eq!(added[0].style.fg, Some(Color::Green));
        assert_eq!(added[1].content.as_ref(), "added line");
        assert_eq!(removed[0].content.as_ref(), "-");
        assert_eq!(removed[0].style.fg, Some(Color::Red));
        assert_eq!(removed[1].content.as_ref(), "removed line");
        assert_eq!(list[0].content.as_ref(), "- ");
        assert_eq!(list[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn structured_body_spans_style_task_checkbox_markers_as_state() {
        let unchecked = structured_body_spans("- [ ] close gap", ui_theme::body());
        let checked = structured_body_spans("- [x] verified", ui_theme::body());

        assert_eq!(unchecked.len(), 3);
        assert_eq!(unchecked[0].content.as_ref(), "- ");
        assert_eq!(unchecked[0].style.fg, Some(Color::DarkGray));
        assert_eq!(unchecked[1].content.as_ref(), "[ ] ");
        assert_eq!(unchecked[1].style.fg, Some(Color::DarkGray));
        assert_eq!(unchecked[2].content.as_ref(), "close gap");
        assert_eq!(unchecked[2].style.fg, Some(Color::White));
        assert_eq!(checked.len(), 3);
        assert_eq!(checked[1].content.as_ref(), "[x] ");
        assert_eq!(checked[1].style.fg, Some(Color::Green));
        assert_eq!(checked[2].content.as_ref(), "verified");
    }

    #[test]
    fn structured_body_spans_style_blockquote_marker_as_muted_structure() {
        let spans = structured_body_spans("> quoted `context`", ui_theme::body());

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "> ");
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[1].content.as_ref(), "quoted ");
        assert_eq!(spans[1].style.fg, Some(Color::White));
        assert_eq!(spans[2].content.as_ref(), "context");
        assert_eq!(spans[2].style.fg, Some(Color::Gray));
    }

    #[test]
    fn structured_body_spans_keep_marked_lines_from_becoming_headings() {
        let list_spans = structured_body_spans("- Current context:", ui_theme::body());
        let quote_spans = structured_body_spans("> Caveat:", ui_theme::body());

        assert_eq!(list_spans[0].content.as_ref(), "- ");
        assert_eq!(list_spans[0].style.fg, Some(Color::DarkGray));
        assert_eq!(list_spans[1].content.as_ref(), "Current context:");
        assert_eq!(list_spans[1].style.fg, Some(Color::White));
        assert_eq!(quote_spans[0].content.as_ref(), "> ");
        assert_eq!(quote_spans[0].style.fg, Some(Color::DarkGray));
        assert_eq!(quote_spans[1].content.as_ref(), "Caveat:");
        assert_eq!(quote_spans[1].style.fg, Some(Color::White));
    }

    #[test]
    fn structured_body_spans_style_markdown_heading_marker_as_structure() {
        let spans = structured_body_spans("## Current context", ui_theme::body());

        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content.as_ref(), "## ");
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[1].content.as_ref(), "Current context");
        assert_eq!(spans[1].style.fg, Some(Color::Cyan));
        assert!(spans[1].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn structured_body_spans_style_markdown_rule_as_muted_separator() {
        let spans = structured_body_spans("---", ui_theme::body());

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.as_ref(), "---");
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn structured_body_spans_style_markdown_table_pipes_as_structure() {
        let spans = structured_body_spans("| Site | `narada-proper` |", ui_theme::body());

        assert_eq!(spans[0].content.as_ref(), "|");
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[2].content.as_ref(), "|");
        assert_eq!(spans[2].style.fg, Some(Color::DarkGray));
        assert!(
            spans
                .iter()
                .any(|span| span.content.as_ref() == "narada-proper"
                    && span.style.fg == Some(Color::Gray))
        );
    }

    #[test]
    fn structured_body_spans_style_markdown_table_separator_row_as_muted() {
        let spans = structured_body_spans("| --- | :--- |", ui_theme::body());

        assert!(
            spans
                .iter()
                .all(|span| span.style.fg == Some(Color::DarkGray))
        );
        assert!(spans.iter().any(|span| span.content.as_ref() == " --- "));
        assert!(spans.iter().any(|span| span.content.as_ref() == " :--- "));
    }

    #[test]
    fn structured_body_spans_style_top_level_key_value_lines() {
        let spans = structured_body_spans("Site: narada-proper", ui_theme::body());

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "Site");
        assert_eq!(spans[0].style.fg, Some(Color::Yellow));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[1].content.as_ref(), ": ");
        assert_eq!(spans[1].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[2].content.as_ref(), "narada-proper");
        assert_eq!(spans[2].style.fg, Some(Color::Gray));
    }

    #[test]
    fn structured_body_spans_style_key_value_windows_path_as_code() {
        let spans = structured_body_spans("Root: D:\\code\\narada", ui_theme::body());

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "Root");
        assert_eq!(spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(spans[2].content.as_ref(), "D:\\code\\narada");
        assert_eq!(spans[2].style.fg, Some(Color::Gray));
    }

    #[test]
    fn structured_body_spans_style_key_value_identifier_lists_as_structure() {
        let spans = structured_body_spans(
            "keys: status, schema, hydrate_current, ...",
            ui_theme::body(),
        );

        assert_eq!(spans[0].content.as_ref(), "keys");
        assert_eq!(spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(spans[2].content.as_ref(), "status");
        assert_eq!(spans[2].style.fg, Some(Color::Gray));
        assert_eq!(spans[3].content.as_ref(), ",");
        assert_eq!(spans[3].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[4].content.as_ref(), " ");
        assert_eq!(spans[4].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[6].content.as_ref(), ",");
        assert_eq!(spans[8].content.as_ref(), "hydrate_current");
        assert_eq!(spans[8].style.fg, Some(Color::Gray));
        assert_eq!(spans[11].content.as_ref(), "...");
        assert_eq!(spans[11].style.fg, Some(Color::Gray));
    }

    #[test]
    fn structured_body_spans_do_not_style_prose_comma_lists_as_identifier_lists() {
        let spans = structured_body_spans("Note: alpha, beta words", ui_theme::body());

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[2].content.as_ref(), "alpha, beta words");
        assert_eq!(spans[2].style.fg, Some(Color::White));
    }

    #[test]
    fn structured_body_spans_style_key_value_status_values_semantically() {
        let ok = structured_body_spans("Status: ok", ui_theme::body());
        let failed = structured_body_spans("Status: failed", ui_theme::body());
        let pending = structured_body_spans("Status: pending", ui_theme::body());
        let mutation_performed =
            structured_body_spans("Mutation performed: true", ui_theme::body());
        let dry_run = structured_body_spans("Dry run: false", ui_theme::body());
        let error = structured_body_spans("Error: null", ui_theme::body());

        assert_eq!(ok[2].content.as_ref(), "ok");
        assert_eq!(ok[2].style.fg, Some(Color::Green));
        assert_eq!(failed[2].content.as_ref(), "failed");
        assert_eq!(failed[2].style.fg, Some(Color::Red));
        assert!(failed[2].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(pending[2].content.as_ref(), "pending");
        assert_eq!(pending[2].style.fg, Some(Color::Magenta));
        assert!(pending[2].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(mutation_performed[2].content.as_ref(), "true");
        assert_eq!(mutation_performed[2].style.fg, Some(Color::Green));
        assert_eq!(dry_run[2].content.as_ref(), "false");
        assert_eq!(dry_run[2].style.fg, Some(Color::DarkGray));
        assert_eq!(error[2].content.as_ref(), "null");
        assert_eq!(error[2].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn structured_body_spans_style_powershell_prompt_as_shell_structure() {
        let spans = structured_body_spans("PS D:\\code\\narada> pnpm test", ui_theme::body());

        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content.as_ref(), "PS D:\\code\\narada> ");
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[1].content.as_ref(), "pnpm test");
        assert_eq!(spans[1].style.fg, Some(Color::Gray));
    }

    #[test]
    fn structured_body_spans_do_not_treat_indented_paths_as_key_values() {
        let spans = structured_body_spans("  Root: D:\\code\\narada", ui_theme::body());

        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content.as_ref(), "  Root: ");
        assert_eq!(spans[0].style.fg, Some(Color::White));
        assert_eq!(spans[1].content.as_ref(), "D:\\code\\narada");
        assert_eq!(spans[1].style.fg, Some(Color::Gray));
    }

    #[test]
    fn inline_code_spans_style_code_content_without_rendering_backticks() {
        let spans = inline_code_spans(
            "Use `config.json` at `D:\\code\\narada` now",
            ui_theme::body(),
        );

        assert_eq!(spans.len(), 5);
        assert_eq!(spans[0].content.as_ref(), "Use ");
        assert_eq!(spans[0].style.fg, Some(Color::White));
        assert_eq!(spans[1].content.as_ref(), "config.json");
        assert_eq!(spans[1].style.fg, Some(Color::Gray));
        assert_eq!(spans[2].content.as_ref(), " at ");
        assert_eq!(spans[2].style.fg, Some(Color::White));
        assert_eq!(spans[3].content.as_ref(), "D:\\code\\narada");
        assert_eq!(spans[3].style.fg, Some(Color::Gray));
        assert_eq!(spans[4].content.as_ref(), " now");
        assert_eq!(spans[4].style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_style_plain_windows_paths_as_code() {
        let spans = inline_code_spans("Current Site at D:\\code\\narada now", ui_theme::body());

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "Current Site at ");
        assert_eq!(spans[0].style.fg, Some(Color::White));
        assert_eq!(spans[1].content.as_ref(), "D:\\code\\narada");
        assert_eq!(spans[1].style.fg, Some(Color::Gray));
        assert_eq!(spans[2].content.as_ref(), " now");
        assert_eq!(spans[2].style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_keep_trailing_punctuation_outside_windows_path() {
        let spans = inline_code_spans("Open D:\\code\\narada.", ui_theme::body());

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1].content.as_ref(), "D:\\code\\narada");
        assert_eq!(spans[1].style.fg, Some(Color::Gray));
        assert_eq!(spans[2].content.as_ref(), ".");
        assert_eq!(spans[2].style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_style_urls_as_code() {
        let spans = inline_code_spans("See https://example.com/docs?q=narada.", ui_theme::body());

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "See ");
        assert_eq!(spans[0].style.fg, Some(Color::White));
        assert_eq!(
            spans[1].content.as_ref(),
            "https://example.com/docs?q=narada"
        );
        assert_eq!(spans[1].style.fg, Some(Color::Gray));
        assert_eq!(spans[2].content.as_ref(), ".");
        assert_eq!(spans[2].style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_style_markdown_link_structure() {
        let spans = inline_code_spans(
            "Read [docs](https://example.com/docs) now",
            ui_theme::body(),
        );

        assert_eq!(spans.len(), 7);
        assert_eq!(spans[0].content.as_ref(), "Read ");
        assert_eq!(spans[0].style.fg, Some(Color::White));
        assert_eq!(spans[1].content.as_ref(), "[");
        assert_eq!(spans[1].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[2].content.as_ref(), "docs");
        assert_eq!(spans[2].style.fg, Some(Color::White));
        assert_eq!(spans[3].content.as_ref(), "](");
        assert_eq!(spans[3].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[4].content.as_ref(), "https://example.com/docs");
        assert_eq!(spans[4].style.fg, Some(Color::Gray));
        assert_eq!(spans[5].content.as_ref(), ")");
        assert_eq!(spans[5].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[6].content.as_ref(), " now");
        assert_eq!(spans[6].style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_style_markdown_image_link_structure() {
        let spans = inline_code_spans(
            "See ![chart](https://example.com/chart.png)",
            ui_theme::body(),
        );

        assert_eq!(spans.len(), 7);
        assert_eq!(spans[0].content.as_ref(), "See ");
        assert_eq!(spans[0].style.fg, Some(Color::White));
        assert_eq!(spans[1].content.as_ref(), "!");
        assert_eq!(spans[1].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[2].content.as_ref(), "[");
        assert_eq!(spans[2].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[3].content.as_ref(), "chart");
        assert_eq!(spans[3].style.fg, Some(Color::White));
        assert_eq!(spans[5].content.as_ref(), "https://example.com/chart.png");
        assert_eq!(spans[5].style.fg, Some(Color::Gray));
    }

    #[test]
    fn inline_code_spans_style_email_addresses_as_code() {
        let spans = inline_code_spans("Reply to ops.team+sonar@example.com.", ui_theme::body());

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "Reply to ");
        assert_eq!(spans[0].style.fg, Some(Color::White));
        assert_eq!(spans[1].content.as_ref(), "ops.team+sonar@example.com");
        assert_eq!(spans[1].style.fg, Some(Color::Gray));
        assert_eq!(spans[2].content.as_ref(), ".");
        assert_eq!(spans[2].style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_style_timestamp_tokens_as_code() {
        let spans = inline_code_spans(
            "lastSyncAt 2026-05-30T18:39:10.000Z and 2026-05-30Z18:39.",
            ui_theme::body(),
        );

        let iso = spans
            .iter()
            .find(|span| span.content.as_ref() == "2026-05-30T18:39:10.000Z")
            .expect("ISO timestamp is rendered");
        let narada = spans
            .iter()
            .find(|span| span.content.as_ref() == "2026-05-30Z18:39")
            .expect("Narada timestamp is rendered");
        let trailing_period = spans.last().expect("trailing punctuation is rendered");

        assert_eq!(iso.style.fg, Some(Color::Gray));
        assert_eq!(narada.style.fg, Some(Color::Gray));
        assert_eq!(trailing_period.content.as_ref(), ".");
        assert_eq!(trailing_period.style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_style_duration_tokens_as_code_without_styling_bare_numbers() {
        let spans = inline_code_spans(
            "Completed in 0.0s after 512ms with 3 retries.",
            ui_theme::body(),
        );

        let seconds = spans
            .iter()
            .find(|span| span.content.as_ref() == "0.0s")
            .expect("seconds duration is rendered");
        let milliseconds = spans
            .iter()
            .find(|span| span.content.as_ref() == "512ms")
            .expect("milliseconds duration is rendered");
        let bare_number = spans
            .iter()
            .find(|span| span.content.as_ref().contains("3 retries"))
            .expect("bare number prose remains rendered");

        assert_eq!(seconds.style.fg, Some(Color::Gray));
        assert_eq!(milliseconds.style.fg, Some(Color::Gray));
        assert_eq!(bare_number.style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_style_narada_identifiers_as_code() {
        let spans = inline_code_spans(
            "Current Site is narada-proper with authority_locus facade_only.",
            ui_theme::body(),
        );

        let narada = spans
            .iter()
            .find(|span| span.content.as_ref() == "narada-proper")
            .expect("narada site id is rendered");
        let authority = spans
            .iter()
            .find(|span| span.content.as_ref() == "authority_locus")
            .expect("authority key is rendered");
        let posture = spans
            .iter()
            .find(|span| span.content.as_ref() == "facade_only")
            .expect("posture value is rendered");
        let trailing_period = spans.last().expect("trailing punctuation is rendered");

        assert_eq!(narada.style.fg, Some(Color::Gray));
        assert_eq!(authority.style.fg, Some(Color::Gray));
        assert_eq!(posture.style.fg, Some(Color::Gray));
        assert_eq!(trailing_period.content.as_ref(), ".");
        assert_eq!(trailing_period.style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_style_command_flags_and_slash_commands_as_code() {
        let spans = inline_code_spans(
            "Run /status with --site-root and -Runtime.",
            ui_theme::body(),
        );

        for token in ["/status", "--site-root", "-Runtime"] {
            let span = spans
                .iter()
                .find(|span| span.content.as_ref() == token)
                .unwrap_or_else(|| panic!("{token} is rendered"));
            assert_eq!(span.style.fg, Some(Color::Gray));
        }
        let trailing_period = spans.last().expect("trailing punctuation is rendered");
        assert_eq!(trailing_period.content.as_ref(), ".");
        assert_eq!(trailing_period.style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_keep_tool_call_parentheses_in_code_span() {
        let spans = inline_code_spans("Call agent_context_startup_sequence({}).", ui_theme::body());

        let call = spans
            .iter()
            .find(|span| span.content.as_ref() == "agent_context_startup_sequence({})")
            .expect("tool call is rendered as one span");
        let trailing_period = spans.last().expect("trailing punctuation is rendered");

        assert_eq!(call.style.fg, Some(Color::Gray));
        assert_eq!(trailing_period.content.as_ref(), ".");
        assert_eq!(trailing_period.style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_do_not_style_ordinary_hyphenated_prose_as_code() {
        let spans = inline_code_spans("Startup was advisory/read-only today", ui_theme::body());

        assert_eq!(spans.len(), 1);
        assert_eq!(
            spans[0].content.as_ref(),
            "Startup was advisory/read-only today"
        );
        assert_eq!(spans[0].style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_style_bold_content_without_rendering_delimiters() {
        let spans = inline_code_spans("Use **strong text** now", ui_theme::body());

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "Use ");
        assert_eq!(spans[0].style.fg, Some(Color::White));
        assert_eq!(spans[1].content.as_ref(), "strong text");
        assert_eq!(spans[1].style.fg, Some(Color::White));
        assert!(spans[1].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[2].content.as_ref(), " now");
        assert_eq!(spans[2].style.fg, Some(Color::White));
    }

    #[test]
    fn inline_code_spans_style_italic_content_without_rendering_delimiters() {
        let spans = inline_code_spans("Use *gentle emphasis* now", ui_theme::body());

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "Use ");
        assert_eq!(spans[1].content.as_ref(), "gentle emphasis");
        assert_eq!(spans[1].style.fg, Some(Color::White));
        assert!(spans[1].style.add_modifier.contains(Modifier::ITALIC));
        assert_eq!(spans[2].content.as_ref(), " now");
    }

    #[test]
    fn structured_body_spans_style_indented_list_prefix_as_muted_structure() {
        let spans = structured_body_spans("  1. numbered item", ui_theme::body());

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "  ");
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[1].content.as_ref(), "1. ");
        assert_eq!(spans[1].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[2].content.as_ref(), "numbered item");
        assert_eq!(spans[2].style.fg, Some(Color::White));
    }

    #[test]
    fn structured_body_spans_style_lettered_option_marker_as_muted_structure() {
        let spans = structured_body_spans("A. (*) Choose this path", ui_theme::body());

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content.as_ref(), "A. ");
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
        assert_eq!(spans[1].content.as_ref(), "(*) ");
        assert_eq!(spans[1].style.fg, Some(Color::Magenta));
        assert!(spans[1].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[2].content.as_ref(), "Choose this path");
        assert_eq!(spans[2].style.fg, Some(Color::White));
    }

    #[test]
    fn inline_and_fenced_code_keep_distinct_body_style() {
        let model = build_app_view(&AppViewInput {
            terminal_size: TerminalSize {
                width: 100,
                height: 18,
            },
            layout_config: LayoutConfig::default(),
            transcript_items: vec![TranscriptItem {
                kind: TranscriptItemKind::ProviderTextDelta,
                actor: TranscriptActor::Agent,
                turn_id: "turn_1".to_string(),
                text: "Use `config.json` now\n```powershell\npnpm test\n```".to_string(),
                sequence: Some(1),
                projection_key: None,
                occurred_at: Some("2026-05-30T00:02:00.000Z".to_string()),
            }],
            status: StatusViewInput {
                identity: "sonar.resident".to_string(),
                session: "carrier_1".to_string(),
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
        let mut buffer = Buffer::empty(TuiRect::new(0, 0, 100, 18));

        render_app_to_buffer(&model, &mut buffer);

        let (normal_x, normal_y) =
            find_text_position(&buffer, "Use ").expect("normal body text is rendered");
        let (inline_code_x, inline_code_y) =
            find_text_position(&buffer, "config.json").expect("inline code is rendered");
        let (code_header_x, code_header_y) =
            find_text_position(&buffer, "code: powershell").expect("code header is rendered");
        let (command_x, command_y) =
            find_text_position(&buffer, "pnpm test").expect("fenced command is rendered");
        let (timestamp_x, timestamp_y) = find_text_position(&buffer, "2026-05-30Z00:02")
            .expect("timestamp after fenced code is rendered");
        let text = buffer_text(&buffer);

        assert!(!text.contains("```powershell"));
        assert!(!text.contains("  ```"));
        assert_eq!(buffer[(normal_x, normal_y)].fg, Color::White);
        assert_eq!(buffer[(inline_code_x, inline_code_y)].fg, Color::Gray);
        assert_eq!(buffer[(code_header_x, code_header_y)].fg, Color::DarkGray);
        assert_eq!(buffer[(command_x, command_y)].fg, Color::Gray);
        assert_eq!(buffer[(timestamp_x, timestamp_y)].fg, Color::DarkGray);
        assert_eq!(inline_code_y, normal_y);
        assert_eq!(code_header_x, normal_x);
        assert_eq!(command_x, normal_x);
        assert_eq!(timestamp_x, normal_x);
        assert_eq!(timestamp_y, command_y + 1);
    }

    #[test]
    fn composer_render_uses_live_composer_with_visible_cursor_contract() {
        let model = model();
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "live".to_string(),
        });
        let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 12));

        render_app_to_buffer_with_composer(&model, &composer, &mut buffer);
        let text = buffer_text(&buffer);

        assert!(text.contains("live"));
        assert_eq!(composer.textarea().cursor_style().fg, Some(Color::Black));
        assert_eq!(composer.textarea().cursor_style().bg, Some(Color::Green));
    }

    #[test]
    fn renders_app_view_with_live_textarea_composer() {
        let model = model();
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "live draft".to_string(),
        });
        let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 12));

        render_app_to_buffer_with_composer(&model, &composer, &mut buffer);
        let text = buffer_text(&buffer);

        assert!(text.contains("Transcript"));
        assert!(text.contains("sonar.resident"));
        assert!(!text.contains("agent="));
        assert!(text.contains("operator -> sonar.resident>"));
        assert!(!text.contains("Composer:"));
        assert!(text.contains("live draft"));
        assert!(!text.contains("operator -> sonar.resident> hello"));
    }

    #[test]
    fn renders_active_turn_with_live_composer_state() {
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
                text: "working".to_string(),
                sequence: None,
                projection_key: None,
                occurred_at: Some("2026-05-30T00:01:00.000Z".to_string()),
            }],
            status: StatusViewInput {
                identity: "sonar.resident".to_string(),
                session: "carrier_1".to_string(),
                turn_state: TurnState::Active,
                active_phase: None,
                active_turn_age: Some("1m 12s".to_string()),
                queued_inputs: 2,
                held_system_directives: 1,
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
                draft_text: "snapshot stale".to_string(),
                turn_state: TurnState::Active,
                queued_operator_notes: 2,
                held_system_directives: 1,
            },
        });
        let composer = TextareaComposer::from_draft(&ComposerDraftState {
            text: "active live note".to_string(),
        });
        let mut buffer = Buffer::empty(TuiRect::new(0, 0, 80, 12));

        render_app_to_buffer_with_composer(&model, &composer, &mut buffer);
        let text = buffer_text(&buffer);

        assert!(text.contains("operator note -> sonar.resident>"));
        assert!(!text.contains("Composer:"));
        assert!(text.contains("active live note"));
        assert!(text.contains("thinking"));
        assert!(text.contains("queued operator steering 2"));
        assert!(!text.contains("snapshot stale"));
    }
}
