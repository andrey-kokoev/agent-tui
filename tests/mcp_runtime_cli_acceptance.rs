use narada_agent_tui::mcp_runtime_contract::mcp_runtime_contract;
use std::fs::{create_dir_all, remove_dir_all, write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_FABRIC_COUNTER: AtomicU64 = AtomicU64::new(0);

fn base_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_narada-agent-tui"));
    command
        .arg("--identity")
        .arg("sonar.resident")
        .arg("--session")
        .arg("carrier_fixture_1")
        .arg("--site-root")
        .arg("D:/code/narada.sonar")
        .env_remove(&mcp_runtime_contract().mcp_fabric_env_var)
        .env_remove(&mcp_runtime_contract().mcp_config_env_var)
        .env_remove(&mcp_runtime_contract().site_mcp_fabric_env_var);
    command
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

fn temp_mcp_fabric() -> PathBuf {
    let unique = TEMP_FABRIC_COUNTER.fetch_add(1, Ordering::Relaxed);
    let fabric = std::env::temp_dir().join(format!(
        "narada-agent-tui-mcp-{}-{unique}",
        std::process::id()
    ));
    create_dir_all(&fabric).expect("create temp fabric");
    fabric
}

fn path_string(path: &std::path::Path) -> String {
    path.display().to_string()
}

#[test]
fn mcp_runtime_cli_acceptance_reports_disabled_by_default() {
    let output = stdout(&mut base_command());

    assert!(output.contains("mcp_status: disabled"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(!output.contains("mcp_config:"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_when_enabled_without_config() {
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            "D:/code/narada.sonar/.ai/mcp",
        );

    let output = stdout(&mut command);

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: missing_mcp_config"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_config_outside_site_fabric() {
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            "D:/other/.ai/mcp/agent-tui.json",
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            "D:/code/narada.sonar/.ai/mcp",
        );

    let output = stdout(&mut command);

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_outside_site_mcp_fabric"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_parent_traversal_config_path() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("..").join("outside-agent-tui.json");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_outside_site_mcp_fabric"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_unreadable_mcp_config() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("missing-agent-tui.json");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_unreadable"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_unparsable_mcp_config() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(&config_path, "not json").expect("write invalid temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_parse_failed"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_non_object_mcp_config_root() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(&config_path, "[]").expect("write non-object root temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_config_shape_invalid"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_non_object_mcp_servers() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(&config_path, "{\"mcpServers\":[]}").expect("write invalid mcpServers temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(
        output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_config_mcp_servers_invalid")
    );
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_non_object_mcp_server_record() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(&config_path, "{\"mcpServers\":{\"site\":[]}}")
        .expect("write invalid mcp server record temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(
        output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_record_invalid:site")
    );
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_mcp_config_without_servers() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(&config_path, "{}").expect("write incomplete temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_missing_mcp_servers"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_non_string_mcp_config_site_id() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"site_id\":1,\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write non-string site id temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_config_site_id_invalid"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_blank_mcp_config_site_id() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"site_id\":\" \",\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write blank site id temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_config_site_id_invalid"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_non_string_mcp_config_carrier() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"carrier\":1,\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write non-string carrier temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_config_carrier_invalid"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_blank_mcp_server_name() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\" \":{\"transport\":\"stdio\",\"command\":\"node\",\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write blank server name temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_name_invalid"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_non_string_mcp_transport() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":1,\"command\":\"node\",\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write non-string transport temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_transport_invalid:site"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_blank_mcp_transport() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\" \",\"command\":\"node\",\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write blank transport temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_transport_invalid:site"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_non_string_mcp_server_command() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":1,\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write non-string command temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(
        output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_command_invalid:site")
    );
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_blank_mcp_server_command() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\" \",\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write blank command temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(
        output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_command_invalid:site")
    );
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_non_string_mcp_target_site_root() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"tools\":[\"site_loop_status\"],\"target_site_root\":1}}}",
    )
    .expect("write non-string target root temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains(
        "mcp_refusal: mcp_config_invalid:mcp_fabric_server_target_site_root_invalid:site"
    ));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_blank_mcp_target_site_root() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"tools\":[\"site_loop_status\"],\"target_site_root\":\" \"}}}",
    )
    .expect("write blank target root temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains(
        "mcp_refusal: mcp_config_invalid:mcp_fabric_server_target_site_root_invalid:site"
    ));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_invalid_mcp_server_env_shape() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"env\":[],\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write invalid env temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_env_invalid:site"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_invalid_mcp_server_env_vars_shape() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"env_vars\":{},\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write invalid env_vars temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(
        output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_env_vars_invalid:site")
    );
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_invalid_mcp_server_env_var() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"env_vars\":[1],\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write invalid env var temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(
        output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_env_var_invalid:site")
    );
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_invalid_mcp_server_args_shape() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"args\":\"site-loop.mjs\",\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write invalid args temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_args_invalid:site"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_non_string_mcp_server_arg() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"args\":[1],\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write non-string arg temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_arg_invalid:site"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_blank_mcp_server_arg() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"args\":[\" \"],\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write blank arg temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_arg_invalid:site"));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_ambiguous_tool_list_fields() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"tools\":[\"site_loop_status\"],\"allowed_tools\":[\"site_loop_run_once\"]}}}",
    )
    .expect("write ambiguous tools temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(
        output
            .contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_tool_list_ambiguous:site")
    );
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_invalid_tool_list_shape() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"tools\":\"site_loop_status\"}}}",
    )
    .expect("write invalid tool list temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(output.contains(
        "mcp_refusal: mcp_config_invalid:mcp_fabric_server_tool_list_invalid:site:tools"
    ));
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_mcp_server_without_tools() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\"}}}",
    )
    .expect("write missing tools temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(
        output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_tools_missing:site")
    );
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_blank_mcp_tool_name() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"tools\":[\" \" ]}}}",
    )
    .expect("write blank tool temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(
        output.contains("mcp_refusal: mcp_config_invalid:mcp_fabric_server_tool_name_invalid:site")
    );
}

#[test]
fn mcp_runtime_cli_acceptance_reports_refusal_for_mcp_config_invalid_transport_shape() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"http\",\"command\":\"node\"}}}",
    )
    .expect("write invalid transport temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: refused"));
    assert!(output.contains("mcp_fabric_access_enabled: false"));
    assert!(
        output
            .contains("mcp_refusal: mcp_config_invalid:mcp_fabric_transport_unsupported:site:http")
    );
}

#[test]
fn mcp_runtime_cli_acceptance_reports_configured_explicit_mcp_posture() {
    let fabric = temp_mcp_fabric();
    let config_path = fabric.join("agent-tui.json");
    write(
        &config_path,
        "{\"mcpServers\":{\"site\":{\"transport\":\"stdio\",\"command\":\"node\",\"tools\":[\"site_loop_status\"]}}}",
    )
    .expect("write temp mcp config");
    let mut command = base_command();
    command
        .env(&mcp_runtime_contract().mcp_fabric_env_var, "true")
        .env(
            &mcp_runtime_contract().mcp_config_env_var,
            path_string(&config_path),
        )
        .env(
            &mcp_runtime_contract().site_mcp_fabric_env_var,
            path_string(&fabric),
        );

    let output = stdout(&mut command);
    remove_dir_all(&fabric).expect("remove temp fabric");

    assert!(output.contains("mcp_status: configured"));
    assert!(output.contains("mcp_fabric_access_enabled: true"));
    assert!(output.contains(&format!(
        "mcp_config_path_policy: {}",
        narada_agent_tui::mcp_runtime_config::config_path_policy()
    )));
    assert!(output.contains(&format!("mcp_config: {}", path_string(&config_path))));
    assert!(output.contains(&format!("site_mcp_fabric: {}", path_string(&fabric))));
}
