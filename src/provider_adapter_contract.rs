use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

// Frozen local snapshot of the former narada
// `packages/carrier-provider-contract/contracts/provider-adapters.json`.
// The upstream package was removed from the narada repo in commit 6cc27e7b
// ("feat: complete invokable intelligence cutover (#2180-#2186)") and has no
// same-shape successor; provider/model selection authority moved to the
// canonical invokable-intelligence registry. This contract is agent-tui-local
// posture, so the last upstream revision is embedded here.
const PROVIDER_ADAPTER_CONTRACT_JSON: &str = r#"{
  "schema": "narada.agent_tui.provider_adapter_contract.v0",
  "provider_execution_env_var": "NARADA_AGENT_TUI_ENABLE_PROVIDER_EXECUTION",
  "provider_adapter_kind_env_var": "NARADA_AGENT_TUI_PROVIDER_ADAPTER_KIND",
  "intelligence_provider_env_var": "NARADA_INTELLIGENCE_PROVIDER",
  "provider_model_env_var": "KIMI_CODE_MODEL",
  "ai_thinking_env_var": "NARADA_AI_THINKING",
  "ai_stream_env_var": "NARADA_AI_STREAM",
  "admitted_providers": [
    "codex-subscription",
    "kimi-api",
    "kimi-code-api",
    "openai-api",
    "anthropic-api",
    "deepseek-api",
    "glm-api",
    "openrouter-api"
  ],
  "scripted_provider_adapter_kind": "scripted_provider_adapter",
  "production_provider_adapter_kind": "codex_subscription_adapter",
  "production_provider_adapter_implemented": true
}
"#;
const EXPECTED_SCHEMA: &str = "narada.agent_tui.provider_adapter_contract.v0";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderAdapterContract {
    pub schema: String,
    pub provider_execution_env_var: String,
    pub provider_adapter_kind_env_var: String,
    pub intelligence_provider_env_var: String,
    #[serde(alias = "provider_model_env_var")]
    pub ai_model_env_var: String,
    pub ai_thinking_env_var: String,
    pub ai_stream_env_var: String,
    pub admitted_providers: Vec<String>,
    pub scripted_provider_adapter_kind: String,
    pub production_provider_adapter_kind: String,
    pub production_provider_adapter_implemented: bool,
}

static PROVIDER_ADAPTER_CONTRACT: OnceLock<ProviderAdapterContract> = OnceLock::new();

pub fn provider_adapter_contract() -> &'static ProviderAdapterContract {
    PROVIDER_ADAPTER_CONTRACT.get_or_init(|| {
        parse_provider_adapter_contract(PROVIDER_ADAPTER_CONTRACT_JSON)
            .expect("agent-tui provider adapter contract is valid")
    })
}

pub fn parse_provider_adapter_contract(json: &str) -> Result<ProviderAdapterContract, String> {
    let contract: ProviderAdapterContract = serde_json::from_str(json)
        .map_err(|error| format!("provider_adapter_contract_parse_failed:{error}"))?;
    if contract.schema.trim() != EXPECTED_SCHEMA {
        return Err("provider_adapter_contract_invalid:schema".to_string());
    }
    if contract.provider_execution_env_var.trim().is_empty() {
        return Err("provider_adapter_contract_invalid:provider_execution_env_var".to_string());
    }
    if contract.provider_adapter_kind_env_var.trim().is_empty() {
        return Err("provider_adapter_contract_invalid:provider_adapter_kind_env_var".to_string());
    }
    if contract.intelligence_provider_env_var.trim().is_empty() {
        return Err("provider_adapter_contract_invalid:intelligence_provider_env_var".to_string());
    }
    if contract.ai_model_env_var.trim().is_empty() {
        return Err("provider_adapter_contract_invalid:ai_model_env_var".to_string());
    }
    if contract.ai_thinking_env_var.trim().is_empty() {
        return Err("provider_adapter_contract_invalid:ai_thinking_env_var".to_string());
    }
    if contract.ai_stream_env_var.trim().is_empty() {
        return Err("provider_adapter_contract_invalid:ai_stream_env_var".to_string());
    }
    if contract.admitted_providers.is_empty()
        || contract
            .admitted_providers
            .iter()
            .any(|provider| provider.trim().is_empty())
    {
        return Err("provider_adapter_contract_invalid:admitted_providers".to_string());
    }
    if contract.scripted_provider_adapter_kind.trim().is_empty() {
        return Err("provider_adapter_contract_invalid:scripted_provider_adapter_kind".to_string());
    }
    if contract.production_provider_adapter_kind.trim().is_empty() {
        return Err(
            "provider_adapter_contract_invalid:production_provider_adapter_kind".to_string(),
        );
    }
    if !contract.production_provider_adapter_implemented {
        return Err(
            "provider_adapter_contract_invalid:production_provider_adapter_implemented".to_string(),
        );
    }
    Ok(contract)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid_contract_json(mut mutate: impl FnMut(&mut ProviderAdapterContract)) -> String {
        let mut contract = provider_adapter_contract().clone();
        mutate(&mut contract);
        serde_json::to_string(&contract).expect("test provider adapter contract serializes")
    }

    #[test]
    fn bundled_provider_adapter_contract_is_valid() {
        let contract = provider_adapter_contract();

        assert!(!contract.provider_execution_env_var.is_empty());
        assert!(!contract.provider_adapter_kind_env_var.is_empty());
        assert!(!contract.intelligence_provider_env_var.is_empty());
        assert!(!contract.ai_model_env_var.is_empty());
        assert!(!contract.ai_thinking_env_var.is_empty());
        assert!(!contract.ai_stream_env_var.is_empty());
        assert!(!contract.admitted_providers.is_empty());
        assert!(!contract.scripted_provider_adapter_kind.is_empty());
        assert!(!contract.production_provider_adapter_kind.is_empty());
        assert!(contract.production_provider_adapter_implemented);
    }

    #[test]
    fn provider_adapter_contract_rejects_invalid_posture() {
        assert_eq!(
            parse_provider_adapter_contract("not json").unwrap_err(),
            "provider_adapter_contract_parse_failed:expected ident at line 1 column 2"
        );
        assert_eq!(
            parse_provider_adapter_contract(&invalid_contract_json(|contract| {
                contract.schema = "narada.agent_tui.wrong_provider_adapter_contract.v0".to_string();
            }))
            .unwrap_err(),
            "provider_adapter_contract_invalid:schema"
        );
        assert_eq!(
            parse_provider_adapter_contract(&invalid_contract_json(|contract| {
                contract.production_provider_adapter_implemented = false;
            }))
            .unwrap_err(),
            "provider_adapter_contract_invalid:production_provider_adapter_implemented"
        );
    }
}
