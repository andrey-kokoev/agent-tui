# Agent TUI Alpha Loop Acceptance

## Canonical launch

Run from `D:\code\narada`:

```powershell
pwsh -File .\narada.ps1 agent-start -Agent narada.resident -Runtime agent-tui -Exec -AgentTuiInteractiveLoop -AgentTuiMaxSteps 10000
```

Do not add `-Json` for this acceptance run. It prints launch JSON before the TUI and makes the operator UX harder to inspect.

## Control path

At startup, `agent-tui` prints the `control.jsonl` path in the launch/session output. Use that path for the snippets below.

Set it once in another PowerShell window:

```powershell
$ControlJsonl = 'PASTE_CONTROL_JSONL_PATH_HERE'
```

## Append valid system input

```powershell
$now = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ss.fffZ'); $id = [guid]::NewGuid().ToString('N'); $record = [ordered]@{ schema='narada.carrier.control.input_event.v1'; control_event_id="control_alpha_$id"; input_event_id="input_alpha_system_$id"; written_at=$now; input=[ordered]@{ schema='narada.carrier.input_event.v1'; event_id="input_alpha_system_$id"; source_kind='system'; source_id='narada-proper.system.directive_emitter'; transport='control_jsonl'; delivery_mode='admit_for_current_turn'; hold_condition='composer_clear_required'; content='run startup sequence'; created_at=$now; authority_ref='alpha_manual_acceptance'; directive_id="dir_alpha_$id"; metadata=[ordered]@{ directive_provenance=[ordered]@{ kind='system_directive'; acceptance='agent_tui_alpha_loop' } } } }; Add-Content -LiteralPath $ControlJsonl -Value ($record | ConvertTo-Json -Compress -Depth 8)
```

## Append malformed control input

```powershell
Add-Content -LiteralPath $ControlJsonl -Value '{not valid json}'
```

## Manual scenario

1. Launch with the canonical command.
2. Type draft text in the composer and do not press Enter.
3. Append valid system input from another PowerShell window.
4. Confirm status shows a held system directive count.
5. Press Enter in the TUI to submit the operator draft.
6. Confirm the held system input releases after the operator input.
7. Append malformed control input.
8. Confirm the TUI reports a parse error without crashing.
9. Append valid system input again.
10. Confirm the valid input is admitted after the malformed line.
11. Press `Ctrl+C` to exit.
12. Confirm the PowerShell prompt is normal and usable.

## Pass criteria

- Operator input is recorded with `source_kind=operator` and `transport=interactive_terminal`.
- System input is recorded with `source_kind=system` and `transport=control_jsonl`.
- Composer-blocked system input records held and released evidence.
- Malformed control input reports an error and does not block later valid input.
- Provider request evidence remains `provider_request_status=recorded_not_dispatched`.
- No `provider_tool_call_requested`, `tool_call_requested`, or `tool_result_received` events are recorded.
- Transcript projection reconstructs operator and system inputs in session order.
- Terminal exits cleanly back to PowerShell.

## Known limits

This alpha loop does not admit real provider dispatch, live Site MCP execution, or native shell authority. Those are separate gates.
