# Agent TUI Governed Session Acceptance

## Purpose

Verify that a governed provider-backed session remains owned by NARS while `agent-tui` provides the operator projection and composer.

## Canonical launch

Run the Narada workspace launcher with the `narada-agent-runtime-server` runtime, the intended intelligence provider, and the `agent-tui` projection selected. Start the TUI from the emitted projection tab. For a direct attach, use the launch binding created by the launcher:

```powershell
cargo run --manifest-path D:\code\agent-tui\Cargo.toml --bin narada-agent-tui -- --launch-binding <launch-binding-path> --identity <canonical-agent-id>
```

The binding must resolve to the NARS WebSocket event endpoint. Do not pass the removed provider, MCP, interactive-loop, control-JSONL, or session-JSONL flags.

## Manual scenario

1. Start the NARS-backed workspace and the emitted TUI projection.
2. Enter `run startup sequence` or another authorized task.
3. Confirm provider text, tool-call, tool-result, and completion events appear as transcript projections when NARS emits them.
4. While the turn is active, type an operator draft without submitting it.
5. Confirm the draft remains local to the composer and does not change the durable transcript until submitted.
6. Submit the draft and confirm the request is reflected by the NARS event stream.
7. Scroll upward beyond the initial replay page and confirm older events are fetched from NARS, not from a local session file.
8. Exit with `Esc` or `Ctrl+C` and confirm terminal cleanup.

## Evidence checks

- Provider and MCP subprocess ownership remains NARS-owned.
- TUI outbound protocol is limited to event subscription/history reads and session submit/cancel/close requests.
- Reconnect resumes from the last durable sequence and does not duplicate transcript items.
- Provider, MCP, terminal-authority, and turn-lifecycle status is not locally admitted or executed by the TUI.
- Operator input is submitted through NARS and then appears from the event stream.
- No TUI read or write occurs against `control.jsonl` or `session.jsonl`.
- The legacy runtime flags are rejected by the TUI CLI.

## Known boundary

This acceptance validates the projection boundary. Provider policy, tool authorization, MCP execution, terminal authority, checkpointing, and durable session semantics must be validated through NARS-owned diagnostics and event records.
