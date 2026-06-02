use narada_agent_tui::carrier_protocol::{SessionEventKind, parse_session_event};
use narada_agent_tui::smoke_runner::{
    AgentTuiSmokeSession, AgentTuiSmokeStepConfig, run_interactive_smoke_step,
};
use narada_agent_tui::transcript_store::TranscriptStore;
use std::fs::{OpenOptions, read_to_string, remove_file};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const CONTROL_FIXTURE: &str =
    include_str!("../../carrier-protocol/fixtures/control-input-event.json");
static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path(name: &str) -> PathBuf {
    let unique = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "narada-agent-tui-runtime-acceptance-{name}-{}-{unique}.jsonl",
        std::process::id()
    ))
}

fn append(path: &Path, content: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("open temp file");
    file.write_all(content.as_bytes())
        .expect("append temp file");
}

fn smoke_config(
    control_path: PathBuf,
    session_path: PathBuf,
    composer_has_draft: bool,
) -> AgentTuiSmokeStepConfig {
    AgentTuiSmokeStepConfig {
        identity: "sonar.resident".to_string(),
        session: "carrier_fixture_1".to_string(),
        site_root: PathBuf::from("D:/code/narada.sonar"),
        control_jsonl: control_path,
        session_jsonl: session_path,
        composer_has_draft,
    }
}

fn operator_control_event(id: u32, content: &str) -> String {
    format!(
        r#"{{"schema":"narada.carrier.control.input_event.v1","control_event_id":"control_operator_{id}","input_event_id":"input_operator_{id}","written_at":"2026-05-30T00:00:0{id}.000Z","input":{{"schema":"narada.carrier.input_event.v1","event_id":"input_operator_{id}","source_kind":"operator","source_id":"operator","transport":"control_jsonl","delivery_mode":"admit_after_active_turn","hold_condition":null,"content":"{content}","created_at":"2026-05-30T00:00:0{id}.000Z","authority_ref":null,"directive_id":null,"metadata":{{}}}}}}"#
    )
}

#[test]
fn smoke_runner_runs_against_control_fixture_without_terminal() {
    let control_path = temp_path("control");
    let session_path = temp_path("session");
    append(&control_path, CONTROL_FIXTURE);
    append(&control_path, "\n");

    let result = run_interactive_smoke_step(&smoke_config(
        control_path.clone(),
        session_path.clone(),
        false,
    ))
    .expect("interactive smoke step succeeds");

    assert_eq!(result.control_evidence_written, 1);
    assert_eq!(result.parse_errors, 0);
    assert!(result.completed_turn.is_some());
    assert_eq!(result.transcript.total_items, 2);

    let mut transcript = TranscriptStore::new();
    transcript
        .ingest_jsonl_file_summary(&session_path)
        .expect("session transcript ingests");
    assert_eq!(transcript.items().len(), 2);
    assert_eq!(transcript.items()[0].actor.as_str(), "system");
    assert_eq!(transcript.items()[0].text, "run startup sequence");
    assert_eq!(transcript.items()[1].actor.as_str(), "agent-tui");
    assert_eq!(transcript.items()[1].text, "completed_without_provider");

    remove_file(control_path).ok();
    remove_file(session_path).ok();
}

#[test]
fn persistent_smoke_session_holds_system_directive_until_composer_clear() {
    let control_path = temp_path("held-control");
    let session_path = temp_path("held-session");
    append(&control_path, CONTROL_FIXTURE);
    append(&control_path, "\n");

    let mut session = AgentTuiSmokeSession::new(&smoke_config(
        control_path.clone(),
        session_path.clone(),
        false,
    ))
    .expect("smoke session initializes");

    let held = session.run_step(true).expect("held smoke step succeeds");

    assert_eq!(held.control_evidence_written, 1);
    assert_eq!(held.released_held, 0);
    assert!(held.completed_turn.is_none());
    assert_eq!(held.transcript.total_items, 1);

    let released = session
        .run_step(false)
        .expect("released smoke step succeeds");

    assert_eq!(released.control_evidence_written, 0);
    assert_eq!(released.released_held, 1);
    assert!(released.completed_turn.is_some());
    assert_eq!(released.transcript.total_items, 4);

    let mut transcript = TranscriptStore::new();
    transcript
        .ingest_jsonl_file_summary(&session_path)
        .expect("session transcript ingests");
    assert_eq!(transcript.items().len(), 4);
    assert_eq!(transcript.items()[0].text, "held input_fixture_system_1");
    assert_eq!(
        transcript.items()[1].text,
        "released input_fixture_system_1"
    );
    assert_eq!(transcript.items()[2].text, "run startup sequence");
    assert_eq!(transcript.items()[3].text, "completed_without_provider");

    remove_file(control_path).ok();
    remove_file(session_path).ok();
}

#[test]
fn persistent_smoke_session_preserves_queued_operator_input_order() {
    let control_path = temp_path("queued-operator-control");
    let session_path = temp_path("queued-operator-session");
    append(
        &control_path,
        &operator_control_event(1, "first operator note"),
    );
    append(&control_path, "\n");
    append(
        &control_path,
        &operator_control_event(2, "second operator note"),
    );
    append(&control_path, "\n");

    let mut session = AgentTuiSmokeSession::new(&smoke_config(
        control_path.clone(),
        session_path.clone(),
        false,
    ))
    .expect("smoke session initializes");

    let first = session
        .run_step(false)
        .expect("first queued operator smoke step succeeds");
    assert_eq!(first.control_evidence_written, 2);
    assert_eq!(first.parse_errors, 0);
    assert!(first.completed_turn.is_some());
    assert_eq!(first.transcript.total_items, 2);

    let second = session
        .run_step(false)
        .expect("second queued operator smoke step succeeds");
    assert_eq!(second.control_evidence_written, 0);
    assert_eq!(second.parse_errors, 0);
    assert!(second.completed_turn.is_some());
    assert_eq!(second.transcript.total_items, 4);

    let mut transcript = TranscriptStore::new();
    transcript
        .ingest_jsonl_file_summary(&session_path)
        .expect("session transcript ingests");
    assert_eq!(transcript.items().len(), 4);
    assert_eq!(transcript.items()[0].actor.as_str(), "operator");
    assert_eq!(transcript.items()[0].text, "first operator note");
    assert_eq!(transcript.items()[1].actor.as_str(), "agent-tui");
    assert_eq!(transcript.items()[1].text, "completed_without_provider");
    assert_eq!(transcript.items()[2].actor.as_str(), "operator");
    assert_eq!(transcript.items()[2].text, "second operator note");
    assert_eq!(transcript.items()[3].actor.as_str(), "agent-tui");
    assert_eq!(transcript.items()[3].text, "completed_without_provider");

    remove_file(control_path).ok();
    remove_file(session_path).ok();
}

#[test]
fn persistent_smoke_session_records_interrupt_without_transcript_rows() {
    let control_path = temp_path("interrupt-control");
    let session_path = temp_path("interrupt-session");
    append(&control_path, "");

    let mut session = AgentTuiSmokeSession::new(&smoke_config(
        control_path.clone(),
        session_path.clone(),
        false,
    ))
    .expect("smoke session initializes");

    let summary = session
        .record_interrupt()
        .expect("interrupt evidence records");
    assert_eq!(summary.projected, 0);
    assert_eq!(summary.ignored, 1);
    assert_eq!(summary.total_items, 0);

    let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
    let lines: Vec<&str> = session_jsonl.lines().collect();
    assert_eq!(lines.len(), 1);
    let event = parse_session_event(lines[0]).expect("interrupt event parses");
    assert_eq!(event.event_kind, SessionEventKind::InterruptRequested);
    assert_eq!(event.payload["source_kind"], "operator");
    assert_eq!(event.payload["transport"], "interactive_terminal");
    assert_eq!(event.payload["reason"], "composer_interrupt");

    let mut transcript = TranscriptStore::new();
    transcript
        .ingest_jsonl_file_summary(&session_path)
        .expect("session transcript ingests");
    assert!(transcript.items().is_empty());

    remove_file(control_path).ok();
    remove_file(session_path).ok();
}

#[test]
fn persistent_smoke_session_reports_malformed_control_without_transcript_rows() {
    let control_path = temp_path("malformed-control");
    let session_path = temp_path("malformed-session");
    append(&control_path, "{not valid json}\n");

    let mut session = AgentTuiSmokeSession::new(&smoke_config(
        control_path.clone(),
        session_path.clone(),
        false,
    ))
    .expect("smoke session initializes");

    let result = session
        .run_step(false)
        .expect("malformed control smoke step succeeds");

    assert_eq!(result.control_evidence_written, 0);
    assert_eq!(result.parse_errors, 1);
    assert_eq!(result.released_held, 0);
    assert!(result.completed_turn.is_none());
    assert_eq!(result.transcript.total_items, 0);

    let mut transcript = TranscriptStore::new();
    let summary = transcript
        .ingest_jsonl_file_summary(&session_path)
        .expect("empty session transcript ingests");
    assert_eq!(summary.total_items, 0);
    assert!(transcript.items().is_empty());
    assert_eq!(read_to_string(&session_path).unwrap_or_default(), "");

    remove_file(control_path).ok();
    remove_file(session_path).ok();
}

#[test]
fn persistent_smoke_session_recovers_after_malformed_control_line() {
    let control_path = temp_path("malformed-recovery-control");
    let session_path = temp_path("malformed-recovery-session");
    append(&control_path, "{not valid json}\n");

    let mut session = AgentTuiSmokeSession::new(&smoke_config(
        control_path.clone(),
        session_path.clone(),
        false,
    ))
    .expect("smoke session initializes");

    let malformed = session
        .run_step(false)
        .expect("malformed control smoke step succeeds");
    assert_eq!(malformed.parse_errors, 1);
    assert!(malformed.completed_turn.is_none());

    append(&control_path, CONTROL_FIXTURE);
    append(&control_path, "\n");

    let recovered = session
        .run_step(false)
        .expect("valid control after malformed line succeeds");
    assert_eq!(recovered.control_evidence_written, 1);
    assert_eq!(recovered.parse_errors, 0);
    assert!(recovered.completed_turn.is_some());

    let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
    assert!(session_jsonl.contains("\"source_kind\":\"system\""));
    assert!(session_jsonl.contains("run startup sequence"));
    assert!(session_jsonl.contains("\"provider_request_status\":\"recorded_not_dispatched\""));
    assert!(!session_jsonl.contains("\"event_kind\":\"provider_tool_call_requested\""));
    assert!(!session_jsonl.contains("\"event_kind\":\"tool_call_requested\""));
    assert!(!session_jsonl.contains("\"event_kind\":\"tool_result_received\""));

    let mut transcript = TranscriptStore::new();
    transcript
        .ingest_jsonl_file_summary(&session_path)
        .expect("session transcript ingests");
    assert_eq!(transcript.items()[0].actor.as_str(), "system");
    assert_eq!(transcript.items()[0].text, "run startup sequence");
    assert_eq!(transcript.items()[1].actor.as_str(), "agent-tui");
    assert_eq!(transcript.items()[1].text, "completed_without_provider");

    remove_file(control_path).ok();
    remove_file(session_path).ok();
}
