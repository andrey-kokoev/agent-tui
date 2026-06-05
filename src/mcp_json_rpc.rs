use crate::mcp_fabric_boundary::{McpToolRequest, McpToolResult};
use crate::mcp_json_rpc_contract::{
    initialize_method, initialized_notification_method, jsonrpc_version, tools_call_method,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpJsonRpcExchange {
    pub request: JsonRpcRequest,
    pub request_line: String,
}

impl JsonRpcRequest {
    pub fn mcp_initialize(id: u64, client_name: impl Into<String>) -> Self {
        Self {
            jsonrpc: jsonrpc_version().to_string(),
            id,
            method: initialize_method().to_string(),
            params: Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": client_name.into(),
                    "version": env!("CARGO_PKG_VERSION"),
                },
            })),
        }
    }

    pub fn mcp_initialized_notification() -> Self {
        Self {
            jsonrpc: jsonrpc_version().to_string(),
            id: 0,
            method: initialized_notification_method().to_string(),
            params: Some(json!({})),
        }
    }

    pub fn mcp_tools_call(id: u64, request: &McpToolRequest, arguments: Value) -> Self {
        Self {
            jsonrpc: jsonrpc_version().to_string(),
            id,
            method: tools_call_method().to_string(),
            params: Some(json!({
                "name": request.tool_name,
                "arguments": arguments,
            })),
        }
    }

    pub fn to_jsonl(&self) -> Result<String, String> {
        serde_json::to_string(self)
            .map(|line| format!("{line}\n"))
            .map_err(|error| format!("mcp_json_rpc_request_encode_failed:{error}"))
    }
}

impl JsonRpcResponse {
    pub fn parse_line(line: &str) -> Result<Self, String> {
        let response: Self = serde_json::from_str(line)
            .map_err(|error| format!("mcp_json_rpc_response_parse_failed:{error}"))?;
        if response.jsonrpc != jsonrpc_version() {
            return Err(format!(
                "mcp_json_rpc_response_version_invalid:{}",
                response.jsonrpc
            ));
        }
        if response.result.is_some() == response.error.is_some() {
            return Err("mcp_json_rpc_response_must_have_exactly_one_result_or_error".to_string());
        }
        Ok(response)
    }

    pub fn into_tool_result(self, tool_name: impl Into<String>, duration_ms: u64) -> McpToolResult {
        let tool_name = tool_name.into();
        match self.error {
            Some(error) => McpToolResult {
                tool_name,
                status: "error".to_string(),
                duration_ms,
                result_summary: format!("json_rpc_error:{}:{}", error.code, error.message),
                result_text: None,
                result_ref: None,
            },
            None => McpToolResult {
                tool_name,
                status: "ok".to_string(),
                duration_ms,
                result_summary: summarize_result(self.result.as_ref()),
                result_text: extract_text_result(self.result.as_ref()),
                result_ref: None,
            },
        }
    }
}

impl McpJsonRpcExchange {
    pub fn for_tool_call(
        id: u64,
        request: &McpToolRequest,
        arguments: Value,
    ) -> Result<Self, String> {
        let request = JsonRpcRequest::mcp_tools_call(id, request, arguments);
        let request_line = request.to_jsonl()?;
        Ok(Self {
            request,
            request_line,
        })
    }
}

fn summarize_result(result: Option<&Value>) -> String {
    let Some(result) = result else {
        return "empty_result".to_string();
    };
    if let Some(content) = result.get("content").and_then(Value::as_array) {
        return format!("content_items={}", content.len());
    }
    if let Some(object) = result.as_object() {
        let keys = object.keys().cloned().collect::<Vec<_>>().join(",");
        return format!("keys={keys}");
    }
    result.to_string()
}

fn extract_text_result(result: Option<&Value>) -> Option<String> {
    const MAX_RESULT_TEXT_CHARS: usize = 8000;
    let content = result?.get("content")?.as_array()?;
    let text = content
        .iter()
        .filter_map(|item| {
            (item.get("type").and_then(Value::as_str) == Some("text"))
                .then(|| item.get("text").and_then(Value::as_str))
                .flatten()
        })
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let truncated = text.chars().take(MAX_RESULT_TEXT_CHARS).collect::<String>();
    Some(if text.chars().count() > MAX_RESULT_TEXT_CHARS {
        format!("{truncated}\n[truncated]")
    } else {
        truncated
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_request() -> McpToolRequest {
        McpToolRequest {
            tool_name: "site_loop_run_once".to_string(),
            arguments_summary: "{}".to_string(),
            arguments_ref: None,
            requesting_agent_id: "sonar.resident".to_string(),
        }
    }

    #[test]
    fn builds_initialize_and_initialized_json_rpc_lines() {
        let initialize = JsonRpcRequest::mcp_initialize(1, "narada-agent-tui");
        let initialized = JsonRpcRequest::mcp_initialized_notification();

        assert_eq!(initialize.method, "initialize");
        assert_eq!(initialize.id, 1);
        assert!(
            initialize
                .to_jsonl()
                .unwrap()
                .contains("\"protocolVersion\"")
        );
        assert_eq!(initialized.method, "notifications/initialized");
        assert_eq!(initialized.id, 0);
    }

    #[test]
    fn builds_mcp_tools_call_json_rpc_line() {
        let exchange = McpJsonRpcExchange::for_tool_call(7, &tool_request(), json!({}))
            .expect("exchange builds");

        assert_eq!(exchange.request.method, "tools/call");
        assert!(exchange.request_line.ends_with('\n'));
        assert!(exchange.request_line.contains("\"jsonrpc\":\"2.0\""));
        assert!(
            exchange
                .request_line
                .contains("\"name\":\"site_loop_run_once\"")
        );
    }

    #[test]
    fn parses_success_response_into_tool_result() {
        let response = JsonRpcResponse::parse_line(
            r#"{"jsonrpc":"2.0","id":7,"result":{"content":[{"type":"text","text":"ok"}]}}"#,
        )
        .expect("response parses");
        let result = response.into_tool_result("site_loop_run_once", 12);

        assert_eq!(result.status, "ok");
        assert_eq!(result.duration_ms, 12);
        assert_eq!(result.result_summary, "content_items=1");
        assert_eq!(result.result_text.as_deref(), Some("ok"));
    }

    #[test]
    fn parses_error_response_into_tool_result() {
        let response = JsonRpcResponse::parse_line(
            r#"{"jsonrpc":"2.0","id":7,"error":{"code":-32601,"message":"missing"}}"#,
        )
        .expect("response parses");
        let result = response.into_tool_result("site_loop_run_once", 9);

        assert_eq!(result.status, "error");
        assert_eq!(result.result_summary, "json_rpc_error:-32601:missing");
        assert_eq!(result.result_text, None);
    }
}
