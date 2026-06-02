use crate::carrier_protocol::{SessionEvent, parse_session_event};
use crate::transcript_projection::{TranscriptItem, TranscriptItemKind, project_session_event};
use std::collections::HashSet;
use std::fs::read_to_string;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptIngestResult {
    Projected,
    Ignored,
    Duplicate,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscriptIngestSummary {
    pub projected: usize,
    pub ignored: usize,
    pub duplicate: usize,
    pub total_items: usize,
}

impl TranscriptIngestSummary {
    pub fn add_result(&mut self, result: TranscriptIngestResult) {
        match result {
            TranscriptIngestResult::Projected => self.projected += 1,
            TranscriptIngestResult::Ignored => self.ignored += 1,
            TranscriptIngestResult::Duplicate => self.duplicate += 1,
        }
    }
}

#[derive(Debug, Default)]
pub struct TranscriptStore {
    items: Vec<TranscriptItem>,
    ingested_event_ids: HashSet<String>,
    ingested_projection_keys: HashSet<String>,
}

impl TranscriptStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn items(&self) -> &[TranscriptItem] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear_projection(&mut self) {
        self.items.clear();
    }

    pub fn ingest_event(&mut self, event: &SessionEvent) -> TranscriptIngestResult {
        if self.ingested_event_ids.contains(&event.event_id) {
            return TranscriptIngestResult::Duplicate;
        }
        self.ingested_event_ids.insert(event.event_id.clone());

        if let Some(item) = project_session_event(event) {
            if let Some(projection_key) = &item.projection_key {
                if self.ingested_projection_keys.contains(projection_key) {
                    return TranscriptIngestResult::Duplicate;
                }
                self.ingested_projection_keys.insert(projection_key.clone());
            }
            if self.merge_streaming_provider_delta(&item) {
                return TranscriptIngestResult::Projected;
            }
            self.items.push(item);
            TranscriptIngestResult::Projected
        } else {
            TranscriptIngestResult::Ignored
        }
    }

    fn merge_streaming_provider_delta(&mut self, item: &TranscriptItem) -> bool {
        if item.kind != TranscriptItemKind::ProviderTextDelta {
            return false;
        }
        let Some(previous) = self.items.last_mut() else {
            return false;
        };
        if previous.kind != TranscriptItemKind::ProviderTextDelta
            || previous.actor != item.actor
            || previous.turn_id != item.turn_id
        {
            return false;
        }
        previous.text.push_str(&item.text);
        previous.sequence = item.sequence;
        true
    }

    pub fn ingest_jsonl_line(&mut self, line: &str) -> Result<TranscriptIngestResult, String> {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            return Ok(TranscriptIngestResult::Ignored);
        }
        let event = parse_session_event(trimmed)?;
        Ok(self.ingest_event(&event))
    }

    pub fn ingest_jsonl_lines(
        &mut self,
        content: &str,
    ) -> Result<Vec<TranscriptIngestResult>, String> {
        let mut results = Vec::new();
        for (index, line) in content.lines().enumerate() {
            results.push(
                self.ingest_jsonl_line(line)
                    .map_err(|error| format!("line_{}:{error}", index + 1))?,
            );
        }
        Ok(results)
    }

    pub fn ingest_jsonl_summary(
        &mut self,
        content: &str,
    ) -> Result<TranscriptIngestSummary, String> {
        let mut summary = TranscriptIngestSummary::default();
        for result in self.ingest_jsonl_lines(content)? {
            summary.add_result(result);
        }
        summary.total_items = self.len();
        Ok(summary)
    }

    pub fn ingest_jsonl_file_summary(
        &mut self,
        path: &Path,
    ) -> Result<TranscriptIngestSummary, String> {
        if !path.exists() {
            return Ok(TranscriptIngestSummary {
                total_items: self.len(),
                ..TranscriptIngestSummary::default()
            });
        }
        let content =
            read_to_string(path).map_err(|error| format!("session_jsonl_read_failed:{error}"))?;
        self.ingest_jsonl_summary(&content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{
        SESSION_EVENT_SCHEMA, SessionEvent, SessionEventKind, TURN_TERMINAL_PAYLOAD_SCHEMA,
    };
    use crate::transcript_projection::{TranscriptActor, TranscriptItemKind};
    use serde_json::json;

    fn event(
        event_id: &str,
        event_kind: SessionEventKind,
        payload: serde_json::Value,
    ) -> SessionEvent {
        SessionEvent {
            schema: SESSION_EVENT_SCHEMA.to_string(),
            event_kind,
            event_id: event_id.to_string(),
            occurred_at: "2026-05-30T00:00:00.000Z".to_string(),
            carrier_session_id: "carrier_fixture_1".to_string(),
            agent_id: "sonar.resident".to_string(),
            site_id: "narada-sonar".to_string(),
            site_root: "D:/code/narada.sonar".to_string(),
            payload,
        }
    }

    #[test]
    fn appends_projected_items_in_ingest_order() {
        let mut store = TranscriptStore::new();

        assert_eq!(
            store.ingest_event(&event(
                "session_event_1",
                SessionEventKind::TurnStarted,
                json!({
                    "turn_id": "turn_1",
                    "input_event_id": "input_1",
                    "source_kind": "operator",
                    "content_preview": "run startup sequence"
                }),
            )),
            TranscriptIngestResult::Projected
        );
        assert_eq!(
            store.ingest_event(&event(
                "session_event_2",
                SessionEventKind::ProviderTextDeltaRecorded,
                json!({
                    "turn_id": "turn_1",
                    "sequence": 1,
                    "text_delta": "done"
                }),
            )),
            TranscriptIngestResult::Projected
        );

        assert_eq!(store.len(), 2);
        assert_eq!(store.items()[0].actor, TranscriptActor::Operator);
        assert_eq!(store.items()[0].text, "run startup sequence");
        assert_eq!(store.items()[1].actor, TranscriptActor::Agent);
        assert_eq!(store.items()[1].text, "done");
    }

    #[test]
    fn merges_streaming_provider_text_deltas_for_same_turn() {
        let mut store = TranscriptStore::new();

        assert_eq!(
            store.ingest_event(&event(
                "session_event_1",
                SessionEventKind::ProviderTextDeltaRecorded,
                json!({
                    "turn_id": "turn_1",
                    "sequence": 1,
                    "text_delta": "hello"
                }),
            )),
            TranscriptIngestResult::Projected
        );
        assert_eq!(
            store.ingest_event(&event(
                "session_event_2",
                SessionEventKind::ProviderTextDeltaRecorded,
                json!({
                    "turn_id": "turn_1",
                    "sequence": 2,
                    "text_delta": " world"
                }),
            )),
            TranscriptIngestResult::Projected
        );

        assert_eq!(store.len(), 1);
        assert_eq!(store.items()[0].kind, TranscriptItemKind::ProviderTextDelta);
        assert_eq!(store.items()[0].actor, TranscriptActor::Agent);
        assert_eq!(store.items()[0].turn_id, "turn_1");
        assert_eq!(store.items()[0].text, "hello world");
        assert_eq!(store.items()[0].sequence, Some(2));
    }

    #[test]
    fn keeps_provider_text_deltas_separate_across_turns() {
        let mut store = TranscriptStore::new();

        store.ingest_event(&event(
            "session_event_1",
            SessionEventKind::ProviderTextDeltaRecorded,
            json!({
                "turn_id": "turn_1",
                "sequence": 1,
                "text_delta": "first"
            }),
        ));
        store.ingest_event(&event(
            "session_event_2",
            SessionEventKind::ProviderTextDeltaRecorded,
            json!({
                "turn_id": "turn_2",
                "sequence": 1,
                "text_delta": "second"
            }),
        ));

        assert_eq!(store.len(), 2);
        assert_eq!(store.items()[0].text, "first");
        assert_eq!(store.items()[1].text, "second");
    }

    #[test]
    fn dedupes_input_admission_and_turn_started_for_same_input_event() {
        let mut store = TranscriptStore::new();

        assert_eq!(
            store.ingest_event(&event(
                "session_event_1",
                SessionEventKind::InputAdmittedToTurn,
                json!({
                    "input_event_id": "input_1",
                    "source_kind": "operator",
                    "content_preview": "run startup sequence"
                }),
            )),
            TranscriptIngestResult::Projected
        );
        assert_eq!(
            store.ingest_event(&event(
                "session_event_2",
                SessionEventKind::TurnStarted,
                json!({
                    "turn_id": "turn_1",
                    "input_event_id": "input_1",
                    "source_kind": "operator",
                    "content_preview": "run startup sequence"
                }),
            )),
            TranscriptIngestResult::Duplicate
        );
        assert_eq!(store.len(), 1);
        assert_eq!(store.items()[0].text, "run startup sequence");
    }

    #[test]
    fn records_duplicates_without_appending_again() {
        let mut store = TranscriptStore::new();
        let event = event(
            "session_event_1",
            SessionEventKind::TurnCompleted,
            json!({
                "schema": TURN_TERMINAL_PAYLOAD_SCHEMA,
                "turn_id": "turn_1",
                "terminal_status": "completed",
                "provider_request_status": "completed",
                "provider_execution_enabled": true
            }),
        );

        assert_eq!(
            store.ingest_event(&event),
            TranscriptIngestResult::Projected
        );
        assert_eq!(
            store.ingest_event(&event),
            TranscriptIngestResult::Duplicate
        );
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn projects_provider_requested_tool_result_when_event_ids_are_unique() {
        let mut store = TranscriptStore::new();

        assert_eq!(
            store.ingest_event(&event(
                "session_event_turn_step241_1",
                SessionEventKind::TurnStarted,
                json!({
                    "turn_id": "turn_step241_1",
                    "input_event_id": "input_operator_composer_1",
                    "source_kind": "operator",
                    "content_preview": "run startup sequence"
                }),
            )),
            TranscriptIngestResult::Projected
        );
        assert_eq!(
            store.ingest_event(&event(
                "session_event_turn_step241_3",
                SessionEventKind::ProviderToolCallRequested,
                json!({
                    "turn_id": "turn_step241_1",
                    "sequence": 1,
                    "tool_name": "startup_sequence",
                    "arguments_summary": "{}"
                }),
            )),
            TranscriptIngestResult::Projected
        );
        assert_eq!(
            store.ingest_event(&event(
                "session_event_turn_step241_provider_tool_1",
                SessionEventKind::ToolResultReceived,
                json!({
                    "turn_id": "turn_step241_1",
                    "tool_name": "startup_sequence",
                    "status": "ok",
                    "duration_ms": 10,
                    "result_summary": "content_items=1"
                }),
            )),
            TranscriptIngestResult::Projected
        );
        assert_eq!(
            store.ingest_event(&event(
                "session_event_turn_step241_4",
                SessionEventKind::TurnCompleted,
                json!({
                    "schema": TURN_TERMINAL_PAYLOAD_SCHEMA,
                    "turn_id": "turn_step241_1",
                    "terminal_status": "completed",
                    "provider_request_status": "completed",
                    "provider_execution_enabled": true
                }),
            )),
            TranscriptIngestResult::Projected
        );

        let rendered = store
            .items()
            .iter()
            .map(|item| item.text.as_str())
            .collect::<Vec<_>>();
        assert_eq!(rendered[0], "run startup sequence");
        assert_eq!(rendered[1], "startup_sequence({})");
        assert_eq!(rendered[2], "ok startup_sequence in 10ms · content_items=1");
        assert_eq!(rendered[3], "completed");
    }

    #[test]
    fn ignores_non_transcript_events_but_still_dedupes_them() {
        let mut store = TranscriptStore::new();
        let event = event(
            "session_event_1",
            SessionEventKind::InputQueuedForTurnBoundary,
            json!({ "input_event_id": "input_1" }),
        );

        assert_eq!(store.ingest_event(&event), TranscriptIngestResult::Ignored);
        assert_eq!(
            store.ingest_event(&event),
            TranscriptIngestResult::Duplicate
        );
        assert!(store.is_empty());
    }

    #[test]
    fn ingests_jsonl_lines() {
        let mut store = TranscriptStore::new();
        let first = serde_json::to_string(&event(
            "session_event_1",
            SessionEventKind::SystemDirectiveHeld,
            json!({
                "input_event_id": "input_1",
                "held_at": "2026-05-30T00:00:00.000Z",
                "held_reason": "composer_nonempty",
                "original_delivery_mode": "admit_for_current_turn"
            }),
        ))
        .expect("event serializes");
        let second = serde_json::to_string(&event(
            "session_event_2",
            SessionEventKind::TurnFailed,
            json!({
                "schema": TURN_TERMINAL_PAYLOAD_SCHEMA,
                "turn_id": "turn_1",
                "terminal_status": "failed",
                "provider_request_status": "failed",
                "provider_execution_enabled": true,
                "error_summary": "failed"
            }),
        ))
        .expect("event serializes");

        let results = store
            .ingest_jsonl_lines(&format!("{first}\n{second}\n"))
            .expect("jsonl ingests");

        assert_eq!(
            results,
            vec![
                TranscriptIngestResult::Projected,
                TranscriptIngestResult::Projected
            ]
        );
        assert_eq!(
            store.items()[0].kind,
            TranscriptItemKind::SystemDirectiveHeld
        );
        assert_eq!(
            store.items()[1].kind,
            TranscriptItemKind::TurnTerminalStatus
        );
    }

    #[test]
    fn reports_jsonl_line_parse_errors_with_line_number() {
        let mut store = TranscriptStore::new();
        let error = store
            .ingest_jsonl_lines("\nnot json\n")
            .expect_err("invalid jsonl is rejected");

        assert!(error.starts_with("line_2:"));
        assert!(store.is_empty());
    }
}
