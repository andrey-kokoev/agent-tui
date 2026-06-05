use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const MCP_JSON_RPC_CONTRACT_JSON: &str =
    include_str!("../../narada/packages/mcp-protocol-contract/contracts/json-rpc.json");
const EXPECTED_SCHEMA: &str = "narada.mcp.json_rpc_contract.v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpJsonRpcContract {
    pub schema: String,
    pub jsonrpc_version: String,
    pub methods: McpJsonRpcMethods,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpJsonRpcMethods {
    pub initialize: String,
    pub initialized_notification: String,
    pub tools_list: String,
    pub tools_call: String,
}

static MCP_JSON_RPC_CONTRACT: OnceLock<McpJsonRpcContract> = OnceLock::new();

pub fn mcp_json_rpc_contract() -> &'static McpJsonRpcContract {
    MCP_JSON_RPC_CONTRACT.get_or_init(|| {
        parse_mcp_json_rpc_contract(MCP_JSON_RPC_CONTRACT_JSON)
            .expect("bundled MCP JSON-RPC contract must be valid")
    })
}

pub fn parse_mcp_json_rpc_contract(json_text: &str) -> Result<McpJsonRpcContract, String> {
    let contract: McpJsonRpcContract = serde_json::from_str(json_text)
        .map_err(|error| format!("mcp_json_rpc_contract_parse_failed:{error}"))?;
    if contract.schema != EXPECTED_SCHEMA {
        return Err("mcp_json_rpc_contract_invalid:schema".to_string());
    }
    if contract.jsonrpc_version.is_empty()
        || contract.methods.initialize.is_empty()
        || contract.methods.initialized_notification.is_empty()
        || contract.methods.tools_list.is_empty()
        || contract.methods.tools_call.is_empty()
    {
        return Err("mcp_json_rpc_contract_invalid:methods".to_string());
    }
    Ok(contract)
}

pub fn jsonrpc_version() -> &'static str {
    mcp_json_rpc_contract().jsonrpc_version.as_str()
}

pub fn initialize_method() -> &'static str {
    mcp_json_rpc_contract().methods.initialize.as_str()
}

pub fn initialized_notification_method() -> &'static str {
    mcp_json_rpc_contract()
        .methods
        .initialized_notification
        .as_str()
}

pub fn tools_call_method() -> &'static str {
    mcp_json_rpc_contract().methods.tools_call.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_mcp_json_rpc_contract_is_valid() {
        let contract = mcp_json_rpc_contract();

        assert_eq!(contract.schema, EXPECTED_SCHEMA);
        assert_eq!(jsonrpc_version(), "2.0");
        assert_eq!(initialize_method(), "initialize");
        assert_eq!(
            initialized_notification_method(),
            "notifications/initialized"
        );
        assert_eq!(tools_call_method(), "tools/call");
    }
}
