# Agent Carrier Protocol

## Purpose

This document defines the shared carrier protocol for Narada interactive agent carriers.

It is UI-independent. `agent-cli` and `agent-tui` may render differently, but they must preserve this protocol when handling input, turns, tools, directives, queues, session logs, and control files.

## Scope

The carrier protocol owns:

- input event taxonomy
- turn lifecycle
- queue lifecycle
- session JSONL evidence
- control JSONL ingress
- carrier-mediated tool events
- provider event boundary
- interruption semantics
- payload reference behavior
- launcher compatibility requirements

The carrier protocol does not own:

- task lifecycle semantics
- directive admission semantics
- site operating loop semantics
- mailbox semantics
- MCP tool authority
- agent identity authority
- provider model behavior

## Protocol Location

The durable protocol should live in a shared carrier package before both `agent-cli` and `agent-tui` depend on it.

The executable shared package is `@narada2/carrier-protocol`.

This document is the prose contract. The package provides shared validators, constants, schema metadata, helpers, and compatibility adapters.

Carrier implementations must not keep private copies of protocol types where the shared package can be used. `agent-cli` compatibility depends on shared types, validators, and adapters.

## Input Event Schema

Every input event has this common shape:

```json
{
  "schema": "narada.carrier.input_event.v1",
  "event_id": "input_...",
  "source_kind": "operator",
  "source_id": "operator",
  "transport": "interactive_terminal",
  "delivery_mode": "admit_for_current_turn",
  "hold_condition": null,
  "content": "run startup sequence",
  "created_at": "2026-05-30T00:00:00.000Z",
  "authority_ref": null,
  "directive_id": null,
  "metadata": {}
}
```

Required fields:

- `schema`
- `event_id`
- `source_kind`
- `source_id`
- `transport`
- `delivery_mode`
- `content`
- `created_at`

Optional fields:

- `hold_condition`
- `authority_ref`
- `directive_id`
- `metadata`

`content` is the exact text admitted or queued by the carrier. The carrier must not silently rewrite authority-bearing content.

## Source Taxonomy

`source_kind` values:

- `operator`: human operator-originated input
- `system`: Narada system-originated input
- `agent`: agent-originated control input
- `external`: non-Narada-origin event source

`source_id` is the concrete emitter identity.

Examples:

- `operator`
- `narada-proper.system.directive_emitter`
- `sonar.resident`
- `mailbox:help@global-maxima.com`

Source is not transport. Source is not authority. Source is who or what caused the input.

`agent` source is valid only for carrier control inputs emitted by an agent-facing runtime path. It is not a way for the assistant model to inject hidden user content into its own turn.

`external` source cannot be admitted directly into an agent turn unless a Narada system or operator admission surface has converted it into a carrier input event. The external source identity remains recorded in metadata or provenance.

## Transport Taxonomy

`transport` values:

- `interactive_terminal`
- `control_jsonl`
- `startup_injection`
- `carrier_server_api`
- `test_harness`

Transport is the ingress surface through which the carrier received the event.

Transport does not change source identity. Operator input sent through `control_jsonl` remains `source_kind=operator`.

`agent_cli_server_api` is a legacy compatibility alias for `carrier_server_api`. New protocol records must use `carrier_server_api`.

## Delivery Modes

`delivery_mode` values:

- `admit_for_current_turn`
- `admit_after_active_turn`

`admit` means the carrier admits input into the agent turn queue. It does not imply execution, authority, or tool action.

## Hold Conditions

`hold_condition` values:

- `composer_clear_required`

`hold_condition` is separate from `delivery_mode`.

A held event keeps its original `delivery_mode`. When the hold condition clears, the carrier applies that delivery mode.

## Input Event Validation

The carrier must reject or quarantine input events when:

- required fields are missing
- `source_kind` is unknown
- `transport` is unknown
- `delivery_mode` is unknown
- `hold_condition` is unknown
- `content` is not a string
- `directive_id` is present but directive compatibility rules fail

Directive compatibility rules:

- `directive_id` may appear only when `source_kind=system` or when an explicit operator directive surface created the event.
- `directive_id` must be paired with `authority_ref` or directive provenance in `metadata`.
- receipt evidence may be recorded only once per `(carrier_session_id, directive_id, input event_id)` tuple.

Tuple uniqueness is a session-store responsibility. Shape validators cannot prove historical uniqueness.

Rejected control events must be recorded as carrier diagnostics, not silently dropped.

## Turn Lifecycle

Turn states:

- `idle`
- `active`
- `interrupt_requested`
- `completed`
- `interrupted`
- `failed`

An active turn starts when an input event is admitted to the model/provider turn.

An active turn ends only when it reaches terminal state:

- `completed`
- `interrupted`
- `failed`

Assistant text chunks and tool results are not turn boundaries.

## Input Admission Rules

`admit_for_current_turn`:

- If no turn is active, admit immediately.
- If a turn is active, reject as invalid.

Provider/tool continuations are not represented as external carrier input events. They are internal turn events.

`admit_after_active_turn`:

- If no turn is active, admit immediately.
- If a turn is active, enqueue for the next turn boundary.

Held event admission:

- If `hold_condition=null`, apply `delivery_mode` immediately.
- If `hold_condition=composer_clear_required` and composer has no nonempty draft, apply `delivery_mode` immediately.
- If `hold_condition=composer_clear_required` and composer has nonempty draft, hold until the draft is submitted or cleared, then apply the original `delivery_mode`.

## Operator Steering

Operator steering is ordinary operator input submitted while a turn is active.

Canonical event fields:

```json
{
  "source_kind": "operator",
  "source_id": "operator",
  "transport": "interactive_terminal",
  "delivery_mode": "admit_after_active_turn",
  "hold_condition": null
}
```

Operator steering is not an operator directive.

Operator steering is admitted FIFO after the active turn reaches terminal state.

Operator steering has no automatic expiry. It remains until:

- admitted to turn
- dropped by operator
- abandoned on session end

## Queue Lifecycle

Queue states:

- `queued_for_turn_boundary`
- `admitted_to_turn`
- `dropped_by_operator`
- `abandoned_on_session_end`

Queue lifecycle event names:

- `input_queued_for_turn_boundary`
- `input_admitted_to_turn`
- `input_dropped_by_operator`
- `input_abandoned_on_session_end`

Queue mutation commands are carrier-local. They do not mutate tasks, directives, MCP state, or site state.

## System Directive Holding

If a system directive arrives while the interactive composer has nonempty draft text:

- it is held with `hold_condition=composer_clear_required`
- it keeps its original `delivery_mode`
- it is not dropped
- it is not rewritten as operator input
- status must expose held count and oldest held age
- it is released immediately when the draft is submitted or cleared

Held system directive evidence must include:

- input event id
- directive id when available
- held_at
- released_at
- held_reason: `composer_nonempty`
- original delivery_mode

## Explicit Operator Directives

Operator directives require an explicit directive surface or directive command.

Ordinary operator input, including working-time input, is not an operator directive.

An operator directive input event should include:

- `source_kind=operator`
- `source_id=operator`
- `authority_ref`
- directive-specific metadata

## Control JSONL

Control JSONL is an append-only carrier ingress file.

Each line must be one JSON object.

Accepted control record shape:

```json
{
  "schema": "narada.carrier.control.input_event.v1",
  "control_event_id": "control_...",
  "input_event_id": "input_...",
  "written_at": "2026-05-30T00:00:00.000Z",
  "input": {
    "schema": "narada.carrier.input_event.v1",
    "event_id": "input_...",
    "source_kind": "system",
    "source_id": "narada-proper.system.directive_emitter",
    "transport": "control_jsonl",
    "delivery_mode": "admit_for_current_turn",
    "hold_condition": "composer_clear_required",
    "content": "run startup sequence",
    "created_at": "2026-05-30T00:00:00.000Z",
    "directive_id": "dir_..."
  }
}
```

`input_event_id` must equal `input.event_id`.

The carrier may accept legacy control records only through an explicit compatibility adapter. The adapter must emit normalized `narada.carrier.input_event.v1` records internally.

## Session JSONL

Session JSONL is the durable carrier audit log.

It is independent of terminal rendering.

Required event envelope:

```json
{
  "schema": "narada.carrier.session_event.v1",
  "event_kind": "input_admitted_to_turn",
  "event_id": "session_event_...",
  "occurred_at": "2026-05-30T00:00:00.000Z",
  "carrier_session_id": "carrier_...",
  "agent_id": "sonar.resident",
  "site_id": "narada-sonar",
  "site_root": "D:/code/narada.sonar",
  "payload": {}
}
```

Required session event kinds:

- `input_queued_for_turn_boundary`
- `input_admitted_to_turn`
- `input_dropped_by_operator`
- `input_abandoned_on_session_end`
- `input_completed`
- `system_directive_held`
- `system_directive_released`
- `directive_receipt_recorded`
- `directive_carrier_accepted_recorded`
- `turn_started`
- `provider_request_recorded`
- `turn_completed`
- `turn_interrupted`
- `turn_failed`
- `interrupt_requested`
- `tool_call_requested`
- `tool_result_received`
- `carrier_command_executed`
- `carrier_diagnostic_recorded`

Read-only carrier commands are not recorded unless diagnostic logging is enabled. Carrier UI may show read-only command output without durable session events.

Carrier commands that mutate carrier state must be recorded.

## Tool Events

Tool calls are carrier-mediated.

The requesting actor is the assistant model operating under agent identity.

The execution actor is the carrier/runtime.

Tool call request session event payload:

```json
{
  "tool_name": "site_loop_run_once",
  "arguments_summary": "{}",
  "arguments_ref": null,
  "requesting_agent_id": "sonar.resident"
}
```

Tool result session event payload:

```json
{
  "tool_name": "site_loop_run_once",
  "status": "ok",
  "duration_ms": 2123,
  "result_summary": "success",
  "result_ref": null
}
```

Large or sensitive tool arguments and results must be summarized and moved behind payload references.

## Payload References

Payload references are required when tool/provider data is too large or too sensitive to inline safely.

Payload reference shape:

```json
{
  "payload_ref": "mcp_payload:<id>@v1",
  "reader_tool": "mcp_payload_read",
  "summary": "large tool result omitted from transcript"
}
```

Canonical reader tool is `mcp_payload_read` for new carrier protocol records.

Compatibility adapters may map existing `mcp_output_show` or other reader tools to the canonical shape.

Exact payload size thresholds are carrier configuration, but must be deterministic within a carrier session and recorded in diagnostics or startup metadata.

The executable protocol package exposes payload policy validation for this metadata.

## Provider Boundary

Provider events are not Narada authority events.

When an input event crosses the active turn boundary, the carrier records `provider_request_recorded` before any provider call is dispatched.

The event records the turn id, input event id, dispatch status, and whether provider execution is enabled. A scaffold carrier may record `provider_request_status=recorded_not_dispatched` through its provider dispatch stub and complete the turn without provider output.

Provider request status values:

- `recorded_not_dispatched`: request boundary was recorded but no provider call was dispatched.
- `dispatched`: provider call was dispatched and has not yet reached terminal state.
- `completed`: provider call completed normally.
- `failed`: provider call failed.
- `interrupted`: provider call was interrupted or cancellation was accepted.

Provider dispatch status maps to turn terminal evidence as follows:

- `recorded_not_dispatched` -> `turn_completed` with `terminal_status=completed_without_provider`.
- `dispatched` -> `turn_completed` with `terminal_status=completed_after_dispatch` until streaming/result events are modeled separately.
- `completed` -> `turn_completed` with `terminal_status=completed`.
- `failed` -> `turn_failed` with `terminal_status=failed`.
- `interrupted` -> `turn_interrupted` with `terminal_status=interrupted`.

Provider text and tool-call requests must be transformed into carrier protocol events before entering transcript or session JSONL.

Provider output placeholder event kinds:

- `provider_text_delta_recorded`: provider text delta was received and mediated by the carrier.
- `provider_tool_call_requested`: provider requested a tool call; tool execution still remains carrier-mediated.

These events are placeholders until real provider dispatch exists. They must not write directly to the terminal UI.

Provider adapters return output records through the dispatch record output buffer. The carrier writes buffered provider output events after `provider_request_recorded` and before terminal turn evidence.

Transcript projection reads mediated session events only. Turn-start events project operator, system, or agent input into transcript items using recorded source and content preview. Provider text deltas project to agent transcript text. Provider tool-call requests project to `agent-tui` mediated tool request transcript items. Terminal turn events project lifecycle status. The transcript projection layer must not read provider streams directly.

Provider stderr must never write directly to interactive terminal UI.

Provider stderr must become mediated diagnostic events.

## MCP Boundary

MCP server stderr must never write directly to interactive terminal UI.

MCP stderr must become mediated diagnostic events unless explicitly suppressed by known-noise policy.

Known-noise suppression may omit durable per-line diagnostics, but the suppression policy and suppression count must be available in carrier diagnostics.

MCP tool availability is governed by site MCP fabric and launcher admission, not by the TUI.

## Interruption Contract

Esc requests active-turn interruption only.

Esc does not mutate:

- composer draft
- queued operator steering
- held system directives
- tasks
- directives
- MCP state
- provider configuration

Interruption must record `interrupt_requested` in session JSONL.

The active turn then ends as either:

- `interrupted`
- `completed`
- `failed`

depending on provider/tool response.

## Literal Slash Input

Slash commands are carrier-local by default.

`//text` submits `/text` as input content to the agent.

Examples:

- `/queue` runs carrier queue inspection.
- `//queue` sends `/queue` to the agent.

## Carrier Commands

Carrier commands may be executed while the agent is working when they mutate carrier-local state only.

Required commands:

- `/queue`
- `/queue clear`
- `/queue drop <index>`

`/queue clear` and `/queue drop <index>` must record `carrier_command_executed` and the resulting dropped input events.

## Compatibility With agent-cli

`agent-cli` compatibility means:

- existing launcher arguments remain accepted or are explicitly mapped
- existing control JSONL records are normalized through compatibility adapters
- existing session JSONL remains readable
- new session JSONL should be written in `narada.carrier.session_event.v1` format when the shared protocol is active
- compatibility adapters may dual-write legacy session events only during migration
- directive receipt and carrier acceptance evidence remain semantically equivalent
- MCP discovery uses the same site fabric
- provider selection and model settings remain equivalent

Compatibility does not require identical terminal rendering.

## Implementation Order Dependency

The protocol must be implemented before `agent-tui` becomes a launcher runtime.

The first implementation step after this document is to create protocol structs/types and tests that validate:

- input event normalization
- input event validation
- queue lifecycle transitions
- session event emission
- control JSONL normalization
- interruption event emission
