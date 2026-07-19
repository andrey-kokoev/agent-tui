# Recognition Phrases

This document records the behavior that the attach-only `agent-tui` binary
recognizes. NARS owns provider execution, MCP execution, session admission,
turn state, queue state, and durable event persistence.

## Attach CLI

Source: `src/main.rs`, `src/nars_projection.rs`.

| Input | Behavior |
| --- | --- |
| `--attach <event-endpoint>` | Connects directly to the NARS WebSocket event endpoint. |
| `--launch-binding <path>` | Waits for the launch binding and resolves the exact NARS event endpoint for that launch session. |
| `--identity <value>` | Fallback identity used until NARS events provide identity. |
| `--session <value>` | Fallback session id used until NARS events provide session identity. |
| `--max-steps <positive integer>` | Bounds the projection loop for tests. |
| `--check-rust-toolchain` | Runs the local Rust/MSVC readiness check without attaching. |
| `--help`, `--version` | Prints CLI information. |
| `--interactive-loop`, `--control-jsonl`, `--session-jsonl`, `--site-root`, and other legacy runtime flags | Rejected; use `--attach` or `--launch-binding`. |

Exactly one attach source is required for a normal run.

## NARS Requests

Source: `src/nars_projection.rs`.

| Request | Projection behavior |
| --- | --- |
| `session.events.subscribe` | Replays the bounded recent event page and subscribes to live events. |
| `session.events.read` | Reads an older durable event page when upward scroll reaches the loaded history boundary. |
| `session.submit` | Submits composer text to NARS, using active-turn steering metadata when applicable. |
| `session.cancel` | Requests interruption of the active NARS turn. |
| `session.health` | Requests NARS health information. |
| `session.recovery` | Requests NARS recovery information. |
| `session.close` | Requests session close before the projection disconnects. |

The client resumes subscriptions from the last durable sequence after a
connection reset and de-duplicates event ids.

## Event Normalization

NARS event names are normalized into the existing projection vocabulary:

| NARS event family | TUI projection |
| --- | --- |
| Input admission and queue events | Operator transcript rows and queue counters. |
| Turn start and terminal events | Inline `thinking` marker, active phase, and terminal status. |
| Provider request and text events | Agent response rows and calling marker. |
| Tool request and result events | Tool request/result transcript rows. |
| Diagnostic events | Last-error projection. |

Unknown event names remain diagnostic projection events; they do not trigger
local provider, MCP, queue, or turn execution.

## Terminal Input

Source: `src/terminal_input.rs`, `src/terminal_input_tick.rs`.

| Input | Behavior |
| --- | --- |
| Printable text and paste | Edits the local composer. |
| Enter | Sends the composer text through `session.submit`. |
| Escape | Cancels an active turn or clears an idle composer. |
| PageUp and PageDown | Scrolls the transcript; PageUp may request older durable history. |
| Ctrl-C | Exits the projection. |
| Other non-key events | Ignored by the projection loop. |

The composer is local display state only. It does not admit turns or write
durable session records.

## Rendering

Source: `src/ratatui_renderer.rs`, `src/transcript_store.rs`,
`src/transcript_view_model.rs`.

- Transcript rows are projected from NARS events, not from a local session log.
- Streaming provider deltas are coalesced for the same turn.
- Replayed events and overlapping history pages are de-duplicated.
- Older history pages are prepended in chronological order.
- Scroll offset is bottom-relative and clamped to the rendered transcript.
- The compact status line omits idle/provider/MCP/terminal runtime details;
  active `thinking` and `calling` state is shown inline with the transcript.

## Ownership Boundary

The following are intentionally absent from the production TUI loop:

- provider subprocesses and provider adapter admission;
- MCP process supervision and JSON-RPC execution;
- control/session JSONL watchers;
- local turn or queue admission;
- local durable transcript writes;
- legacy interactive runtime modes.

The retained carrier and rendering contracts are compatibility vocabulary and
test fixtures. They are not an alternate runtime owner.
