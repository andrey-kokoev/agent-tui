use crate::transcript_projection::{TranscriptActor, TranscriptItem, TranscriptItemKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptRow {
    pub key: String,
    pub actor: TranscriptActor,
    pub actor_label: String,
    pub kind: TranscriptItemKind,
    pub turn_id: String,
    pub text: String,
    pub occurred_at: Option<String>,
}

pub fn build_transcript_rows(items: &[TranscriptItem]) -> Vec<TranscriptRow> {
    let mut rows: Vec<TranscriptRow> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if !is_visible_transcript_item(item) {
            continue;
        }
        if let Some(last) = rows.last_mut() {
            if can_coalesce_provider_text(last, item) {
                last.text.push_str(&item.text);
                last.occurred_at = item
                    .occurred_at
                    .clone()
                    .or_else(|| last.occurred_at.clone());
                continue;
            }
        }
        rows.push(build_transcript_row(index, item));
    }
    rows
}

fn is_visible_transcript_item(item: &TranscriptItem) -> bool {
    !(item.kind == TranscriptItemKind::ProviderTextDelta && item.text.trim().is_empty())
}

fn can_coalesce_provider_text(row: &TranscriptRow, item: &TranscriptItem) -> bool {
    row.kind == TranscriptItemKind::ProviderTextDelta
        && item.kind == TranscriptItemKind::ProviderTextDelta
        && row.actor == item.actor
        && row.turn_id == item.turn_id
}

pub fn build_transcript_row(index: usize, item: &TranscriptItem) -> TranscriptRow {
    TranscriptRow {
        key: stable_row_key(index, item),
        actor: item.actor.clone(),
        actor_label: actor_label(&item.actor).to_string(),
        kind: item.kind.clone(),
        turn_id: item.turn_id.clone(),
        text: item.text.clone(),
        occurred_at: item.occurred_at.clone(),
    }
}

fn stable_row_key(index: usize, item: &TranscriptItem) -> String {
    let turn = if item.turn_id.is_empty() {
        "session".to_string()
    } else {
        item.turn_id.clone()
    };
    let sequence = item
        .sequence
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string());
    format!(
        "row_{index}_{}_{}_{}",
        turn,
        transcript_kind_slug(&item.kind),
        sequence
    )
}

fn actor_label(actor: &TranscriptActor) -> &'static str {
    match actor {
        TranscriptActor::Operator => "operator",
        TranscriptActor::OperatorSteering => "operator steering",
        TranscriptActor::OperatorDirective => "operator directive",
        TranscriptActor::System => "system",
        TranscriptActor::Agent => "agent",
        TranscriptActor::AgentTui => "agent-tui",
        TranscriptActor::Provider => "provider",
    }
}

fn transcript_kind_slug(kind: &TranscriptItemKind) -> &'static str {
    match kind {
        TranscriptItemKind::InputAdmitted => "input_admitted",
        TranscriptItemKind::SystemDirectiveHeld => "system_directive_held",
        TranscriptItemKind::SystemDirectiveReleased => "system_directive_released",
        TranscriptItemKind::ProviderTextDelta => "provider_text_delta",
        TranscriptItemKind::ProviderToolCallRequest => "provider_tool_call_request",
        TranscriptItemKind::ToolResultReceived => "tool_result_received",
        TranscriptItemKind::TurnTerminalStatus => "turn_terminal_status",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(
        kind: TranscriptItemKind,
        actor: TranscriptActor,
        turn_id: &str,
        text: &str,
        sequence: Option<u64>,
    ) -> TranscriptItem {
        TranscriptItem {
            kind,
            actor,
            turn_id: turn_id.to_string(),
            text: text.to_string(),
            sequence,
            projection_key: None,
            occurred_at: Some("2026-05-30T00:00:00.000Z".to_string()),
        }
    }

    #[test]
    fn builds_rows_in_transcript_order() {
        let rows = build_transcript_rows(&[
            item(
                TranscriptItemKind::InputAdmitted,
                TranscriptActor::Operator,
                "turn_1",
                "run startup sequence",
                None,
            ),
            item(
                TranscriptItemKind::ProviderTextDelta,
                TranscriptActor::Agent,
                "turn_1",
                "done",
                Some(1),
            ),
        ]);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].actor_label, "operator");
        assert_eq!(rows[0].text, "run startup sequence");
        assert_eq!(rows[1].actor_label, "agent");
        assert_eq!(rows[1].text, "done");
    }

    #[test]
    fn skips_blank_provider_text_delta_rows() {
        let rows = build_transcript_rows(&[
            item(
                TranscriptItemKind::ProviderTextDelta,
                TranscriptActor::Agent,
                "turn_1",
                "  \n\t  ",
                Some(1),
            ),
            item(
                TranscriptItemKind::ProviderTextDelta,
                TranscriptActor::Agent,
                "turn_1",
                "Visible response.",
                Some(2),
            ),
        ]);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "Visible response.");
        assert_eq!(rows[0].key, "row_1_turn_1_provider_text_delta_2");
    }

    #[test]
    fn coalesces_consecutive_provider_text_deltas_for_same_turn() {
        let rows = build_transcript_rows(&[
            item(
                TranscriptItemKind::ProviderTextDelta,
                TranscriptActor::Agent,
                "turn_1",
                "Hello ",
                Some(1),
            ),
            item(
                TranscriptItemKind::ProviderTextDelta,
                TranscriptActor::Agent,
                "turn_1",
                "world.",
                Some(2),
            ),
        ]);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "Hello world.");
        assert_eq!(rows[0].key, "row_0_turn_1_provider_text_delta_1");
    }

    #[test]
    fn does_not_coalesce_provider_text_across_tool_boundaries() {
        let rows = build_transcript_rows(&[
            item(
                TranscriptItemKind::ProviderTextDelta,
                TranscriptActor::Agent,
                "turn_1",
                "Before tool. ",
                Some(1),
            ),
            item(
                TranscriptItemKind::ProviderToolCallRequest,
                TranscriptActor::AgentTui,
                "turn_1",
                "site_loop_run_once({})",
                Some(2),
            ),
            item(
                TranscriptItemKind::ProviderTextDelta,
                TranscriptActor::Agent,
                "turn_1",
                "After tool.",
                Some(3),
            ),
        ]);

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].text, "Before tool. ");
        assert_eq!(rows[1].text, "site_loop_run_once({})");
        assert_eq!(rows[2].text, "After tool.");
    }

    #[test]
    fn builds_stable_keys_from_order_turn_kind_and_sequence() {
        let rows = build_transcript_rows(&[
            item(
                TranscriptItemKind::ProviderTextDelta,
                TranscriptActor::Agent,
                "turn_1",
                "hello",
                Some(3),
            ),
            item(
                TranscriptItemKind::TurnTerminalStatus,
                TranscriptActor::AgentTui,
                "turn_1",
                "completed",
                None,
            ),
        ]);

        assert_eq!(rows[0].key, "row_0_turn_1_provider_text_delta_3");
        assert_eq!(rows[1].key, "row_1_turn_1_turn_terminal_status_none");
    }

    #[test]
    fn uses_session_key_scope_for_rows_without_turn_id() {
        let rows = build_transcript_rows(&[item(
            TranscriptItemKind::SystemDirectiveHeld,
            TranscriptActor::System,
            "",
            "system directive held input_1",
            None,
        )]);

        assert_eq!(rows[0].key, "row_0_session_system_directive_held_none");
        assert_eq!(rows[0].actor_label, "system");
    }
}
