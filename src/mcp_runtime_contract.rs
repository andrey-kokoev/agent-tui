use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const MCP_RUNTIME_CONTRACT_JSON: &str = include_str!("../contracts/mcp-runtime.json");
const EXPECTED_SCHEMA: &str = "narada.agent_tui.mcp_runtime_contract.v0";
const EXPECTED_MCP_FABRIC_ENV_VAR: &str = "NARADA_AGENT_TUI_ENABLE_MCP_FABRIC";
const EXPECTED_MCP_CONFIG_ENV_VAR: &str = "NARADA_AGENT_TUI_MCP_CONFIG";
const EXPECTED_SITE_MCP_FABRIC_ENV_VAR: &str = "NARADA_SITE_MCP_FABRIC";
const EXPECTED_MCP_CONFIG_PATH_POLICY: &str = "inside_site_mcp_fabric_without_parent_traversal";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpRuntimeContract {
    pub schema: String,
    pub mcp_fabric_env_var: String,
    pub mcp_config_env_var: String,
    pub site_mcp_fabric_env_var: String,
    pub mcp_config_path_policy: String,
}

static MCP_RUNTIME_CONTRACT: OnceLock<McpRuntimeContract> = OnceLock::new();

pub fn mcp_runtime_contract() -> &'static McpRuntimeContract {
    MCP_RUNTIME_CONTRACT.get_or_init(|| {
        parse_mcp_runtime_contract(MCP_RUNTIME_CONTRACT_JSON)
            .expect("agent-tui MCP runtime contract is valid")
    })
}

pub fn parse_mcp_runtime_contract(json: &str) -> Result<McpRuntimeContract, String> {
    let contract: McpRuntimeContract = serde_json::from_str(json)
        .map_err(|error| format!("mcp_runtime_contract_parse_failed:{error}"))?;
    if contract.schema.trim() != EXPECTED_SCHEMA {
        return Err("mcp_runtime_contract_invalid:schema".to_string());
    }
    if contract.mcp_fabric_env_var.trim() != EXPECTED_MCP_FABRIC_ENV_VAR {
        return Err("mcp_runtime_contract_invalid:mcp_fabric_env_var".to_string());
    }
    if contract.mcp_config_env_var.trim() != EXPECTED_MCP_CONFIG_ENV_VAR {
        return Err("mcp_runtime_contract_invalid:mcp_config_env_var".to_string());
    }
    if contract.site_mcp_fabric_env_var.trim() != EXPECTED_SITE_MCP_FABRIC_ENV_VAR {
        return Err("mcp_runtime_contract_invalid:site_mcp_fabric_env_var".to_string());
    }
    if contract.mcp_config_path_policy.trim() != EXPECTED_MCP_CONFIG_PATH_POLICY {
        return Err("mcp_runtime_contract_invalid:mcp_config_path_policy".to_string());
    }
    Ok(contract)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid_contract_json(mut mutate: impl FnMut(&mut McpRuntimeContract)) -> String {
        let mut contract = mcp_runtime_contract().clone();
        mutate(&mut contract);
        serde_json::to_string(&contract).expect("test MCP runtime contract serializes")
    }

    #[test]
    fn mcp_runtime_contract_rejects_malformed_json() {
        assert_eq!(
            parse_mcp_runtime_contract("not json").unwrap_err(),
            "mcp_runtime_contract_parse_failed:expected ident at line 1 column 2"
        );
    }

    #[test]
    fn mcp_runtime_contract_rejects_wrong_schema() {
        assert_eq!(
            parse_mcp_runtime_contract(&invalid_contract_json(|contract| {
                contract.schema = "narada.agent_tui.wrong_mcp_runtime_contract.v0".to_string();
            }))
            .unwrap_err(),
            "mcp_runtime_contract_invalid:schema"
        );
    }

    #[test]
    fn mcp_runtime_contract_rejects_wrong_path_policy() {
        assert_eq!(
            parse_mcp_runtime_contract(&invalid_contract_json(|contract| {
                contract.mcp_config_path_policy = "prefix_only".to_string();
            }))
            .unwrap_err(),
            "mcp_runtime_contract_invalid:mcp_config_path_policy"
        );
    }

    #[test]
    fn bundled_mcp_runtime_contract_is_valid() {
        let contract = mcp_runtime_contract();

        assert_eq!(contract.mcp_fabric_env_var, EXPECTED_MCP_FABRIC_ENV_VAR);
        assert_eq!(contract.mcp_config_env_var, EXPECTED_MCP_CONFIG_ENV_VAR);
        assert_eq!(
            contract.site_mcp_fabric_env_var,
            EXPECTED_SITE_MCP_FABRIC_ENV_VAR
        );
        assert_eq!(
            contract.mcp_config_path_policy,
            EXPECTED_MCP_CONFIG_PATH_POLICY
        );
    }
}
