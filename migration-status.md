# Agent TUI Migration Status

## Current target

`agent-tui` is an attach-only Ratatui projection for a Narada Agent Runtime Session (NARS). It does not start or supervise a provider, MCP server, terminal-authority process, turn coordinator, control-file watcher, or session-file writer.

## Completed boundary migration

- The CLI accepts `--attach <event-endpoint>` or `--launch-binding <path>` as its runtime entry points.
- A launch binding resolves the NARS WebSocket event endpoint and waits for the binding/session index to become available during concurrent workspace startup.
- The projection subscribes to NARS events with replay, resumes from the last durable sequence after reconnect, and deduplicates event IDs/sequences locally.
- PageUp requests older durable pages through `session.events.read`; the transcript store prepends those events without reordering live items.
- Composer submit, cancel, and close actions are sent to NARS session methods.
- Rendering, composer state, terminal lifecycle, local scroll state, reconnect state, and transcript projection remain TUI-owned.
- Provider, MCP, terminal authority, turn lifecycle, queueing, persistence, and canonical event semantics remain NARS-owned.
- The deleted local runtime modules and their acceptance tests are no longer part of the production path.

## Verification

Focused checks currently cover:

- NARS event normalization and launch-binding endpoint discovery.
- Chronological history-page prepending and live transcript retention.
- Composer redraw and submit behavior.
- Transcript rendering, wrapping, scroll offset, status suppression, and terminal lifecycle cleanup.
- Narada workspace launcher plans that keep NARS hidden/detached and expose only the selected projection surface.

## Deliberate compatibility behavior

The old runtime flags remain listed in the CLI parser only so they fail with a migration error instead of being silently ignored. They are not supported execution paths. Shared carrier protocol fixture names such as `input_queued_for_turn_boundary` remain for event-vocabulary compatibility; they do not imply a local TUI queue.

## Remaining verification boundary

End-to-end WebSocket integration against a live NARS instance still belongs to the Narada runtime test surface. Agent-tui unit and acceptance tests use deterministic projection inputs and do not recreate NARS provider or MCP execution locally.
