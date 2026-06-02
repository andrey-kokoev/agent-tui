use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const LAUNCH_SLICE_CONTRACT_JSON: &str = include_str!("../contracts/launch-slice.json");
const EXPECTED_SCHEMA: &str = "narada.agent_tui.launch_slice_contract.v0";
const EXPECTED_ADMITTED_RUNTIME_SLICE: &str = "terminal_interactive_loop";
const EXPECTED_CARRIER_FLAG: &str = "--interactive-loop";
const EXPECTED_TOOL_FABRIC_ADAPTER_KIND: &str = "narada-agent-tui-terminal-interactive-loop";
const EXPECTED_CAPABILITY_POLICY_TERMINAL_SESSION: &str =
    "interactive_terminal_control_jsonl_session_jsonl";
const EXPECTED_TERMINAL_MODE: bool = true;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LaunchSliceContract {
    pub schema: String,
    pub admitted_runtime_slice: String,
    pub carrier_flag: String,
    pub tool_fabric_adapter_kind: String,
    pub capability_policy_terminal_session: String,
    pub terminal_mode: bool,
}

static LAUNCH_SLICE_CONTRACT: OnceLock<LaunchSliceContract> = OnceLock::new();

pub fn launch_slice_contract() -> &'static LaunchSliceContract {
    LAUNCH_SLICE_CONTRACT.get_or_init(|| {
        parse_launch_slice_contract(LAUNCH_SLICE_CONTRACT_JSON)
            .expect("bundled launch slice contract must be valid")
    })
}

pub fn parse_launch_slice_contract(json_text: &str) -> Result<LaunchSliceContract, String> {
    let contract: LaunchSliceContract = serde_json::from_str(json_text)
        .map_err(|error| format!("launch_slice_contract_parse_failed:{error}"))?;
    if contract.schema != EXPECTED_SCHEMA {
        return Err("launch_slice_contract_invalid:schema".to_string());
    }
    if contract.admitted_runtime_slice != EXPECTED_ADMITTED_RUNTIME_SLICE {
        return Err("launch_slice_contract_invalid:admitted_runtime_slice".to_string());
    }
    if contract.carrier_flag != EXPECTED_CARRIER_FLAG {
        return Err("launch_slice_contract_invalid:carrier_flag".to_string());
    }
    if contract.tool_fabric_adapter_kind != EXPECTED_TOOL_FABRIC_ADAPTER_KIND {
        return Err("launch_slice_contract_invalid:tool_fabric_adapter_kind".to_string());
    }
    if contract.capability_policy_terminal_session != EXPECTED_CAPABILITY_POLICY_TERMINAL_SESSION {
        return Err("launch_slice_contract_invalid:capability_policy_terminal_session".to_string());
    }
    if contract.terminal_mode != EXPECTED_TERMINAL_MODE {
        return Err("launch_slice_contract_invalid:terminal_mode".to_string());
    }
    Ok(contract)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid_contract_json(mut mutate: impl FnMut(&mut LaunchSliceContract)) -> String {
        let mut contract = launch_slice_contract().clone();
        mutate(&mut contract);
        serde_json::to_string(&contract).expect("test launch slice contract serializes")
    }

    #[test]
    fn bundled_launch_slice_contract_is_valid() {
        let contract = launch_slice_contract();

        assert_eq!(contract.carrier_flag, EXPECTED_CARRIER_FLAG);
        assert_eq!(
            contract.admitted_runtime_slice,
            EXPECTED_ADMITTED_RUNTIME_SLICE
        );
        assert_eq!(
            contract.tool_fabric_adapter_kind,
            EXPECTED_TOOL_FABRIC_ADAPTER_KIND
        );
        assert_eq!(
            contract.capability_policy_terminal_session,
            EXPECTED_CAPABILITY_POLICY_TERMINAL_SESSION
        );
        assert_eq!(contract.terminal_mode, EXPECTED_TERMINAL_MODE);
    }

    #[test]
    fn launch_slice_contract_parser_rejects_invalid_contracts() {
        assert!(
            parse_launch_slice_contract("{")
                .unwrap_err()
                .starts_with("launch_slice_contract_parse_failed:")
        );
        assert_eq!(
            parse_launch_slice_contract(&invalid_contract_json(|contract| {
                contract.admitted_runtime_slice =
                    "bounded_non_terminal_interactive_step_once".to_string();
            }))
            .unwrap_err(),
            "launch_slice_contract_invalid:admitted_runtime_slice"
        );
        assert_eq!(
            parse_launch_slice_contract(&invalid_contract_json(|contract| {
                contract.carrier_flag = "--wrong-step".to_string();
            }))
            .unwrap_err(),
            "launch_slice_contract_invalid:carrier_flag"
        );
    }
}
