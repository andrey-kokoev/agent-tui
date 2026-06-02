use crate::mcp_fabric_boundary::McpToolResult;
use crate::mcp_fabric_transport::McpFabricPreparedToolCall;
use crate::mcp_json_rpc::JsonRpcResponse;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Instant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpStdioProcessIoResult {
    pub server_name: String,
    pub request_turn_id: Option<String>,
    pub tool_result: McpToolResult,
    pub response_line: String,
}

pub fn exchange_prepared_tool_call<R, W>(
    prepared: &McpFabricPreparedToolCall,
    reader: &mut R,
    writer: &mut W,
    duration_ms: u64,
) -> Result<McpStdioProcessIoResult, String>
where
    R: BufRead,
    W: Write,
{
    writer
        .write_all(prepared.json_rpc.request_line.as_bytes())
        .map_err(|error| format!("mcp_stdio_write_failed:{}:{error}", prepared.server_name))?;
    writer
        .flush()
        .map_err(|error| format!("mcp_stdio_flush_failed:{}:{error}", prepared.server_name))?;

    let mut response_line = String::new();
    let bytes_read = reader
        .read_line(&mut response_line)
        .map_err(|error| format!("mcp_stdio_read_failed:{}:{error}", prepared.server_name))?;
    if bytes_read == 0 {
        return Err(format!(
            "mcp_stdio_response_missing:{}:{}",
            prepared.server_name, prepared.tool_name
        ));
    }

    let response = JsonRpcResponse::parse_line(response_line.trim_end())?;
    if response.id != prepared.json_rpc.request.id {
        return Err(format!(
            "mcp_stdio_response_id_mismatch:{}:{}:{}",
            prepared.server_name, prepared.json_rpc.request.id, response.id
        ));
    }
    let tool_result = response.into_tool_result(prepared.tool_name.clone(), duration_ms);
    Ok(McpStdioProcessIoResult {
        server_name: prepared.server_name.clone(),
        request_turn_id: prepared
            .request_event
            .payload
            .get("turn_id")
            .and_then(|value| value.as_str())
            .map(ToString::to_string),
        tool_result,
        response_line,
    })
}

pub fn execute_prepared_tool_call_once(
    prepared: &McpFabricPreparedToolCall,
) -> Result<McpStdioProcessIoResult, String> {
    let started = Instant::now();
    let mut child = Command::new(&prepared.command)
        .args(&prepared.args)
        .envs(&prepared.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("mcp_stdio_spawn_failed:{}:{error}", prepared.server_name))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| format!("mcp_stdio_stdin_unavailable:{}", prepared.server_name))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("mcp_stdio_stdout_unavailable:{}", prepared.server_name))?;
    let mut reader = BufReader::new(stdout);
    let result = exchange_prepared_tool_call(
        prepared,
        &mut reader,
        &mut stdin,
        started.elapsed().as_millis() as u64,
    );

    let _ = child.kill();
    let _ = child.wait();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{SESSION_EVENT_SCHEMA, SessionEvent, SessionEventKind};
    use crate::mcp_json_rpc::McpJsonRpcExchange;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::fs;
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn prepared() -> McpFabricPreparedToolCall {
        prepared_with_command("node", vec!["site-loop.mjs".to_string()], BTreeMap::new())
    }

    fn prepared_with_command(
        command: &str,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    ) -> McpFabricPreparedToolCall {
        McpFabricPreparedToolCall {
            server_name: "sonar-site-loop".to_string(),
            command: command.to_string(),
            args,
            env,
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
    fn exchanges_one_json_rpc_line_over_stdio_like_streams() {
        let mut reader = Cursor::new(
            b"{\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"ok\"}]}}\n"
                .to_vec(),
        );
        let mut writer = Vec::new();

        let result = exchange_prepared_tool_call(&prepared(), &mut reader, &mut writer, 14)
            .expect("exchange succeeds");

        assert_eq!(result.server_name, "sonar-site-loop");
        assert_eq!(result.tool_result.status, "ok");
        assert_eq!(result.tool_result.duration_ms, 14);
        assert_eq!(result.tool_result.result_summary, "content_items=1");
        let written = String::from_utf8(writer).expect("writer contains utf8");
        assert!(written.contains("\"method\":\"tools/call\""));
        assert!(written.ends_with('\n'));
    }

    #[test]
    fn spawned_process_receives_prepared_env() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock works")
            .as_nanos();
        let script_path =
            std::env::temp_dir().join(format!("narada-agent-tui-mcp-env-{unique}.mjs"));
        fs::write(
            &script_path,
            r#"
process.stdin.setEncoding('utf8');
let input = '';
let done = false;
process.stdin.on('data', chunk => {
  if (done) return;
  input += chunk;
  const newline = input.indexOf('\n');
  if (newline === -1) return;
  done = true;
  const request = JSON.parse(input.slice(0, newline));
  const value = process.env.NARADA_AGENT_TUI_TEST_ENV || '';
  process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: request.id, result: { content: [{ type: 'text', text: value }] } }) + '\n');
});
"#,
        )
        .expect("write temp mcp script");
        let mut env = BTreeMap::new();
        env.insert(
            "NARADA_AGENT_TUI_TEST_ENV".to_string(),
            "env-value-from-prepared-call".to_string(),
        );
        let prepared = prepared_with_command("node", vec![script_path.display().to_string()], env);

        let result = execute_prepared_tool_call_once(&prepared).expect("spawned exchange succeeds");
        let _ = fs::remove_file(&script_path);

        assert_eq!(result.tool_result.status, "ok");
        assert!(
            result
                .response_line
                .contains("env-value-from-prepared-call")
        );
    }

    #[test]
    fn rejects_mismatched_response_id() {
        let mut reader = Cursor::new(b"{\"jsonrpc\":\"2.0\",\"id\":8,\"result\":{}}\n".to_vec());
        let mut writer = Vec::new();

        let error = exchange_prepared_tool_call(&prepared(), &mut reader, &mut writer, 1)
            .expect_err("mismatched id rejected");

        assert_eq!(error, "mcp_stdio_response_id_mismatch:sonar-site-loop:7:8");
    }

    #[test]
    fn rejects_empty_response_stream() {
        let mut reader = Cursor::new(Vec::new());
        let mut writer = Vec::new();

        let error = exchange_prepared_tool_call(&prepared(), &mut reader, &mut writer, 1)
            .expect_err("empty response rejected");

        assert_eq!(
            error,
            "mcp_stdio_response_missing:sonar-site-loop:site_loop_run_once"
        );
    }
}
