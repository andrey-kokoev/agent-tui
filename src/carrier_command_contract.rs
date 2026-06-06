use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const CARRIER_COMMAND_CONTRACT_JSON: &str =
    include_str!("../../narada/packages/carrier-command-contract/contracts/commands.json");
const EXPECTED_SCHEMA: &str = "narada.carrier.command_contract.v1";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierCommandContract {
    pub schema: String,
    pub commands: Vec<CarrierCommandRecord>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CarrierCommandRecord {
    pub name: String,
    pub primary: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub effect: String,
    pub help: String,
    #[serde(default)]
    pub argument: Option<String>,
}

static CARRIER_COMMAND_CONTRACT: OnceLock<CarrierCommandContract> = OnceLock::new();

pub fn carrier_command_contract() -> &'static CarrierCommandContract {
    CARRIER_COMMAND_CONTRACT.get_or_init(|| {
        parse_carrier_command_contract(CARRIER_COMMAND_CONTRACT_JSON)
            .expect("bundled carrier command contract must be valid")
    })
}

pub fn parse_carrier_command_contract(json_text: &str) -> Result<CarrierCommandContract, String> {
    let contract: CarrierCommandContract = serde_json::from_str(json_text)
        .map_err(|error| format!("carrier_command_contract_parse_failed:{error}"))?;
    if contract.schema != EXPECTED_SCHEMA {
        return Err("carrier_command_contract_invalid:schema".to_string());
    }
    for required_name in [
        "help",
        "status",
        "goal",
        "stats",
        "model",
        "thinking",
        "tool_output",
        "tools",
        "observers",
        "observer_mute",
        "observer_unmute",
        "queue_show",
        "queue_clear",
        "queue_drop",
        "clear",
        "exit",
    ] {
        command_named_in(&contract, required_name)
            .ok_or_else(|| format!("carrier_command_contract_invalid:missing:{required_name}"))?;
    }
    Ok(contract)
}

pub fn command_named(name: &str) -> Option<&'static CarrierCommandRecord> {
    command_named_in(carrier_command_contract(), name)
}

fn command_named_in<'a>(
    contract: &'a CarrierCommandContract,
    name: &str,
) -> Option<&'a CarrierCommandRecord> {
    contract
        .commands
        .iter()
        .find(|command| command.name == name)
}

pub fn command_name_for_token(token: &str) -> Option<&'static str> {
    let normalized = normalize_command_token(token);
    carrier_command_contract()
        .commands
        .iter()
        .find(|command| {
            normalize_command_token(&command.primary) == normalized
                || command
                    .aliases
                    .iter()
                    .any(|alias| normalize_command_token(alias) == normalized)
        })
        .map(|command| command.name.as_str())
}

pub fn command_tokens() -> Vec<&'static str> {
    carrier_command_contract()
        .commands
        .iter()
        .flat_map(|command| {
            std::iter::once(command.primary.as_str())
                .chain(command.aliases.iter().map(String::as_str))
        })
        .collect()
}

pub fn normalize_command_token(token: &str) -> String {
    token
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_carrier_command_contract_is_valid() {
        let contract = carrier_command_contract();

        assert_eq!(contract.schema, EXPECTED_SCHEMA);
        assert_eq!(
            command_tokens(),
            vec![
                "/help",
                "/status",
                "/goal",
                "/stats",
                "/model",
                "/thinking",
                "/tool-output",
                "/tool-outputs",
                "/tools",
                "/tool",
                "/observers",
                "/observer mute",
                "/observer unmute",
                "/queue",
                "/queue clear",
                "/queue drop <index>",
                "/clear",
                "/exit",
                "/quit",
                "exit",
            ]
        );
        assert_eq!(command_name_for_token(" EXIT "), Some("exit"));
    }

    #[test]
    fn carrier_command_contract_parser_rejects_invalid_contracts() {
        assert!(
            parse_carrier_command_contract("{")
                .unwrap_err()
                .starts_with("carrier_command_contract_parse_failed:")
        );
        let mut contract = carrier_command_contract().clone();
        contract.schema = "wrong".to_string();
        let json = serde_json::to_string(&contract).expect("contract serializes");
        assert_eq!(
            parse_carrier_command_contract(&json).unwrap_err(),
            "carrier_command_contract_invalid:schema"
        );
    }
}
