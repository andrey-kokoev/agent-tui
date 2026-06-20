# Recognition Phrases

This document inventories the current text and protocol patterns that `agent-tui` recognizes and turns into behavior. This is broader than hardcoded strings: it includes command tokens, aliases, JSON envelopes, ref patterns, status markers, CLI flags, environment values, key/mouse events, and renderer classifiers.

Scope: included items are phrases or patterns that affect behavior, routing, admission, styling, parsing, or validation. Ordinary labels that are only displayed are excluded unless they are also parsed or classified.

## Operator Composer Input

Source: `src/carrier_command.rs`, `src/runtime_coordinator.rs`.

| Recognized text or pattern | Behavior |
| --- | --- |
| Empty or whitespace-only input | No submit action. |
| `//...` | Escapes a leading slash and submits `/...` as agent input. |
| Text that does not start with `/` | Submitted as agent input. |
| Unknown `/...` command | Treated as local unknown-command feedback, not agent input. |
| `/help` | Shows carrier help. |
| `/status` | Shows identity/session/model/thinking/queue/turn status. |
| `/stats [arguments...]` | Runs Codex transcript stats with the remaining text as arguments. |
| `/model [value...]` | Shows current model when no value is provided; otherwise sets the session model to the remaining text. |
| `/thinking [value]` | Shows current thinking when no value is provided; otherwise accepts only `none`, `low`, `medium`, or `high`. |
| `/tool-output [value]`, `/tool-outputs [value]` | Controls tool output display. Accepted values are `on`, `show`, `shown`, `off`, `hide`, `hidden`, `toggle`, and `status`; no value means `toggle`. |
| `/queue` | Shows queued operator inputs. |
| `/queue clear` | Drops all queued operator inputs. |
| `/queue drop <index>` | Drops one queued operator input by numeric index. |
| `/clear` | Clears the display. |
| `/exit`, `/quit` | Exits the TUI. |
| `exit` | Present in the shared command contract fixture, but not parsed by `parse_operator_submit` as a local command unless another surface maps it before this parser. |

## Direct Operator Tool Routing

Source of truth: `../narada/packages/carrier-routing-contract/contracts/operator-routing.json`. Loader and dispatch use: `src/operator_routing_contract.rs`, `src/provider_dispatch.rs`.

| Recognized text or pattern | Behavior |
| --- | --- |
| `run startup sequence` | Routes directly to `agent_context_startup_sequence {}` after trim, trailing-period removal, whitespace collapse, and lowercase normalization. |
| `startup sequence` | Same direct startup routing. |
| Raw JSON object containing `narada_tool_call` | Routes directly to the named MCP tool without provider reasoning. |
| Fenced JSON or fenced text containing a `narada_tool_call` object | Fence is stripped and the tool call is routed directly. |
| Text that starts like an incomplete `narada_tool_call` JSON envelope or starts with a code fence | Held back from streaming as normal provider text while it may become a direct tool call. |
| Any text asking for `mcp_output_show`, `output reader`, `startup output reader`, `read startup output`, `read the startup output`, or `read the output ref`, plus an `mcp_output:<id>` ref | Routes directly to `mcp_output_show` with the extracted ref and `output_limit: 10000`. |
| `mcp_output:<id>` where id chars are ASCII alphanumeric, `_`, or `-` | Extracted as an output ref when paired with a reader request phrase. |

## Provider Tool-Call Bridge

Source of truth: `../narada/packages/carrier-routing-contract/contracts/operator-routing.json`. Loader and bridge use: `src/operator_routing_contract.rs`, `src/provider_tool_call_bridge.rs`.

| Recognized text or pattern | Behavior |
| --- | --- |
| Provider output kind `ToolCallRequest` | Converted into an MCP tool request. Non-tool provider output is ignored by the bridge. |
| Tool aliases `startup_sequence`, `agent_context_startup_sequence` | Resolved to whichever startup tool name is admitted by the MCP boundary. |
| Tool aliases `mcp_payload_read`, `mcp_payload_show` | Resolved to whichever payload reader name is admitted by the MCP boundary. |
| JSON result with `truncated: true`, `reader_tool: "mcp_output_show"`, and `output_ref` or `ref` | Formats a follow-up advisory telling the agent to emit a `narada_tool_call` envelope for `mcp_output_show`. It does not auto-run the reader. |
| `arguments_summary` as JSON | Parsed into tool arguments for the MCP request. |

## Runtime Admission and Environment Values

Sources of truth: `../narada/packages/carrier-runtime-contract/contracts/*.json`, including `boolean-values.json`, and `../narada/packages/carrier-provider-contract/contracts/provider-adapters.json`. Loaders and runtime use: `src/runtime_boolean_contract.rs`, `src/provider_adapter_contract.rs`, `src/provider_runtime_config.rs`, `src/provider_adapter_admission.rs`, `src/mcp_runtime_config.rs`, `src/mcp_runtime_contract.rs`, `src/terminal_runtime_config.rs`, `src/terminal_runtime_contract.rs`, `src/runtime_config_snapshot.rs`.

| Recognized text or pattern | Behavior |
| --- | --- |
| Truthy env values `1`, `true`, `on`, `yes` | Enable provider execution, MCP fabric access, or terminal rendering depending on the env var. Values are trimmed and lowercased. |
| Provider stream false values `0`, `false`, `off`, `no` | Disable provider streaming. Any other configured provider stream value leaves streaming on. |
| `NARADA_AGENT_TUI_ENABLE_PROVIDER_EXECUTION` | Gates provider runtime admission. |
| `NARADA_INTELLIGENCE_PROVIDER` | Provider value. Admitted providers are `codex-subscription`, `openai-api`, and `anthropic-api`. |
| `NARADA_AI_MODEL` | Required model value when provider execution is enabled. |
| `NARADA_AI_THINKING` | Initial provider thinking value. |
| `NARADA_AI_STREAM` | Provider streaming flag. |
| `NARADA_AGENT_TUI_PROVIDER_ADAPTER_KIND` | Provider adapter kind selector. Recognized kinds are `scripted_provider_adapter` and `codex_subscription_adapter`. |
| `NARADA_AGENT_TUI_ENABLE_MCP_FABRIC` | Gates MCP runtime admission. |
| `NARADA_AGENT_TUI_MCP_CONFIG` | Required MCP config path when MCP fabric is enabled. |
| `NARADA_SITE_MCP_FABRIC` | Required MCP fabric root when MCP fabric is enabled. |
| MCP config path inside fabric after lexical normalization, no `..`, suffix begins with `/` | Required path policy for admitted MCP fabric config. |
| Error prefix `mcp_fabric_config_read_failed:` | Normalized to `mcp_config_unreadable`. |
| Error prefix `mcp_fabric_config_parse_failed:` | Normalized to `mcp_config_parse_failed`. |
| `NARADA_AGENT_TUI_ENABLE_TERMINAL_RENDERING` | Gates terminal runtime admission. |
| `NARADA_AGENT_TUI_TERMINAL_MODE=interactive_loop` | Required terminal mode for env-admitted interactive terminal rendering. |
| `NARADA_SITE_ROOT` | Used as the Codex provider working directory and checked in MCP fabric server env/site-root handling. |
| `NARADA_AGENT_TUI_CODEX_COMMAND` | Overrides the Codex command executable. |

## CLI Arguments

Source: `src/main.rs`, `src/launch_slice_contract.rs`.

| Recognized flag | Behavior |
| --- | --- |
| `--identity <value>` | Sets agent identity. |
| `--session <value>` | Sets carrier session id. |
| `--site-root <path>` | Sets site root. |
| `--control-jsonl <path>` | Sets control input stream path. Required for terminal interactive loop. |
| `--session-jsonl <path>` | Sets session evidence stream path. Required for terminal interactive loop. |
| `--interactive-loop` | Contract carrier flag for terminal interactive loop. Requires `--max-steps > 0`. |
| `--render-once` | Renders once. |
| `--max-steps <positive integer>` | Loop step limit. Valid only with `--interactive-loop`. |
| `--composer-has-draft` | Starts with composer draft state for smoke/test flow. |
| `--check-rust-toolchain` | Toolchain check mode. |
| `--help`, `-h` | Help mode. |
| `--version`, `-V` | Version mode. |
| Any unknown flag | Argument parse error. |

## Keyboard, Paste, and Mouse Input

Sources: `src/terminal_input.rs`, `src/terminal_input_tick.rs`.

| Recognized event | Behavior |
| --- | --- |
| Printable character with no modifier or Shift | Inserts character into composer. |
| Paste event | Inserts pasted text into composer. |
| Enter | Submit composer. |
| Esc | Interrupt active turn or clear composer state. |
| Backspace, Delete | Edit composer. |
| Left, Right, Home, End | Move composer cursor. |
| PageUp, PageDown | Scroll transcript up/down. |
| Mouse wheel ScrollUp, ScrollDown | Scroll transcript up/down in the current worktree. |
| Ctrl-C | Exit. |
| Up, Down, Alt-modified chars, non-press key events, other non-key events | Ignored by input decoding or tick handling. |

## Transcript and Status Rendering Classifiers

Source of truth: `../narada/packages/carrier-rendering-contract/contracts/transcript-classifiers.json`. Loader and renderer use: `src/rendering_classifier_contract.rs`, `src/ratatui_renderer.rs`, `src/status_view_model.rs`.

| Recognized text or pattern | Behavior |
| --- | --- |
| Turn state `active` | Displayed as `thinking`. |
| Turn state `active <age>` | Displayed as `thinking <age>`. |
| Active marker phase `thinking` | Shows inline marker `thinking · <submit hint> · <interrupt hint>`. |
| Active marker phase prefix `thinking ` | Shows inline marker preserving the thinking detail. |
| Active marker phase prefix `calling ` | Shows inline marker preserving the call detail. |
| Turn marker status `idle`, `active`, or empty | Suppressed as not significant. |
| Other non-empty turn marker status | Title-cased after replacing `_` and `-` with spaces. |
| Interrupt hint `Esc interrupt` | Rendered as `Esc to interrupt` in the inline marker. |
| Agent-TUI terminal status `completed` or `completed_without_provider` | Styled as positive and humanized by replacing `_` with spaces. |
| Inline technical token | Styled as code in terminal status rows. |
| Terminal status prefix `queue: ` | Styled as queue status. |
| Queue detail fragments containing ` · `, role labels `operator`, `system`, `agent`, `queued note`, `queued turn`, `held directive`, or duration phrases | Styled as queue details. |
| Diagnostic body prefix `diagnostic ` on an `AgentTui` row | Splits severity from detail and styles severity. |
| Diagnostic severities `warn`, `warning` | Warning style. |
| Diagnostic severities `error`, `failed`, `failure` | Negative style. |
| Diagnostic severities `ok`, `success` | Positive style. |
| System directive held/released item kinds | Style directive state as warning or positive. |
| Code fence line starting with three backticks after leading whitespace | Enters/exits code styling and renders a code header. |
| Markdown rule line of at least three `-`, `*`, or `_` chars | Styled muted. |
| Blockquote marker | Styled as marked body text. |
| Markdown list marker | Styled as marked body text. |
| Diff line starting with `+` where the rest is non-empty and does not start with space or `+` | Positive diff styling. |
| Diff line starting with `-` where the rest is non-empty and does not start with space or `-` | Negative diff styling. |
| Markdown heading marker | Styled as heading. |
| Section heading line | Styled as heading. |
| Markdown table row or fragment containing `|` with table-like edges | Styled as table text. |
| PowerShell prompt pattern | Splits prompt and command, styling command as code. |
| `key: value` line in non-muted body text | Styles key and value separately. |
| Indented body line | Preserves muted indentation and styles remaining text normally. |
| Status segment keys `turn_state`, `esc_action`, `provider_state`, `provider_adapter_state`, `mcp_state`, `terminal_state` | Hidden from compact status line. |
| Status segment values `queued_inputs=0`, `held_system_directives=0`, `transcript_items=0`, `last_error=none` | Hidden from compact status line. |

## Carrier Protocol and Evidence Tokens

Source of truth: `../narada/packages/carrier-protocol-contract/contracts/carrier-protocol.json`. Loader and protocol code use: `src/carrier_protocol_contract.rs`, `src/carrier_protocol.rs`, `src/rendering_boundary.rs`.

| Recognized text or pattern | Behavior |
| --- | --- |
| Schema `narada.carrier.input_event.v1` | Required for input event parsing. |
| Schema `narada.carrier.control.input_event.v1` | Required for control input event parsing. |
| Schema `narada.carrier.session_event.v1` | Required for session event parsing. |
| Schema `narada.carrier.payload_ref.v1` | Required for payload ref parsing. |
| Schema `narada.carrier.payload_policy.v1` | Required for payload policy parsing. |
| Schema `narada.agent_tui.provider_request_payload.v0` | Provider request payload schema marker. |
| Schema `narada.agent_tui.provider_output_payload.v0` | Provider output payload schema marker. |
| Schema `narada.agent_tui.turn_terminal_payload.v0` | Turn terminal payload schema marker. |
| Input event id prefix `input_` | Required for input events. |
| Control event id prefix `control_` | Required for control input events. |
| Session event id prefix `session_event_` | Required for session events. |
| RFC3339 UTC timestamps | Required for event time fields. |
| Source kinds `operator`, `system`, `agent`, `external` | Parsed through serde snake_case enum values. Agent source requires `agent_control_input: true`; external source requires `admitted_by`. |
| Transports `interactive_terminal`, `control_jsonl`, `startup_injection`, `carrier_server_api`, `test_harness` | Parsed through serde snake_case enum values. |
| Delivery modes `admit_for_current_turn`, `admit_after_active_turn` | Parsed through serde snake_case enum values. |
| Hold condition `composer_clear_required` | Parsed through serde snake_case enum value. |
| Session event kinds such as `provider_tool_call_requested`, `tool_call_requested`, `tool_result_received`, `carrier_command_executed`, `carrier_diagnostic_recorded` | Drive session event validation and transcript reconstruction. |
| Payload reader `mcp_payload_show` | Emitted for payload refs created by the rendering boundary when content must be referenced instead of inlined. |
| Diagnostic sources `provider_stderr`, `mcp_stderr`, `known_noise_suppression`, `terminal_resize`, `payload_policy` | Emitted as mediated diagnostic event source values. |
| Rendering boundary marker `mediated_diagnostic_event` and `terminal_write: false` | Marks diagnostic events as mediated instead of direct terminal writes. |

## MCP and JSON-RPC Protocol Tokens

Sources: `src/mcp_json_rpc.rs`, `src/mcp_runtime_execution.rs`, `src/mcp_fabric_transport.rs`, `src/mcp_stdio_process.rs`.

| Recognized text or pattern | Behavior |
| --- | --- |
| JSON-RPC methods `initialize`, `notifications/initialized`, `tools/list`, `tools/call` | Used for MCP server initialization, tool listing, and tool execution. |
| JSON-RPC response/error shapes | Parsed to detect request success or failure. |
| MCP fabric config `mcpServers` | Server map section used by fabric transport. |
| Server env var name `NARADA_SITE_ROOT` | Compared with target site root after path normalization; mismatches are refused. |
| Missing fabric target site root | Refused with `mcp_fabric_server_target_site_root_missing:<server>`. |
| Fabric server site-root mismatch | Refused with `mcp_fabric_server_site_root_mismatch:<server>:<expected>:<actual>`. |

## Codex Provider JSON Stream Tokens

Source: `src/provider_dispatch.rs`.

| Recognized text or pattern | Behavior |
| --- | --- |
| Event type containing `delta` or `stream` | Treated as a possible streaming text event. |
| Fields `delta`, `text_delta`, or `text` | Extracted as streaming text when present. |
| Event type `item.completed` with item type `agent_message` | Extracted as completed agent message text. |
| Provider failure strings such as `codex_exec_spawn_failed:`, `codex_subscription_site_root_missing`, `codex_subscription_prompt_missing`, `codex_exec_stdin_unavailable` | Reported as provider dispatch failure text/status. |
