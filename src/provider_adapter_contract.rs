use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const PROVIDER_ADAPTER_CONTRACT_JSON: &str = include_str!("../contracts/provider-adapters.json");
const EXPECTED_SCHEMA: &str = "narada.agent_tui.provider_adapter_contract.v0";
const EXPECTED_PROVIDER_EXECUTION_ENV_VAR: &str = "NARADA_AGENT_TUI_ENABLE_PROVIDER_EXECUTION";
const EXPECTED_PROVIDER_ADAPTER_KIND_ENV_VAR: &str = "NARADA_AGENT_TUI_PROVIDER_ADAPTER_KIND";
const EXPECTED_INTELLIGENCE_PROVIDER_ENV_VAR: &str = "NARADA_INTELLIGENCE_PROVIDER";
const EXPECTED_AI_MODEL_ENV_VAR: &str = "NARADA_AI_MODEL";
const EXPECTED_AI_THINKING_ENV_VAR: &str = "NARADA_AI_THINKING";
const EXPECTED_AI_STREAM_ENV_VAR: &str = "NARADA_AI_STREAM";
const EXPECTED_ADMITTED_PROVIDERS: [&str; 3] =
    ["codex-subscription", "openai-api", "anthropic-api"];
const EXPECTED_SCRIPTED_PROVIDER_ADAPTER_KIND: &str = "scripted_provider_adapter";
const EXPECTED_PRODUCTION_PROVIDER_ADAPTER_KIND: &str = "codex_subscription_adapter";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderAdapterContract {
    pub schema: String,
    pub provider_execution_env_var: String,
    pub provider_adapter_kind_env_var: String,
    pub intelligence_provider_env_var: String,
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
    if contract.provider_execution_env_var.trim() != EXPECTED_PROVIDER_EXECUTION_ENV_VAR {
        return Err("provider_adapter_contract_invalid:provider_execution_env_var".to_string());
    }
    if contract.provider_adapter_kind_env_var.trim() != EXPECTED_PROVIDER_ADAPTER_KIND_ENV_VAR {
        return Err("provider_adapter_contract_invalid:provider_adapter_kind_env_var".to_string());
    }
    if contract.intelligence_provider_env_var.trim() != EXPECTED_INTELLIGENCE_PROVIDER_ENV_VAR {
        return Err("provider_adapter_contract_invalid:intelligence_provider_env_var".to_string());
    }
    if contract.ai_model_env_var.trim() != EXPECTED_AI_MODEL_ENV_VAR {
        return Err("provider_adapter_contract_invalid:ai_model_env_var".to_string());
    }
    if contract.ai_thinking_env_var.trim() != EXPECTED_AI_THINKING_ENV_VAR {
        return Err("provider_adapter_contract_invalid:ai_thinking_env_var".to_string());
    }
    if contract.ai_stream_env_var.trim() != EXPECTED_AI_STREAM_ENV_VAR {
        return Err("provider_adapter_contract_invalid:ai_stream_env_var".to_string());
    }
    if contract.admitted_providers != EXPECTED_ADMITTED_PROVIDERS {
        return Err("provider_adapter_contract_invalid:admitted_providers".to_string());
    }
    if contract.scripted_provider_adapter_kind.trim() != EXPECTED_SCRIPTED_PROVIDER_ADAPTER_KIND {
        return Err("provider_adapter_contract_invalid:scripted_provider_adapter_kind".to_string());
    }
    if contract.production_provider_adapter_kind.trim() != EXPECTED_PRODUCTION_PROVIDER_ADAPTER_KIND
    {
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

        assert_eq!(
            contract.provider_execution_env_var,
            EXPECTED_PROVIDER_EXECUTION_ENV_VAR
        );
        assert_eq!(
            contract.provider_adapter_kind_env_var,
            EXPECTED_PROVIDER_ADAPTER_KIND_ENV_VAR
        );
        assert_eq!(
            contract.intelligence_provider_env_var,
            EXPECTED_INTELLIGENCE_PROVIDER_ENV_VAR
        );
        assert_eq!(contract.ai_model_env_var, EXPECTED_AI_MODEL_ENV_VAR);
        assert_eq!(contract.ai_thinking_env_var, EXPECTED_AI_THINKING_ENV_VAR);
        assert_eq!(contract.ai_stream_env_var, EXPECTED_AI_STREAM_ENV_VAR);
        assert_eq!(contract.admitted_providers, EXPECTED_ADMITTED_PROVIDERS);
        assert_eq!(
            contract.scripted_provider_adapter_kind,
            EXPECTED_SCRIPTED_PROVIDER_ADAPTER_KIND
        );
        assert_eq!(
            contract.production_provider_adapter_kind,
            EXPECTED_PRODUCTION_PROVIDER_ADAPTER_KIND
        );
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
