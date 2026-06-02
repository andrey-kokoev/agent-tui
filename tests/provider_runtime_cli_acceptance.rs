use narada_agent_tui::provider_adapter_contract::provider_adapter_contract;
use std::fs::{read_to_string, remove_file, write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const CONTROL_FIXTURE: &str =
    include_str!("../../carrier-protocol/fixtures/control-input-event.json");
static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

fn admitted_provider() -> &'static str {
    provider_adapter_contract()
        .admitted_providers
        .first()
        .expect("provider contract has at least one admitted provider")
        .as_str()
}

fn base_command() -> Command {
    let contract = provider_adapter_contract();
    let mut command = Command::new(env!("CARGO_BIN_EXE_narada-agent-tui"));
    command
        .arg("--identity")
        .arg("sonar.resident")
        .arg("--session")
        .arg("carrier_fixture_1")
        .arg("--site-root")
        .arg("D:/code/narada.sonar")
        .env_remove(&contract.provider_execution_env_var)
        .env_remove(&contract.intelligence_provider_env_var)
        .env_remove(&contract.ai_model_env_var)
        .env_remove(&contract.ai_thinking_env_var)
        .env_remove(&contract.ai_stream_env_var)
        .env_remove(&contract.provider_adapter_kind_env_var);
    command
}

fn with_provider_env(command: &mut Command, pairs: &[(&str, &str)]) {
    let contract = provider_adapter_contract();
    for (semantic_key, value) in pairs {
        let env_key = match *semantic_key {
            "execution_enabled" => &contract.provider_execution_env_var,
            "provider" => &contract.intelligence_provider_env_var,
            "model" => &contract.ai_model_env_var,
            "thinking" => &contract.ai_thinking_env_var,
            "stream" => &contract.ai_stream_env_var,
            "adapter_kind" => &contract.provider_adapter_kind_env_var,
            unexpected => panic!("unknown provider runtime env semantic key: {unexpected}"),
        };
        command.env(env_key, value);
    }
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

fn temp_path(name: &str) -> PathBuf {
    let unique = TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "narada-agent-tui-provider-runtime-{name}-{}-{unique}.jsonl",
        std::process::id()
    ))
}

#[test]
fn provider_runtime_cli_acceptance_reports_disabled_by_default() {
    let output = stdout(&mut base_command());

    assert!(output.contains("provider_status: disabled"));
    assert!(output.contains("provider_execution_enabled: false"));
    assert!(output.contains("stream: off"));
    assert!(!output.contains(&format!("provider: {}", admitted_provider())));
}

#[test]
fn provider_runtime_cli_acceptance_reports_refusal_when_enabled_without_model() {
    let mut command = base_command();
    with_provider_env(
        &mut command,
        &[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
        ],
    );

    let output = stdout(&mut command);

    assert!(output.contains("provider_status: refused"));
    assert!(output.contains("provider_execution_enabled: false"));
    assert!(output.contains("provider_refusal: missing_model"));
}

#[test]
fn provider_runtime_cli_acceptance_reports_configured_without_execution_adapter() {
    let mut command = base_command();
    with_provider_env(
        &mut command,
        &[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
            ("thinking", "medium"),
            ("stream", "false"),
        ],
    );

    let output = stdout(&mut command);

    assert!(output.contains("provider_status: configured"));
    assert!(output.contains("provider_execution_enabled: false"));
    assert!(!output.contains("provider_refusal:"));
    assert!(output.contains("provider_adapter_status: configured_without_adapter"));
    assert!(output.contains("provider_adapter_execution_enabled: false"));
    assert!(output.contains("provider_adapter_refusal: provider_adapter_not_admitted"));
    assert!(output.contains(&format!("provider: {}", admitted_provider())));
    assert!(output.contains("model: gpt-5.5"));
    assert!(output.contains("thinking: medium"));
    assert!(output.contains("stream: off"));
}

#[test]
fn provider_runtime_cli_acceptance_reports_unknown_adapter_as_refused() {
    let mut command = base_command();
    with_provider_env(
        &mut command,
        &[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
            ("adapter_kind", "unknown_adapter"),
        ],
    );

    let output = stdout(&mut command);

    assert!(output.contains("provider_status: configured"));
    assert!(output.contains("provider_execution_enabled: false"));
    assert!(output.contains("provider_adapter_status: refused"));
    assert!(output.contains("provider_adapter_execution_enabled: false"));
    assert!(output.contains("provider_adapter_kind: unknown_adapter"));
    assert!(output.contains("provider_adapter_refusal: unknown_provider_adapter:unknown_adapter"));
}

#[test]
fn provider_runtime_cli_acceptance_reports_requested_adapter_as_admitted_when_implemented() {
    let mut command = base_command();
    let contract = provider_adapter_contract();
    with_provider_env(
        &mut command,
        &[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
            (
                "adapter_kind",
                contract.production_provider_adapter_kind.as_str(),
            ),
        ],
    );

    let output = stdout(&mut command);

    assert!(output.contains("provider_status: configured"));
    assert!(output.contains("provider_adapter_status: admitted"));
    assert!(output.contains("provider_adapter_execution_enabled: true"));
    assert!(output.contains(&format!(
        "provider_adapter_kind: {}",
        contract.production_provider_adapter_kind
    )));
    assert!(!output.contains("provider_adapter_refusal:"));
}

fn assert_configured_provider_posture_recorded(session_jsonl: &str) {
    assert_configured_provider_posture_recorded_with_adapter(
        session_jsonl,
        "configured_without_adapter",
        "\"provider_adapter_kind\":null",
        "provider_adapter_not_admitted",
    );
}

fn assert_configured_provider_posture_recorded_with_adapter(
    session_jsonl: &str,
    adapter_status: &str,
    adapter_kind_fragment: &str,
    refusal_reason: &str,
) {
    assert!(session_jsonl.contains("\"provider_request_status\":\"recorded_not_dispatched\""));
    assert!(session_jsonl.contains("\"provider_runtime_status\":\"configured\""));
    assert!(session_jsonl.contains(&format!("\"provider\":\"{}\"", admitted_provider())));
    assert!(session_jsonl.contains("\"model\":\"gpt-5.5\""));
    assert!(session_jsonl.contains(&format!(
        "\"provider_adapter_admission_status\":\"{adapter_status}\""
    )));
    assert!(session_jsonl.contains(adapter_kind_fragment));
    assert!(session_jsonl.contains(&format!(
        "\"provider_adapter_refusal_reason\":\"{refusal_reason}\""
    )));
}

#[test]
fn provider_runtime_cli_acceptance_records_runtime_posture_in_runtime_step_evidence() {
    let control_path = temp_path("control");
    let session_path = temp_path("session");
    write(&control_path, format!("{CONTROL_FIXTURE}\n")).expect("control fixture writes");

    let mut command = base_command();
    command
        .arg("--control-jsonl")
        .arg(&control_path)
        .arg("--session-jsonl")
        .arg(&session_path)
        .arg("--runtime-step-once");
    with_provider_env(
        &mut command,
        &[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ],
    );

    let output = stdout(&mut command);
    assert!(output.contains("runtime_step_once: ok"));

    let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
    assert_configured_provider_posture_recorded(&session_jsonl);

    remove_file(control_path).ok();
    remove_file(session_path).ok();
}

#[test]
fn provider_runtime_cli_acceptance_records_production_adapter_admission_in_runtime_step_evidence() {
    let control_path = temp_path("control");
    let session_path = temp_path("session");
    write(&control_path, format!("{CONTROL_FIXTURE}\n")).expect("control fixture writes");

    let mut command = base_command();
    let contract = provider_adapter_contract();
    command
        .arg("--control-jsonl")
        .arg(&control_path)
        .arg("--session-jsonl")
        .arg(&session_path)
        .arg("--runtime-step-once");
    with_provider_env(
        &mut command,
        &[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
            (
                "adapter_kind",
                contract.production_provider_adapter_kind.as_str(),
            ),
        ],
    );
    command.env(
        "NARADA_AGENT_TUI_CODEX_COMMAND",
        "definitely-missing-codex-fixture",
    );

    let output = stdout(&mut command);
    assert!(output.contains("runtime_step_once: ok"));

    let adapter_kind_fragment = format!(
        "\"provider_adapter_kind\":\"{}\"",
        contract.production_provider_adapter_kind
    );
    let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
    assert!(session_jsonl.contains("\"provider_request_status\":\"failed\""));
    assert!(session_jsonl.contains("\"provider_execution_enabled\":true"));
    assert!(session_jsonl.contains("\"provider_adapter_admission_status\":\"admitted\""));
    assert!(session_jsonl.contains(&adapter_kind_fragment));
    assert!(session_jsonl.contains("provider dispatch failed:"));

    remove_file(control_path).ok();
    remove_file(session_path).ok();
}

#[test]
fn provider_runtime_cli_acceptance_records_runtime_posture_in_interactive_step_evidence() {
    let control_path = temp_path("control");
    let session_path = temp_path("session");
    write(&control_path, format!("{CONTROL_FIXTURE}\n")).expect("control fixture writes");

    let mut command = base_command();
    command
        .arg("--control-jsonl")
        .arg(&control_path)
        .arg("--session-jsonl")
        .arg(&session_path)
        .arg("--interactive-step-once");
    with_provider_env(
        &mut command,
        &[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ],
    );

    let output = stdout(&mut command);
    assert!(output.contains("interactive_step_once: ok"));

    let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
    assert_configured_provider_posture_recorded(&session_jsonl);

    remove_file(control_path).ok();
    remove_file(session_path).ok();
}
