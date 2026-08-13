# Agent TUI Alpha Loop Acceptance

## Purpose

Verify that `agent-tui` is a projection client attached to a Narada Agent Runtime Session (NARS). NARS owns the provider, MCP, terminal authority, turn lifecycle, and durable event log; the TUI owns only the interactive projection.

## Canonical launch

Run the Narada workspace launcher with the `narada-agent-runtime-server` runtime and the `agent-tui` projection selected. Start the TUI from the emitted projection tab, or attach directly with the binding produced by the launcher:

```powershell
cargo run --manifest-path <src-root>\agent-tui\Cargo.toml --bin narada-agent-tui -- --launch-binding <launch-binding-path> --identity <canonical-agent-id>
```

The binding must resolve to the NARS WebSocket event endpoint. Do not pass the removed legacy runtime flags or `--site-root`, `--control-jsonl`, or `--session-jsonl`.

## Manual scenario

1. Launch the workspace and start the emitted TUI projection.
2. Confirm the transcript renders the existing replay and the current session identity.
3. Enter `run startup sequence` and press Enter.
4. Confirm the TUI shows the resulting NARS event projections under the agent identity.
5. Press PageUp after the transcript reaches the replay boundary; confirm an older durable event page is loaded and appears in chronological order.
6. Press PageDown and confirm the view returns toward the live tail.
7. Type a draft, then cancel or clear it and confirm the composer remains responsive.
8. Exit with `Esc` or `Ctrl+C` and confirm the terminal returns to its prior state.

## Pass criteria

- The TUI starts only through `--attach` or `--launch-binding`.
- Startup and submitted prompts travel through NARS session methods, not a TUI-owned control file.
- Transcript replay, live events, and older-page reads remain chronologically ordered and deduplicated after reconnect.
- PageUp requests older durable history through `session.events.read` when the loaded page is exhausted.
- The compact status line does not display `idle`, provider, MCP, terminal, or carrier-runtime ownership fields.
- No `control.jsonl` or `session.jsonl` file is read or written by `agent-tui`.
- Terminal cleanup is correct on normal exit and interrupted input.

## Evidence

Use the NARS session event and request surfaces for protocol evidence. The TUI may send only projection-bound session requests such as `session.events.subscribe`, `session.events.read`, `session.submit`, `session.cancel`, and `session.close`; provider and MCP process evidence remains NARS-owned.
