# Agent TUI Governed Session Acceptance

## Purpose

This acceptance verifies that `agent-tui` can run a governed provider-backed session while keeping provider execution, MCP execution, and terminal rendering as explicit launch admissions.

## Canonical launch

Run from `D:\code\narada`:

```powershell
pwsh -File .\narada.ps1 agent-start -Agent narada.resident -Runtime agent-tui -Exec -AgentTuiInteractiveLoop -AgentTuiProviderExecution -AgentTuiMcpFabric -AgentTuiMaxSteps 10000
```

Do not use `-Json` for operator UX inspection.

## Expected launch posture

The launch result must show:

- `agent_tui_launch.provider_execution_enabled: true`
- `agent_tui_launch.mcp_fabric_access_enabled: true`
- provider gate status `admitted_by_explicit_governed_session_flag`
- MCP gate status `admitted_by_explicit_governed_session_flag`
- terminal rendering admitted for `interactive_loop`
- native shell authority still false/unadmitted

## Manual scenario

1. Launch with the canonical command.
2. Enter `run startup sequence`.
3. If the provider emits text, confirm it appears under the agent identity in the transcript.
4. If the provider emits a Narada tool-call request, confirm the TUI shows carrier-mediated request/result flow:
   - `agent -> agent-tui`
   - `agent-tui -> agent`
5. While the turn is active, type an operator draft without submitting.
6. Append a system control input to `control.jsonl`.
7. Confirm held system directive count is visible while the draft is nonempty.
8. Submit or clear the draft and confirm the held system input releases.
9. Exit with `Ctrl+C` and confirm PowerShell is normal.

## Evidence checks

Inspect `session.jsonl` and verify:

- provider request status is not `recorded_not_dispatched` for the governed turn
- `provider_execution_enabled` is `true`
- provider adapter admission status is `admitted`
- provider adapter kind is `codex_subscription_adapter`
- provider text deltas or provider tool-call requests are recorded
- MCP `tool_call_requested` and `tool_result_received` appear only when the requested tool is policy-visible
- operator input remains `source_kind=operator` and `transport=interactive_terminal`
- system input remains `source_kind=system` and `transport=control_jsonl`
- no native shell authority is recorded as admitted

## Known limits

The first production adapter uses bounded `codex exec --json` semantics. It does not grant native Codex tool execution; Narada tool use remains carrier-mediated through `agent-tui` and Site MCP fabric policy.
