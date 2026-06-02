use narada_agent_tui::terminal_runtime_contract::terminal_runtime_contract;
use std::fs::{remove_file, write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

const RENDERED_SESSION_EVENTS: &str = r#"{"schema":"narada.carrier.session_event.v1","event_kind":"turn_started","event_id":"session_event_probe_001","occurred_at":"2026-05-30T18:29:00.000Z","carrier_session_id":"carrier_fixture_1","agent_id":"sonar.resident","site_id":"sonar","site_root":"D:/code/narada.sonar","payload":{"turn_id":"turn_probe_1","input_event_id":"input_probe_1","source_kind":"operator","source_id":"operator","content_preview":"run startup sequence"}}
{"schema":"narada.carrier.session_event.v1","event_kind":"provider_text_delta_recorded","event_id":"session_event_probe_002","occurred_at":"2026-05-30T18:29:01.000Z","carrier_session_id":"carrier_fixture_1","agent_id":"sonar.resident","site_id":"sonar","site_root":"D:/code/narada.sonar","payload":{"schema":"narada.agent_tui.provider_output_payload.v0","turn_id":"turn_probe_1","provider_output_kind":"text_delta","sequence":1,"text_delta":"I’ll run the startup sequence and report the scoped Site posture.","text_delta_ref":null}}
{"schema":"narada.carrier.session_event.v1","event_kind":"provider_tool_call_requested","event_id":"session_event_probe_003","occurred_at":"2026-05-30T18:29:02.000Z","carrier_session_id":"carrier_fixture_1","agent_id":"sonar.resident","site_id":"sonar","site_root":"D:/code/narada.sonar","payload":{"schema":"narada.agent_tui.provider_output_payload.v0","turn_id":"turn_probe_1","provider_output_kind":"tool_call_request","sequence":2,"tool_name":"agent_context_startup_sequence","arguments_summary":"{}","arguments_ref":null}}
{"schema":"narada.carrier.session_event.v1","event_kind":"tool_result_received","event_id":"session_event_probe_004","occurred_at":"2026-05-30T18:29:03.000Z","carrier_session_id":"carrier_fixture_1","agent_id":"sonar.resident","site_id":"sonar","site_root":"D:/code/narada.sonar","payload":{"tool_name":"agent_context_startup_sequence","status":"ok","duration_ms":12,"result_summary":"success · narada.agent_context.startup_sequence_result.v0","result_ref":null}}
{"schema":"narada.carrier.session_event.v1","event_kind":"turn_completed","event_id":"session_event_probe_005","occurred_at":"2026-05-30T18:29:04.000Z","carrier_session_id":"carrier_fixture_1","agent_id":"sonar.resident","site_id":"sonar","site_root":"D:/code/narada.sonar","payload":{"schema":"narada.agent_tui.turn_terminal_payload.v0","turn_id":"turn_probe_1","input_event_id":"input_probe_1","provider_request_status":"completed","terminal_status":"completed","provider_execution_enabled":true}}
"#;

fn base_command() -> Command {
    let contract = terminal_runtime_contract();
    let mut command = Command::new(env!("CARGO_BIN_EXE_narada-agent-tui"));
    command
        .arg("--identity")
        .arg("sonar.resident")
        .arg("--session")
        .arg("carrier_fixture_1")
        .arg("--site-root")
        .arg("D:/code/narada.sonar")
        .env_remove(&contract.terminal_rendering_env_var)
        .env_remove(&contract.terminal_mode_env_var);
    command
}

fn enable_terminal_rendering(command: &mut Command) {
    let contract = terminal_runtime_contract();
    command.env(&contract.terminal_rendering_env_var, "true");
}

fn configure_terminal_mode(command: &mut Command, mode: &str) {
    let contract = terminal_runtime_contract();
    command
        .env(&contract.terminal_rendering_env_var, "true")
        .env(&contract.terminal_mode_env_var, mode);
}

fn stdout(command: &mut Command) -> String {
    let output = command.output().expect("binary runs");
    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is utf8")
}

fn run(command: &mut Command) -> std::process::Output {
    command.output().expect("binary runs")
}

fn temp_path(name: &str) -> PathBuf {
    let unique = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "narada-agent-tui-terminal-runtime-{name}-{}-{unique}.jsonl",
        std::process::id()
    ))
}

fn strip_ansi(text: &str) -> String {
    let mut output = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            output.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('[') => {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next();
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' && chars.peek().copied() == Some('\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    output
}

#[test]
fn terminal_runtime_cli_acceptance_reports_disabled_by_default() {
    let output = stdout(&mut base_command());

    assert!(output.contains("terminal_status: disabled"));
    assert!(output.contains("terminal_rendering_enabled: false"));
    assert!(!output.contains("terminal_mode:"));
}

#[test]
fn terminal_runtime_cli_acceptance_reports_refusal_when_enabled_without_mode() {
    let mut command = base_command();
    enable_terminal_rendering(&mut command);

    let output = stdout(&mut command);

    assert!(output.contains("terminal_status: refused"));
    assert!(output.contains("terminal_rendering_enabled: false"));
    assert!(output.contains("terminal_refusal: missing_terminal_mode"));
}

#[test]
fn terminal_runtime_cli_acceptance_reports_refusal_for_unsupported_mode() {
    let mut command = base_command();
    configure_terminal_mode(&mut command, "render_once");

    let output = stdout(&mut command);

    assert!(output.contains("terminal_status: refused"));
    assert!(output.contains("terminal_rendering_enabled: false"));
    assert!(output.contains("terminal_refusal: unsupported_terminal_mode:render_once"));
}

#[test]
fn terminal_runtime_cli_acceptance_reports_configured_interactive_loop() {
    let mut command = base_command();
    configure_terminal_mode(
        &mut command,
        terminal_runtime_contract().required_terminal_mode.as_str(),
    );

    let output = stdout(&mut command);

    assert!(output.contains("terminal_status: configured"));
    assert!(output.contains("terminal_rendering_enabled: true"));
    assert!(output.contains("terminal_mode: interactive_loop"));
}

#[test]
fn terminal_runtime_cli_acceptance_interactive_loop_flag_admits_without_env_gate() {
    let control_path = temp_path("control");
    let session_path = temp_path("session");

    let mut command = base_command();
    command
        .arg("--control-jsonl")
        .arg(&control_path)
        .arg("--session-jsonl")
        .arg(&session_path)
        .arg("--interactive-loop")
        .arg("--max-steps")
        .arg("1");

    let output = run(&mut command);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf8");
    assert!(!stderr.contains("terminal_interactive_loop_not_admitted"));
    assert!(!stderr.contains("terminal_rendering_not_enabled"));
    assert!(stderr.contains("control_jsonl_open_failed"));
}

#[test]
fn terminal_runtime_cli_acceptance_renders_polished_transcript_frame_from_session_jsonl() {
    let control_path = temp_path("render-control");
    let session_path = temp_path("render-session");
    write(&control_path, "").expect("control jsonl writes");
    write(&session_path, RENDERED_SESSION_EVENTS).expect("session jsonl writes");

    let mut command = base_command();
    command
        .arg("--control-jsonl")
        .arg(&control_path)
        .arg("--session-jsonl")
        .arg(&session_path)
        .arg("--interactive-loop")
        .arg("--max-steps")
        .arg("1");

    let output = run(&mut command);

    remove_file(&control_path).ok();
    remove_file(&session_path).ok();

    assert!(
        output.status.success(),
        "process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf8");
    let rendered = strip_ansi(&stdout);

    assert!(rendered.contains("operator -> sonar.resident: run startup sequence"));
    assert!(rendered.contains("sonar.resident:"));
    assert!(rendered.contains("I’ll run the startup sequence"));
    assert!(rendered.contains("sonar.resident -> agent-tui: agent_context_startup_sequence({})"));
    assert!(
        rendered.contains("agent-tui -> sonar.resident: ok agent_context_startup_sequence in 12ms")
    );
    assert!(rendered.contains("narada.agent_context.startup_sequence_result.v0"));
    assert!(rendered.contains("agent-tui:"));
    assert!(rendered.contains("completed"));
    assert!(rendered.contains("2026-05-30Z18:29"));
    assert!(rendered.contains("terminal configured"));
    assert!(rendered.contains("operator -> sonar.resident>"));
}
