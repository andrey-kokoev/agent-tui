use narada_agent_tui::nars_projection::{
    resolve_event_endpoint_from_launch_binding, run_attached_projection,
};
use std::env;
use std::path::{Path, PathBuf};

const VERSION: &str = env!("CARGO_PKG_VERSION");
#[derive(Debug, Default, PartialEq, Eq)]
struct Args {
    attach: Option<String>,
    launch_binding: Option<String>,
    identity: Option<String>,
    session: Option<String>,
    max_steps: Option<u64>,
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
    let (endpoint, binding_identity, binding_session) =
        match (args.attach.as_deref(), args.launch_binding.as_deref()) {
            (Some(endpoint), None) => (endpoint.to_string(), None, None),
            (None, Some(binding_path)) => {
                let resolution = resolve_event_endpoint_from_launch_binding(binding_path)?;
                (
                    resolution.event_endpoint,
                    resolution.identity,
                    resolution.session,
                )
            }
            _ => return Err(
                "exactly one of --attach <event_endpoint> or --launch-binding <path> is required"
                    .to_string(),
            ),
        };
    run_attached_projection(
        &endpoint,
        args.identity.clone().or(binding_identity),
        args.session.clone().or(binding_session),
        args.max_steps,
    )
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

fn print_help() {
    println!(
        "narada-agent-tui {VERSION}\n\nUsage:\n  narada-agent-tui --attach <event_endpoint> [--identity <agent-id>] [--session <session-id>]\n  narada-agent-tui --launch-binding <path> [--identity <agent-id>] [--session <session-id>]\n\nOptions:\n  --attach <event-endpoint>      Attach directly to the NARS session WebSocket endpoint\n  --launch-binding <path>        Wait for the exact NARS endpoint published for a launcher binding\n  --identity <agent-id>          Optional identity fallback; normally discovered from events\n  --session <session-id>         Optional session fallback; normally discovered from events\n  --max-steps <n>                Optional bounded loop count for tests\n  --check-rust-toolchain         Check cargo and MSVC link.exe readiness for Rust tests\n  --version                      Print version\n  --help                         Show help\n\nStatus:\n  Agent TUI is a Ratatui projection client. NARS owns provider, MCP, session, turn, and durable event state."
    );
}
fn validate_launch_args(args: &Args) -> Result<(), String> {
    if args.check_rust_toolchain {
        return Ok(());
    }
    let has_attach = args
        .attach
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    let has_binding = args
        .launch_binding
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty());
    if has_attach == has_binding {
        return Err(
            "exactly one of --attach <event_endpoint> or --launch-binding <path> is required"
                .to_string(),
        );
    }
    if let Some(value) = args.max_steps {
        if value == 0 {
            return Err("--max-steps must be greater than zero".to_string());
        }
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
            "--attach" => parsed.attach = Some(require_value(&mut iter, "--attach")?),
            "--launch-binding" => {
                parsed.launch_binding = Some(require_value(&mut iter, "--launch-binding")?)
            }
            "--identity" => parsed.identity = Some(require_value(&mut iter, "--identity")?),
            "--session" => parsed.session = Some(require_value(&mut iter, "--session")?),
            "--runtime-step-once"
            | "--runtime-loop"
            | "--interactive-step-once"
            | "--interactive-smoke-loop"
            | "--persistent-smoke-session"
            | "--interactive-loop"
            | "--render-once"
            | "--site-root"
            | "--control-jsonl"
            | "--session-jsonl"
            | "--composer-has-draft" => {
                return Err(format!(
                    "{arg} has been removed; attach to NARS with --attach <event_endpoint>"
                ));
            }
            "--max-steps" => {
                let value = require_value(&mut iter, "--max-steps")?;
                parsed.max_steps = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| "invalid --max-steps".to_string())?,
                );
            }
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
    fn parses_nars_attach_arguments() {
        let args = parse(&[
            "--attach",
            "ws://127.0.0.1:12345/events",
            "--identity",
            "sonar.resident",
            "--session",
            "session_1",
            "--max-steps",
            "3",
        ])
        .expect("args parse");

        assert_eq!(args.attach.as_deref(), Some("ws://127.0.0.1:12345/events"));
        assert_eq!(args.identity.as_deref(), Some("sonar.resident"));
        assert_eq!(args.session.as_deref(), Some("session_1"));
        assert_eq!(args.max_steps, Some(3));
        validate_launch_args(&args).expect("attach arguments validate");
    }

    #[test]
    fn rejects_removed_runtime_flags() {
        for flag in [
            "--runtime-step-once",
            "--runtime-loop",
            "--interactive-step-once",
            "--interactive-smoke-loop",
            "--persistent-smoke-session",
            "--interactive-loop",
            "--render-once",
            "--control-jsonl",
            "--session-jsonl",
        ] {
            let err = parse(&[flag]).expect_err("removed flag rejected");
            assert!(err.contains("has been removed"), "{flag}: {err}");
            assert!(err.contains("--attach"), "{flag}: {err}");
        }
    }

    #[test]
    fn parses_rust_toolchain_check_without_launch_identity() {
        let args = parse(&["--check-rust-toolchain"]).expect("args parse");

        assert!(args.check_rust_toolchain);
        validate_launch_args(&args).expect("toolchain check bypasses launch identity");
    }

    #[test]
    fn parses_launch_binding_arguments() {
        let args = parse(&["--launch-binding", "C:\\binding.json"]).expect("args parse");

        assert_eq!(args.launch_binding.as_deref(), Some("C:\\binding.json"));
        validate_launch_args(&args).expect("binding arguments validate");
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
    fn requires_attach_source() {
        let err = validate_launch_args(&Args::default()).expect_err("invalid args");
        assert_eq!(
            err,
            "exactly one of --attach <event_endpoint> or --launch-binding <path> is required"
        );
    }
}
