use crate::mcp_fabric_transport::McpFabricPreparedToolCall;
use crate::mcp_json_rpc::JsonRpcResponse;
use crate::mcp_process_supervisor::handshake_plan;
use crate::mcp_runtime_execution::McpRuntimeToolExecutor;
use crate::mcp_stdio_process::{McpStdioProcessIoResult, exchange_prepared_tool_call};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpProcessTimeoutPolicy {
    pub request_timeout_ms: u64,
    pub handshake_timeout_ms: u64,
}

impl Default for McpProcessTimeoutPolicy {
    fn default() -> Self {
        Self {
            request_timeout_ms: 30_000,
            handshake_timeout_ms: 10_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReusableMcpProcessStatus {
    pub server_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub call_count: u64,
    pub handshake_completed: bool,
}

pub struct ReusableMcpProcessExecutor {
    timeout_policy: McpProcessTimeoutPolicy,
    processes: BTreeMap<String, ReusableMcpProcess>,
}

struct ReusableMcpProcess {
    server_name: String,
    command: String,
    args: Vec<String>,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    call_count: u64,
    handshake_completed: bool,
}

impl ReusableMcpProcessExecutor {
    pub fn new(timeout_policy: McpProcessTimeoutPolicy) -> Self {
        Self {
            timeout_policy,
            processes: BTreeMap::new(),
        }
    }

    pub fn timeout_policy(&self) -> &McpProcessTimeoutPolicy {
        &self.timeout_policy
    }

    pub fn process_count(&self) -> usize {
        self.processes.len()
    }

    pub fn process_status(&self, server_name: &str) -> Option<ReusableMcpProcessStatus> {
        self.processes
            .get(server_name)
            .map(ReusableMcpProcess::status)
    }

    pub fn stop_server(&mut self, server_name: &str) {
        if let Some(mut process) = self.processes.remove(server_name) {
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
    }

    pub fn stop_all(&mut self) {
        let server_names = self.processes.keys().cloned().collect::<Vec<_>>();
        for server_name in server_names {
            self.stop_server(&server_name);
        }
    }

    fn process_for(
        &mut self,
        prepared: &McpFabricPreparedToolCall,
    ) -> Result<&mut ReusableMcpProcess, String> {
        let should_replace = self
            .processes
            .get(&prepared.server_name)
            .map(|process| process.command != prepared.command || process.args != prepared.args)
            .unwrap_or(false);
        if should_replace {
            self.stop_server(&prepared.server_name);
        }
        if !self.processes.contains_key(&prepared.server_name) {
            let process =
                ReusableMcpProcess::spawn(prepared, self.timeout_policy.handshake_timeout_ms)?;
            self.processes.insert(prepared.server_name.clone(), process);
        }
        self.processes
            .get_mut(&prepared.server_name)
            .ok_or_else(|| format!("mcp_reusable_process_missing:{}", prepared.server_name))
    }
}

impl Default for ReusableMcpProcessExecutor {
    fn default() -> Self {
        Self::new(McpProcessTimeoutPolicy::default())
    }
}

impl Drop for ReusableMcpProcessExecutor {
    fn drop(&mut self) {
        self.stop_all();
    }
}

impl McpRuntimeToolExecutor for ReusableMcpProcessExecutor {
    fn execute_tool_call(
        &mut self,
        prepared: &McpFabricPreparedToolCall,
    ) -> Result<McpStdioProcessIoResult, String> {
        let request_timeout_ms = self.timeout_policy.request_timeout_ms;
        let process = self.process_for(prepared)?;
        let timeout_guard = TimeoutCancellationGuard::spawn(
            prepared.server_name.clone(),
            process.child.id(),
            request_timeout_ms,
        );
        let started = Instant::now();
        let result =
            exchange_prepared_tool_call(prepared, &mut process.stdout, &mut process.stdin, 0);
        timeout_guard.complete();
        let elapsed_ms = started.elapsed().as_millis() as u64;
        match result {
            Ok(mut result) => {
                process.call_count += 1;
                result.tool_result.duration_ms = elapsed_ms;
                if elapsed_ms > request_timeout_ms {
                    self.stop_server(&prepared.server_name);
                    return Err(timeout_error(
                        &prepared.server_name,
                        elapsed_ms,
                        request_timeout_ms,
                    ));
                }
                Ok(result)
            }
            Err(error) => {
                self.stop_server(&prepared.server_name);
                if elapsed_ms >= request_timeout_ms {
                    Err(timeout_error(
                        &prepared.server_name,
                        elapsed_ms,
                        request_timeout_ms,
                    ))
                } else {
                    Err(error)
                }
            }
        }
    }
}

fn timeout_error(server_name: &str, elapsed_ms: u64, timeout_ms: u64) -> String {
    format!("mcp_reusable_process_timeout:{server_name}:{elapsed_ms}:{timeout_ms}")
}

struct TimeoutCancellationGuard {
    completed: Arc<AtomicBool>,
}

impl TimeoutCancellationGuard {
    fn spawn(server_name: String, process_id: u32, timeout_ms: u64) -> Self {
        let completed = Arc::new(AtomicBool::new(false));
        let completed_for_thread = Arc::clone(&completed);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(timeout_ms));
            if !completed_for_thread.load(Ordering::SeqCst) {
                terminate_process(process_id);
                eprintln!(
                    "mcp_reusable_process_timeout_cancelled:{server_name}:{process_id}:{timeout_ms}"
                );
            }
        });
        Self { completed }
    }

    fn complete(&self) {
        self.completed.store(true, Ordering::SeqCst);
    }
}

#[cfg(windows)]
fn terminate_process(process_id: u32) {
    let _ = Command::new("taskkill")
        .args(["/PID", &process_id.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(unix)]
fn terminate_process(process_id: u32) {
    let _ = Command::new("kill")
        .args(["-TERM", &process_id.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(not(any(windows, unix)))]
fn terminate_process(_process_id: u32) {}

pub fn exchange_initialize_handshake<R, W>(
    server_name: &str,
    reader: &mut R,
    writer: &mut W,
    timeout_ms: u64,
) -> Result<u64, String>
where
    R: BufRead,
    W: Write,
{
    let started = Instant::now();
    let plan = handshake_plan(server_name, 1)?;
    writer
        .write_all(plan.initialize_line.as_bytes())
        .map_err(|error| {
            format!("mcp_reusable_process_initialize_write_failed:{server_name}:{error}")
        })?;
    writer.flush().map_err(|error| {
        format!("mcp_reusable_process_initialize_flush_failed:{server_name}:{error}")
    })?;
    let mut response_line = String::new();
    let bytes_read = reader.read_line(&mut response_line).map_err(|error| {
        format!("mcp_reusable_process_initialize_read_failed:{server_name}:{error}")
    })?;
    if bytes_read == 0 {
        return Err(format!(
            "mcp_reusable_process_initialize_response_missing:{server_name}"
        ));
    }
    let response = JsonRpcResponse::parse_line(response_line.trim_end())?;
    if response.id != 1 {
        return Err(format!(
            "mcp_reusable_process_initialize_id_mismatch:{server_name}:1:{}",
            response.id
        ));
    }
    if let Some(error) = response.error {
        return Err(format!(
            "mcp_reusable_process_initialize_error:{server_name}:{}:{}",
            error.code, error.message
        ));
    }
    writer
        .write_all(plan.initialized_line.as_bytes())
        .map_err(|error| {
            format!("mcp_reusable_process_initialized_write_failed:{server_name}:{error}")
        })?;
    writer.flush().map_err(|error| {
        format!("mcp_reusable_process_initialized_flush_failed:{server_name}:{error}")
    })?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    if elapsed_ms > timeout_ms {
        return Err(format!(
            "mcp_reusable_process_handshake_timeout:{server_name}:{elapsed_ms}:{timeout_ms}"
        ));
    }
    Ok(elapsed_ms)
}

impl ReusableMcpProcess {
    fn spawn(
        prepared: &McpFabricPreparedToolCall,
        handshake_timeout_ms: u64,
    ) -> Result<Self, String> {
        let mut child = Command::new(&prepared.command)
            .args(&prepared.args)
            .envs(&prepared.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                format!(
                    "mcp_reusable_process_spawn_failed:{}:{error}",
                    prepared.server_name
                )
            })?;
        let mut stdin = child.stdin.take().ok_or_else(|| {
            format!(
                "mcp_reusable_process_stdin_unavailable:{}",
                prepared.server_name
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            format!(
                "mcp_reusable_process_stdout_unavailable:{}",
                prepared.server_name
            )
        })?;
        let mut stdout = BufReader::new(stdout);
        if let Err(error) = exchange_initialize_handshake(
            &prepared.server_name,
            &mut stdout,
            &mut stdin,
            handshake_timeout_ms,
        ) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok(Self {
            server_name: prepared.server_name.clone(),
            command: prepared.command.clone(),
            args: prepared.args.clone(),
            child,
            stdin,
            stdout,
            call_count: 0,
            handshake_completed: true,
        })
    }

    fn status(&self) -> ReusableMcpProcessStatus {
        ReusableMcpProcessStatus {
            server_name: self.server_name.clone(),
            command: self.command.clone(),
            args: self.args.clone(),
            call_count: self.call_count,
            handshake_completed: self.handshake_completed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{SESSION_EVENT_SCHEMA, SessionEvent, SessionEventKind};
    use crate::mcp_json_rpc::McpJsonRpcExchange;
    use serde_json::json;
    use std::io::Cursor;

    fn prepared(server_name: &str, command: &str, args: Vec<String>) -> McpFabricPreparedToolCall {
        McpFabricPreparedToolCall {
            server_name: server_name.to_string(),
            command: command.to_string(),
            args,
            env: std::collections::BTreeMap::new(),
            tool_name: "site_loop_run_once".to_string(),
            request_event: SessionEvent {
                schema: SESSION_EVENT_SCHEMA.to_string(),
                event_kind: SessionEventKind::ToolCallRequested,
                event_id: "session_event_tool_request_1".to_string(),
                occurred_at: "2026-05-30T00:00:00.000Z".to_string(),
                carrier_session_id: "carrier_fixture_1".to_string(),
                agent_id: "sonar.resident".to_string(),
                site_id: "narada-sonar".to_string(),
                site_root: "D:/code/narada.sonar".to_string(),
                payload: json!({"tool_name":"site_loop_run_once"}),
            },
            json_rpc: McpJsonRpcExchange::for_tool_call(
                7,
                &crate::mcp_fabric_boundary::McpToolRequest {
                    tool_name: "site_loop_run_once".to_string(),
                    arguments_summary: "{}".to_string(),
                    arguments_ref: None,
                    requesting_agent_id: "sonar.resident".to_string(),
                },
                json!({}),
            )
            .expect("json rpc builds"),
        }
    }

    #[test]
    fn initialize_handshake_writes_initialize_and_initialized_frames() {
        let mut reader = Cursor::new(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"protocolVersion\":\"2024-11-05\"}}\n"
                .to_vec(),
        );
        let mut writer = Vec::new();

        let elapsed =
            exchange_initialize_handshake("sonar-site-loop", &mut reader, &mut writer, 10_000)
                .expect("handshake succeeds");

        assert_eq!(elapsed, 0);
        let written = String::from_utf8(writer).expect("writer contains utf8");
        assert!(written.contains("\"method\":\"initialize\""));
        assert!(written.contains("notifications/initialized"));
    }

    #[test]
    fn initialize_handshake_rejects_mismatched_response_id() {
        let mut reader = Cursor::new(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}\n".to_vec());
        let mut writer = Vec::new();

        let error =
            exchange_initialize_handshake("sonar-site-loop", &mut reader, &mut writer, 10_000)
                .expect_err("mismatch rejected");

        assert_eq!(
            error,
            "mcp_reusable_process_initialize_id_mismatch:sonar-site-loop:1:2"
        );
    }

    #[test]
    fn initialize_handshake_rejects_error_response() {
        let mut reader = Cursor::new(
            b"{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32602,\"message\":\"bad init\"}}\n"
                .to_vec(),
        );
        let mut writer = Vec::new();

        let error =
            exchange_initialize_handshake("sonar-site-loop", &mut reader, &mut writer, 10_000)
                .expect_err("error response rejected");

        assert_eq!(
            error,
            "mcp_reusable_process_initialize_error:sonar-site-loop:-32602:bad init"
        );
    }

    #[test]
    fn timeout_error_has_stable_shape() {
        assert_eq!(
            timeout_error("sonar-site-loop", 31, 30),
            "mcp_reusable_process_timeout:sonar-site-loop:31:30"
        );
    }

    #[test]
    fn cancellation_guard_can_complete_before_deadline() {
        let guard =
            TimeoutCancellationGuard::spawn("sonar-site-loop".to_string(), u32::MAX, 10_000);

        guard.complete();

        assert!(guard.completed.load(Ordering::SeqCst));
    }

    #[test]
    fn timeout_policy_has_bounded_defaults() {
        let executor = ReusableMcpProcessExecutor::default();

        assert_eq!(executor.timeout_policy().request_timeout_ms, 30_000);
        assert_eq!(executor.timeout_policy().handshake_timeout_ms, 10_000);
        assert_eq!(executor.process_count(), 0);
    }

    #[test]
    fn stop_unknown_server_is_noop() {
        let mut executor = ReusableMcpProcessExecutor::default();

        executor.stop_server("missing");

        assert_eq!(executor.process_count(), 0);
    }

    #[test]
    fn spawn_error_does_not_cache_failed_process() {
        let mut executor = ReusableMcpProcessExecutor::default();
        let error = executor
            .execute_tool_call(&prepared(
                "bad-server",
                "definitely-not-a-command",
                Vec::new(),
            ))
            .expect_err("spawn fails");

        assert!(error.starts_with("mcp_reusable_process_spawn_failed:bad-server:"));
        assert_eq!(executor.process_count(), 0);
    }
}
