# Agent TUI Target Functionality

## Purpose

`agent-tui` is Narada's pane/composer-oriented terminal carrier for interactive agents.

It replaces line-oriented terminal interaction problems in `agent-cli` without changing Narada agent semantics. It owns terminal rendering and input composition; it does not own task, directive, site-loop, mailbox, or authority semantics.

## Normative Protocol

Carrier event semantics are defined by `carrier-protocol.md` and executable validators in `@narada2/carrier-protocol`.

Runtime-admission constants shared by the Rust carrier and Narada proper launcher metadata are indexed in `contracts/README.md` and stored as machine-readable JSON under `contracts/`.

This document describes target functionality and UX. If this document conflicts with `carrier-protocol.md` or `@narada2/carrier-protocol`, the protocol contract wins for event schemas, lifecycle states, JSONL records, tool events, compatibility, and admission rules.

## Non-Goals

- Do not fork Narada task lifecycle semantics.
- Do not fork directive semantics.
- Do not fork MCP discovery or tool execution authority.
- Do not introduce a separate agent identity model.
- Do not replace noninteractive or server-mode carrier APIs until parity is explicit.
- Do not treat queued operator text as an operator directive unless explicitly marked as directive input.

## Carrier Identity

- Runtime name: `agent-tui`
- Binary name: `narada-agent-tui`
- Carrier kind: `agent-tui`
- Runtime substrate kind: `agent-tui`
- UI class: terminal TUI carrier
- Implementation target: Rust with `ratatui`, `crossterm`, and `tui-textarea`

`agent-cli` remains the line-oriented CLI carrier. `agent-tui` is a separate carrier, not `agent-cli` version 2.

## Shared Narada Semantics

`agent-tui` must use the same Narada semantics as `agent-cli`:

- agent identity
- site root
- workspace root
- carrier session id
- agent start event id
- MCP fabric
- session JSONL
- control JSONL
- directive receipt and acceptance evidence
- task lifecycle authority
- site operating loop authority

The TUI layer may change presentation and input ergonomics only.

## Input Event Semantics

`agent-tui` uses the carrier input taxonomy from `carrier-protocol.md`:

- `source_kind` and `source_id`: who or what caused input
- `transport`: how the carrier received input
- `delivery_mode`: how the carrier admits input to agent turns
- `hold_condition`: UI or carrier condition that must clear before admission

Important consequences:

- Source is not transport.
- Transport is not authority.
- Delivery mode is not authority.
- Hold condition is separate from delivery mode.
- External sources cannot directly enter an agent turn without system or operator admission.
- Agent-sourced input is limited to explicit agent-facing control paths and cannot be hidden model self-injection.

## Active Turn Boundary

An active turn ends only when the assistant response reaches a terminal state:

- `completed`
- `interrupted`
- `failed`

Text chunks and tool results are not turn boundaries.

## Screen Model

The terminal is divided into stable regions:

- Transcript pane: rendered view of semantic conversation and tool transcript.
- Status line: ephemeral current operational state.
- Composer: persistent operator input area.

Transcript output must never corrupt the composer. Operator typing must never corrupt transcript output.

## Transcript Pane

The transcript pane shows durable semantic events:

- `operator -> <agent>: <message>`
- `<agent>: <message>`
- `<agent> -> agent-tui: <tool_call>`
- `agent-tui -> <agent>: <tool_result>`
- `system directive: <content>`
- `operator directive -> <agent>: <content>`

The transcript pane is not durable truth. Session JSONL is durable truth. The transcript pane is a rendered view with wrapping, scrolling, and truncation.

The transcript pane must not show ephemeral UI events as durable messages:

- queued acknowledgements
- spinner frames
- draft length
- keypress hints
- transient progress updates

Important ephemeral events may still be recorded in session JSONL according to `carrier-protocol.md`.

## Composer

The composer is a persistent input field at the bottom of the terminal.

It must support:

- text insertion
- backspace/delete
- left/right movement
- home/end
- paste
- multiline paste admission without display corruption
- terminal resize
- visible cursor
- retained draft while agent is working

Pasted content must be preserved exactly. If a full multiline editor is not ready, multiline paste is submitted as one input event and rendered with a safe preview.

## Input Semantics

Typed text is a draft until submitted.

Enter behavior:

- When idle: submit as operator input with `delivery_mode=admit_for_current_turn` and `hold_condition=null`.
- When agent is working: queue as operator steering with `delivery_mode=admit_after_active_turn` and `hold_condition=null`.
- When text is empty or whitespace only: no operator message is created.

Esc behavior:

- Esc requests active-turn interruption only.
- Esc does not clear draft text.
- Esc does not clear queued operator steering.
- Esc does not mutate tasks, directives, MCP state, or queue state.

Slash commands remain local carrier commands unless explicitly submitted as agent input. `//text` submits `/text` to the agent; `/text` remains a local command.

## Operator Steering

Operator steering is non-authoritative working-time operator input queued for later admission.

It is represented by the carrier protocol as operator-sourced input with `delivery_mode=admit_after_active_turn`.

Rules:

- It is not an operator directive.
- It is not delivered mid-turn.
- It becomes transcript-visible only when admitted to a turn.
- It is recorded in session JSONL when queued and when admitted, dropped, or abandoned.
- It is delivered in FIFO order after the active turn reaches terminal state.
- It has no automatic expiry; it remains until admitted, dropped, or session end.

## Explicit Operator Directives

An operator directive requires an explicit directive command or directive surface.

Ordinary working-time input is operator steering, not an operator directive.

## Queue Commands

`agent-tui` must provide local queue commands:

- `/queue` shows queued items with index, source, age, and first-line preview.
- `/queue clear` clears queued operator steering after explicit command semantics.
- `/queue drop <index>` removes one queued item.
- `/stats [args]` shows local Codex transcript statistics using the operator-local `narada-tools` checkout or PATH binary. It is scoped to Codex transcript storage and is not gated to Codex runtime sessions.

Queue commands execute immediately as carrier-local UI mutations, even while the agent is working.

These commands affect carrier input queues only. They do not mutate tasks or directives.

Slash commands that mutate carrier state are recorded in session JSONL. Read-only slash commands are not recorded unless diagnostic logging is enabled.

## System Directives And Composer Drafts

If a system directive arrives while the composer has nonempty draft text:

- it is held with `hold_condition=composer_clear_required`
- it keeps its original `delivery_mode`
- it is released immediately when the draft is submitted or cleared
- it remains visible in status while held

The status line shows:

- held system directive count
- oldest held age

Format example:

- `held system directives 2 | oldest 1m 14s`

## Status Line

The status line is ephemeral and must use operational language.

Examples:

- `thinking 1m 12s | draft 34 chars | queued operator steering 2 | Esc interrupt`
- `calling site_loop_run_once 8s | held system directives 1 | oldest 22s`
- `interrupt requested | waiting for provider`

The status line must distinguish:

- draft text
- queued operator steering
- held system directives
- interrupt requested
- current phase
- current tool call

It must not say `operator directive` unless the queued input is explicitly an operator directive.

## Tool Display

Tool calls are semantic transcript events.

Format:

- `<agent> -> agent-tui: <tool_name>(<summary_args>)`
- `agent-tui -> <agent>: <status> <tool_name> in <duration> ...`

This is UI shorthand for a carrier-mediated tool request. The requesting actor is the assistant model under agent identity. The tool execution actor is the carrier/runtime.

Tool arguments and results follow the protocol payload reference rules. Large or sensitive arguments/results must use summaries plus `arguments_ref` or `result_ref`.

## Control JSONL

`agent-tui` must support the existing control JSONL model through the normalized carrier protocol.

Control JSONL events must be processed without corrupting the composer.

Control JSONL input uses the same event taxonomy as terminal input. It does not get a separate authority model.

Legacy control records may be accepted only through compatibility adapters that normalize to the carrier protocol.

## Session JSONL

Session JSONL must remain the durable audit trail.

It records carrier events according to `carrier-protocol.md`, including:

- input queued for turn boundary
- input admitted to turn
- input dropped by operator
- input abandoned on session end
- input completed
- system directive held/released
- directive receipt evidence
- directive acceptance evidence
- interrupt requested
- turn started/completed/interrupted/failed
- tool call requested
- tool result received
- carrier diagnostics
- carrier-local state-mutating slash commands

It must not depend on terminal rendering state.

## Rendering Contract

All terminal writes must go through the TUI renderer.

Rules:

- No direct transcript `console.log` equivalent in interactive mode.
- Provider stderr and MCP stderr must never write raw text into the TUI.
- Provider stderr and MCP stderr are routed through mediated diagnostic events.
- Known-noise suppression exposes suppression policy and count in diagnostics.
- Payload threshold policy is deterministic per carrier session and recorded through protocol metadata or diagnostics.
- Spinner/status updates must not enter transcript.
- Redraw must preserve draft input.
- Resize must recalculate layout without losing transcript, status, composer, draft, or queue.

On resize, the renderer reflows the rendered transcript view while preserving:

- session log
- scroll position intent
- composer draft
- input queue
- status state

## Runtime Parity With agent-cli

Before `agent-tui` can become the default carrier, it must match `agent-cli` for:

- agent launch arguments
- site root handling
- MCP discovery
- tool listing
- tool calls
- provider selection
- model selection
- thinking-level selection
- streaming on/off setting
- slash commands
- control JSONL
- startup system directive option
- session JSONL compatibility
- directive receipt evidence
- carrier heartbeat

The first useful version should implement:

- launch as `narada-agent-tui`
- expose explicit MCP fabric configuration posture without granting live Site MCP execution by default
- display transcript pane
- display status line
- display composer
- submit idle operator messages
- queue working-time operator steering
- inspect, drop, and clear queue
- hold system directives while composer draft is nonempty
- show mediated agent/tool output without composer corruption
- preserve session JSONL compatibility

Live Site MCP execution, real provider dispatch, and default terminal promotion are separate admission gates; configuring their inputs is not the same as admitting their execution.

It may defer:

- mouse support
- rich scrollback search
- split panes beyond transcript/status/composer
- full multiline editor
- theme customization
- noninteractive/server mode parity

## Acceptance Tests

The carrier is not acceptable until these scenarios pass:

- Agent streams output while operator types; typed draft remains intact.
- Agent calls a tool while operator types; transcript and composer stay separated.
- Enter during working queues steering and does not create a durable transcript block immediately.
- Queued operator steering is delivered after the current turn and appears in transcript at delivery time.
- `/queue` shows queued operator steering content.
- `/queue clear` clears queued operator steering.
- `/queue drop <index>` removes the chosen item.
- `/stats [args]` shows local Codex transcript statistics.
- `//help` submits `/help` to the agent.
- Esc interrupts current work without clearing draft or queue.
- System directive arriving during nonempty draft is held and visible in status.
- Held system directive releases when draft clears or submits.
- Multiline paste is preserved and does not corrupt the display.
- Terminal resize preserves transcript, status, composer, draft, and queue.
- Provider stderr is mediated and does not corrupt the TUI.
- MCP stderr is mediated and does not corrupt the TUI.
- Known-noise stderr suppression remains visible through diagnostics.
- Large tool argument uses payload reference and does not flood the transcript.
- Large tool result uses payload reference and does not flood the transcript.
- Session JSONL contains enough evidence to reconstruct queued, admitted, dropped, abandoned, held, and released input events.

## Migration Policy

`agent-tui` is introduced in parallel with `agent-cli`.

Migration steps:

1. Keep this target contract aligned with `carrier-protocol.md`.
2. Keep protocol types, validators, and compatibility adapters in `@narada2/carrier-protocol`.
3. Implement `agent-tui` against the shared carrier protocol.
4. Add launcher runtime admission for `agent-tui`.
5. Run `agent-cli` and `agent-tui` side by side on the same sites.
6. Make `agent-tui` default only after runtime parity and UX acceptance tests pass.
7. Retire `agent-cli` only after all known sites launch cleanly through `agent-tui`.
