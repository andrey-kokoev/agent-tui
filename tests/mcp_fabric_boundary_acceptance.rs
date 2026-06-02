use narada_agent_tui::carrier_protocol::{SessionEventKind, parse_session_event};
use narada_agent_tui::mcp_fabric_boundary::{
    McpFabricAccessStatus, McpFabricBoundary, McpFabricPolicy, McpToolRequest, McpToolResult,
};

fn admitted_boundary() -> McpFabricBoundary {
    McpFabricBoundary::admitted(McpFabricPolicy::from_allowed_tools(
        "D:/code/narada.sonar/.ai/mcp",
        "site_mcp_policy_fixture",
        ["site_loop_run_once", "task_lifecycle_next"],
    ))
}

fn context() -> narada_agent_tui::input_queue::SessionEvidenceContext {
    narada_agent_tui::input_queue::SessionEvidenceContext {
        carrier_session_id: "carrier_fixture_1".to_string(),
        agent_id: "sonar.resident".to_string(),
        site_id: "narada-sonar".to_string(),
        site_root: "D:/code/narada.sonar".to_string(),
    }
}

#[test]
fn mcp_fabric_boundary_acceptance_defaults_to_disabled_no_visibility() {
    let boundary = McpFabricBoundary::disabled_until_admitted();

    assert_eq!(boundary.status, McpFabricAccessStatus::Disabled);
    assert_eq!(boundary.status.as_str(), "disabled");
    assert_eq!(boundary.tool_visibility, "none");
    assert_eq!(
        boundary.reason,
        "Rust agent-tui has no admitted Site MCP fabric client"
    );
}

#[test]
fn mcp_fabric_boundary_acceptance_rejects_site_tool_access_until_admitted() {
    let boundary = McpFabricBoundary::disabled_until_admitted();

    let error = boundary
        .assert_tool_access("site_loop_run_once")
        .expect_err("Site MCP tool access must be rejected");

    assert_eq!(
        error,
        "mcp_fabric_access_disabled:site_loop_run_once:Rust agent-tui has no admitted Site MCP fabric client"
    );
}

#[test]
fn mcp_fabric_boundary_acceptance_exposes_policy_bound_visibility_when_admitted() {
    let boundary = admitted_boundary();

    assert_eq!(boundary.status, McpFabricAccessStatus::Admitted);
    assert_eq!(boundary.tool_visibility, "policy_bound");
    assert_eq!(
        boundary.visible_tools(),
        vec![
            "site_loop_run_once".to_string(),
            "task_lifecycle_next".to_string()
        ]
    );
    assert!(boundary.assert_tool_access("site_loop_run_once").is_ok());
    assert_eq!(
        boundary
            .assert_tool_access("shell_exec")
            .expect_err("tool not visible"),
        "mcp_tool_not_visible:shell_exec:site_mcp_policy_fixture"
    );
}

#[test]
fn mcp_fabric_boundary_acceptance_records_tool_request_and_result_evidence() {
    let boundary = admitted_boundary();
    let request = boundary
        .tool_request_event(
            &McpToolRequest {
                tool_name: "site_loop_run_once".to_string(),
                arguments_summary: "{}".to_string(),
                arguments_ref: None,
                requesting_agent_id: "sonar.resident".to_string(),
            },
            &context(),
            "session_event_tool_request_1",
            "2026-05-30T00:00:00.000Z",
        )
        .expect("request admitted");
    let result = boundary
        .tool_result_event(
            &McpToolResult {
                tool_name: "site_loop_run_once".to_string(),
                status: "ok".to_string(),
                duration_ms: 42,
                result_summary: "loop pass complete".to_string(),
                result_ref: None,
            },
            &context(),
            "session_event_tool_result_1",
            "2026-05-30T00:00:01.000Z",
        )
        .expect("result admitted");

    assert_eq!(request.event_kind, SessionEventKind::ToolCallRequested);
    assert_eq!(result.event_kind, SessionEventKind::ToolResultReceived);
    assert!(parse_session_event(&serde_json::to_string(&request).unwrap()).is_ok());
    assert!(parse_session_event(&serde_json::to_string(&result).unwrap()).is_ok());
}

#[test]
fn mcp_fabric_boundary_acceptance_separates_mcp_from_provider_boundary() {
    let boundary = McpFabricBoundary::disabled_until_admitted();

    assert_eq!(boundary.status.as_str(), "disabled");
    assert_eq!(boundary.tool_visibility, "none");
    assert!(boundary.reason.contains("Site MCP fabric client"));
}
