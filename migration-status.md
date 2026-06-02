# Agent TUI Migration Status

## Status

`agent-tui` is a Rust carrier prototype with a bounded runtime scaffold, control JSONL polling, queue admission, transcript projection, terminal rendering, explicit provider-boundary evidence, and a governed-session provider/MCP admission path.

It is now admitted as the `agent-start -Runtime agent-tui` terminal interactive-loop carrier slice. Provider execution and Site MCP execution remain separately gated; terminal rendering no longer depends on the old bounded non-terminal launch slice.

## Implemented In `@narada2/carrier-protocol`

- Canonical input event, control JSONL, session event, payload reference, and payload policy schemas.
- Validation for source, transport, delivery mode, hold condition, payload policy, tool call/result payloads, diagnostics, and turn terminal states.
- Legacy input/control normalization for current `agent-cli` compatibility.
- Exported JSON fixtures consumed by Rust acceptance tests.

## Implemented In `@narada2/agent-cli`

- Depends on `@narada2/carrier-protocol`.
- Normalizes interactive, programmatic, and control input through protocol-shaped events.
- Emits protocol session evidence for queue admission, completion, interruption, tool calls/results, commands, diagnostics, and system directive hold/release.
- Supports native `narada.carrier.control.input_event.v1` control JSONL records.
- Preserves legacy `system_directive.deliver` control frames through protocol normalization.
- Uses `operator_steering` for working-time operator input instead of `operator_directive`.

## Implemented In `packages/agent-tui`

- Contract index at `contracts/README.md` defines machine-readable runtime-admission contracts consumed by both Rust `agent-tui` and Narada proper launcher metadata.
- Rust crate and library target: `narada-agent-tui` / `narada_agent_tui`.
- CLI flags for identity, session, Site root, control/session JSONL, runtime step/loop, interactive step/loop, smoke loop, render once, Rust toolchain readiness check, and bounded max steps.
- Rust protocol structs and validators for carrier protocol fixtures.
- Rust validator parity now enforces non-object metadata rejection, agent/external source provenance metadata, directive source/provenance rules, stricter control `input_event_id` prefix validation, strict payload-ref shape, and session payload contracts for queue lifecycle, held/released directives, turn state, interrupts, tool calls/results, commands, and diagnostics.
- Control JSONL parser and watcher with appended-record, partial-line, truncation, malformed-line, and unsupported-line handling.
- Append-only session JSONL writer.
- Input queue with idle admission, active-turn queueing, queue summary/drop/clear operations, system-directive hold until composer clear, and release evidence.
- Runtime coordinator for control polling, queue admission, carrier-local queue commands, literal slash input, composer submit, interrupt evidence, held release, and session evidence writing.
- Interactive TUI loop keeps drawing and admitting input while provider/MCP turn work runs in a background worker; operator submits during active turns remain queued and visible.
- Turn coordinator with provider dispatch boundary recording, background worker execution for interactive TUI turns, and carrier-validated, schema-marked explicit `completed_without_provider` terminal evidence built through shared Rust/TypeScript payload factories.
- Provider dispatch trait plus carrier-validated, schema-marked provider request/output payload contracts built through shared Rust/TypeScript factories, centralized adapter factory, stub, scripted admitted adapter, and explicit governed-session `codex_subscription_adapter`; real provider execution remains disabled outside explicit adapter admission.
- Provider adapter admission boundary distinguishes disabled runtime, refused runtime, configured-without-adapter posture, explicit requested adapter kind via `NARADA_AGENT_TUI_PROVIDER_ADAPTER_KIND`, typed adapter-registry rejection for unknown adapter kinds, admitted scripted adapter execution, and admitted production adapter execution behind the `packages/agent-tui/contracts/provider-adapters.json` implementation flag; the provider execution environment gate is read from the same contract by Rust runtime config and `agent-start` launch metadata.
- Provider boundary acceptance verifies request and terminal evidence both record `recorded_not_dispatched` and `provider_execution_enabled=false`.
- Provider request evidence carries provider runtime posture, configured provider/model when present, explicit streaming contract status, and separate adapter refusal reason so disabled/configured turns are reconstructable from session JSONL.
- Provider dispatch now sends the full admitted input content to the provider adapter; preview truncation remains only an evidence/rendering concern, not the provider prompt.
- Provider boundary acceptance verifies ordered provider text deltas project as one accumulated agent transcript message for the turn.
- Provider runtime config is explicit: provider/model env reaches `configured` runtime posture without carrying execution authority; adapter admission separately records configured-without-adapter, unknown-adapter refusal, explicit admitted scripted execution, and explicit admitted production execution.
- Provider runtime CLI acceptance verifies the binary reports disabled-by-default, refused missing-model, configured provider/model/streaming posture, configured-without-adapter posture, unknown adapter refusal, and production adapter admission; runtime-step and interactive-step evidence both carry separate runtime and adapter posture.
- Agent-start can preseed an operator-authorized system starting directive into `agent-tui` control JSONL via `--agent-tui-starting-directive` or `--agent-tui-starting-directive-file`, before the runtime process is spawned.
- Narada proper has an `agent-tui` Site MCP fabric config at `.ai/mcp/config.json` exposing the filesystem MCP `write_file` tool for governed result artifacts.
- `tools/agent-start/agent-tui-live-turn-acceptance.mjs` productizes the live provider-plus-MCP proof: it writes a starting directive, launches `agent-tui` with explicit provider and MCP admission, captures runtime output to a log, validates the session event chain, validates the written result artifact, and writes compact proof JSON.
- Agent-start launch env keeps `NARADA_AGENT_TUI_ENABLE_PROVIDER_EXECUTION=false` by default and switches it to `true` only when explicit governed-session provider execution is requested.
- Rendering boundary model converts provider stderr, MCP stderr, known-noise suppression, terminal resize, and payload threshold decisions into mediated diagnostics or payload references instead of raw terminal writes; mediated diagnostic rows omit the raw diagnostic marker, render severity as semantic state, align wrapped diagnostic detail under the detail column, and carrier terminal-state rows render completed states as positive scan data.
- Provider output constructors and transcript projection enforce payload references for oversized or sensitive provider text/tool arguments instead of inlining them into transcript rows.
- Live provider streaming is implemented for provider adapters that emit incremental JSON-line text deltas: the Codex-subscription adapter drains stdout while the provider process is still active, writes provider text-delta session evidence through the turn worker sink, keeps the TUI responsive during the active turn, and preserves buffered fallback behavior for non-streaming provider output.
- MCP fabric boundary acceptance verifies disabled-by-default posture, policy-bound tool visibility after admission, and valid tool request/result evidence records.
- MCP fabric transport parses carrier MCP config, validates the config root, `mcpServers` map, and server record shapes, normalizes nonblank top-level string metadata and server string metadata, validates nonblank stdio transport values, server env maps, and inherited env var names with CLI acceptance, derives policy-bound visibility from one unambiguous configured nonblank array tool list, resolves tools to nonblank stdio server launch commands, nonblank string args, explicit server env values, and selected inherited env values with explicit-env override precedence, and prepares evidenced JSON-RPC `tools/call` requests for supervised stdio execution.
- MCP JSON-RPC contract builds newline-delimited `tools/call` requests, parses success/error responses, and classifies responses into MCP tool result evidence summaries.
- MCP stdio process I/O can write one prepared JSON-RPC request, read one response line, validate response identity, classify the result, and has a process-backed one-shot execution wrapper with prepared environment propagation coverage.
- MCP process supervisor contract models server lifecycle states, initialize/initialized handshake frames, readiness gating, a bounded restart decision, and recovery diagnostics as session evidence.
- MCP runtime execution bridge owns per-server supervisor state, gates tool calls on readiness, writes tool request/result evidence to session JSONL, records MCP runtime posture on tool evidence, and records recovery diagnostics when injected execution fails.
- Runtime-step and interactive runtime construction wire the provider tool-call executor from MCP runtime config; production launcher metadata still keeps MCP fabric disabled until live Site MCP admission is explicitly promoted.
- Provider tool-call bridge converts provider `tool_call_request` outputs into MCP tool requests, validates inline JSON arguments, ignores non-tool output, and executes admitted calls through the supervised MCP runtime bridge.
- Provider tool-call executor factory constructs a supervised reusable MCP process executor from configured MCP runtime posture, or preserves the no-op executor when MCP fabric is disabled/refused.
- Turn coordinator accepts an injectable provider tool-call executor and writes provider-origin tool request/result evidence before turn completion when the bridge is supplied.
- Reusable MCP process executor implements the runtime tool executor boundary, executes initialize/initialized handshake when a server process is first spawned, caches one stdio child per server, replaces stale launch specs, records elapsed duration, preemptively terminates blocked reads at timeout, classifies timeout failures, and drops failed processes from the pool.
- Agent-start MCP gate metadata now records the Rust MCP bridge as implemented but withheld from the production runtime slice until live Site MCP execution is admitted.
- MCP runtime config is explicit: Site MCP fabric remains disabled unless the environment gate in `packages/agent-tui/contracts/mcp-runtime.json` is enabled; process launch MCP config/fabric env reaches `configured` posture only when the MCP config path is inside the declared Site MCP fabric root without parent traversal, readable, parseable by the Rust MCP fabric transport client, and declares at least one valid server with at least one visible tool; scaffold output, MCP tool evidence, and agent-start metadata record the config path policy from the same contract, while production exposure remains controlled by launch admission gates.
- Agent-start launch env forces `NARADA_AGENT_TUI_ENABLE_MCP_FABRIC=false` for the bounded runtime slice and records that MCP environment gate in launch metadata.
- Transcript projection and store for operator/system input, system directive hold/release, provider output placeholders, provider tool-call placeholders, and terminal turn status; blank provider text deltas are suppressed before rendering so empty agent blocks do not appear in the transcript.
- Layout, status, composer, transcript, and aggregate app view models, including one runtime posture bundle for disabled/refused/configured provider posture, explicit provider-adapter admission posture, plus disabled/refused/configured MCP and terminal-rendering posture in status output; process env is snapshotted through the library runtime config snapshot boundary before deriving the combined posture.
- Ratatui renderer for transcript/status/composer regions.
- Renderer acceptance verifies nonblank buffer rendering, core region text, stable layout rectangles, and preserved composer draft in compact frames.
- Status rendering and compact status view-model text humanize runtime posture tokens before display, hide non-actionable zero/none counters, report transcript count from visible transcript rows after projection filtering, color status keys separately from values, split active/calling phase values into phase/tool/duration scan spans, split typing/queued operator directive and note phrases into participant/mode/count spans, keep provider adapter posture adjacent to provider posture, style session IDs and transcript counters as neutral scan data, prioritize active errors before runtime posture details, render a muted fill instead of an empty strip when too narrow for any segment, and report terminal-rendering admission refusals with copyable PowerShell environment setup.
- Participant label colors distinguish operator, agent, system, operator directive, carrier/tool runtime, and provider roles at render time; composite directive labels split participant identity from directive mode (`operator` plus `directive`, `system` plus `directive`), long standalone participant labels truncate with a muted ellipsis instead of hard terminal clipping while preserving participant colors, carrier-local queue feedback colors embedded participant names, queued states, durations, and wrapped queue continuations instead of flattening them to muted carrier text, system/operator-directive/provider bodies use role color without bold, technical provider tokens remain code-styled, and directive status bodies render state plus identifier as semantic scan data with wrapped detail aligned under the detail column.
- Transcript body rendering normalizes terminal-control characters before wrapping, trims boundary-only blank rows, preserves internal paragraph spacing, keeps exactly one blank separator line between transcript blocks, truncates overlong timestamp rows with a muted ellipsis, and pins the participant label when tailing an oversized latest block, preventing accidental extra blank lines before timestamps or participant labels while preserving speaker context.
- Transcript scroll state is carrier-local: PageUp/PageDown change the rendered transcript offset without mutating composer draft text, queue state, or session JSONL truth, the app view model and renderer both expose the non-tail scroll offset, the renderer trims artificial separator rows at scrolled viewport boundaries, shows a muted context marker instead of an empty pane when a scrolled slice contains only separators, and composer content is preserved while showing an older transcript window.
- Active-turn composer mode is visibly distinct from idle operator input: the prompt keeps `operator` as the green participant identity while styling `note` as a warning/action mode before the agent target label, active-turn draft text uses the same warning/action styling instead of ordinary operator-input green, long composer prompt labels truncate with a muted ellipsis while preserving participant colors, nonzero queued-note and held-directive affordances render as whole tokens in the composer title with status/count colors or a muted omission marker when width is tight, and whitespace-only drafts are treated as empty in status just as they are for input admission and system-directive holds.
- Wrapped carrier-mediated tool request/result rows align continuation text and timestamp text under the first payload column and preserve payload styling on continuations instead of falling back to muted carrier text; when a narrow transcript cannot fit a directional tool label plus useful payload, the label stays visible and payload/timestamp move under the standard body indent; tool-result status prefixes such as `ok`/`failed` render with their following separator as semantic state before the code-styled tool payload, semantic result summaries such as `success`/`error` render as positive or negative scan data, and tool-call argument text is not misread as result status.
- Status rendering truncates overlong high-priority segments in place with a muted ellipsis while preserving value-span colors, so active work/error context remains visible in narrow terminals without flattening phase/tool/duration styling; when a terminal is too tight to show a clean ellipsis, omitted lower-priority segments are dropped instead of leaving orphan separators or single-dot fragments.
- Transcript body rendering aligns wrapped key/value continuations under the value column, wrapped diff continuations under the diff payload column, and wrapped PowerShell command continuations under the command column; it keeps a muted blockquote marker on wrapped blockquote continuations, and styles list markers, list indentation, wrapped continuation indentation, and Markdown table pipes/fragments as muted structure while keeping list item, continuation, and table cell text in normal body color.
- Short standalone colon-terminated body lines and Markdown headings render as bold cyan section headings, with wrapped Markdown heading continuations aligned under heading text and kept in heading style without repeating the raw marker.
- Fenced code blocks render with a muted `code: <language>` header, omit raw closing fence lines, keep code contents styled as code, preserve literal inline marker characters while wrapping fenced code, and place the timestamp immediately after the code content without an extra fence/blank row.
- Inline code renders as styled code content without showing raw backtick delimiters, including when a narrow transcript wraps the inline-code span across multiple rows.
- Simple bold emphasis renders as bold body text without showing raw `**` delimiters.
- Simple italic emphasis renders as italic body text without showing raw `*` delimiters, while list-marker handling remains separate.
- Blockquote markers render as muted transcript structure while quoted content keeps normal body and inline-code styling.
- Marked transcript lines keep list/blockquote structure even when their content ends with `:`, instead of being promoted to section headings.
- Markdown heading markers render as muted structure while heading text uses the transcript heading style.
- Top-level key-value summary lines render keys, colon separators, technical values, comma-separated identifier lists, semantic status values, and boolean/null values as distinct visual roles while avoiding indented path/prose false positives.
- Lettered choice markers such as `A. ` render as muted structure, and the `(*) ` recommended marker renders as highlighted structure instead of accidental italic markup.
- PowerShell prompt lines render the prompt/path prefix as muted shell structure and the submitted command as code-styled content.
- Transcript timestamp text keeps the `YYYY-MM-DDZHH:mm` layout and offsets while styling date/separator and time as distinct scan targets; inline ISO, Narada timestamp, and compact duration tokens render as code-styled spans.
- Plain Windows paths inside transcript prose render as code-styled spans, with trailing sentence punctuation kept outside the path style.
- Unbackticked Narada/runtime identifiers in prose, such as `narada-proper`, `authority_locus`, and `facade_only`, render as code-styled spans while ordinary hyphenated prose remains body text.
- Plain URL, email-address, slash-command, command-flag, and technical tool-call tokens render as code-styled spans, while Windows path detection requires a drive-letter boundary so `https://` is not split as a false path.
- Markdown link and image-link delimiters render as muted structure while the label remains body-styled and the URL remains code-styled.
- Wrapped list and blockquote body lines align continuation text under the item or quote content column instead of under the marker.
- Markdown task-list checkboxes render as distinct state markers: unchecked muted, checked positive, and wrapped task items align continuation text under the task content column.
- Markdown horizontal rule lines render as muted separators instead of ordinary body text.
- Diff-like addition and deletion lines render their leading marker as positive or negative while preserving normal bullet-list behavior.
- Markdown table row pipes render as muted structure while cell contents keep normal inline styling; table separator rows render fully muted.
- Terminal lifecycle state guard with clean leave semantics after normal exit and render-loop error.
- Terminal input decoding, input tick reduction, composer draft reducer, and render-loop action reducer.
- `tui-textarea` dependency and `TextareaComposer` adapter scaffold with the same submit, clear, interrupt, and draft-state effects as the current reducer.
- Terminal input tick now applies decoded key intents through `TextareaComposer` while preserving the public `ComposerDraftState` and `TerminalInputTickOutcome` bridge.
- Interactive render-loop state now owns a long-lived `TextareaComposer`; runtime and view APIs receive draft snapshots only at their existing boundaries.
- Composer region rendering now uses the `tui-textarea` widget path, with an explicit live-composer render entry point available beside snapshot rendering.
- Terminal interactive draw loop now calls the live-composer renderer so textarea cursor and viewport state are preserved during interactive rendering.
- Terminal runtime config remains explicit for scaffold/status reporting, but terminal entry is admitted by explicit CLI mode: `--render-once` and `--interactive-loop` no longer require the hidden environment gate. CLI acceptance still verifies disabled, refused, and configured terminal posture reporting from `packages/agent-tui/contracts/terminal-runtime.json`, plus direct `--interactive-loop` admission without the env gate.
- Rust terminal runtime config reads `terminal-runtime.json` through an explicit contract module, matching the other shared carrier contracts.
- Terminal drawing has a backend-generic helper used by both real terminal sessions and scripted TestBackend frame acceptance.
- `TerminalLifecycleHarness` records enter/draw/leave behavior for scripted terminal backends without binding tests to process stdout.
- `TerminalSession` and `TerminalLifecycleHarness` are both admitted through the same `InteractiveTerminalFrame` contract, with scripted draw-through-contract acceptance.
- Interactive loop body is extracted behind injectable terminal, input, and clock seams; production `--interactive-loop` now uses the same driver as scripted acceptance.
- Injected-loop acceptance covers active-turn operator submit queuing, exit input handling, input read failure surfacing, malformed control JSONL surfacing, and final draw evidence.
- Injected-loop test fixtures now use shared constructors for runtime paths, terminal frame capture, input outcomes, and deterministic clocks.
- Buffer-level live-composer rendering acceptance covers active-turn prompt text, visible live draft content, multiline draft color, queued-note status, and stale snapshot exclusion.
- Composer redraw acceptance verifies key ticks preserve draft text across redraws, backspace updates rendered text, and submit clears draft while admitting operator text through the bridge once.
- Interactive runtime module that owns polling, held release, one-turn drain, transcript ingestion, and view assembly.
- Smoke runner API: `AgentTuiSmokeSession`, `run_interactive_smoke_step`, and `interactive_smoke_step_summary_lines`.
- Smoke acceptance covers canonical system directive delivery, held directive release, queued operator FIFO ordering, interrupt evidence, malformed control JSONL, and provider-boundary transcript rows.
- Narada proper `agent-start` admits `agent-tui` as a distinct runtime and launches the terminal interactive-loop slice defined in `packages/agent-tui/contracts/launch-slice.json` by default.
- Rust `agent-tui` reads the same launch-slice contract for its admitted terminal interactive-loop carrier flag instead of keeping an independent CLI constant.
- Agent-start launch metadata names the admitted terminal runtime slice, the terminal rendering environment gate, and the separate provider/MCP governed-session gates.
- Agent-start launch metadata keeps provider execution disabled and records the implemented-but-withheld provider adapter contract plus evidence, streaming, and tool-call contracts required before production dispatch admission.
- Rust provider runtime and adapter admission now share one `provider-adapters.json` contract reader instead of parsing the same contract independently.
- Runtime config snapshot reads provider adapter-kind selection from the shared `provider-adapters.json` contract instead of hardcoding its environment key.
- Rust provider runtime reads provider/model/thinking/stream environment keys and admitted provider IDs from `provider-adapters.json` instead of hardcoding them.
- Agent-start launch metadata keeps Site MCP fabric access disabled for the bounded runtime slice and records the implemented-but-withheld Rust MCP bridge plus remaining live Site execution admission requirements.
- Rust MCP runtime config reads `mcp-runtime.json` through an explicit contract module, matching the launch-slice and provider-adapter contract shape.
- Rust MCP runtime config reads MCP config and Site MCP fabric environment keys from `mcp-runtime.json` instead of hardcoding them.
- Agent-start uses named metadata builders for the shared `agent-tui` terminal, provider, and MCP promotion gates instead of inline ad hoc gate objects.
- Agent-start promotion metadata includes a machine-readable checklist for Rust test availability, terminal-loop acceptance, carrier command acceptance, rendering/diagnostic boundary acceptance, payload-reference policy, provider adapter admission, MCP fabric client admission, Site rollout acceptance, and launch metadata runtime slice evidence; completed checklist items are marked satisfied instead of retained as stale rollout blockers.
- Agent-start launch metadata includes a concrete `site_rollout_acceptance` matrix for all launcher-registry Narada Sites, with per-Site side-by-side agent-cli/agent-tui evidence requirements and default promotion blocked until acceptance is current.
- `tools/agent-start/agent-tui-rollout-acceptance.mjs` builds a non-launching rollout acceptance report from the launch metadata, accepts explicit `--known-site-root site=path` root resolution, accepts per-Site `--agent-cli-evidence site=path` and `--agent-tui-evidence site=path`, validates evidence as complete live authoritative `narada.agent_start.result.v0` launch packets for the expected runtime, `launching` status, `dry_run=false`, `exec=true`, authoritative agent-start and carrier-session records, event/session IDs, carrier-specific session/control paths, terminal `agent-tui` interactive-loop slice, and matching Site root, marks a Site accepted only when both evidence paths are valid for that Site, names unresolved or missing Site roots and missing, incomplete, or invalid side-by-side evidence, and can write `.narada/crew/agent-tui-rollout-acceptance/latest.json`.
- Agent-start launch metadata exposes the `agent-tui` Rust toolchain readiness preflight command, expected blocker, and exit-code semantics.
- Agent-start tests assert production launch does not opt into `--interactive-smoke-loop`, `--persistent-smoke-session`, legacy `--runtime-loop`, or `--max-steps`.

## Promotion Gate

`agent-tui` now uses the terminal interactive-loop launch path for `agent-start -Runtime agent-tui`. The remaining promotion gates are not about terminal rendering admission; they govern what the terminal carrier may execute once running:

- Rust tests run in CI or a documented local toolchain with MSVC `link.exe` available.
- `--interactive-step-once` and `--interactive-smoke-loop --max-steps <n>` remain regression harnesses for queued operator input, held system directives, release, interrupt, malformed control JSONL, and transcript projection.
- `--interactive-loop --max-steps <n>` has scripted terminal-frame acceptance that verifies no blank frame, stable layout rectangles, preserved composer draft, and clean terminal leave on exit/error.
- Provider dispatch remains explicitly disabled unless a production provider adapter implementation is admitted with its own authority and evidence contract.
- Site MCP access remains explicitly disabled unless production Site MCP exposure is admitted with policy-bound tool visibility.
- `agent-start` launch metadata names the runtime slice accurately as terminal interactive loop and records provider/MCP gates separately.
## Promotion Gate Gap Audit

Current launch metadata now distinguishes satisfied gates from remaining promotion blockers:

- Satisfied: carrier-local queue commands and literal slash input have parser, runtime coordinator, session evidence, and Rust tests.
- Satisfied: rendering and diagnostic boundaries cover provider stderr, known-noise suppression, payload threshold policy, resize behavior, and stable renderer frames.
- Satisfied: payload reference policy is enforced for large or sensitive provider/tool payloads at transcript boundaries.
- Satisfied: launcher-registry Site rollout has accepted side-by-side `agent-cli` and bounded `agent-tui` evidence for all known Sites.
- Partial: Rust tests pass through the documented VS DevCmd toolchain; plain-shell readiness remains a diagnostic preflight.
- Partial: terminal-loop acceptance has scripted frame, lifecycle, injected-loop, live-composer rendering, and terminal runtime config coverage; real-terminal promotion is blocked on provider admission, MCP admission, and explicit terminal-mode promotion.
- Partial: provider admission has disabled/refused/configured runtime posture, explicit adapter admission evidence, a scripted admitted adapter contract exercised through turn and runtime-step paths, live JSON-line provider text streaming through the Codex-subscription adapter, streaming transcript accumulation, provider-origin tool-call mediation, cancellation, and provider process-tree termination; broad real-terminal production promotion remains withheld.
- Partial: MCP admission has config/fabric posture, policy visibility, request/response framing, supervised stdio execution, runtime-posture evidence, and launch gating; production Site MCP exposure remains withheld until live Site execution is admitted.

## Live Turn Acceptance

`tools/agent-start/agent-tui-live-turn-acceptance.mjs` is the deterministic live-turn acceptance runner for the governed provider plus MCP path. It launches `narada.resident` through `agent-start -Runtime agent-tui`, preloads a starting directive, admits provider execution and Site MCP fabric explicitly, asks the provider to emit one `write_file` tool call, bounds the live subprocess with `--timeout-ms`, and validates the resulting session evidence chain.

The acceptance proof is compact by default; runtime stdout/stderr are captured in a log file instead of flooding the terminal.

```powershell
pnpm agent-tui:live-turn-acceptance
```

Focused validator tests do not call the live provider:

```powershell
pnpm agent-tui:live-turn-acceptance:test
```

Latest live proof:

```text
D:\code\narada\.narada\crew\agent-tui-live-turn-acceptance\latest.json
```

The proof must show:

- `status: passed`
- process exit code `0`
- result artifact schema `narada.agent_tui.live_turn_result.v0`
- session events `input_admitted_to_turn`, `provider_request_recorded`, `provider_tool_call_requested`, `tool_call_requested`, `tool_result_received`, and `turn_completed`
- `tool_result_received` status `ok`, tool `write_file`, and `mcp_runtime_execution: supervised_stdio`

Current verified proof: `D:\code\narada\.narada\crew\agent-tui-live-turn-acceptance\latest.json` generated at `2026-06-01T03:59:51.935Z`.

## Live Rollout Evidence

Latest rollout report:

```text
D:\code\narada\.narada\crew\agent-tui-rollout-acceptance\latest.json
```

Current report state:

The rollout scope is the launcher registry, not the earlier four-site pilot. Current matrix Sites:

- `narada-proper`
- `narada-andrey`
- `narada-staccato`
- `narada-revolution`
- `narada-timour-marketing-agent`
- `narada-utz`
- `narada-sonar`
- `smart-scheduling`
- `thoughts-project`

All launcher-registry Sites now have accepted side-by-side `agent-cli` and bounded `agent-tui` evidence.

Recorded accepted rollout evidence:

```text
narada-proper agent-cli: D:\code\narada\.narada\crew\agent-start-results\agent_start_20260531_000417547_narada_architect.result.json
narada-proper agent-tui: D:\code\narada\.narada\crew\agent-start-results\agent_start_20260531_041426614_narada_architect.result.json
narada-andrey agent-cli: C:\Users\Andrey\Narada\.ai\runtime\agent-start-results\evt-2026-05-31_00-04-19_e5147cef.result.json
narada-andrey agent-tui: C:\Users\Andrey\Narada\.ai\runtime\agent-start-results\evt-2026-05-31_04-27-29_a23016ab.result.json
narada-staccato agent-cli: C:\Users\Andrey\OneDrive - Global Maxima LLC\!Business\!Clients\Staccato\.narada\.ai\runtime\agent-start-results\evt-2026-05-31_00-04-20_131e872d.result.json
narada-staccato agent-tui: C:\Users\Andrey\OneDrive - Global Maxima LLC\!Business\!Clients\Staccato\.narada\.ai\runtime\agent-start-results\evt-2026-05-31_05-04-35_975e949a.result.json
narada-revolution agent-cli: C:\Users\Andrey\OneDrive - Global Maxima LLC\!Business\!Clients\!Revolution\.narada\.ai\runtime\agent-start-results\evt-2026-05-31_00-04-22_c13890e1.result.json
narada-revolution agent-tui: C:\Users\Andrey\OneDrive - Global Maxima LLC\!Business\!Clients\!Revolution\.narada\.ai\runtime\agent-start-results\evt-2026-05-31_05-04-38_77515bed.result.json
narada-timour-marketing-agent agent-cli: C:\Users\Andrey\Vose Software BE\Timour Koupeev - MarketingAgent\.narada\.ai\runtime\agent-start-results\evt-2026-05-31_00-04-23_466ee6d9.result.json
narada-timour-marketing-agent agent-tui: C:\Users\Andrey\Vose Software BE\Timour Koupeev - MarketingAgent\.narada\.ai\runtime\agent-start-results\evt-2026-05-31_05-04-41_c370a882.result.json
narada-utz agent-cli: C:\Users\Andrey\OneDrive - Global Maxima LLC\!Business\!Clients\Utz\.narada\.ai\runtime\agent-start-results\evt-2026-05-31_00-04-24_747be7a9.result.json
narada-utz agent-tui: C:\Users\Andrey\OneDrive - Global Maxima LLC\!Business\!Clients\Utz\.narada\.ai\runtime\agent-start-results\evt-2026-05-31_05-04-44_fc733c37.result.json
narada-sonar agent-cli: D:\code\narada.sonar\.ai\runtime\agent-start-results\evt-2026-05-31_00-04-25_28290811.result.json
narada-sonar agent-tui: D:\code\narada.sonar\.ai\runtime\agent-start-results\evt-2026-05-31_05-04-28_cb53f734.result.json
smart-scheduling agent-cli: D:\code\smart-scheduling\.narada\.ai\runtime\agent-start-results\evt-2026-05-31_00-04-26_b975144f.result.json
smart-scheduling agent-tui: D:\code\smart-scheduling\.narada\.ai\runtime\agent-start-results\evt-2026-05-31_05-04-31_40fb2fd0.result.json
thoughts-project agent-cli: D:\code\thoughts\.narada\.ai\runtime\agent-start-results\evt-2026-05-31_00-04-27_275473a1.result.json
thoughts-project agent-tui: D:\code\thoughts\.narada\.ai\runtime\agent-start-results\evt-2026-05-31_05-04-47_8b47a838.result.json
```

Default promotion is no longer blocked on launcher-registry Site side-by-side rollout evidence. Provider execution, MCP fabric execution, and terminal promotion remain gated separately.

## Site Launcher Propagation

The following Site-local launcher copies are outside the Narada proper Git repository and therefore are not committed with this package. They were updated in place to admit the bounded `agent-tui` runtime slice and verified to carry the same SHA-256 hash:

```text
sha256: 3f25e2dbd6e65b47174af8b606fae3277a4882ecdcdc285702afe9ebdd532340

C:\Users\Andrey\OneDrive - Global Maxima LLC\!Business\!Clients\Staccato\.narada\tools\agent-start\start-agent.mjs
C:\Users\Andrey\OneDrive - Global Maxima LLC\!Business\!Clients\!Revolution\.narada\tools\agent-start\start-agent.mjs
C:\Users\Andrey\Vose Software BE\Timour Koupeev - MarketingAgent\.narada\tools\agent-start\start-agent.mjs
C:\Users\Andrey\OneDrive - Global Maxima LLC\!Business\!Clients\Utz\.narada\tools\agent-start\start-agent.mjs
D:\code\thoughts\.narada\tools\agent-start\start-agent.mjs
```

## Current Verification

Quiet defaults:

```powershell
pnpm agent-tui:test:focused
pnpm agent-tui:test
pnpm --filter @narada2/carrier-protocol test
pnpm agent-start:test
```

Verbose Rust output is intentionally explicit:

```powershell
pnpm agent-tui:test:verbose
```

Plain non-developer shells may still report blocked:

```powershell
node D:\code\narada\tools\agent-start\check-agent-tui-rust-toolchain.mjs
```

Reason: `link.exe` and Windows SDK libraries are available after `VsDevCmd.bat` loads the developer environment. They may not be visible in ordinary shells; the preflight reports this as `missing_msvc_link_exe_on_path` and `windows_sdk_lib_not_loaded_in_LIB`.

## Rust Toolchain Recovery

Use this diagnostic path before treating agent-tui test failures as implementation failures:

```powershell
where.exe link
```

If that does not find `link.exe`, install or modify Visual Studio Build Tools with the C++ build tools workload. If `link.exe` exists but `gdi32.lib` is missing, install or modify Visual Studio Build Tools to include a Windows SDK.

Readiness command:

```powershell
node D:\code\narada\tools\agent-start\check-agent-tui-rust-toolchain.mjs
```

Agent-start launch results expose the same preflight as `agent_tui_launch.rust_toolchain_readiness`.

Expected quiet verification after recovery:

```powershell
pnpm agent-tui:test
```

If `pnpm agent-tui:test` still fails after `VsDevCmd.bat` reports a ready toolchain, treat the new failure as an `agent-tui` implementation issue instead of a toolchain blocker.


## Next Step

Promote the next blocked carrier capability, not more Site rollout evidence. The launcher-registry rollout gate is accepted for all known Sites. The remaining promotion gates are:

- Provider adapter admission: real production provider dispatch now has live JSON-line text streaming over the admitted Codex-subscription adapter contract; remaining work is real-terminal promotion evidence against the live provider, not more buffered dispatch scaffolding.
- MCP fabric launch admission: Rust-side Site MCP discovery, policy-bound visibility, request/response, evidence, and runtime-config executor construction exist; production Site MCP exposure remains withheld by launcher policy until explicitly admitted.
- Terminal interactive launch is admitted: production `agent-start -Runtime agent-tui` now uses terminal rendering by default; remaining work is live provider/MCP execution admission inside that terminal carrier.
- Compact status text now has one shared contract in `status_view_model`: app aggregate lines and renderer width accounting use the same visibility, humanization, draft-count, and scroll-count formatting rules.
- System-originated admitted input now renders as `system directive:` instead of `system -> <agent>:` so transcript provenance matches agent-cli and does not imply operator-style routing.
- Tool-result projection suppresses the ` · ` summary separator when no nonblank result summary exists, preventing dangling punctuation before timestamps.
- Timestamp compaction now renders only strict UTC RFC3339-shaped values as `YYYY-MM-DDZHH:mm`; malformed or non-UTC timestamps are left out instead of being partially styled as authoritative UI time.
- Admitted working-time operator input now preserves `delivery_mode` in session evidence and renders as `operator steering -> <agent>:` with a distinct steering color instead of collapsing into ordinary operator input or operator directives.
- Carrier terminal transcript rows humanize protocol tokens for display, so `completed_without_provider` evidence renders as `completed without provider` while retaining positive carrier-status styling.
- Carrier diagnostic transcript rows omit the redundant rendered `diagnostic` body prefix; the `agent-tui:` label carries the carrier context and the body starts with the styled severity.
- Carrier queue summary rows now style the `queue` label with the carrier color instead of muting it as generic body text, while queue counts and entry states remain warning scan data.

## Live Rollout Evidence Commands

Primary bounded collection path:

```powershell
Set-Location D:\code\narada
.\tools\agent-start\Collect-AgentTuiRolloutEvidence.ps1
```

The script verifies all launcher-registry Sites and rewrites the rollout acceptance report. Site roots and seed evidence paths are parameterized, so a moved checkout or newer baseline can be supplied without editing the script. `-RefreshAgentCli` is refused unless paired with `-AllowInteractiveAgentCliRefresh`, because `agent-cli` baseline collection can block at an operator prompt.
