use crate::mcp_fabric_transport::McpFabricTransportClient;
use crate::mcp_runtime_config::McpRuntimeConfig;
use crate::provider_adapter_admission::ProviderAdapterAdmission;
use crate::provider_adapter_contract::provider_adapter_contract;
use crate::provider_runtime_config::ProviderRuntimeConfig;
use crate::status_view_model::RuntimePostureState;
use crate::terminal_runtime_config::TerminalRuntimeConfig;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfigSnapshot {
    pub provider: ProviderRuntimeConfig,
    pub provider_adapter: ProviderAdapterAdmission,
    pub mcp: McpRuntimeConfig,
    pub terminal: TerminalRuntimeConfig,
}
impl RuntimeConfigSnapshot {
    pub fn from_env_map(env_map: &BTreeMap<String, String>) -> Self {
        Self::from_env_map_with_mcp_config_readiness(env_map, |_| Ok(()))
    }

    pub fn from_process_env_map(env_map: &BTreeMap<String, String>) -> Self {
        Self::from_env_map_with_mcp_config_readiness(env_map, mcp_config_file_readiness)
    }

    pub fn from_env_map_with_mcp_config_readiness(
        env_map: &BTreeMap<String, String>,
        config_readiness: impl Fn(&str) -> Result<(), String>,
    ) -> Self {
        let provider = ProviderRuntimeConfig::from_env_map(env_map);
        let provider_adapter_kind = env_map
            .get(&provider_adapter_contract().provider_adapter_kind_env_var)
            .map(String::as_str);
        let provider_adapter =
            ProviderAdapterAdmission::from_runtime_config(&provider, provider_adapter_kind);
        Self {
            provider,
            provider_adapter,
            mcp: McpRuntimeConfig::from_env_map_with_config_readiness(env_map, config_readiness),
            terminal: TerminalRuntimeConfig::from_env_map(env_map),
        }
    }

    pub fn posture(&self) -> RuntimePostureState {
        RuntimePostureState::from_runtime_configs(
            &self.provider,
            &self.provider_adapter,
            &self.mcp,
            &self.terminal,
        )
    }
}

fn mcp_config_file_readiness(path: &str) -> Result<(), String> {
    if !Path::new(path).is_file() {
        return Err("mcp_config_unreadable".to_string());
    }
    let client = McpFabricTransportClient::from_path(path).map_err(normalize_mcp_config_error)?;
    if client.servers.is_empty() {
        return Err("mcp_config_missing_mcp_servers".to_string());
    }
    Ok(())
}

fn normalize_mcp_config_error(error: String) -> String {
    if error.starts_with("mcp_fabric_config_read_failed:") {
        "mcp_config_unreadable".to_string()
    } else if error.starts_with("mcp_fabric_config_parse_failed:") {
        "mcp_config_parse_failed".to_string()
    } else {
        format!("mcp_config_invalid:{error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp_runtime_contract::mcp_runtime_contract;
    use crate::terminal_runtime_contract::terminal_runtime_contract;

    #[test]
    fn snapshot_reads_narada_env_map_once() {
        let provider_contract = provider_adapter_contract();
        let mcp_contract = mcp_runtime_contract();
        let terminal_contract = terminal_runtime_contract();
        let env_map = BTreeMap::from([
            (
                provider_contract.provider_execution_env_var.clone(),
                "true".to_string(),
            ),
            (
                provider_contract.intelligence_provider_env_var.clone(),
                provider_contract.admitted_providers[0].clone(),
            ),
            (
                provider_contract.ai_model_env_var.clone(),
                "gpt-5.5".to_string(),
            ),
            (
                provider_contract.provider_adapter_kind_env_var.clone(),
                provider_contract.production_provider_adapter_kind.clone(),
            ),
            (mcp_contract.mcp_fabric_env_var.clone(), "true".to_string()),
            (
                mcp_contract.mcp_config_env_var.clone(),
                "D:/site/.ai/mcp/config.json".to_string(),
            ),
            (
                mcp_contract.site_mcp_fabric_env_var.clone(),
                "D:/site/.ai/mcp".to_string(),
            ),
            (
                terminal_contract.terminal_rendering_env_var.clone(),
                "true".to_string(),
            ),
            (
                terminal_contract.terminal_mode_env_var.clone(),
                terminal_contract.required_terminal_mode.clone(),
            ),
        ]);

        let snapshot = RuntimeConfigSnapshot::from_env_map(&env_map);
        let posture = snapshot.posture();

        assert_eq!(snapshot.provider.status.as_str(), "configured");
        assert_eq!(snapshot.provider_adapter.status.as_str(), "refused");
        assert_eq!(
            snapshot.provider_adapter.adapter_kind.as_deref(),
            Some(provider_contract.production_provider_adapter_kind.as_str())
        );
        assert_eq!(snapshot.mcp.status.as_str(), "configured");
        assert_eq!(snapshot.terminal.status.as_str(), "configured");
        assert_eq!(posture.provider_state.as_str(), "configured");
        assert_eq!(posture.provider_adapter_state.as_str(), "refused");
        assert_eq!(posture.mcp_state.as_str(), "configured");
        assert_eq!(posture.terminal_state.as_str(), "configured");
    }
}
