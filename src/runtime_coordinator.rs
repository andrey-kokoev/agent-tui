use crate::carrier_command::{CarrierCommand, OperatorSubmit, parse_operator_submit};
use crate::carrier_input_pipeline_contract::{
    CarrierInputPipelineAdmission, CarrierInputPipelineState, classify_carrier_input_pipeline,
};
use crate::carrier_protocol::{
    DeliveryMode, InputEvent, SessionEvent, SessionEventKind, SourceKind, Transport,
    input_event_schema, session_event_schema,
};
use crate::carrier_protocol_contract::{
    observer_muted_suppression_reason, observer_visibility_default, observer_visibility_is_valid,
    observer_visibility_values,
};
use crate::control_jsonl::ControlJsonlError;
use crate::control_watcher::ControlJsonlWatcher;
use crate::input_queue::{
    AdmissionDecision, InputQueue, QueuedInputSummary, SessionEvidenceContext,
};
use crate::mcp_fabric_transport::McpFabricTransportClient;
use crate::mcp_runtime_config::{McpRuntimeAdmissionStatus, McpRuntimeConfig};
use crate::provider_adapter_contract::provider_adapter_contract;
use crate::session_jsonl::append_session_event;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct RuntimeCoordinatorClock {
    pub occurred_at: String,
    pub event_id_prefix: String,
}

#[derive(Debug)]
pub struct RuntimeCoordinatorPollResult {
    pub admitted_or_queued: usize,
    pub parse_errors: Vec<ControlJsonlError>,
    pub evidence_written: usize,
    pub bytes_read: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeOperatorSubmitResult {
    Empty,
    AgentInput(AdmissionDecision),
    HelpShown,
    StatusShown {
        identity: String,
        session: String,
        model: Option<String>,
        thinking: Option<String>,
        goal: Option<String>,
        queued: usize,
        held: usize,
        turn_state: String,
    },
    StatsShown {
        output: String,
    },
    ModelShown {
        value: Option<String>,
    },
    ModelChanged {
        value: String,
    },
    ThinkingShown {
        value: Option<String>,
    },
    ThinkingChanged {
        value: String,
    },
    ThinkingRejected {
        value: String,
    },
    GoalShown {
        value: Option<String>,
    },
    GoalChanged {
        value: String,
    },
    GoalCleared,
    ToolOutputShown {
        shown: bool,
    },
    ToolOutputChanged {
        shown: bool,
    },
    ToolOutputRejected {
        value: String,
    },
    ToolsShown {
        filter: Option<String>,
        output: String,
    },
    ObserversShown {
        interjections_muted: bool,
        visibility_values: Vec<String>,
        output: String,
    },
    ObserverMuted,
    ObserverUnmuted,
    LocalMessage {
        command: String,
        message: String,
    },
    ClearDisplay,
    Exit,
    UnknownCommand {
        command: String,
    },
    QueueShown {
        queued: Vec<QueuedInputSummary>,
    },
    QueueCleared {
        dropped: usize,
    },
    QueueDrop {
        index: usize,
        dropped_input_event_id: Option<String>,
    },
}

#[derive(Debug)]
pub struct RuntimeCoordinator {
    watcher: ControlJsonlWatcher,
    queue: InputQueue,
    evidence_context: SessionEvidenceContext,
    session_jsonl_path: PathBuf,
    next_evidence_index: u64,
    session_model: Option<String>,
    session_thinking: Option<String>,
    session_goal: Option<String>,
    display_tool_outputs: bool,
    observer_interjections_muted: bool,
    tool_catalog: RuntimeToolCatalog,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeToolCatalog {
    pub status: String,
    pub source: Option<String>,
    pub entries: Vec<RuntimeToolCatalogEntry>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeToolCatalogEntry {
    pub tool_name: String,
    pub server_name: String,
}

fn run_codex_transcript_stats(value: Option<&str>) -> String {
    let extra_args = shell_like_words(value.unwrap_or_default());
    let args = if extra_args.is_empty() {
        vec!["--top".to_string(), "10".to_string()]
    } else {
        extra_args
    };
    let configured_root = std::env::var("NARADA_TOOLS_ROOT").ok();
    let default_root = if cfg!(windows) {
        "D:/code/narada-tools"
    } else {
        "/home/andrey/src/narada-tools"
    };
    let tools_root = configured_root.as_deref().unwrap_or(default_root);
    let script_path = PathBuf::from(tools_root)
        .join("packages")
        .join("codex-transcript-stats")
        .join("src")
        .join("codex-transcript-stats.mjs");

    let mut command = if script_path.exists() {
        let node = std::env::var("NARADA_NODE").unwrap_or_else(|_| "node".to_string());
        let mut command = Command::new(node);
        command.arg(&script_path);
        command
    } else {
        Command::new("codex-transcript-stats")
    };
    command.args(&args);
    if PathBuf::from(tools_root).exists() {
        command.current_dir(tools_root);
    }
    match command.output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            [stdout, stderr]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n")
        }
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let body = [stdout, stderr]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "Codex transcript stats failed with exit {}.{}{}",
                output.status,
                if body.is_empty() { "" } else { "\n" },
                body
            )
        }
        Err(error) => format!(
            "Codex transcript stats unavailable. Expected tool at {} or codex-transcript-stats on PATH.\nError: {}",
            script_path.display(),
            error
        ),
    }
}

fn shell_like_words(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .map(|part| part.trim_matches('"').trim_matches('\'').to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

fn initial_session_goal_from_env() -> Option<String> {
    [
        "NARADA_AGENT_TUI_GOAL",
        "NARADA_CARRIER_GOAL",
        "NARADA_GOAL",
    ]
    .into_iter()
    .find_map(|key| std::env::var(key).ok())
    .map(|value| normalize_session_goal(&value))
    .filter(|value| !value.is_empty())
}

fn normalize_session_goal(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_goal_clear_value(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("clear")
}

impl RuntimeCoordinator {
    pub fn new(
        control_jsonl_path: impl Into<PathBuf>,
        session_jsonl_path: impl Into<PathBuf>,
        evidence_context: SessionEvidenceContext,
    ) -> Self {
        let provider_contract = provider_adapter_contract();
        Self {
            watcher: ControlJsonlWatcher::new(control_jsonl_path),
            queue: InputQueue::new(),
            evidence_context,
            session_jsonl_path: session_jsonl_path.into(),
            next_evidence_index: 1,
            session_model: std::env::var(&provider_contract.ai_model_env_var)
                .ok()
                .filter(|value| !value.trim().is_empty()),
            session_thinking: std::env::var(&provider_contract.ai_thinking_env_var)
                .ok()
                .filter(|value| !value.trim().is_empty()),
            session_goal: initial_session_goal_from_env(),
            display_tool_outputs: true,
            observer_interjections_muted: false,
            tool_catalog: RuntimeToolCatalog::from_process_env(),
        }
    }

    pub fn display_tool_outputs(&self) -> bool {
        self.display_tool_outputs
    }

    pub fn observer_interjections_muted(&self) -> bool {
        self.observer_interjections_muted
    }

    pub fn set_tool_catalog(&mut self, tool_catalog: RuntimeToolCatalog) {
        self.tool_catalog = tool_catalog;
    }

    pub fn queue(&self) -> &InputQueue {
        &self.queue
    }

    pub fn queue_mut(&mut self) -> &mut InputQueue {
        &mut self.queue
    }

    pub fn control_jsonl_path(&self) -> &Path {
        self.watcher.path()
    }

    pub fn session_jsonl_path(&self) -> &Path {
        &self.session_jsonl_path
    }

    pub fn poll_once(
        &mut self,
        composer_has_draft: bool,
        clock: &RuntimeCoordinatorClock,
    ) -> Result<RuntimeCoordinatorPollResult, String> {
        let poll = self.watcher.poll_once()?;
        let mut admitted_or_queued = 0;
        let mut evidence_written = 0;

        for entry in poll.entries {
            let input = entry.event.input;
            let pipeline = classify_carrier_input_pipeline(
                &input,
                CarrierInputPipelineState {
                    active_turn: self.queue.turn_state() == crate::input_queue::TurnState::Active,
                    composer_has_draft,
                    observer_muted: self.observer_interjections_muted,
                },
            );
            if self.is_observer_pipeline(&pipeline) {
                let observer_events = self.observer_pipeline_events(&input, &pipeline, clock);
                let observer_event_count = observer_events.len();
                for event in observer_events {
                    self.write_evidence(&event)?;
                }
                evidence_written += observer_event_count;
                if !pipeline.dispatch_to_provider {
                    continue;
                }
            }
            if pipeline.should_defer && input.hold_condition.is_some() {
                let decision = self
                    .queue
                    .admit_input_event(input.clone(), composer_has_draft);
                let event = self.evidence_for_input_decision(&decision, &input, clock);
                self.write_evidence(&event)?;
                admitted_or_queued += 1;
                evidence_written += 1;
                continue;
            }
            let decision = self
                .queue
                .admit_input_event(input.clone(), composer_has_draft);
            let event = self.evidence_for_input_decision(&decision, &input, clock);
            self.write_evidence(&event)?;
            admitted_or_queued += 1;
            evidence_written += 1;
        }

        Ok(RuntimeCoordinatorPollResult {
            admitted_or_queued,
            parse_errors: poll.errors,
            evidence_written,
            bytes_read: poll.bytes_read,
        })
    }

    pub fn release_held_when_composer_clear(
        &mut self,
        clock: &RuntimeCoordinatorClock,
    ) -> Result<usize, String> {
        let releases = self.queue.release_held_when_composer_clear();
        let mut evidence_written = 0;
        for release in releases {
            let event_id = self.next_event_id(clock);
            let event = release.to_session_event(
                &self.evidence_context,
                event_id,
                clock.occurred_at.clone(),
            );
            self.write_evidence(&event)?;
            evidence_written += 1;
        }
        Ok(evidence_written)
    }

    pub fn admit_operator_composer_submit(
        &mut self,
        text: impl Into<String>,
        clock: &RuntimeCoordinatorClock,
    ) -> Result<AdmissionDecision, String> {
        let input = self.operator_composer_input(text.into(), clock);
        let decision = self.queue.admit_input_event(input.clone(), false);
        let event = self.evidence_for_input_decision(&decision, &input, clock);
        self.write_evidence(&event)?;
        Ok(decision)
    }

    pub fn handle_operator_submit(
        &mut self,
        text: impl Into<String>,
        clock: &RuntimeCoordinatorClock,
    ) -> Result<RuntimeOperatorSubmitResult, String> {
        match parse_operator_submit(&text.into()) {
            OperatorSubmit::Empty => Ok(RuntimeOperatorSubmitResult::Empty),
            OperatorSubmit::AgentInput(text) => self
                .admit_operator_composer_submit(text, clock)
                .map(RuntimeOperatorSubmitResult::AgentInput),
            OperatorSubmit::CarrierCommand(command) => self.execute_carrier_command(command, clock),
        }
    }

    fn execute_carrier_command(
        &mut self,
        command: CarrierCommand,
        clock: &RuntimeCoordinatorClock,
    ) -> Result<RuntimeOperatorSubmitResult, String> {
        match command {
            CarrierCommand::Help => {
                self.write_carrier_command_evidence("/help", clock, json!({}))?;
                Ok(RuntimeOperatorSubmitResult::HelpShown)
            }
            CarrierCommand::Status => {
                self.write_carrier_command_evidence("/status", clock, json!({}))?;
                Ok(RuntimeOperatorSubmitResult::StatusShown {
                    identity: self.evidence_context.agent_id.clone(),
                    session: self.evidence_context.carrier_session_id.clone(),
                    model: self.session_model.clone(),
                    thinking: self.session_thinking.clone(),
                    goal: self.session_goal.clone(),
                    queued: self.queue.queued_count(),
                    held: self.queue.held_count(),
                    turn_state: format!("{:?}", self.queue.turn_state()).to_ascii_lowercase(),
                })
            }
            CarrierCommand::Stats { value } => {
                let output = run_codex_transcript_stats(value.as_deref());
                self.write_carrier_command_evidence(
                    "/stats",
                    clock,
                    json!({ "arguments": value, "runtime_scope": "codex_transcript_store" }),
                )?;
                Ok(RuntimeOperatorSubmitResult::StatsShown { output })
            }
            CarrierCommand::Model { value } => match value {
                Some(value) => {
                    self.session_model = Some(value.clone());
                    self.write_carrier_command_evidence(
                        "/model",
                        clock,
                        json!({ "value": value }),
                    )?;
                    Ok(RuntimeOperatorSubmitResult::ModelChanged { value })
                }
                None => {
                    self.write_carrier_command_evidence("/model", clock, json!({}))?;
                    Ok(RuntimeOperatorSubmitResult::ModelShown {
                        value: self.session_model.clone(),
                    })
                }
            },
            CarrierCommand::Thinking { value } => match value {
                Some(value) if matches!(value.as_str(), "none" | "low" | "medium" | "high") => {
                    self.session_thinking = Some(value.clone());
                    self.write_carrier_command_evidence(
                        "/thinking",
                        clock,
                        json!({ "value": value }),
                    )?;
                    Ok(RuntimeOperatorSubmitResult::ThinkingChanged { value })
                }
                Some(value) => Ok(RuntimeOperatorSubmitResult::ThinkingRejected { value }),
                None => {
                    self.write_carrier_command_evidence("/thinking", clock, json!({}))?;
                    Ok(RuntimeOperatorSubmitResult::ThinkingShown {
                        value: self.session_thinking.clone(),
                    })
                }
            },
            CarrierCommand::Goal { value } => match value {
                Some(value) if is_goal_clear_value(&value) => {
                    self.session_goal = None;
                    self.write_carrier_command_evidence(
                        "/goal",
                        clock,
                        json!({ "action": "clear", "value": null }),
                    )?;
                    Ok(RuntimeOperatorSubmitResult::GoalCleared)
                }
                Some(value) => {
                    let value = normalize_session_goal(&value);
                    self.session_goal = Some(value.clone());
                    self.write_carrier_command_evidence(
                        "/goal",
                        clock,
                        json!({ "action": "set", "value": value }),
                    )?;
                    Ok(RuntimeOperatorSubmitResult::GoalChanged { value })
                }
                None => {
                    self.write_carrier_command_evidence(
                        "/goal",
                        clock,
                        json!({ "action": "show" }),
                    )?;
                    Ok(RuntimeOperatorSubmitResult::GoalShown {
                        value: self.session_goal.clone(),
                    })
                }
            },
            CarrierCommand::ToolOutput { value } => {
                let normalized = value.as_deref().map(str::to_ascii_lowercase);
                match normalized.as_deref() {
                    Some("on" | "show" | "shown") => {
                        self.display_tool_outputs = true;
                        self.write_carrier_command_evidence(
                            "/tool-output",
                            clock,
                            json!({ "value": "shown" }),
                        )?;
                        Ok(RuntimeOperatorSubmitResult::ToolOutputChanged { shown: true })
                    }
                    Some("off" | "hide" | "hidden") => {
                        self.display_tool_outputs = false;
                        self.write_carrier_command_evidence(
                            "/tool-output",
                            clock,
                            json!({ "value": "hidden" }),
                        )?;
                        Ok(RuntimeOperatorSubmitResult::ToolOutputChanged { shown: false })
                    }
                    Some("toggle") | None => {
                        self.display_tool_outputs = !self.display_tool_outputs;
                        self.write_carrier_command_evidence(
                        "/tool-output",
                        clock,
                        json!({ "value": if self.display_tool_outputs { "shown" } else { "hidden" } }),
                    )?;
                        Ok(RuntimeOperatorSubmitResult::ToolOutputChanged {
                            shown: self.display_tool_outputs,
                        })
                    }
                    Some("status") => {
                        self.write_carrier_command_evidence("/tool-output", clock, json!({}))?;
                        Ok(RuntimeOperatorSubmitResult::ToolOutputShown {
                            shown: self.display_tool_outputs,
                        })
                    }
                    Some(value) => Ok(RuntimeOperatorSubmitResult::ToolOutputRejected {
                        value: value.to_string(),
                    }),
                }
            }
            CarrierCommand::Tools { value } => {
                let output = self.tool_catalog.render(value.as_deref());
                self.write_carrier_command_evidence(
                    "/tools",
                    clock,
                    json!({ "arguments": value }),
                )?;
                Ok(RuntimeOperatorSubmitResult::ToolsShown {
                    filter: value,
                    output,
                })
            }
            CarrierCommand::Observers => {
                self.write_carrier_command_evidence("/observers", clock, json!({}))?;
                let visibility_values = observer_visibility_values().to_vec();
                Ok(RuntimeOperatorSubmitResult::ObserversShown {
                    interjections_muted: self.observer_interjections_muted,
                    output: observer_posture_text(self.observer_interjections_muted),
                    visibility_values,
                })
            }
            CarrierCommand::ObserverMute => {
                self.observer_interjections_muted = true;
                self.write_carrier_command_evidence(
                    "/observer mute",
                    clock,
                    json!({ "observer_interjections_muted": true }),
                )?;
                Ok(RuntimeOperatorSubmitResult::ObserverMuted)
            }
            CarrierCommand::ObserverUnmute => {
                self.observer_interjections_muted = false;
                self.write_carrier_command_evidence(
                    "/observer unmute",
                    clock,
                    json!({ "observer_interjections_muted": false }),
                )?;
                Ok(RuntimeOperatorSubmitResult::ObserverUnmuted)
            }
            CarrierCommand::Clear => {
                self.write_carrier_command_evidence("/clear", clock, json!({}))?;
                Ok(RuntimeOperatorSubmitResult::ClearDisplay)
            }
            CarrierCommand::Exit => {
                self.write_carrier_command_evidence("/exit", clock, json!({}))?;
                Ok(RuntimeOperatorSubmitResult::Exit)
            }
            CarrierCommand::Unknown { command } => {
                Ok(RuntimeOperatorSubmitResult::UnknownCommand { command })
            }
            CarrierCommand::QueueShow => Ok(RuntimeOperatorSubmitResult::QueueShown {
                queued: self.queue.queued_summaries(),
            }),
            CarrierCommand::QueueClear => {
                let dropped = self.queue.clear_queued_operator_inputs();
                let dropped_count = dropped.len();
                self.write_carrier_command_evidence(
                    "queue.clear",
                    clock,
                    json!({
                        "dropped": dropped_count,
                    }),
                )?;
                for input in dropped {
                    let event = self.input_dropped_event(&input, "queue_clear", clock);
                    self.write_evidence(&event)?;
                }
                Ok(RuntimeOperatorSubmitResult::QueueCleared {
                    dropped: dropped_count,
                })
            }
            CarrierCommand::QueueDrop { index } => {
                let dropped = self.queue.drop_queued_by_index(index);
                let dropped_input_event_id = dropped.as_ref().map(|input| input.event_id.clone());
                self.write_carrier_command_evidence(
                    "queue.drop",
                    clock,
                    json!({
                        "index": index,
                        "dropped_input_event_id": dropped_input_event_id,
                    }),
                )?;
                if let Some(input) = dropped {
                    let event = self.input_dropped_event(&input, "queue_drop", clock);
                    self.write_evidence(&event)?;
                }
                Ok(RuntimeOperatorSubmitResult::QueueDrop {
                    index,
                    dropped_input_event_id,
                })
            }
        }
    }

    pub fn record_composer_interrupt(
        &mut self,
        clock: &RuntimeCoordinatorClock,
    ) -> Result<(), String> {
        let event = SessionEvent {
            schema: session_event_schema().to_string(),
            event_kind: SessionEventKind::InterruptRequested,
            event_id: self.next_event_id(clock),
            occurred_at: clock.occurred_at.clone(),
            carrier_session_id: self.evidence_context.carrier_session_id.clone(),
            agent_id: self.evidence_context.agent_id.clone(),
            site_id: self.evidence_context.site_id.clone(),
            site_root: self.evidence_context.site_root.clone(),
            payload: json!({
                "turn_id": "turn_unbound_composer_interrupt",
                "source_kind": "operator",
                "source_id": "operator",
                "transport": "interactive_terminal",
                "reason": "composer_interrupt"
            }),
        };
        self.write_evidence(&event)
    }

    fn evidence_for_decision(
        &mut self,
        decision: &crate::input_queue::AdmissionDecision,
        clock: &RuntimeCoordinatorClock,
    ) -> SessionEvent {
        let event_id = self.next_event_id(clock);
        decision.to_session_event(&self.evidence_context, event_id, clock.occurred_at.clone())
    }

    fn evidence_for_input_decision(
        &mut self,
        decision: &crate::input_queue::AdmissionDecision,
        input: &InputEvent,
        clock: &RuntimeCoordinatorClock,
    ) -> SessionEvent {
        let mut event = self.evidence_for_decision(decision, clock);
        if matches!(
            decision,
            AdmissionDecision::AdmitNow { .. } | AdmissionDecision::QueueForTurnBoundary { .. }
        ) {
            let mut payload = json!({
                "input_event_id": input.event_id,
                "source_kind": input.source_kind,
                "source_id": input.source_id,
                "transport": input.transport,
                "delivery_mode": input.delivery_mode,
                "hold_condition": input.hold_condition,
                "content_preview": input.content,
                "authority_ref": input.authority_ref,
                "directive_id": input.directive_id,
                "metadata": input.metadata
            });
            if matches!(decision, AdmissionDecision::QueueForTurnBoundary { .. }) {
                payload["queue_state"] = json!("queued_for_turn_boundary");
            }
            event.payload = payload;
        }
        event
    }

    fn input_dropped_event(
        &mut self,
        input: &InputEvent,
        drop_reason: &str,
        clock: &RuntimeCoordinatorClock,
    ) -> SessionEvent {
        SessionEvent {
            schema: session_event_schema().to_string(),
            event_kind: SessionEventKind::InputDroppedByOperator,
            event_id: self.next_event_id(clock),
            occurred_at: clock.occurred_at.clone(),
            carrier_session_id: self.evidence_context.carrier_session_id.clone(),
            agent_id: self.evidence_context.agent_id.clone(),
            site_id: self.evidence_context.site_id.clone(),
            site_root: self.evidence_context.site_root.clone(),
            payload: json!({
                "input_event_id": input.event_id,
                "drop_reason": drop_reason,
                "source_kind": input.source_kind,
                "source_id": input.source_id,
            }),
        }
    }

    fn is_observer_pipeline(&self, pipeline: &CarrierInputPipelineAdmission) -> bool {
        pipeline
            .admission_event_kinds
            .iter()
            .any(|kind| kind.starts_with("observer_"))
    }

    fn observer_pipeline_events(
        &mut self,
        input: &InputEvent,
        pipeline: &CarrierInputPipelineAdmission,
        clock: &RuntimeCoordinatorClock,
    ) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        for kind in &pipeline.admission_event_kinds {
            let event_kind = match *kind {
                "observer_observation_recorded" => SessionEventKind::ObserverObservationRecorded,
                "observer_interjection_proposed" => SessionEventKind::ObserverInterjectionProposed,
                "observer_interjection_suppressed" => {
                    SessionEventKind::ObserverInterjectionSuppressed
                }
                "observer_interjection_admitted" => SessionEventKind::ObserverInterjectionAdmitted,
                "input_admitted_to_turn" => continue,
                _ => continue,
            };
            let suppression_reason =
                if event_kind == SessionEventKind::ObserverInterjectionSuppressed {
                    pipeline
                        .suppression_reason
                        .or(Some(observer_muted_suppression_reason()))
                } else {
                    None
                };
            events.push(SessionEvent {
                schema: session_event_schema().to_string(),
                event_kind,
                event_id: self.next_event_id(clock),
                occurred_at: clock.occurred_at.clone(),
                carrier_session_id: self.evidence_context.carrier_session_id.clone(),
                agent_id: self.evidence_context.agent_id.clone(),
                site_id: self.evidence_context.site_id.clone(),
                site_root: self.evidence_context.site_root.clone(),
                payload: observer_payload_from_input(input, suppression_reason),
            });
        }
        events
    }

    fn write_carrier_command_evidence(
        &mut self,
        command: &str,
        clock: &RuntimeCoordinatorClock,
        details: serde_json::Value,
    ) -> Result<(), String> {
        let event = SessionEvent {
            schema: session_event_schema().to_string(),
            event_kind: SessionEventKind::CarrierCommandExecuted,
            event_id: self.next_event_id(clock),
            occurred_at: clock.occurred_at.clone(),
            carrier_session_id: self.evidence_context.carrier_session_id.clone(),
            agent_id: self.evidence_context.agent_id.clone(),
            site_id: self.evidence_context.site_id.clone(),
            site_root: self.evidence_context.site_root.clone(),
            payload: json!({ "command": command, "details": details }),
        };
        self.write_evidence(&event)
    }

    fn next_event_id(&mut self, clock: &RuntimeCoordinatorClock) -> String {
        let id = format!("{}_{}", clock.event_id_prefix, self.next_evidence_index);
        self.next_evidence_index += 1;
        id
    }

    fn operator_composer_input(&self, text: String, clock: &RuntimeCoordinatorClock) -> InputEvent {
        InputEvent {
            schema: input_event_schema().to_string(),
            event_id: format!("input_operator_composer_{}", self.next_evidence_index),
            source_kind: SourceKind::Operator,
            source_id: "operator".to_string(),
            transport: Transport::InteractiveTerminal,
            delivery_mode: DeliveryMode::AdmitForCurrentTurn,
            hold_condition: None,
            content: text,
            created_at: clock.occurred_at.clone(),
            authority_ref: None,
            directive_id: None,
            metadata: json!({
                "composer_source": "agent_tui",
                "admission_bridge": "operator_composer_submit"
            }),
        }
    }

    fn write_evidence(&self, event: &SessionEvent) -> Result<(), String> {
        append_session_event(&self.session_jsonl_path, event)
    }
}

impl RuntimeToolCatalog {
    pub fn disabled() -> Self {
        Self {
            status: "disabled".to_string(),
            source: None,
            entries: Vec::new(),
            error: None,
        }
    }

    pub fn configured(
        source: impl Into<String>,
        entries: impl IntoIterator<Item = RuntimeToolCatalogEntry>,
    ) -> Self {
        Self {
            status: "configured".to_string(),
            source: Some(source.into()),
            entries: entries.into_iter().collect(),
            error: None,
        }
    }

    fn from_process_env() -> Self {
        let env = std::env::vars().collect::<BTreeMap<_, _>>();
        Self::from_mcp_runtime_config(&McpRuntimeConfig::from_env_map(&env))
    }

    pub fn from_mcp_runtime_config(config: &McpRuntimeConfig) -> Self {
        match config.status {
            McpRuntimeAdmissionStatus::Disabled => Self::disabled(),
            McpRuntimeAdmissionStatus::Refused => Self {
                status: "refused".to_string(),
                source: config.config_path.clone(),
                entries: Vec::new(),
                error: config.refusal_reason.clone(),
            },
            McpRuntimeAdmissionStatus::Configured => {
                let Some(config_path) = config.config_path.as_deref() else {
                    return Self {
                        status: "refused".to_string(),
                        source: None,
                        entries: Vec::new(),
                        error: Some("missing_mcp_config".to_string()),
                    };
                };
                match McpFabricTransportClient::from_path(config_path) {
                    Ok(client) => Self {
                        status: "configured".to_string(),
                        source: Some(config_path.to_string()),
                        entries: client
                            .tool_to_server
                            .into_iter()
                            .map(|(tool_name, server_name)| RuntimeToolCatalogEntry {
                                tool_name,
                                server_name,
                            })
                            .collect(),
                        error: None,
                    },
                    Err(error) => Self {
                        status: "refused".to_string(),
                        source: Some(config_path.to_string()),
                        entries: Vec::new(),
                        error: Some(error),
                    },
                }
            }
        }
    }

    fn render(&self, filter: Option<&str>) -> String {
        let filter = filter.map(str::trim).filter(|value| !value.is_empty());
        let mut lines = Vec::new();
        lines.push(match filter {
            Some(filter) => format!("Tool catalog ({}, filter: {filter})", self.status),
            None => format!("Tool catalog ({})", self.status),
        });
        if let Some(source) = &self.source {
            lines.push(format!("Source: {source}"));
        }
        if let Some(error) = &self.error {
            lines.push(format!("Error: {error}"));
        }
        let filtered = self
            .entries
            .iter()
            .filter(|entry| match filter {
                Some(filter) => {
                    let filter = filter.to_ascii_lowercase();
                    entry.tool_name.to_ascii_lowercase().contains(&filter)
                        || entry.server_name.to_ascii_lowercase().contains(&filter)
                }
                None => true,
            })
            .collect::<Vec<_>>();
        if filtered.is_empty() {
            lines.push("No tools matched.".to_string());
        } else {
            for entry in filtered {
                lines.push(format!("- {} ({})", entry.tool_name, entry.server_name));
            }
        }
        lines.join("\n")
    }
}

impl RuntimeToolCatalogEntry {
    pub fn new(tool_name: impl Into<String>, server_name: impl Into<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            server_name: server_name.into(),
        }
    }
}

fn observer_posture_text(observer_interjections_muted: bool) -> String {
    format!(
        "Observer posture\nInterjections muted: {}\nSupported visibility: {}",
        if observer_interjections_muted {
            "yes"
        } else {
            "no"
        },
        observer_visibility_values().join(", ")
    )
}

fn observer_payload_from_input(input: &InputEvent, suppression_reason: Option<&str>) -> Value {
    let observer = input.metadata.get("observer").unwrap_or(&Value::Null);
    let visibility = observer
        .get("visibility")
        .and_then(Value::as_str)
        .filter(|value| observer_visibility_is_valid(value))
        .map(str::to_string)
        .unwrap_or_else(|| observer_visibility_default().to_string());
    let mut payload = json!({
        "observer_id": observer
            .get("observer_id")
            .or_else(|| observer.get("id"))
            .and_then(Value::as_str)
            .unwrap_or(input.source_id.as_str()),
        "rule_id": observer
            .get("rule_id")
            .and_then(Value::as_str)
            .unwrap_or("observer_interjection"),
        "visibility": visibility,
        "content": input.content,
        "input_event_id": input.event_id,
    });
    if let Some(confidence) = observer.get("confidence").and_then(Value::as_str) {
        payload["confidence"] = json!(confidence);
    }
    if let Some(reason) = suppression_reason {
        payload["suppression_reason"] = json!(reason);
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{
        control_input_event_schema, input_event_schema, parse_session_event,
    };
    use serde_json::{Value, json};
    use std::fs::{OpenOptions, read_to_string, remove_file};
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    const CONTROL_FIXTURE: &str =
        include_str!("../../narada/packages/carrier-protocol/fixtures/control-input-event.json");

    fn temp_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock works")
            .as_nanos();
        std::env::temp_dir().join(format!("narada-agent-tui-{name}-{unique}.jsonl"))
    }

    fn append(path: &Path, content: &str) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("open temp file");
        file.write_all(content.as_bytes())
            .expect("append temp file");
    }

    fn context() -> SessionEvidenceContext {
        SessionEvidenceContext {
            carrier_session_id: "carrier_fixture_1".to_string(),
            agent_id: "sonar.resident".to_string(),
            site_id: "narada-sonar".to_string(),
            site_root: "D:/code/narada.sonar".to_string(),
        }
    }

    fn clock() -> RuntimeCoordinatorClock {
        RuntimeCoordinatorClock {
            occurred_at: "2026-05-30T00:00:02.000Z".to_string(),
            event_id_prefix: "session_event_runtime".to_string(),
        }
    }

    fn observer_control_event(input_event_id: &str, content: &str, visibility: &str) -> String {
        serde_json::to_string(&json!({
            "schema": control_input_event_schema(),
            "control_event_id": format!("control_{input_event_id}"),
            "input_event_id": input_event_id,
            "written_at": "2026-05-30T00:00:01.000Z",
            "input": {
                "schema": input_event_schema(),
                "event_id": input_event_id,
                "source_kind": "agent",
                "source_id": "narada.observer.fixture",
                "transport": "control_jsonl",
                "delivery_mode": "admit_for_current_turn",
                "hold_condition": null,
                "content": content,
                "created_at": "2026-05-30T00:00:01.000Z",
                "authority_ref": null,
                "directive_id": null,
                "metadata": {
                    "observer": {
                        "role": "observer",
                        "observer_id": "observer.fixture",
                        "rule_id": "fixture_rule",
                        "visibility": visibility
                    }
                }
            }
        }))
        .expect("observer control event serializes")
    }

    fn fixture_tool_catalog() -> RuntimeToolCatalog {
        RuntimeToolCatalog::configured(
            "fixture-mcp-config.json",
            [
                RuntimeToolCatalogEntry::new("site_loop_run_once", "sonar-site-loop"),
                RuntimeToolCatalogEntry::new("task_lifecycle_next", "task-lifecycle"),
                RuntimeToolCatalogEntry::new("mcp_output_show", "agent-context"),
            ],
        )
    }

    fn event_kinds(session_path: &Path) -> Vec<SessionEventKind> {
        read_to_string(session_path)
            .expect("session jsonl exists")
            .lines()
            .map(|line| parse_session_event(line).expect("event parses").event_kind)
            .collect()
    }

    #[test]
    fn carrier_tool_output_toggles_display_state_and_records_evidence() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());

        let result = coordinator
            .handle_operator_submit("/tool-output off", &clock())
            .expect("tool output command handled");
        assert_eq!(
            result,
            RuntimeOperatorSubmitResult::ToolOutputChanged { shown: false }
        );
        assert_eq!(coordinator.display_tool_outputs(), false);

        let status = coordinator
            .handle_operator_submit("/tool-output status", &clock())
            .expect("tool output status handled");
        assert_eq!(
            status,
            RuntimeOperatorSubmitResult::ToolOutputShown { shown: false }
        );

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let events = session_jsonl
            .lines()
            .map(|line| parse_session_event(line).expect("event parses"))
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0].event_kind,
            SessionEventKind::CarrierCommandExecuted
        );
        assert_eq!(events[0].payload["command"], "/tool-output");
        assert_eq!(events[0].payload["details"]["value"], "hidden");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn goal_command_sets_shows_and_clears_session_goal() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());

        let changed = coordinator
            .handle_operator_submit("/goal   finish   carrier parity  ", &clock())
            .expect("goal set handled");
        assert_eq!(
            changed,
            RuntimeOperatorSubmitResult::GoalChanged {
                value: "finish carrier parity".to_string()
            }
        );

        let shown = coordinator
            .handle_operator_submit("/goal", &clock())
            .expect("goal show handled");
        assert_eq!(
            shown,
            RuntimeOperatorSubmitResult::GoalShown {
                value: Some("finish carrier parity".to_string())
            }
        );

        let cleared = coordinator
            .handle_operator_submit("/goal clear", &clock())
            .expect("goal clear handled");
        assert_eq!(cleared, RuntimeOperatorSubmitResult::GoalCleared);

        let none_goal = coordinator
            .handle_operator_submit("/goal none", &clock())
            .expect("goal none handled");
        assert_eq!(
            none_goal,
            RuntimeOperatorSubmitResult::GoalChanged {
                value: "none".to_string()
            }
        );

        let reset_goal = coordinator
            .handle_operator_submit("/goal reset", &clock())
            .expect("goal reset handled");
        assert_eq!(
            reset_goal,
            RuntimeOperatorSubmitResult::GoalChanged {
                value: "reset".to_string()
            }
        );

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let events = session_jsonl
            .lines()
            .map(|line| parse_session_event(line).expect("event parses"))
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 5);
        assert_eq!(events[0].payload["command"], "/goal");
        assert_eq!(events[0].payload["details"]["action"], "set");
        assert_eq!(
            events[0].payload["details"]["value"],
            "finish carrier parity"
        );
        assert_eq!(events[1].payload["details"]["action"], "show");
        assert_eq!(events[2].payload["details"]["action"], "clear");
        assert_eq!(events[2].payload["details"]["value"], Value::Null);
        assert_eq!(events[3].payload["details"]["action"], "set");
        assert_eq!(events[3].payload["details"]["value"], "none");
        assert_eq!(events[4].payload["details"]["action"], "set");
        assert_eq!(events[4].payload["details"]["value"], "reset");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn tools_command_returns_filtered_catalog_without_provider_turn() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());
        coordinator.set_tool_catalog(fixture_tool_catalog());

        let result = coordinator
            .handle_operator_submit("/tools lifecycle", &clock())
            .expect("tools command handled");

        match result {
            RuntimeOperatorSubmitResult::ToolsShown { filter, output } => {
                assert_eq!(filter.as_deref(), Some("lifecycle"));
                assert!(output.contains("Tool catalog (configured, filter: lifecycle)"));
                assert!(output.contains("task_lifecycle_next"));
                assert!(!output.contains("site_loop_run_once"));
                assert!(!output.contains("not implemented"));
            }
            other => panic!("unexpected result: {other:?}"),
        }
        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 1);
        let command = parse_session_event(lines[0]).expect("command event parses");
        assert_eq!(command.event_kind, SessionEventKind::CarrierCommandExecuted);
        assert_eq!(command.payload["command"], "/tools");
        assert_eq!(command.payload["details"]["arguments"], "lifecycle");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn observers_command_reports_current_posture_and_visibility_values() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());
        coordinator
            .handle_operator_submit("/observer mute", &clock())
            .expect("observer mute succeeds");

        let result = coordinator
            .handle_operator_submit("/observers", &clock())
            .expect("observers command succeeds");

        match result {
            RuntimeOperatorSubmitResult::ObserversShown {
                interjections_muted,
                visibility_values,
                output,
            } => {
                assert!(interjections_muted);
                assert!(visibility_values.contains(&"operator_visible".to_string()));
                assert!(visibility_values.contains(&"conversation_visible".to_string()));
                assert!(output.contains("Interjections muted: yes"));
                assert!(output.contains("operator_visible"));
                assert!(!output.contains("not implemented"));
            }
            other => panic!("unexpected result: {other:?}"),
        }

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn muted_record_only_observer_records_observation_without_suppression() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());

        let mute = coordinator
            .handle_operator_submit("/observer mute", &clock())
            .expect("observer mute succeeds");
        assert_eq!(mute, RuntimeOperatorSubmitResult::ObserverMuted);
        assert!(coordinator.observer_interjections_muted());

        append(
            &control_path,
            &observer_control_event(
                "input_observer_record_only_1",
                "record-only observer note",
                "record_only",
            ),
        );
        append(&control_path, "\n");
        let result = coordinator
            .poll_once(false, &clock())
            .expect("poll once succeeds");

        assert_eq!(result.admitted_or_queued, 0);
        assert_eq!(result.evidence_written, 1);
        assert_eq!(coordinator.queue().queued_count(), 0);
        assert_eq!(
            event_kinds(&session_path),
            vec![
                SessionEventKind::CarrierCommandExecuted,
                SessionEventKind::ObserverObservationRecorded,
            ]
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn muted_visible_observer_records_observation_proposal_and_suppression() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());

        coordinator
            .handle_operator_submit("/observer mute", &clock())
            .expect("observer mute succeeds");

        append(
            &control_path,
            &observer_control_event(
                "input_observer_fixture_1",
                "visible observer note",
                "conversation_visible",
            ),
        );
        append(&control_path, "\n");
        let result = coordinator
            .poll_once(false, &clock())
            .expect("poll once succeeds");

        assert_eq!(result.admitted_or_queued, 0);
        assert_eq!(result.evidence_written, 3);
        assert_eq!(coordinator.queue().queued_count(), 0);
        assert_eq!(
            event_kinds(&session_path),
            vec![
                SessionEventKind::CarrierCommandExecuted,
                SessionEventKind::ObserverObservationRecorded,
                SessionEventKind::ObserverInterjectionProposed,
                SessionEventKind::ObserverInterjectionSuppressed,
            ]
        );

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 4);
        let command = parse_session_event(lines[0]).expect("command event parses");
        assert_eq!(command.payload["command"], "/observer mute");
        assert_eq!(
            command.payload["details"]["observer_interjections_muted"],
            true
        );
        let suppressed = parse_session_event(lines[3]).expect("suppression event parses");
        assert_eq!(
            suppressed.event_kind,
            SessionEventKind::ObserverInterjectionSuppressed
        );
        assert_eq!(
            suppressed.payload["input_event_id"],
            "input_observer_fixture_1"
        );
        assert_eq!(suppressed.payload["observer_id"], "observer.fixture");
        assert_eq!(suppressed.payload["rule_id"], "fixture_rule");
        assert_eq!(suppressed.payload["visibility"], "conversation_visible");
        assert_eq!(
            suppressed.payload["suppression_reason"],
            observer_muted_suppression_reason()
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn unmuted_operator_visible_observer_completes_without_provider_turn_admission() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());

        append(
            &control_path,
            &observer_control_event(
                "input_observer_operator_visible_1",
                "operator-visible observer note",
                "operator_visible",
            ),
        );
        append(&control_path, "\n");
        let result = coordinator
            .poll_once(false, &clock())
            .expect("poll once succeeds");

        assert_eq!(result.admitted_or_queued, 0);
        assert_eq!(result.evidence_written, 3);
        assert_eq!(
            event_kinds(&session_path),
            vec![
                SessionEventKind::ObserverObservationRecorded,
                SessionEventKind::ObserverInterjectionProposed,
                SessionEventKind::ObserverInterjectionAdmitted,
            ]
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn unmuted_agent_visible_observer_records_observer_admission_and_turn_admission() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());

        append(
            &control_path,
            &observer_control_event(
                "input_observer_agent_visible_1",
                "agent-visible observer note",
                "agent_visible",
            ),
        );
        append(&control_path, "\n");
        let result = coordinator
            .poll_once(false, &clock())
            .expect("poll once succeeds");

        assert_eq!(result.admitted_or_queued, 1);
        assert_eq!(result.evidence_written, 4);
        assert_eq!(
            event_kinds(&session_path),
            vec![
                SessionEventKind::ObserverObservationRecorded,
                SessionEventKind::ObserverInterjectionProposed,
                SessionEventKind::ObserverInterjectionAdmitted,
                SessionEventKind::InputAdmittedToTurn,
            ]
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn observer_unmute_restores_conversation_visible_turn_admission() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());

        coordinator
            .handle_operator_submit("/observer mute", &clock())
            .expect("observer mute succeeds");
        let unmute = coordinator
            .handle_operator_submit("/observer unmute", &clock())
            .expect("observer unmute succeeds");
        assert_eq!(unmute, RuntimeOperatorSubmitResult::ObserverUnmuted);
        assert!(!coordinator.observer_interjections_muted());

        append(
            &control_path,
            &observer_control_event(
                "input_observer_fixture_2",
                "observer note after unmute",
                "conversation_visible",
            ),
        );
        append(&control_path, "\n");
        let result = coordinator
            .poll_once(false, &clock())
            .expect("poll once succeeds");

        assert_eq!(result.admitted_or_queued, 1);
        assert_eq!(result.evidence_written, 4);
        assert_eq!(
            event_kinds(&session_path),
            vec![
                SessionEventKind::CarrierCommandExecuted,
                SessionEventKind::CarrierCommandExecuted,
                SessionEventKind::ObserverObservationRecorded,
                SessionEventKind::ObserverInterjectionProposed,
                SessionEventKind::ObserverInterjectionAdmitted,
                SessionEventKind::InputAdmittedToTurn,
            ]
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn poll_once_admits_control_events_and_writes_evidence() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        append(&control_path, CONTROL_FIXTURE);
        append(&control_path, "\n");

        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());
        let result = coordinator
            .poll_once(false, &clock())
            .expect("poll once succeeds");

        assert_eq!(result.admitted_or_queued, 1);
        assert!(result.parse_errors.is_empty());
        assert_eq!(result.evidence_written, 1);

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 1);
        let event = parse_session_event(lines[0]).expect("session event parses");
        assert_eq!(event.event_id, "session_event_runtime_1");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn held_directive_release_writes_evidence() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        append(&control_path, CONTROL_FIXTURE);
        append(&control_path, "\n");

        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());
        let result = coordinator
            .poll_once(true, &clock())
            .expect("poll once succeeds");
        assert_eq!(result.evidence_written, 1);
        assert_eq!(coordinator.queue().held_count(), 1);

        let released = coordinator
            .release_held_when_composer_clear(&RuntimeCoordinatorClock {
                occurred_at: "2026-05-30T00:00:03.000Z".to_string(),
                event_id_prefix: "session_event_runtime".to_string(),
            })
            .expect("release succeeds");
        assert_eq!(released, 1);

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 2);
        let release = parse_session_event(lines[1]).expect("release event parses");
        assert_eq!(release.event_id, "session_event_runtime_2");
        assert_eq!(release.payload["released_reason"], "composer_clear");
        assert_eq!(release.payload["released_at"], "2026-05-30T00:00:03.000Z");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn carrier_queue_show_is_read_only_and_returns_queued_summary() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());
        coordinator
            .queue_mut()
            .set_turn_state(crate::input_queue::TurnState::Active);
        coordinator
            .handle_operator_submit("queued note", &clock())
            .expect("queued note submits");

        let shown = coordinator
            .handle_operator_submit("/queue", &clock())
            .expect("queue show succeeds");
        match shown {
            RuntimeOperatorSubmitResult::QueueShown { queued } => {
                assert_eq!(queued.len(), 1);
                assert_eq!(queued[0].index, 1);
                assert_eq!(queued[0].content_preview, "queued note");
            }
            other => panic!("unexpected result: {other:?}"),
        }

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        assert_eq!(session_jsonl.lines().count(), 1);
        let queued_event = parse_session_event(
            session_jsonl
                .lines()
                .next()
                .expect("queued evidence line exists"),
        )
        .expect("queued evidence parses");
        assert_eq!(
            queued_event.event_kind,
            SessionEventKind::InputQueuedForTurnBoundary
        );
        assert_eq!(
            queued_event.payload["queue_state"],
            "queued_for_turn_boundary"
        );
        assert_eq!(queued_event.payload["source_kind"], "operator");
        assert_eq!(
            queued_event.payload["delivery_mode"],
            "admit_for_current_turn"
        );
        assert_eq!(queued_event.payload["content_preview"], "queued note");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn carrier_queue_clear_records_command_and_drop_evidence() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());
        coordinator
            .queue_mut()
            .set_turn_state(crate::input_queue::TurnState::Active);
        coordinator
            .handle_operator_submit("queued note", &clock())
            .expect("queued note submits");

        let result = coordinator
            .handle_operator_submit("/queue clear", &clock())
            .expect("queue clear succeeds");
        assert_eq!(
            result,
            RuntimeOperatorSubmitResult::QueueCleared { dropped: 1 }
        );
        assert_eq!(coordinator.queue().queued_count(), 0);

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 3);
        let command = parse_session_event(lines[1]).expect("command event parses");
        assert_eq!(command.event_kind, SessionEventKind::CarrierCommandExecuted);
        assert_eq!(command.payload["command"], "queue.clear");
        let dropped = parse_session_event(lines[2]).expect("drop event parses");
        assert_eq!(dropped.event_kind, SessionEventKind::InputDroppedByOperator);
        assert_eq!(dropped.payload["drop_reason"], "queue_clear");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn carrier_queue_drop_records_command_and_single_drop_evidence() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());
        coordinator
            .queue_mut()
            .set_turn_state(crate::input_queue::TurnState::Active);
        coordinator
            .handle_operator_submit("first queued note", &clock())
            .expect("first queued note submits");
        coordinator
            .handle_operator_submit("second queued note", &clock())
            .expect("second queued note submits");

        let result = coordinator
            .handle_operator_submit("/queue drop 2", &clock())
            .expect("queue drop succeeds");
        assert_eq!(
            result,
            RuntimeOperatorSubmitResult::QueueDrop {
                index: 2,
                dropped_input_event_id: Some("input_operator_composer_2".to_string()),
            }
        );
        assert_eq!(coordinator.queue().queued_count(), 1);

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 4);
        let command = parse_session_event(lines[2]).expect("command event parses");
        assert_eq!(command.payload["command"], "queue.drop");
        let dropped = parse_session_event(lines[3]).expect("drop event parses");
        assert_eq!(
            dropped.payload["input_event_id"],
            "input_operator_composer_2"
        );
        assert_eq!(dropped.payload["drop_reason"], "queue_drop");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn status_reports_current_model_and_thinking() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());

        coordinator
            .handle_operator_submit("/model gpt-5.5-mini", &clock())
            .expect("model command succeeds");
        coordinator
            .handle_operator_submit("/thinking high", &clock())
            .expect("thinking command succeeds");
        coordinator
            .handle_operator_submit("/goal finish carrier parity", &clock())
            .expect("goal command succeeds");
        let result = coordinator
            .handle_operator_submit("/status", &clock())
            .expect("status command succeeds");

        assert_eq!(
            result,
            RuntimeOperatorSubmitResult::StatusShown {
                identity: "sonar.resident".to_string(),
                session: "carrier_fixture_1".to_string(),
                model: Some("gpt-5.5-mini".to_string()),
                thinking: Some("high".to_string()),
                goal: Some("finish carrier parity".to_string()),
                queued: 0,
                held: 0,
                turn_state: "idle".to_string(),
            }
        );

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn double_slash_submit_records_literal_slash_agent_input() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());

        let result = coordinator
            .handle_operator_submit("//help", &clock())
            .expect("literal slash submits");
        assert!(matches!(result, RuntimeOperatorSubmitResult::AgentInput(_)));

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 1);
        let event = parse_session_event(lines[0]).expect("session event parses");
        assert_eq!(event.payload["content_preview"], "/help");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn composer_submit_writes_operator_input_admission_evidence() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());

        let decision = coordinator
            .admit_operator_composer_submit("run startup sequence", &clock())
            .expect("composer submit admits");
        assert!(matches!(decision, AdmissionDecision::AdmitNow { .. }));

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 1);
        let event = parse_session_event(lines[0]).expect("session event parses");
        assert_eq!(event.event_id, "session_event_runtime_1");
        assert_eq!(event.payload["input_event_id"], "input_operator_composer_1");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }

    #[test]
    fn composer_interrupt_writes_interrupt_evidence() {
        let control_path = temp_path("control");
        let session_path = temp_path("session");
        let mut coordinator = RuntimeCoordinator::new(&control_path, &session_path, context());

        coordinator
            .record_composer_interrupt(&clock())
            .expect("interrupt evidence writes");

        let session_jsonl = read_to_string(&session_path).expect("session jsonl exists");
        let lines: Vec<&str> = session_jsonl.lines().collect();
        assert_eq!(lines.len(), 1);
        let event = parse_session_event(lines[0]).expect("session event parses");
        assert_eq!(event.event_kind, SessionEventKind::InterruptRequested);
        assert_eq!(event.payload["turn_id"], "turn_unbound_composer_interrupt");
        assert_eq!(event.payload["reason"], "composer_interrupt");

        remove_file(control_path).ok();
        remove_file(session_path).ok();
    }
}
