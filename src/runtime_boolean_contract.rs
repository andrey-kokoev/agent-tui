use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const RUNTIME_BOOLEAN_VALUES_CONTRACT_JSON: &str = include_str!(
    "../../narada/packages/operator-surface-runtime-contract/contracts/boolean-values.json"
);
const EXPECTED_SCHEMA: &str = "narada.carrier.runtime_boolean_values.v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RuntimeBooleanValuesContract {
    pub schema: String,
    pub truthy: Vec<String>,
    pub falsey: Vec<String>,
}

static RUNTIME_BOOLEAN_VALUES_CONTRACT: OnceLock<RuntimeBooleanValuesContract> = OnceLock::new();

pub fn runtime_boolean_values_contract() -> &'static RuntimeBooleanValuesContract {
    RUNTIME_BOOLEAN_VALUES_CONTRACT.get_or_init(|| {
        parse_runtime_boolean_values_contract(RUNTIME_BOOLEAN_VALUES_CONTRACT_JSON)
            .expect("bundled runtime boolean values contract must be valid")
    })
}

pub fn parse_runtime_boolean_values_contract(
    json_text: &str,
) -> Result<RuntimeBooleanValuesContract, String> {
    let contract: RuntimeBooleanValuesContract = serde_json::from_str(json_text)
        .map_err(|error| format!("runtime_boolean_values_contract_parse_failed:{error}"))?;
    if contract.schema != EXPECTED_SCHEMA {
        return Err("runtime_boolean_values_contract_invalid:schema".to_string());
    }
    if contract.truthy.is_empty() {
        return Err("runtime_boolean_values_contract_invalid:truthy".to_string());
    }
    if contract.falsey.is_empty() {
        return Err("runtime_boolean_values_contract_invalid:falsey".to_string());
    }
    Ok(contract)
}

pub fn env_flag_enabled(value: Option<&String>) -> bool {
    normalized_value_is_in(value, &runtime_boolean_values_contract().truthy)
}

pub fn env_flag_disabled(value: Option<&String>) -> bool {
    normalized_value_is_in(value, &runtime_boolean_values_contract().falsey)
}

fn normalized_value_is_in(value: Option<&String>, admitted: &[String]) -> bool {
    let Some(normalized) = value.map(|value| value.trim().to_ascii_lowercase()) else {
        return false;
    };
    admitted.iter().any(|candidate| candidate == &normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_runtime_boolean_values_contract_is_valid() {
        let contract = runtime_boolean_values_contract();

        assert_eq!(contract.schema, EXPECTED_SCHEMA);
        assert!(env_flag_enabled(Some(&" yes ".to_string())));
        assert!(env_flag_disabled(Some(&" OFF ".to_string())));
        assert!(!env_flag_enabled(Some(&"false".to_string())));
    }
}
