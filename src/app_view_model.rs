use crate::composer_view_model::{ComposerViewInput, ComposerViewModel, build_composer_view};
use crate::layout_model::{AgentTuiLayout, LayoutConfig, TerminalSize, compute_layout};
use crate::status_view_model::{
    StatusSegment, StatusViewInput, StatusViewModel, build_status_view,
    status_segment_compact_text, status_segment_is_visible,
};
use crate::transcript_projection::TranscriptItem;
use crate::transcript_view_model::{TranscriptRow, build_transcript_rows};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppViewInput {
    pub terminal_size: TerminalSize,
    pub layout_config: LayoutConfig,
    pub transcript_items: Vec<TranscriptItem>,
    pub status: StatusViewInput,
    pub composer: ComposerViewInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppViewModel {
    pub layout: AgentTuiLayout,
    pub transcript_rows: Vec<TranscriptRow>,
    pub transcript_scroll_offset: usize,
    pub status: StatusViewModel,
    pub composer: ComposerViewModel,
}

impl AppViewModel {
    pub fn set_transcript_scroll_offset(&mut self, offset: usize) {
        self.transcript_scroll_offset = offset;
        remove_status_segment(&mut self.status.segments, "transcript_scroll_offset");
        if offset > 0 {
            self.status.segments.push(StatusSegment {
                key: "transcript_scroll_offset".to_string(),
                label: "scroll".to_string(),
                value: offset.to_string(),
            });
        }
        self.status.compact_line = compact_status_line(&self.status.segments);
    }
}

pub fn build_app_view(input: &AppViewInput) -> AppViewModel {
    let transcript_rows = build_transcript_rows(&input.transcript_items);
    let mut status = build_status_view(&input.status);
    set_status_segment_value(
        &mut status.segments,
        "transcript_items",
        &transcript_rows.len().to_string(),
    );
    status.compact_line = compact_status_line(&status.segments);

    AppViewModel {
        layout: compute_layout(input.terminal_size, input.layout_config),
        transcript_rows,
        transcript_scroll_offset: 0,
        status,
        composer: build_composer_view(&input.composer),
    }
}

fn compact_status_line(segments: &[StatusSegment]) -> String {
    segments
        .iter()
        .filter(|segment| status_segment_is_visible(segment))
        .filter(|segment| segment.key != "identity")
        .map(status_segment_compact_text)
        .collect::<Vec<_>>()
        .join(" | ")
}

fn remove_status_segment(segments: &mut Vec<StatusSegment>, key: &str) {
    segments.retain(|segment| segment.key != key);
}

fn set_status_segment_value(segments: &mut [StatusSegment], key: &str, value: &str) {
    if let Some(segment) = segments.iter_mut().find(|segment| segment.key == key) {
        segment.value = value.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_queue::TurnState;
    use crate::status_view_model::{
        McpRuntimeState, ProviderAdapterState, ProviderRuntimeState, RuntimePostureState,
        TerminalRuntimeState,
    };
    use crate::transcript_projection::{TranscriptActor, TranscriptItemKind};

    fn input() -> AppViewInput {
        AppViewInput {
            terminal_size: TerminalSize {
                width: 100,
                height: 20,
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
        }
    }

    #[test]
    fn builds_aggregate_view_model() {
        let model = build_app_view(&input());

        assert_eq!(model.layout.transcript.width, 100);
        assert_eq!(model.layout.composer.height, 4);
        assert_eq!(model.transcript_rows.len(), 1);
        assert_eq!(model.transcript_rows[0].actor_label, "operator");
        assert_eq!(
            model.status.compact_line,
            "session carrier_1 | idle | transcript 1 | provider disabled | provider adapter disabled | mcp disabled | terminal disabled"
        );
        assert_eq!(model.composer.prompt_label, "operator -> sonar.resident>");
    }

    #[test]
    fn status_transcript_count_matches_visible_transcript_rows() {
        let model = build_app_view(&AppViewInput {
            transcript_items: vec![
                TranscriptItem {
                    kind: TranscriptItemKind::ProviderTextDelta,
                    actor: TranscriptActor::Agent,
                    turn_id: "turn_1".to_string(),
                    text: "  \n  ".to_string(),
                    sequence: Some(1),
                    projection_key: None,
                    occurred_at: Some("2026-05-30T00:00:00.000Z".to_string()),
                },
                TranscriptItem {
                    kind: TranscriptItemKind::ProviderTextDelta,
                    actor: TranscriptActor::Agent,
                    turn_id: "turn_1".to_string(),
                    text: "visible".to_string(),
                    sequence: Some(2),
                    projection_key: None,
                    occurred_at: Some("2026-05-30T00:00:01.000Z".to_string()),
                },
            ],
            status: StatusViewInput {
                transcript_items: 2,
                ..input().status
            },
            ..input()
        });

        assert_eq!(model.transcript_rows.len(), 1);
        assert!(model.status.compact_line.contains("transcript 1"));
        assert!(!model.status.compact_line.contains("transcript 2"));
    }

    #[test]
    fn compact_status_hides_only_non_actionable_zero_and_none_segments() {
        let quiet = build_app_view(&input());

        assert!(
            !quiet
                .status
                .compact_line
                .contains("queued operator steering 0")
        );
        assert!(
            !quiet
                .status
                .compact_line
                .contains("held system directives 0")
        );
        assert!(!quiet.status.compact_line.contains("error none"));

        let active = build_app_view(&AppViewInput {
            status: StatusViewInput {
                queued_inputs: 2,
                held_system_directives: 1,
                last_error: Some("provider_cancelled".to_string()),
                ..input().status
            },
            ..input()
        });

        assert!(
            active
                .status
                .compact_line
                .contains("queued operator steering 2")
        );
        assert!(
            active
                .status
                .compact_line
                .contains("held system directives 1")
        );
        assert!(
            active
                .status
                .compact_line
                .contains("error provider cancelled")
        );
    }

    #[test]
    fn scroll_offset_updates_compact_status_without_duplicate_segments() {
        let mut model = build_app_view(&input());

        model.set_transcript_scroll_offset(16);
        model.set_transcript_scroll_offset(16);

        assert_eq!(model.transcript_scroll_offset, 16);
        assert_eq!(
            model
                .status
                .segments
                .iter()
                .filter(|segment| segment.key == "transcript_scroll_offset")
                .count(),
            1
        );
        assert!(model.status.compact_line.contains("scroll 16 lines"));

        model.set_transcript_scroll_offset(0);

        assert_eq!(model.transcript_scroll_offset, 0);
        assert!(!model.status.compact_line.contains("scroll"));
        assert!(
            !model
                .status
                .segments
                .iter()
                .any(|segment| segment.key == "transcript_scroll_offset")
        );
    }

    #[test]
    fn humanizes_internal_status_values_for_display() {
        let model = build_app_view(&AppViewInput {
            status: StatusViewInput {
                runtime_posture: RuntimePostureState {
                    provider_state: ProviderRuntimeState::Configured,
                    provider_adapter_state: ProviderAdapterState::ConfiguredWithoutAdapter,
                    mcp_state: McpRuntimeState::Configured,
                    terminal_state: TerminalRuntimeState::Configured,
                },
                last_error: Some("provider_cancelled".to_string()),
                ..input().status
            },
            ..input()
        });

        assert!(model.status.compact_line.contains("provider configured"));
        assert!(
            model
                .status
                .compact_line
                .contains("provider adapter configured without adapter")
        );
        assert!(model.status.compact_line.contains("mcp configured"));
        assert!(model.status.compact_line.contains("terminal configured"));
        assert!(
            model
                .status
                .compact_line
                .ends_with("error provider cancelled")
        );
        assert!(
            !model
                .status
                .compact_line
                .contains("configured_without_adapter")
        );
        assert!(!model.status.compact_line.contains("provider_cancelled"));
    }

    #[test]
    fn formats_singular_draft_character_status() {
        let model = build_app_view(&AppViewInput {
            composer: ComposerViewInput {
                draft_text: "x".to_string(),
                ..input().composer
            },
            ..input()
        });

        assert!(!model.status.compact_line.contains("draft"));
    }

    #[test]
    fn whitespace_only_draft_does_not_create_status_draft_segment() {
        let model = build_app_view(&AppViewInput {
            composer: ComposerViewInput {
                draft_text: "  \t  ".to_string(),
                ..input().composer
            },
            ..input()
        });

        assert!(model.composer.draft_is_empty);
        assert!(!model.status.compact_line.contains("draft"));
        assert!(
            !model
                .status
                .segments
                .iter()
                .any(|segment| segment.key == "draft_chars")
        );
    }

    #[test]
    fn reflects_active_turn_state_across_status_and_composer() {
        let model = build_app_view(&AppViewInput {
            status: StatusViewInput {
                turn_state: TurnState::Active,
                active_phase: None,
                active_turn_age: Some("1m 12s".to_string()),
                runtime_posture: RuntimePostureState {
                    provider_state: ProviderRuntimeState::Working,
                    ..RuntimePostureState::disabled()
                },
                queued_inputs: 2,
                ..input().status
            },
            composer: ComposerViewInput {
                turn_state: TurnState::Active,
                queued_operator_notes: 2,
                ..input().composer
            },
            ..input()
        });

        assert_eq!(model.status.segments[2].value, "active 1m 12s");
        assert!(model.status.compact_line.contains("thinking 1m 12s"));
        assert_eq!(model.status.segments[3].value, "2");
        assert_eq!(model.status.segments[5].key, "esc_action");
        assert_eq!(model.status.segments[5].value, "interrupt");
        assert_eq!(model.status.segments[7].value, "working");
        assert_eq!(
            model.composer.prompt_label,
            "operator note -> sonar.resident>"
        );
        assert_eq!(model.composer.submit_hint, "Enter queues note");
    }
}
