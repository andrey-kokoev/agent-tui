# Agent TUI Architecture

`agent-tui` is a Ratatui projection client for a Narada Agent Runtime Server
(NARS) session. It does not own provider execution, MCP execution, turn
admission, session persistence, authority decisions, or durable transcript
semantics.

## Ownership And Migration

| Concern | Canonical owner | TUI responsibility | Migration state |
| --- | --- | --- | --- |
| Provider selection and execution | NARS provider runtime | Render provider events | Moved |
| MCP fabric and tool calls | NARS capability gateway | Render tool request/result events | Moved |
| Session, turn, queue, cancel, close | NARS session core | Submit protocol requests and render state | Moved |
| Durable events and replay cursor | NARS event log and event stream | Subscribe, replay, deduplicate, reconnect | Implemented |
| Transcript projection | Shared event vocabulary plus TUI renderer | Normalize NARS events into the existing projection model | Implemented |
| Terminal lifecycle and input | Rust TUI | Own Ratatui, composer, scroll, and local display state | Retained |
| Launch selection | Narada launch matrix and launcher | Resolve `--launch-binding` or use `--attach <event_endpoint>` | Implemented |
| Legacy control/session JSONL runtime | None in TUI | No production ownership | Rejected by CLI |

## Dependency Graph

```text
Narada launcher
  -> NARS runtime server
      -> provider runtime
      -> MCP capability gateway
      -> durable event log
      -> WebSocket /events
          -> agent-tui NARS projection client
              -> event normalization
              -> TranscriptStore and AppViewModel
              -> Ratatui renderer and composer
```

The attach client uses the NARS session methods
`session.events.subscribe`, `session.submit`, `session.cancel`,
`session.health`, `session.recovery`, and `session.close`. Replay resumes from
the last durable sequence and also deduplicates event ids, so reconnects cannot
duplicate transcript rows.

`projection_state` contains only render-facing turn state and identity context;
queue admission, directive holding, turn transitions, and durable event writes
are received from NARS events rather than decided locally.

## Deletion Ledger

- Completed: removed `AgentTuiInteractiveRuntime` construction from the
  production binary.
- Completed: removed TUI provider adapter, MCP process, JSONL watcher,
  turn-coordinator, queue-admission, and legacy interactive-loop modules.
- Completed: CLI rejects legacy runtime-loop, control-JSONL, and session-JSONL
  launch flags; projection launches attach through the NARS event endpoint.
- Retained: Ratatui rendering, textarea input, scroll behavior, event
  normalization, replay/deduplication, and focused projection tests.
