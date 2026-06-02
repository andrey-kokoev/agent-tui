use narada_agent_tui::carrier_protocol::{
    DeliveryMode, SessionEventKind, create_provider_request_payload, parse_input_event,
    parse_session_event,
};
use narada_agent_tui::input_queue::{InputQueue, SessionEvidenceContext};
use narada_agent_tui::provider_adapter_contract::provider_adapter_contract;
use narada_agent_tui::provider_dispatch::{
    ProviderAdapter, ProviderCancellationToken, ProviderDispatchRecord, ProviderDispatchStatus,
    ProviderOutputRecord,
};
use narada_agent_tui::transcript_store::TranscriptStore;
use narada_agent_tui::turn_coordinator::{TurnCoordinator, TurnCoordinatorClock};
use std::fs::{read_to_string, remove_file};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const INPUT_FIXTURE: &str = include_str!("../../carrier-protocol/fixtures/input-event.json");
static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

fn admitted_provider() -> String {
    provider_adapter_contract()
        .admitted_providers
        .first()
        .expect("provider contract has at least one admitted provider")
        .clone()
}

fn temp_session_path() -> PathBuf {
    let unique = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "narada-agent-tui-provider-boundary-{}-{unique}.jsonl",
        std::process::id()
    ))
}

fn context() -> SessionEvidenceContext {
    SessionEvidenceContext {
        carrier_session_id: "carrier_fixture_1".to_string(),
        agent_id: "sonar.resident".to_string(),
        site_id: "sonar".to_string(),
        site_root: "D:/code/narada.sonar".to_string(),
    }
}

fn clock() -> TurnCoordinatorClock {
    TurnCoordinatorClock {
        occurred_at: "2026-05-30T00:00:04.000Z".to_string(),
        event_id_prefix: "session_event_turn".to_string(),
        turn_id_prefix: "turn".to_string(),
    }
}

struct StreamingProviderAdapter;

impl ProviderAdapter for StreamingProviderAdapter {
    fn dispatch_request(
        &self,
        input: &narada_agent_tui::carrier_protocol::InputEvent,
        turn_id: &str,
        _cancellation: &ProviderCancellationToken,
    ) -> ProviderDispatchRecord {
        ProviderDispatchRecord {
            status: ProviderDispatchStatus::Completed,
            provider_execution_enabled: true,
            payload: create_provider_request_payload(
                turn_id,
                &input.event_id,
                "completed",
                true,
                "configured",
                "admitted",
                Some("scripted".to_string()),
                Some(admitted_provider()),
                Some("gpt-5.5".to_string()),
                None,
                true,
                "streaming_text_delta_events",
                None,
                &input.content,
            ),
            outputs: vec![
                ProviderOutputRecord::text_delta(turn_id, "Startup ", 1),
                ProviderOutputRecord::text_delta(turn_id, "sequence ", 2),
                ProviderOutputRecord::text_delta(turn_id, "completed.", 3),
            ],
        }
    }
}

#[test]
fn provider_boundary_acceptance_records_disabled_provider_posture() {
    let path = temp_session_path();
    let mut input = parse_input_event(INPUT_FIXTURE).expect("input fixture parses");
    input.delivery_mode = DeliveryMode::AdmitAfterActiveTurn;
    let mut queue = InputQueue::new();
    queue.admit_input_event(input.clone(), false);
    let mut coordinator = TurnCoordinator::new(&path, context());

    let completed = coordinator
        .run_one_ready_turn(&mut queue, &clock())
        .expect("turn run succeeds")
        .expect("turn completes at provider boundary");

    assert_eq!(completed.turn_id, "turn_1");
    assert_eq!(completed.input_event_id, input.event_id);
    assert_eq!(completed.evidence_written, 3);

    let session_jsonl = read_to_string(&path).expect("session jsonl exists");
    let lines: Vec<&str> = session_jsonl.lines().collect();
    assert_eq!(lines.len(), 3);

    let started = parse_session_event(lines[0]).expect("turn started parses");
    let provider_request = parse_session_event(lines[1]).expect("provider request parses");
    let terminal = parse_session_event(lines[2]).expect("terminal event parses");

    assert_eq!(started.event_kind, SessionEventKind::TurnStarted);
    assert_eq!(started.payload["turn_id"], "turn_1");
    assert_eq!(started.payload["input_event_id"], input.event_id);
    assert_eq!(started.payload["source_kind"], "operator");

    assert_eq!(
        provider_request.event_kind,
        SessionEventKind::ProviderRequestRecorded
    );
    assert_eq!(provider_request.payload["turn_id"], "turn_1");
    assert_eq!(provider_request.payload["input_event_id"], input.event_id);
    assert_eq!(
        provider_request.payload["provider_request_status"],
        "recorded_not_dispatched"
    );
    assert_eq!(
        provider_request.payload["provider_execution_enabled"],
        false
    );
    assert_eq!(
        provider_request.payload["provider_runtime_status"],
        "disabled"
    );
    assert_eq!(
        provider_request.payload["provider_adapter_refusal_reason"],
        serde_json::Value::Null
    );
    assert_eq!(
        provider_request.payload["provider_streaming_contract"],
        "not_requested"
    );
    assert_eq!(provider_request.payload["content_preview"], input.content);

    assert_eq!(terminal.event_kind, SessionEventKind::TurnCompleted);
    assert_eq!(terminal.payload["turn_id"], "turn_1");
    assert_eq!(terminal.payload["input_event_id"], input.event_id);
    assert_eq!(
        terminal.payload["provider_request_status"],
        "recorded_not_dispatched"
    );
    assert_eq!(
        terminal.payload["terminal_status"],
        "completed_without_provider"
    );
    assert_eq!(terminal.payload["provider_execution_enabled"], false);

    remove_file(path).ok();
}

#[test]
fn provider_boundary_acceptance_projects_streaming_text_as_one_agent_message() {
    let path = temp_session_path();
    let mut input = parse_input_event(INPUT_FIXTURE).expect("input fixture parses");
    input.delivery_mode = DeliveryMode::AdmitAfterActiveTurn;
    let mut queue = InputQueue::new();
    queue.admit_input_event(input.clone(), false);
    let mut coordinator = TurnCoordinator::with_provider_adapter(
        &path,
        context(),
        Box::new(StreamingProviderAdapter),
    );

    let completed = coordinator
        .run_one_ready_turn(&mut queue, &clock())
        .expect("turn run succeeds")
        .expect("turn completes with provider output");

    assert_eq!(completed.evidence_written, 6);

    let session_jsonl = read_to_string(&path).expect("session jsonl exists");
    let lines: Vec<&str> = session_jsonl.lines().collect();
    assert_eq!(lines.len(), 6);
    assert_eq!(
        parse_session_event(lines[1])
            .expect("provider request parses")
            .payload["provider_streaming_contract"],
        "streaming_text_delta_events"
    );
    assert_eq!(
        parse_session_event(lines[2])
            .expect("first delta parses")
            .event_kind,
        SessionEventKind::ProviderTextDeltaRecorded
    );
    assert_eq!(
        parse_session_event(lines[4])
            .expect("third delta parses")
            .payload["sequence"],
        3
    );

    let mut transcript = TranscriptStore::new();
    let summary = transcript
        .ingest_jsonl_file_summary(&path)
        .expect("session transcript ingests");
    assert_eq!(summary.total_items, 3);
    assert_eq!(transcript.items()[0].actor.as_str(), "operator");
    assert_eq!(transcript.items()[1].actor.as_str(), "agent");
    assert_eq!(transcript.items()[1].text, "Startup sequence completed.");
    assert_eq!(transcript.items()[1].sequence, Some(3));
    assert_eq!(transcript.items()[2].actor.as_str(), "agent-tui");
    assert_eq!(transcript.items()[2].text, "completed");

    remove_file(path).ok();
}
