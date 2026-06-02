use narada_agent_tui::app_view_model::{AppViewInput, AppViewModel, build_app_view};
use narada_agent_tui::composer_view_model::ComposerViewInput;
use narada_agent_tui::input_queue::{SessionEvidenceContext, TurnState};
use narada_agent_tui::interactive_runtime::AgentTuiInteractiveRuntime;
use narada_agent_tui::launch_slice_contract::launch_slice_contract;
use narada_agent_tui::layout_model::{LayoutConfig, TerminalSize};
use narada_agent_tui::provider_dispatch::provider_adapter_from_runtime_config;
use narada_agent_tui::provider_tool_call_bridge::provider_tool_call_executor_from_mcp_runtime_config;
use narada_agent_tui::runtime_clock::RuntimeClock;
use narada_agent_tui::runtime_config_snapshot::RuntimeConfigSnapshot;
use narada_agent_tui::runtime_step::RuntimeStep;
use narada_agent_tui::smoke_runner::{
    AgentTuiSmokeSession, AgentTuiSmokeStepConfig, interactive_smoke_step_summary_lines,
    run_interactive_smoke_step_with_provider_runtime_config_and_adapter_admission,
};
use narada_agent_tui::status_view_model::StatusViewInput;
use narada_agent_tui::terminal_input_tick::CrosstermTerminalInputReader;
use narada_agent_tui::terminal_lifecycle::TerminalSession;
use narada_agent_tui::terminal_runtime_config::TerminalRuntimeConfig;
use narada_agent_tui::tui_render_loop::{
    AgentTuiLoopState, RuntimeClockInteractiveSource, TerminalInputTickSource,
    run_injected_interactive_loop,
};
use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const INTERACTIVE_INPUT_IDLE_WAIT_MS: u64 = 25;

#[derive(Debug, Default, PartialEq, Eq)]
struct Args {
    identity: Option<String>,
    session: Option<String>,
    site_root: Option<PathBuf>,
    control_jsonl: Option<PathBuf>,
    session_jsonl: Option<PathBuf>,
    runtime_step_once: bool,
    runtime_loop: bool,
    interactive_step_once: bool,
    interactive_smoke_loop: bool,
    interactive_loop: bool,
    render_once: bool,
    max_steps: Option<u64>,
    composer_has_draft: bool,
    persistent_smoke_session: bool,
    check_rust_toolchain: bool,
    help: bool,
    version: bool,
}

fn main() {
    match parse_args(env::args().skip(1)) {
        Ok(args) => {
            if args.help {
                print_help();
                return;
            }
            if args.version {
                println!("narada-agent-tui {VERSION}");
                return;
            }
            if args.check_rust_toolchain {
                std::process::exit(run_rust_toolchain_check());
            }
            if let Err(message) = validate_launch_args(&args) {
                eprintln!("narada-agent-tui: {message}");
                eprintln!("Try --help for usage.");
                std::process::exit(2);
            }
            if let Err(message) = run(args) {
                eprintln!("narada-agent-tui: {message}");
                std::process::exit(1);
            }
        }
        Err(message) => {
            eprintln!("narada-agent-tui: {message}");
            eprintln!("Try --help for usage.");
            std::process::exit(2);
        }
    }
}

fn run(args: Args) -> Result<(), String> {
    if args.runtime_step_once {
        run_runtime_step_once(args)
    } else if args.runtime_loop {
        run_runtime_loop(args)
    } else if args.interactive_step_once {
        run_interactive_step_once(args)
    } else if args.interactive_smoke_loop {
        run_interactive_smoke_loop(args)
    } else if args.interactive_loop {
        run_interactive_loop(args)
    } else if args.render_once {
        run_render_once(args)
    } else {
        print_scaffold(&args);
        Ok(())
    }
}

fn run_rust_toolchain_check() -> i32 {
    let cargo = find_executable("cargo");
    let linker = find_executable("link");
    let ready = cargo.is_some() && linker.is_some();

    println!("schema: narada.agent_tui.rust_toolchain_readiness.v0");
    println!("status: {}", if ready { "ready" } else { "blocked" });
    println!(
        "cargo: {}",
        cargo
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "not_found".to_string())
    );
    println!(
        "msvc_linker: {}",
        linker
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "not_found".to_string())
    );
    if !ready {
        println!("next_check: where.exe link");
        println!(
            "recovery: install or load Visual Studio Build Tools C++ workload, then rerun pnpm agent-tui:test from the repo root"
        );
    }

    if ready { 0 } else { 1 }
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        for candidate in executable_candidates(name) {
            let path = dir.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn executable_candidates(name: &str) -> Vec<String> {
    if Path::new(name).extension().is_some() {
        return vec![name.to_string()];
    }
    if cfg!(windows) {
        vec![
            format!("{name}.exe"),
            format!("{name}.cmd"),
            format!("{name}.bat"),
            name.to_string(),
        ]
    } else {
        vec![name.to_string()]
    }
}

fn print_scaffold(args: &Args) {
    let runtime_config = runtime_config_snapshot_from_process_env();
    let provider_config = &runtime_config.provider;
    let provider_adapter = &runtime_config.provider_adapter;
    let mcp_config = &runtime_config.mcp;
    let terminal_config = &runtime_config.terminal;
    println!("narada-agent-tui scaffold");
    println!("identity: {}", args.identity.as_deref().unwrap_or(""));
    println!("session: {}", args.session.as_deref().unwrap_or(""));
    println!(
        "site_root: {}",
        args.site_root
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    if let Some(path) = &args.control_jsonl {
        println!("control_jsonl: {}", path.display());
    }
    if let Some(path) = &args.session_jsonl {
        println!("session_jsonl: {}", path.display());
    }
    println!("provider_status: {}", provider_config.status.as_str());
    println!(
        "provider_execution_enabled: {}",
        provider_adapter.provider_execution_enabled
    );
    if let Some(provider) = &provider_config.provider {
        println!("provider: {provider}");
    }
    if let Some(model) = &provider_config.model {
        println!("model: {model}");
    }
    if let Some(thinking) = &provider_config.thinking {
        println!("thinking: {thinking}");
    }
    println!(
        "stream: {}",
        if provider_config.stream { "on" } else { "off" }
    );
    if let Some(reason) = &provider_config.refusal_reason {
        println!("provider_refusal: {reason}");
    }
    println!(
        "provider_adapter_status: {}",
        provider_adapter.status.as_str()
    );
    println!(
        "provider_adapter_execution_enabled: {}",
        provider_adapter.provider_execution_enabled
    );
    if let Some(adapter_kind) = &provider_adapter.adapter_kind {
        println!("provider_adapter_kind: {adapter_kind}");
    }
    if let Some(reason) = &provider_adapter.refusal_reason {
        println!("provider_adapter_refusal: {reason}");
    }
    println!("mcp_status: {}", mcp_config.status.as_str());
    println!(
        "mcp_fabric_access_enabled: {}",
        mcp_config.mcp_fabric_access_enabled
    );
    println!("mcp_config_path_policy: {}", mcp_config.config_path_policy);
    if let Some(config_path) = &mcp_config.config_path {
        println!("mcp_config: {config_path}");
    }
    if let Some(site_mcp_fabric) = &mcp_config.site_mcp_fabric {
        println!("site_mcp_fabric: {site_mcp_fabric}");
    }
    if let Some(reason) = &mcp_config.refusal_reason {
        println!("mcp_refusal: {reason}");
    }
    println!("terminal_status: {}", terminal_config.status.as_str());
    println!(
        "terminal_rendering_enabled: {}",
        terminal_config.terminal_rendering_enabled
    );
    if let Some(mode) = &terminal_config.mode {
        println!("terminal_mode: {mode}");
    }
    if let Some(reason) = &terminal_config.refusal_reason {
        println!("terminal_refusal: {reason}");
    }
}

fn runtime_config_snapshot_from_process_env() -> RuntimeConfigSnapshot {
    let env_map = env::vars()
        .filter(|(key, _)| key.starts_with("NARADA_"))
        .collect::<BTreeMap<_, _>>();
    RuntimeConfigSnapshot::from_process_env_map(&env_map)
}

fn runtime_config_snapshot_for_explicit_terminal_mode(mode: &str) -> RuntimeConfigSnapshot {
    let mut snapshot = runtime_config_snapshot_from_process_env();
    snapshot.terminal = TerminalRuntimeConfig::configured_for_explicit_terminal_mode(mode);
    snapshot
}

fn terminal_config_for_explicit_terminal_mode(mode: &str) -> TerminalRuntimeConfig {
    runtime_config_snapshot_for_explicit_terminal_mode(mode).terminal
}

fn run_runtime_step_once(args: Args) -> Result<(), String> {
    let composer_has_draft = args.composer_has_draft;
    let mut step = build_runtime_step(&args)?;
    let result = step.run_once(composer_has_draft)?;

    println!("runtime_step_once: ok");
    print_runtime_step_summary(1, &result);
    Ok(())
}

fn run_runtime_loop(args: Args) -> Result<(), String> {
    let max_steps = args.max_steps.expect("validated max steps");
    let composer_has_draft = args.composer_has_draft;
    let mut step = build_runtime_step(&args)?;
    let mut total_parse_errors = 0usize;
    let mut total_evidence_written = 0usize;
    let mut completed_turns = 0usize;
    let mut transcript_projected = 0usize;
    let mut transcript_ignored = 0usize;
    let mut transcript_duplicate = 0usize;
    let mut transcript_total_items = 0usize;

    for step_index in 1..=max_steps {
        let result = step.run_once(composer_has_draft)?;
        total_parse_errors += result.poll.parse_errors.len();
        total_evidence_written += result.poll.evidence_written + result.released_held;
        transcript_projected += result.transcript.projected;
        transcript_ignored += result.transcript.ignored;
        transcript_duplicate += result.transcript.duplicate;
        transcript_total_items = result.transcript.total_items;
        if let Some(turn) = &result.completed_turn {
            completed_turns += 1;
            total_evidence_written += turn.evidence_written;
        }
        print_runtime_step_summary(step_index, &result);
    }

    println!("runtime_loop: ok");
    println!("steps: {max_steps}");
    println!("total_parse_errors: {total_parse_errors}");
    println!("total_evidence_written: {total_evidence_written}");
    println!("completed_turns: {completed_turns}");
    println!("transcript_projected: {transcript_projected}");
    println!("transcript_ignored: {transcript_ignored}");
    println!("transcript_duplicate: {transcript_duplicate}");
    println!("transcript_total_items: {transcript_total_items}");
    Ok(())
}

fn run_render_once(args: Args) -> Result<(), String> {
    let terminal_config = terminal_config_for_explicit_terminal_mode("render_once");
    assert_terminal_rendering_admitted(&terminal_config, "render_once")?;
    let model = build_scaffold_app_view_with_terminal_mode(&args, "render_once")?;
    let mut session = TerminalSession::enter()?;
    session.draw_once(&model)?;
    session.leave()
}
fn run_interactive_step_once(args: Args) -> Result<(), String> {
    let config = build_smoke_step_config(&args)?;
    let runtime_config = runtime_config_snapshot_from_process_env();
    let result = if args.persistent_smoke_session {
        let mut session = AgentTuiSmokeSession::with_provider_runtime_config_and_adapter_admission(
            &config,
            runtime_config.provider,
            runtime_config.provider_adapter,
        )?;
        session.run_step(config.composer_has_draft)?
    } else {
        run_interactive_smoke_step_with_provider_runtime_config_and_adapter_admission(
            &config,
            runtime_config.provider,
            runtime_config.provider_adapter,
        )?
    };

    println!("interactive_step_once: ok");
    print_interactive_smoke_step_summary(&result);
    Ok(())
}

fn run_interactive_smoke_loop(args: Args) -> Result<(), String> {
    let max_steps = args.max_steps.expect("validated max steps");
    let config = build_smoke_step_config(&args)?;
    let runtime_config = runtime_config_snapshot_from_process_env();
    let mut session = AgentTuiSmokeSession::with_provider_runtime_config_and_adapter_admission(
        &config,
        runtime_config.provider,
        runtime_config.provider_adapter,
    )?;

    for step_index in 1..=max_steps {
        let result = session.run_step(config.composer_has_draft)?;
        println!("interactive_smoke_loop_step: {step_index}");
        print_interactive_smoke_step_summary(&result);
    }

    println!("interactive_smoke_loop: ok");
    println!("steps: {max_steps}");
    Ok(())
}

fn print_interactive_smoke_step_summary(
    result: &narada_agent_tui::interactive_runtime::InteractiveStepResult,
) {
    for line in interactive_smoke_step_summary_lines(result) {
        println!("{line}");
    }
}

fn build_smoke_step_config(args: &Args) -> Result<AgentTuiSmokeStepConfig, String> {
    Ok(AgentTuiSmokeStepConfig {
        identity: args.identity.clone().unwrap_or_default(),
        session: args.session.clone().unwrap_or_default(),
        site_root: args.site_root.clone().expect("validated site root"),
        control_jsonl: args.control_jsonl.clone().expect("validated control jsonl"),
        session_jsonl: args.session_jsonl.clone().expect("validated session jsonl"),
        composer_has_draft: args.composer_has_draft,
    })
}

fn run_interactive_loop(args: Args) -> Result<(), String> {
    let terminal_config = terminal_config_for_explicit_terminal_mode("interactive_loop");
    assert_terminal_interactive_loop_admitted(&terminal_config)?;
    let max_steps = args.max_steps.expect("validated max steps");
    let mut runtime = build_interactive_runtime(&args)?;
    let mut clock = RuntimeClock::system_now()?;
    let mut loop_state = AgentTuiLoopState::default();
    let mut input_reader = CrosstermTerminalInputReader;
    let mut input = TerminalInputTickSource {
        reader: &mut input_reader,
        wait: Duration::from_millis(INTERACTIVE_INPUT_IDLE_WAIT_MS),
    };
    let mut clock_source = RuntimeClockInteractiveSource { clock: &mut clock };
    let mut terminal = TerminalSession::enter()?;

    run_injected_interactive_loop(
        &mut runtime,
        &mut loop_state,
        &mut terminal,
        &mut input,
        &mut clock_source,
        max_steps,
    )?;
    terminal.leave()
}

fn assert_terminal_interactive_loop_admitted(config: &TerminalRuntimeConfig) -> Result<(), String> {
    assert_terminal_rendering_admitted(config, "interactive_loop")
}

fn assert_terminal_rendering_admitted(
    config: &TerminalRuntimeConfig,
    requested_mode: &str,
) -> Result<(), String> {
    if !config.terminal_rendering_enabled {
        let reason = config
            .refusal_reason
            .as_deref()
            .unwrap_or("terminal_rendering_not_enabled");
        return Err(format!(
            "terminal_{requested_mode}_not_admitted:status={}:reason={reason}",
            config.status.as_str()
        ));
    }
    if config.mode.as_deref() != Some(requested_mode) {
        return Err(format!(
            "terminal_{requested_mode}_not_admitted:status={}:reason=wrong_terminal_mode:{}",
            config.status.as_str(),
            config.mode.as_deref().unwrap_or("none")
        ));
    }
    Ok(())
}

fn build_interactive_runtime(args: &Args) -> Result<AgentTuiInteractiveRuntime, String> {
    let identity = args.identity.clone().unwrap_or_default();
    let session = args.session.clone().unwrap_or_default();
    let control_jsonl = args.control_jsonl.clone().expect("validated control jsonl");
    let session_jsonl = args.session_jsonl.clone().expect("validated session jsonl");
    let context = build_evidence_context(args);
    let runtime_config = runtime_config_snapshot_for_explicit_terminal_mode("interactive_loop");
    let runtime_posture = runtime_config.posture();
    let provider_tool_call_executor = provider_tool_call_executor_from_mcp_runtime_config(
        &session_jsonl,
        context.clone(),
        &runtime_config.mcp,
    )?;
    Ok(
        AgentTuiInteractiveRuntime::with_provider_adapter_tool_executor_and_state(
            identity,
            session,
            control_jsonl,
            session_jsonl,
            context,
            provider_adapter_from_runtime_config(
                runtime_config.provider,
                runtime_config.provider_adapter,
            ),
            provider_tool_call_executor,
            runtime_posture,
        ),
    )
}
#[allow(dead_code)]
fn build_interactive_app_view(
    runtime: &AgentTuiInteractiveRuntime,
    loop_state: &AgentTuiLoopState,
) -> Result<AppViewModel, String> {
    let terminal_size = current_terminal_size()?;
    Ok(runtime.build_view(
        terminal_size,
        &loop_state.draft_state(),
        loop_state.last_error.clone(),
    ))
}

fn print_runtime_step_summary(
    step_index: u64,
    result: &narada_agent_tui::runtime_step::RuntimeStepResult,
) {
    println!("step: {step_index}");
    println!("bytes_read: {}", result.poll.bytes_read);
    println!("admitted_or_queued: {}", result.poll.admitted_or_queued);
    println!("parse_errors: {}", result.poll.parse_errors.len());
    println!("released_held: {}", result.released_held);
    println!(
        "completed_turn: {}",
        result
            .completed_turn
            .as_ref()
            .map(|turn| turn.turn_id.as_str())
            .unwrap_or("none")
    );
    println!("transcript_projected: {}", result.transcript.projected);
    println!("transcript_ignored: {}", result.transcript.ignored);
    println!("transcript_duplicate: {}", result.transcript.duplicate);
    println!("transcript_total_items: {}", result.transcript.total_items);
}
fn build_runtime_step(args: &Args) -> Result<RuntimeStep, String> {
    let control_jsonl = args.control_jsonl.clone().expect("validated control jsonl");
    let session_jsonl = args.session_jsonl.clone().expect("validated session jsonl");
    let context = build_evidence_context(args);
    let clock = RuntimeClock::system_now()?;
    let runtime_config = runtime_config_snapshot_from_process_env();
    let provider_tool_call_executor = provider_tool_call_executor_from_mcp_runtime_config(
        &session_jsonl,
        context.clone(),
        &runtime_config.mcp,
    )?;
    Ok(RuntimeStep::with_provider_adapter_and_tool_executor(
        control_jsonl,
        session_jsonl,
        context,
        clock,
        provider_adapter_from_runtime_config(
            runtime_config.provider,
            runtime_config.provider_adapter,
        ),
        provider_tool_call_executor,
    ))
}
fn build_evidence_context(args: &Args) -> SessionEvidenceContext {
    let identity = args.identity.clone().unwrap_or_default();
    let session = args.session.clone().unwrap_or_default();
    let site_root = args.site_root.clone().expect("validated site root");
    let site_id = derive_site_id(&identity);
    SessionEvidenceContext {
        carrier_session_id: session,
        agent_id: identity,
        site_id,
        site_root: site_root.display().to_string(),
    }
}

fn build_scaffold_app_view_with_terminal_mode(
    args: &Args,
    mode: &str,
) -> Result<AppViewModel, String> {
    build_scaffold_app_view_with_runtime_config(
        args,
        runtime_config_snapshot_for_explicit_terminal_mode(mode),
    )
}

fn build_scaffold_app_view_with_runtime_config(
    args: &Args,
    runtime_config: RuntimeConfigSnapshot,
) -> Result<AppViewModel, String> {
    let identity = args.identity.clone().unwrap_or_default();
    let session = args.session.clone().unwrap_or_default();
    Ok(build_app_view(&AppViewInput {
        terminal_size: current_terminal_size()?,
        layout_config: LayoutConfig::default(),
        transcript_items: Vec::new(),
        status: StatusViewInput {
            identity: identity.clone(),
            session,
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_age: None,
            queued_inputs: 0,
            held_system_directives: 0,
            oldest_held_age: None,
            transcript_items: 0,
            runtime_posture: runtime_config.posture(),
            last_error: None,
        },
        composer: ComposerViewInput {
            identity,
            draft_text: String::new(),
            turn_state: TurnState::Idle,
            queued_operator_notes: 0,
            held_system_directives: 0,
        },
    }))
}

fn current_terminal_size() -> Result<TerminalSize, String> {
    let (width, height) = crossterm::terminal::size()
        .map_err(|error| format!("terminal_size_read_failed:{error}"))?;
    Ok(TerminalSize { width, height })
}

fn derive_site_id(identity: &str) -> String {
    identity
        .rsplit_once('.')
        .map(|(site, _)| site.to_string())
        .unwrap_or_else(|| "unknown-site".to_string())
}

fn print_help() {
    let launch_slice_flag = launch_slice_contract().carrier_flag.as_str();
    println!(
        "narada-agent-tui {VERSION}\n\nUsage:\n  narada-agent-tui --identity <agent-id> --session <carrier-session-id> --site-root <path> [--control-jsonl <path>] [--session-jsonl <path>] [--runtime-step-once | --runtime-loop --max-steps <n> | --interactive-step-once | --interactive-smoke-loop --max-steps <n> | {launch_slice_flag} --max-steps <n> | --render-once]\n\nOptions:\n  --identity <agent-id>          Agent identity, e.g. sonar.resident\n  --session <carrier-session>    Carrier session id\n  --site-root <path>             Narada site root\n  --control-jsonl <path>         Optional carrier control JSONL path\n  --session-jsonl <path>         Optional carrier session JSONL path\n  --runtime-step-once            Run one non-UI runtime pass and exit\n  --runtime-loop                 Run bounded non-UI runtime passes and exit\n  {launch_slice_flag:<29} Run one interactive runtime pass without entering TUI mode\n  --interactive-smoke-loop       Run bounded persistent smoke passes without entering TUI mode\n  --interactive-loop             Run bounded TUI draw/input passes and exit\n  --max-steps <n>                Required positive step count for loop modes\n  --render-once                  Enter TUI mode, draw one scaffold frame, and exit\n  --composer-has-draft           Hold composer-clear system directives during runtime pass\n  --persistent-smoke-session     Use reusable smoke session path for interactive smoke step\n  --check-rust-toolchain         Check cargo and MSVC link.exe readiness for Rust tests\n  --version                      Print version\n  --help                         Show help\n\nStatus:\n  Interactive TUI scaffold has control JSONL polling, input queuing, transcript projection, provider-boundary evidence, and a gated MCP fabric bridge. Real provider dispatch and production MCP exposure remain withheld until their admission gates pass."
    );
}
fn validate_launch_args(args: &Args) -> Result<(), String> {
    if args.check_rust_toolchain {
        return Ok(());
    }
    if args.identity.as_deref().unwrap_or("").trim().is_empty() {
        return Err("missing required --identity".to_string());
    }
    if args.session.as_deref().unwrap_or("").trim().is_empty() {
        return Err("missing required --session".to_string());
    }
    if args.site_root.is_none() {
        return Err("missing required --site-root".to_string());
    }
    let selected_modes = [
        args.runtime_step_once,
        args.runtime_loop,
        args.interactive_step_once,
        args.interactive_smoke_loop,
        args.interactive_loop,
        args.render_once,
    ]
    .iter()
    .filter(|selected| **selected)
    .count();
    if selected_modes > 1 {
        return Err("choose only one runtime mode".to_string());
    }
    if args.runtime_step_once
        || args.runtime_loop
        || args.interactive_step_once
        || args.interactive_smoke_loop
        || args.interactive_loop
    {
        if args.control_jsonl.is_none() {
            return Err("runtime mode requires --control-jsonl".to_string());
        }
        if args.session_jsonl.is_none() {
            return Err("runtime mode requires --session-jsonl".to_string());
        }
    }
    if args.runtime_loop || args.interactive_smoke_loop || args.interactive_loop {
        match args.max_steps {
            Some(value) if value > 0 => {}
            _ => return Err("loop mode requires --max-steps > 0".to_string()),
        }
    } else if args.max_steps.is_some() {
        return Err("--max-steps requires a loop mode".to_string());
    }
    if args.persistent_smoke_session && !args.interactive_step_once {
        return Err("--persistent-smoke-session requires --interactive-step-once".to_string());
    }
    Ok(())
}

fn parse_args<I>(args: I) -> Result<Args, String>
where
    I: IntoIterator<Item = String>,
{
    let mut parsed = Args::default();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--identity" => parsed.identity = Some(require_value(&mut iter, "--identity")?),
            "--session" => parsed.session = Some(require_value(&mut iter, "--session")?),
            "--site-root" => {
                parsed.site_root = Some(PathBuf::from(require_value(&mut iter, "--site-root")?))
            }
            "--control-jsonl" => {
                parsed.control_jsonl =
                    Some(PathBuf::from(require_value(&mut iter, "--control-jsonl")?))
            }
            "--session-jsonl" => {
                parsed.session_jsonl =
                    Some(PathBuf::from(require_value(&mut iter, "--session-jsonl")?))
            }
            "--runtime-step-once" => parsed.runtime_step_once = true,
            "--runtime-loop" => parsed.runtime_loop = true,
            "--interactive-step-once" => parsed.interactive_step_once = true,
            "--interactive-smoke-loop" => parsed.interactive_smoke_loop = true,
            flag if flag == launch_slice_contract().carrier_flag => parsed.interactive_loop = true,
            "--render-once" => parsed.render_once = true,
            "--max-steps" => {
                let value = require_value(&mut iter, "--max-steps")?;
                parsed.max_steps = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| "invalid --max-steps".to_string())?,
                );
            }
            "--composer-has-draft" => parsed.composer_has_draft = true,
            "--persistent-smoke-session" => parsed.persistent_smoke_session = true,
            "--check-rust-toolchain" => parsed.check_rust_toolchain = true,
            "--help" | "-h" => parsed.help = true,
            "--version" | "-V" => parsed.version = true,
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    Ok(parsed)
}

fn require_value<I>(iter: &mut I, flag: &str) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    iter.next()
        .filter(|value| !value.starts_with('-'))
        .ok_or_else(|| format!("missing value for {flag}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(values: &[&str]) -> Result<Args, String> {
        parse_args(values.iter().map(|value| value.to_string()))
    }

    #[test]
    fn parses_required_identity_session_and_site_root() {
        let args = parse(&[
            "--identity",
            "sonar.resident",
            "--session",
            "carrier_1",
            "--site-root",
            "D:/code/narada.sonar",
        ])
        .expect("args parse");

        assert_eq!(args.identity.as_deref(), Some("sonar.resident"));
        assert_eq!(args.session.as_deref(), Some("carrier_1"));
        assert_eq!(args.site_root, Some(PathBuf::from("D:/code/narada.sonar")));
        assert!(!args.runtime_step_once);
        assert!(!args.runtime_loop);
        assert!(!args.interactive_smoke_loop);
        assert!(!args.interactive_loop);
        assert!(!args.render_once);
    }

    #[test]
    fn parses_runtime_file_paths_and_loop_flags() {
        let args = parse(&[
            "--identity",
            "sonar.resident",
            "--session",
            "carrier_1",
            "--site-root",
            "D:/code/narada.sonar",
            "--control-jsonl",
            "control.jsonl",
            "--session-jsonl",
            "session.jsonl",
            "--runtime-loop",
            "--max-steps",
            "3",
            "--composer-has-draft",
        ])
        .expect("args parse");

        assert_eq!(args.control_jsonl, Some(PathBuf::from("control.jsonl")));
        assert_eq!(args.session_jsonl, Some(PathBuf::from("session.jsonl")));
        assert!(args.runtime_loop);
        assert_eq!(args.max_steps, Some(3));
        assert!(args.composer_has_draft);
    }

    #[test]
    fn parses_interactive_step_once_mode() {
        let args = parse(&[
            "--identity",
            "sonar.resident",
            "--session",
            "carrier_1",
            "--site-root",
            "D:/code/narada.sonar",
            "--control-jsonl",
            "control.jsonl",
            "--session-jsonl",
            "session.jsonl",
            "--interactive-step-once",
        ])
        .expect("args parse");

        assert!(args.interactive_step_once);
    }

    #[test]
    fn parses_persistent_smoke_session_option() {
        let args = parse(&[
            "--identity",
            "sonar.resident",
            "--session",
            "carrier_1",
            "--site-root",
            "D:/code/narada.sonar",
            "--control-jsonl",
            "control.jsonl",
            "--session-jsonl",
            "session.jsonl",
            "--interactive-step-once",
            "--persistent-smoke-session",
        ])
        .expect("args parse");

        assert!(args.interactive_step_once);
        assert!(args.persistent_smoke_session);
    }

    #[test]
    fn parses_interactive_smoke_loop_mode() {
        let args = parse(&[
            "--identity",
            "sonar.resident",
            "--session",
            "carrier_1",
            "--site-root",
            "D:/code/narada.sonar",
            "--control-jsonl",
            "control.jsonl",
            "--session-jsonl",
            "session.jsonl",
            "--interactive-smoke-loop",
            "--max-steps",
            "2",
        ])
        .expect("args parse");

        assert!(args.interactive_smoke_loop);
        assert_eq!(args.max_steps, Some(2));
    }

    #[test]
    fn parses_interactive_loop_mode() {
        let args = parse(&[
            "--identity",
            "sonar.resident",
            "--session",
            "carrier_1",
            "--site-root",
            "D:/code/narada.sonar",
            "--control-jsonl",
            "control.jsonl",
            "--session-jsonl",
            "session.jsonl",
            "--interactive-loop",
            "--max-steps",
            "5",
        ])
        .expect("args parse");

        assert!(args.interactive_loop);
        assert_eq!(args.max_steps, Some(5));
    }

    #[test]
    fn parses_rust_toolchain_check_without_launch_identity() {
        let args = parse(&["--check-rust-toolchain"]).expect("args parse");

        assert!(args.check_rust_toolchain);
        validate_launch_args(&args).expect("toolchain check bypasses launch identity");
    }

    #[test]
    fn executable_candidates_include_windows_extensions() {
        if cfg!(windows) {
            assert_eq!(
                executable_candidates("link"),
                vec![
                    "link.exe".to_string(),
                    "link.cmd".to_string(),
                    "link.bat".to_string(),
                    "link".to_string(),
                ]
            );
        } else {
            assert_eq!(executable_candidates("link"), vec!["link".to_string()]);
        }
    }

    #[test]
    fn parses_render_once_mode() {
        let args = parse(&[
            "--identity",
            "sonar.resident",
            "--session",
            "carrier_1",
            "--site-root",
            "D:/code/narada.sonar",
            "--render-once",
        ])
        .expect("args parse");

        assert!(args.render_once);
    }

    #[test]
    fn requires_identity_session_and_site_root() {
        let err = validate_launch_args(&Args::default()).expect_err("invalid args");
        assert_eq!(err, "missing required --identity");
    }

    #[test]
    fn runtime_mode_requires_control_and_session_paths() {
        let args = Args {
            identity: Some("sonar.resident".to_string()),
            session: Some("carrier_1".to_string()),
            site_root: Some(PathBuf::from("D:/code/narada.sonar")),
            runtime_step_once: true,
            ..Args::default()
        };

        let err = validate_launch_args(&args).expect_err("invalid runtime args");
        assert_eq!(err, "runtime mode requires --control-jsonl");
    }

    #[test]
    fn persistent_smoke_session_requires_interactive_step_once() {
        let args = Args {
            identity: Some("sonar.resident".to_string()),
            session: Some("carrier_1".to_string()),
            site_root: Some(PathBuf::from("D:/code/narada.sonar")),
            control_jsonl: Some(PathBuf::from("control.jsonl")),
            session_jsonl: Some(PathBuf::from("session.jsonl")),
            persistent_smoke_session: true,
            ..Args::default()
        };

        let err = validate_launch_args(&args).expect_err("invalid persistent smoke args");
        assert_eq!(
            err,
            "--persistent-smoke-session requires --interactive-step-once"
        );
    }

    #[test]
    fn rejects_multiple_runtime_modes() {
        let args = Args {
            identity: Some("sonar.resident".to_string()),
            session: Some("carrier_1".to_string()),
            site_root: Some(PathBuf::from("D:/code/narada.sonar")),
            control_jsonl: Some(PathBuf::from("control.jsonl")),
            session_jsonl: Some(PathBuf::from("session.jsonl")),
            runtime_step_once: true,
            runtime_loop: true,
            ..Args::default()
        };

        let err = validate_launch_args(&args).expect_err("invalid runtime args");
        assert_eq!(err, "choose only one runtime mode");
    }
}
