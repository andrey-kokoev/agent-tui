use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const OPERATOR_ROUTING_CONTRACT_JSON: &str =
    include_str!("../../narada/packages/carrier-routing-contract/contracts/operator-routing.json");
const EXPECTED_SCHEMA: &str = "narada.carrier.operator_routing_contract.v1";
const EXPECTED_STARTUP_ROUTE_ID: &str = "startup_sequence";
const EXPECTED_STARTUP_TOOL_NAME: &str = "agent_context_startup_sequence";
const EXPECTED_OUTPUT_READER_ROUTE_ID: &str = "mcp_output_reader";
const EXPECTED_OUTPUT_READER_TOOL_NAME: &str = "mcp_output_show";
const EXPECTED_OUTPUT_REF_PREFIX: &str = "mcp_output:";
const EXPECTED_TOOL_CALL_ENVELOPE_FIELD: &str = "narada_tool_call";
const EXPECTED_STARTUP_ALIAS_GROUP_ID: &str = "startup_sequence";
const EXPECTED_PAYLOAD_READER_ALIAS_GROUP_ID: &str = "mcp_payload_reader";
const EXPECTED_PAYLOAD_READER_PRIMARY_TOOL: &str = "mcp_payload_show";
const EXPECTED_PAYLOAD_READER_ALIAS_TOOL: &str = "mcp_payload_read";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OperatorRoutingContract {
    pub schema: String,
    pub direct_tool_routes: Vec<DirectToolRoute>,
    pub reader_routes: Vec<ReaderRoute>,
    pub tool_call_envelope: ToolCallEnvelope,
    pub tool_alias_groups: Vec<ToolAliasGroup>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DirectToolRoute {
    pub id: String,
    pub phrases: Vec<String>,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReaderRoute {
    pub id: String,
    pub phrases: Vec<String>,
    pub ref_prefix: String,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolCallEnvelope {
    pub field: String,
    pub fenced_json_admitted: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolAliasGroup {
    pub id: String,
    pub tools: Vec<String>,
}

static OPERATOR_ROUTING_CONTRACT: OnceLock<OperatorRoutingContract> = OnceLock::new();

pub fn operator_routing_contract() -> &'static OperatorRoutingContract {
    OPERATOR_ROUTING_CONTRACT.get_or_init(|| {
        parse_operator_routing_contract(OPERATOR_ROUTING_CONTRACT_JSON)
            .expect("bundled operator routing contract must be valid")
    })
}

pub fn parse_operator_routing_contract(json_text: &str) -> Result<OperatorRoutingContract, String> {
    let contract: OperatorRoutingContract = serde_json::from_str(json_text)
        .map_err(|error| format!("operator_routing_contract_parse_failed:{error}"))?;
    if contract.schema != EXPECTED_SCHEMA {
        return Err("operator_routing_contract_invalid:schema".to_string());
    }
    let startup_route = contract
        .direct_tool_routes
        .iter()
        .find(|route| route.id == EXPECTED_STARTUP_ROUTE_ID)
        .ok_or_else(|| "operator_routing_contract_invalid:startup_route_missing".to_string())?;
    if startup_route.tool_name != EXPECTED_STARTUP_TOOL_NAME {
        return Err("operator_routing_contract_invalid:startup_tool_name".to_string());
    }
    if startup_route.phrases.is_empty() {
        return Err("operator_routing_contract_invalid:startup_phrases".to_string());
    }
    if !startup_route.arguments.is_object() {
        return Err("operator_routing_contract_invalid:startup_arguments".to_string());
    }

    let output_reader_route = contract
        .reader_routes
        .iter()
        .find(|route| route.id == EXPECTED_OUTPUT_READER_ROUTE_ID)
        .ok_or_else(|| {
            "operator_routing_contract_invalid:output_reader_route_missing".to_string()
        })?;
    if output_reader_route.tool_name != EXPECTED_OUTPUT_READER_TOOL_NAME {
        return Err("operator_routing_contract_invalid:output_reader_tool_name".to_string());
    }
    if output_reader_route.ref_prefix != EXPECTED_OUTPUT_REF_PREFIX {
        return Err("operator_routing_contract_invalid:output_reader_ref_prefix".to_string());
    }
    if output_reader_route.phrases.is_empty() {
        return Err("operator_routing_contract_invalid:output_reader_phrases".to_string());
    }
    if output_reader_route
        .arguments
        .get("output_limit")
        .and_then(Value::as_u64)
        .is_none()
    {
        return Err("operator_routing_contract_invalid:output_reader_limit".to_string());
    }

    if contract.tool_call_envelope.field != EXPECTED_TOOL_CALL_ENVELOPE_FIELD {
        return Err("operator_routing_contract_invalid:tool_call_envelope_field".to_string());
    }
    if !contract.tool_call_envelope.fenced_json_admitted {
        return Err("operator_routing_contract_invalid:fenced_json_admission".to_string());
    }
    let startup_aliases = required_alias_group(&contract, EXPECTED_STARTUP_ALIAS_GROUP_ID)?;
    require_alias_tool(startup_aliases, EXPECTED_STARTUP_TOOL_NAME)?;
    require_alias_tool(startup_aliases, "startup_sequence")?;
    let payload_reader_aliases =
        required_alias_group(&contract, EXPECTED_PAYLOAD_READER_ALIAS_GROUP_ID)?;
    require_alias_tool(payload_reader_aliases, EXPECTED_PAYLOAD_READER_PRIMARY_TOOL)?;
    require_alias_tool(payload_reader_aliases, EXPECTED_PAYLOAD_READER_ALIAS_TOOL)?;
    Ok(contract)
}

fn required_alias_group<'a>(
    contract: &'a OperatorRoutingContract,
    id: &str,
) -> Result<&'a ToolAliasGroup, String> {
    contract
        .tool_alias_groups
        .iter()
        .find(|group| group.id == id)
        .ok_or_else(|| format!("operator_routing_contract_invalid:alias_group_missing:{id}"))
}

fn require_alias_tool(group: &ToolAliasGroup, tool_name: &str) -> Result<(), String> {
    if group.tools.iter().any(|tool| tool == tool_name) {
        Ok(())
    } else {
        Err(format!(
            "operator_routing_contract_invalid:alias_group_tool_missing:{}:{tool_name}",
            group.id
        ))
    }
}

pub fn tool_aliases_for(tool_name: &str) -> Option<&'static [String]> {
    operator_routing_contract()
        .tool_alias_groups
        .iter()
        .find(|group| group.tools.iter().any(|tool| tool == tool_name))
        .map(|group| group.tools.as_slice())
}

pub fn payload_reader_tools() -> &'static [String] {
    required_alias_group(
        operator_routing_contract(),
        EXPECTED_PAYLOAD_READER_ALIAS_GROUP_ID,
    )
    .expect("bundled operator routing contract has payload reader aliases")
    .tools
    .as_slice()
}

pub fn payload_reader_tool_name() -> &'static str {
    payload_reader_tools()
        .first()
        .expect("bundled operator routing contract has primary payload reader")
        .as_str()
}

pub fn output_reader_tool_name() -> &'static str {
    operator_routing_contract()
        .reader_routes
        .iter()
        .find(|route| route.id == EXPECTED_OUTPUT_READER_ROUTE_ID)
        .expect("bundled operator routing contract has output reader route")
        .tool_name
        .as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid_contract_json(mut mutate: impl FnMut(&mut OperatorRoutingContract)) -> String {
        let mut contract = operator_routing_contract().clone();
        mutate(&mut contract);
        serde_json::to_string(&contract).expect("test operator routing contract serializes")
    }

    #[test]
    fn bundled_operator_routing_contract_is_valid() {
        let contract = operator_routing_contract();

        assert_eq!(contract.schema, EXPECTED_SCHEMA);
        assert_eq!(
            contract.tool_call_envelope.field,
            EXPECTED_TOOL_CALL_ENVELOPE_FIELD
        );
        assert!(contract.tool_call_envelope.fenced_json_admitted);
        assert_eq!(
            contract.direct_tool_routes[0].tool_name,
            EXPECTED_STARTUP_TOOL_NAME
        );
        assert_eq!(
            contract.reader_routes[0].tool_name,
            EXPECTED_OUTPUT_READER_TOOL_NAME
        );
        assert_eq!(
            payload_reader_tool_name(),
            EXPECTED_PAYLOAD_READER_PRIMARY_TOOL
        );
        assert_eq!(
            tool_aliases_for(EXPECTED_STARTUP_TOOL_NAME).expect("startup aliases exist"),
            &[
                EXPECTED_STARTUP_TOOL_NAME.to_string(),
                "startup_sequence".to_string()
            ]
        );
    }

    #[test]
    fn operator_routing_contract_parser_rejects_invalid_contracts() {
        assert!(
            parse_operator_routing_contract("{")
                .unwrap_err()
                .starts_with("operator_routing_contract_parse_failed:")
        );
        assert_eq!(
            parse_operator_routing_contract(&invalid_contract_json(|contract| {
                contract.schema = "narada.carrier.wrong_operator_routing_contract.v1".to_string();
            }))
            .unwrap_err(),
            "operator_routing_contract_invalid:schema"
        );
        assert_eq!(
            parse_operator_routing_contract(&invalid_contract_json(|contract| {
                contract.reader_routes[0].ref_prefix = "payload:".to_string();
            }))
            .unwrap_err(),
            "operator_routing_contract_invalid:output_reader_ref_prefix"
        );
        assert_eq!(
            parse_operator_routing_contract(&invalid_contract_json(|contract| {
                contract.tool_alias_groups[1].tools = vec!["mcp_payload_show".to_string()];
            }))
            .unwrap_err(),
            "operator_routing_contract_invalid:alias_group_tool_missing:mcp_payload_reader:mcp_payload_read"
        );
    }
}
