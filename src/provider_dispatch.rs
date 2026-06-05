use crate::carrier_protocol::{
    InputEvent, SessionEventKind, create_provider_request_payload,
    create_provider_text_delta_payload, create_provider_tool_call_payload,
};
use crate::provider_adapter_admission::{ProviderAdapterAdmission, ProviderAdapterKind};
use crate::provider_process_tree::ProviderProcess;
use crate::provider_runtime_config::ProviderRuntimeConfig;
use crate::rendering_boundary::{
    InlinePayloadDecision, decide_payload_inline, default_payload_policy,
};
use serde_json::{Value, json};
use std::env;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderDispatchStatus {
    RecordedNotDispatched,
    Dispatched,
    Completed,
    Failed,
    Interrupted,
}

impl ProviderDispatchStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RecordedNotDispatched => "recorded_not_dispatched",
            Self::Dispatched => "dispatched",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderDispatchRecord {
    pub status: ProviderDispatchStatus,
    pub provider_execution_enabled: bool,
    pub payload: Value,
    pub outputs: Vec<ProviderOutputRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAdapterRequest {
    pub turn_id: String,
    pub input_event_id: String,
    pub content_preview: String,
    pub provider_runtime_status: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub stream: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderOutputKind {
    TextDelta,
    ToolCallRequest,
}

impl ProviderOutputKind {
    pub fn session_event_kind(&self) -> SessionEventKind {
        match self {
            Self::TextDelta => SessionEventKind::ProviderTextDeltaRecorded,
            Self::ToolCallRequest => SessionEventKind::ProviderToolCallRequested,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TextDelta => "text_delta",
            Self::ToolCallRequest => "tool_call_request",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderOutputRecord {
    pub kind: ProviderOutputKind,
    pub payload: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl ProviderCancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

pub trait ProviderAdapter: Send {
    fn dispatch_start_record(
        &self,
        _input: &InputEvent,
        _turn_id: &str,
    ) -> Option<ProviderDispatchRecord> {
        None
    }

    fn set_session_model(&mut self, _model: Option<String>) {}

    fn set_session_thinking(&mut self, _thinking: Option<String>) {}

    fn dispatch_request(
        &self,
        input: &InputEvent,
        turn_id: &str,
        cancellation: &ProviderCancellationToken,
    ) -> ProviderDispatchRecord;

    fn dispatch_request_streaming(
        &self,
        input: &InputEvent,
        turn_id: &str,
        cancellation: &ProviderCancellationToken,
        _sink: &mut dyn ProviderOutputSink,
    ) -> ProviderDispatchRecord {
        self.dispatch_request(input, turn_id, cancellation)
    }
}

fn refresh_adapter_admission(
    runtime_config: &ProviderRuntimeConfig,
    adapter_admission: &ProviderAdapterAdmission,
) -> ProviderAdapterAdmission {
    ProviderAdapterAdmission::from_runtime_config(
        runtime_config,
        adapter_admission.adapter_kind.as_deref(),
    )
}

pub trait ProviderOutputSink {
    fn emit_provider_output(&mut self, output: ProviderOutputRecord) -> Result<(), String>;
}

pub struct NoopProviderOutputSink;

impl ProviderOutputSink for NoopProviderOutputSink {
    fn emit_provider_output(&mut self, _output: ProviderOutputRecord) -> Result<(), String> {
        Err("provider_output_sink_disabled".to_string())
    }
}

#[derive(Debug, Clone)]
enum ProviderExecutionResult {
    Completed(Vec<ProviderOutputRecord>),
    Interrupted(String),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct ScriptedProviderAdapter {
    runtime_config: ProviderRuntimeConfig,
    adapter_admission: ProviderAdapterAdmission,
    outputs: Vec<ProviderOutputRecord>,
}

#[derive(Debug, Clone)]
pub struct ProviderDispatchStub {
    runtime_config: ProviderRuntimeConfig,
    adapter_admission: ProviderAdapterAdmission,
}

#[derive(Debug, Clone)]
pub struct CodexSubscriptionProviderAdapter {
    runtime_config: ProviderRuntimeConfig,
    adapter_admission: ProviderAdapterAdmission,
}

impl ProviderAdapterRequest {
    pub fn from_input(
        input: &InputEvent,
        turn_id: impl Into<String>,
        runtime_config: &ProviderRuntimeConfig,
    ) -> Self {
        Self {
            turn_id: turn_id.into(),
            input_event_id: input.event_id.clone(),
            content_preview: input.content.clone(),
            provider_runtime_status: runtime_config.status.as_str().to_string(),
            provider: runtime_config.provider.clone(),
            model: runtime_config.model.clone(),
            thinking: runtime_config.thinking.clone(),
            stream: runtime_config.stream,
        }
    }

    pub fn dispatch_payload(
        &self,
        status: &ProviderDispatchStatus,
        adapter_admission: &ProviderAdapterAdmission,
    ) -> Value {
        create_provider_request_payload(
            &self.turn_id,
            &self.input_event_id,
            status.as_str(),
            adapter_admission.provider_execution_enabled,
            &self.provider_runtime_status,
            adapter_admission.status.as_str(),
            adapter_admission.adapter_kind.clone(),
            self.provider.clone(),
            self.model.clone(),
            self.thinking.clone(),
            self.stream,
            provider_streaming_contract(adapter_admission.provider_execution_enabled, self.stream),
            adapter_admission.refusal_reason.clone(),
            &self.content_preview,
        )
    }
}

fn provider_streaming_contract(provider_execution_enabled: bool, stream: bool) -> &'static str {
    match (provider_execution_enabled, stream) {
        (true, true) => "streaming_text_delta_events",
        (true, false) => "single_provider_output_batch",
        (false, true) => "requested_but_not_dispatched",
        (false, false) => "not_requested",
    }
}

impl ProviderOutputRecord {
    pub fn text_delta(turn_id: &str, delta: &str, sequence: u64) -> Self {
        let policy = default_payload_policy();
        let payload_ref_id = format!("mcp_payload:provider_text_{turn_id}_{sequence}@v1");
        let decision = decide_payload_inline(
            delta,
            false,
            payload_ref_id,
            "provider text delta omitted from transcript",
            &policy,
        );
        let (text_delta, text_delta_ref) = inline_text_or_ref(delta, decision);
        Self {
            kind: ProviderOutputKind::TextDelta,
            payload: create_provider_text_delta_payload(
                turn_id,
                sequence,
                &text_delta,
                text_delta_ref,
            ),
        }
    }

    pub fn tool_call_request(
        turn_id: &str,
        tool_name: &str,
        arguments_summary: &str,
        sequence: u64,
    ) -> Self {
        Self::tool_call_request_with_sensitivity(
            turn_id,
            tool_name,
            arguments_summary,
            sequence,
            false,
        )
    }

    pub fn sensitive_tool_call_request(
        turn_id: &str,
        tool_name: &str,
        arguments_summary: &str,
        sequence: u64,
    ) -> Self {
        Self::tool_call_request_with_sensitivity(
            turn_id,
            tool_name,
            arguments_summary,
            sequence,
            true,
        )
    }

    fn tool_call_request_with_sensitivity(
        turn_id: &str,
        tool_name: &str,
        arguments_summary: &str,
        sequence: u64,
        _sensitive: bool,
    ) -> Self {
        Self {
            kind: ProviderOutputKind::ToolCallRequest,
            payload: create_provider_tool_call_payload(
                turn_id,
                sequence,
                tool_name,
                arguments_summary,
                Value::Null,
            ),
        }
    }
}

fn inline_text_or_ref(text: &str, decision: InlinePayloadDecision) -> (String, Value) {
    match decision {
        InlinePayloadDecision::Inline => (text.to_string(), Value::Null),
        InlinePayloadDecision::RequiresRef(payload_ref) => {
            (payload_ref.summary.clone(), json!(payload_ref))
        }
    }
}

impl ScriptedProviderAdapter {
    pub fn try_new(
        runtime_config: ProviderRuntimeConfig,
        adapter_kind: ProviderAdapterKind,
        outputs: Vec<ProviderOutputRecord>,
    ) -> Result<Self, String> {
        let adapter_admission = ProviderAdapterAdmission::try_admit(&runtime_config, adapter_kind)?;
        Ok(Self {
            runtime_config,
            adapter_admission,
            outputs,
        })
    }
}

impl ProviderAdapter for ScriptedProviderAdapter {
    fn set_session_model(&mut self, model: Option<String>) {
        self.runtime_config.model = model;
        self.adapter_admission =
            refresh_adapter_admission(&self.runtime_config, &self.adapter_admission);
    }

    fn set_session_thinking(&mut self, thinking: Option<String>) {
        self.runtime_config.thinking = thinking;
        self.adapter_admission =
            refresh_adapter_admission(&self.runtime_config, &self.adapter_admission);
    }

    fn dispatch_request(
        &self,
        input: &InputEvent,
        turn_id: &str,
        _cancellation: &ProviderCancellationToken,
    ) -> ProviderDispatchRecord {
        let status = ProviderDispatchStatus::Completed;
        let request = ProviderAdapterRequest::from_input(input, turn_id, &self.runtime_config);
        ProviderDispatchRecord {
            status: status.clone(),
            provider_execution_enabled: self.adapter_admission.provider_execution_enabled,
            payload: request.dispatch_payload(&status, &self.adapter_admission),
            outputs: self.outputs.clone(),
        }
    }
}

pub fn provider_adapter_from_runtime_config(
    runtime_config: ProviderRuntimeConfig,
    adapter_admission: ProviderAdapterAdmission,
) -> Box<dyn ProviderAdapter> {
    if adapter_admission.provider_execution_enabled
        && adapter_admission.adapter_kind.as_deref()
            == Some(ProviderAdapterKind::CodexSubscription.as_str())
    {
        return Box::new(CodexSubscriptionProviderAdapter {
            runtime_config,
            adapter_admission,
        });
    }
    Box::new(
        ProviderDispatchStub::with_runtime_config_and_adapter_admission(
            runtime_config,
            adapter_admission,
        ),
    )
}

impl ProviderDispatchStub {
    pub fn disabled() -> Self {
        let runtime_config = ProviderRuntimeConfig::disabled();
        let adapter_admission =
            ProviderAdapterAdmission::from_runtime_config(&runtime_config, None);
        Self {
            runtime_config,
            adapter_admission,
        }
    }

    pub fn with_runtime_config(runtime_config: ProviderRuntimeConfig) -> Self {
        let adapter_admission =
            ProviderAdapterAdmission::from_runtime_config(&runtime_config, None);
        Self {
            runtime_config,
            adapter_admission,
        }
    }

    pub fn with_runtime_config_and_adapter_admission(
        runtime_config: ProviderRuntimeConfig,
        adapter_admission: ProviderAdapterAdmission,
    ) -> Self {
        Self {
            runtime_config,
            adapter_admission,
        }
    }

    pub fn record_request(&self, input: &InputEvent, turn_id: &str) -> ProviderDispatchRecord {
        self.dispatch_request(input, turn_id, &ProviderCancellationToken::new())
    }
}

impl Default for ProviderDispatchStub {
    fn default() -> Self {
        Self::disabled()
    }
}

impl ProviderAdapter for ProviderDispatchStub {
    fn set_session_model(&mut self, model: Option<String>) {
        self.runtime_config.model = model;
        self.adapter_admission =
            refresh_adapter_admission(&self.runtime_config, &self.adapter_admission);
    }

    fn set_session_thinking(&mut self, thinking: Option<String>) {
        self.runtime_config.thinking = thinking;
        self.adapter_admission =
            refresh_adapter_admission(&self.runtime_config, &self.adapter_admission);
    }

    fn dispatch_request(
        &self,
        input: &InputEvent,
        turn_id: &str,
        _cancellation: &ProviderCancellationToken,
    ) -> ProviderDispatchRecord {
        let status = ProviderDispatchStatus::RecordedNotDispatched;
        let admission = &self.adapter_admission;
        let request = ProviderAdapterRequest::from_input(input, turn_id, &self.runtime_config);
        ProviderDispatchRecord {
            status: status.clone(),
            provider_execution_enabled: admission.provider_execution_enabled,
            payload: request.dispatch_payload(&status, admission),
            outputs: Vec::new(),
        }
    }
}

impl ProviderAdapter for CodexSubscriptionProviderAdapter {
    fn set_session_model(&mut self, model: Option<String>) {
        self.runtime_config.model = model;
        self.adapter_admission =
            refresh_adapter_admission(&self.runtime_config, &self.adapter_admission);
    }

    fn set_session_thinking(&mut self, thinking: Option<String>) {
        self.runtime_config.thinking = thinking;
        self.adapter_admission =
            refresh_adapter_admission(&self.runtime_config, &self.adapter_admission);
    }

    fn dispatch_start_record(
        &self,
        input: &InputEvent,
        turn_id: &str,
    ) -> Option<ProviderDispatchRecord> {
        let status = ProviderDispatchStatus::Dispatched;
        let request = ProviderAdapterRequest::from_input(input, turn_id, &self.runtime_config);
        Some(ProviderDispatchRecord {
            status: status.clone(),
            provider_execution_enabled: self.adapter_admission.provider_execution_enabled,
            payload: request.dispatch_payload(&status, &self.adapter_admission),
            outputs: Vec::new(),
        })
    }

    fn dispatch_request(
        &self,
        input: &InputEvent,
        turn_id: &str,
        cancellation: &ProviderCancellationToken,
    ) -> ProviderDispatchRecord {
        let request = ProviderAdapterRequest::from_input(input, turn_id, &self.runtime_config);
        let mut sink = NoopProviderOutputSink;
        self.dispatch_codex_request(turn_id, &request, cancellation, &mut sink)
    }

    fn dispatch_request_streaming(
        &self,
        input: &InputEvent,
        turn_id: &str,
        cancellation: &ProviderCancellationToken,
        sink: &mut dyn ProviderOutputSink,
    ) -> ProviderDispatchRecord {
        let request = ProviderAdapterRequest::from_input(input, turn_id, &self.runtime_config);
        self.dispatch_codex_request(turn_id, &request, cancellation, sink)
    }
}

impl CodexSubscriptionProviderAdapter {
    fn dispatch_codex_request(
        &self,
        turn_id: &str,
        request: &ProviderAdapterRequest,
        cancellation: &ProviderCancellationToken,
        sink: &mut dyn ProviderOutputSink,
    ) -> ProviderDispatchRecord {
        if let Some((tool_name, arguments_summary)) =
            direct_operator_intent_tool_call(&request.content_preview)
        {
            let status = ProviderDispatchStatus::Completed;
            return ProviderDispatchRecord {
                status: status.clone(),
                provider_execution_enabled: self.adapter_admission.provider_execution_enabled,
                payload: request.dispatch_payload(&status, &self.adapter_admission),
                outputs: vec![ProviderOutputRecord::tool_call_request(
                    turn_id,
                    &tool_name,
                    &arguments_summary,
                    1,
                )],
            };
        }
        match run_codex_subscription_request(request, cancellation, sink) {
            ProviderExecutionResult::Completed(outputs) => {
                let status = ProviderDispatchStatus::Completed;
                ProviderDispatchRecord {
                    status: status.clone(),
                    provider_execution_enabled: self.adapter_admission.provider_execution_enabled,
                    payload: request.dispatch_payload(&status, &self.adapter_admission),
                    outputs,
                }
            }
            ProviderExecutionResult::Interrupted(reason) => {
                let status = ProviderDispatchStatus::Interrupted;
                let mut payload = request.dispatch_payload(&status, &self.adapter_admission);
                payload["error_summary"] = json!(reason);
                ProviderDispatchRecord {
                    status: status.clone(),
                    provider_execution_enabled: self.adapter_admission.provider_execution_enabled,
                    payload,
                    outputs: Vec::new(),
                }
            }
            ProviderExecutionResult::Failed(error) => {
                let status = ProviderDispatchStatus::Failed;
                ProviderDispatchRecord {
                    status: status.clone(),
                    provider_execution_enabled: self.adapter_admission.provider_execution_enabled,
                    payload: request.dispatch_payload(&status, &self.adapter_admission),
                    outputs: vec![ProviderOutputRecord::text_delta(
                        turn_id,
                        &format!("provider dispatch failed: {error}"),
                        1,
                    )],
                }
            }
        }
    }
}

fn direct_operator_intent_tool_call(content: &str) -> Option<(String, String)> {
    let normalized = content
        .trim()
        .trim_end_matches('.')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    match normalized.as_str() {
        "run startup sequence" | "startup sequence" => Some((
            "agent_context_startup_sequence".to_string(),
            "{}".to_string(),
        )),
        _ => None,
    }
}

fn run_codex_subscription_request(
    request: &ProviderAdapterRequest,
    cancellation: &ProviderCancellationToken,
    sink: &mut dyn ProviderOutputSink,
) -> ProviderExecutionResult {
    let prompt = request.content_preview.clone();
    if prompt.trim().is_empty() {
        return ProviderExecutionResult::Failed("codex_subscription_prompt_missing".to_string());
    }
    let cwd = match env::var("NARADA_SITE_ROOT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| env::current_dir().ok())
    {
        Some(cwd) => cwd,
        None => {
            return ProviderExecutionResult::Failed(
                "codex_subscription_cwd_unavailable".to_string(),
            );
        }
    };
    let command = codex_command();
    let mut args = vec![
        "exec".to_string(),
        "--json".to_string(),
        "--dangerously-bypass-approvals-and-sandbox".to_string(),
        "-m".to_string(),
        request
            .model
            .clone()
            .unwrap_or_else(|| "gpt-5.5".to_string()),
        "-c".to_string(),
        "approval_policy=\"never\"".to_string(),
    ];
    if let Some(effort) = reasoning_effort(request.thinking.as_deref()) {
        args.push("-c".to_string());
        args.push(format!("model_reasoning_effort=\"{effort}\""));
    }
    args.push("-C".to_string());
    args.push(cwd.display().to_string());
    args.push("-".to_string());

    let mut child = match ProviderProcess::spawn(&command, &args, &cwd) {
        Ok(child) => child,
        Err(error) => {
            return ProviderExecutionResult::Failed(format!("codex_exec_spawn_failed:{error}"));
        }
    };
    let Some(mut stdin) = child.child_mut().stdin.take() else {
        child.terminate_tree();
        let _ = child.wait();
        return ProviderExecutionResult::Failed("codex_exec_stdin_unavailable".to_string());
    };
    if let Err(error) = stdin.write_all(prompt.as_bytes()) {
        child.terminate_tree();
        let _ = child.wait();
        return ProviderExecutionResult::Failed(format!("codex_exec_stdin_write_failed:{error}"));
    }
    drop(stdin);

    let (stdout_sender, stdout_receiver) = mpsc::channel();
    if let Some(child_stdout) = child.child_mut().stdout.take() {
        thread::spawn(move || {
            let reader = BufReader::new(child_stdout);
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                if stdout_sender.send(line).is_err() {
                    break;
                }
            }
        });
    }

    let mut content = String::new();
    let mut streamed_any = false;
    let mut sequence = 1;

    let status = loop {
        drain_codex_stdout_lines(
            &stdout_receiver,
            &request.turn_id,
            &mut content,
            &mut streamed_any,
            &mut sequence,
            sink,
        );
        if cancellation.is_cancelled() {
            child.terminate_tree();
            let _ = child.wait();
            return ProviderExecutionResult::Interrupted(format!(
                "provider_cancelled:{:?}",
                child.termination_kind()
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                drain_codex_stdout_lines(
                    &stdout_receiver,
                    &request.turn_id,
                    &mut content,
                    &mut streamed_any,
                    &mut sequence,
                    sink,
                );
                break status;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                child.terminate_tree();
                let _ = child.wait();
                return ProviderExecutionResult::Failed(format!("codex_exec_wait_failed:{error}"));
            }
        }
    };

    let mut stderr = String::new();
    if let Some(mut child_stderr) = child.child_mut().stderr.take() {
        let _ = child_stderr.read_to_string(&mut stderr);
    }
    if !status.success() {
        return ProviderExecutionResult::Failed(format!(
            "codex_exec_failed:{}:{}",
            status.code().unwrap_or(-1),
            stderr.trim().chars().take(500).collect::<String>()
        ));
    }
    if content.trim().is_empty() {
        return ProviderExecutionResult::Failed("codex_exec_empty_response".to_string());
    }
    if let Some((tool_name, arguments_summary)) = parse_narada_tool_call(&content) {
        return ProviderExecutionResult::Completed(vec![ProviderOutputRecord::tool_call_request(
            &request.turn_id,
            &tool_name,
            &arguments_summary,
            1,
        )]);
    }
    if streamed_any {
        return ProviderExecutionResult::Completed(Vec::new());
    }
    ProviderExecutionResult::Completed(vec![ProviderOutputRecord::text_delta(
        &request.turn_id,
        &content,
        1,
    )])
}

fn drain_codex_stdout_lines(
    receiver: &mpsc::Receiver<String>,
    turn_id: &str,
    content: &mut String,
    streamed_any: &mut bool,
    sequence: &mut u64,
    sink: &mut dyn ProviderOutputSink,
) {
    while let Ok(line) = receiver.try_recv() {
        process_codex_stdout_line(&line, turn_id, content, streamed_any, sequence, sink);
    }
}

fn process_codex_stdout_line(
    line: &str,
    turn_id: &str,
    content: &mut String,
    streamed_any: &mut bool,
    sequence: &mut u64,
    sink: &mut dyn ProviderOutputSink,
) {
    let Ok(event) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if let Some(delta) = codex_streaming_text_delta(&event) {
        content.push_str(&delta);
        let output = ProviderOutputRecord::text_delta(turn_id, &delta, *sequence);
        *sequence += 1;
        if sink.emit_provider_output(output).is_ok() {
            *streamed_any = true;
        }
        return;
    }
    if event.get("type").and_then(Value::as_str) != Some("item.completed") {
        return;
    }
    let Some(item) = event.get("item") else {
        return;
    };
    if item.get("type").and_then(Value::as_str) == Some("agent_message") {
        if let Some(text) = item.get("text").and_then(Value::as_str) {
            if !*streamed_any {
                content.push_str(text);
                if !is_potential_narada_tool_call_text(content) {
                    let output = ProviderOutputRecord::text_delta(turn_id, text, *sequence);
                    *sequence += 1;
                    if sink.emit_provider_output(output).is_ok() {
                        *streamed_any = true;
                    }
                }
            }
        }
    }
}

fn codex_streaming_text_delta(event: &Value) -> Option<String> {
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !(event_type.contains("delta") || event_type.contains("stream")) {
        return None;
    }
    for key in ["delta", "text_delta", "text"] {
        if let Some(value) = event.get(key).and_then(Value::as_str) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    let item = event.get("item")?;
    for key in ["delta", "text_delta", "text"] {
        if let Some(value) = item.get(key).and_then(Value::as_str) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn codex_command() -> String {
    if let Ok(value) = env::var("NARADA_AGENT_TUI_CODEX_COMMAND") {
        if !value.trim().is_empty() {
            return value;
        }
    }
    if cfg!(windows) {
        for name in ["codex.cmd", "codex.exe"] {
            if let Some(path) = find_on_path(name) {
                return path.display().to_string();
            }
        }
    }
    "codex".to_string()
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn reasoning_effort(thinking: Option<&str>) -> Option<&'static str> {
    match thinking.unwrap_or("medium") {
        "none" => None,
        "low" => Some("low"),
        "high" => Some("high"),
        _ => Some("medium"),
    }
}

fn parse_narada_tool_call(content: &str) -> Option<(String, String)> {
    let trimmed = content.trim();
    let without_fence_prefix = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .trim();
    let without_fence = without_fence_prefix
        .strip_suffix("```")
        .unwrap_or(without_fence_prefix)
        .trim();
    let start = without_fence.find('{').unwrap_or(0);
    let end = without_fence
        .rfind('}')
        .map(|index| index + 1)
        .unwrap_or(without_fence.len());
    let candidate = without_fence[start..end].trim();
    let parsed: Value = serde_json::from_str(candidate).ok()?;
    let call = parsed.get("narada_tool_call")?;
    let name = call.get("name")?.as_str()?.to_string();
    let arguments = call.get("arguments").cloned().unwrap_or_else(|| json!({}));
    Some((name, arguments.to_string()))
}

fn is_potential_narada_tool_call_text(content: &str) -> bool {
    let text = content.trim_start();
    if text.is_empty() {
        return false;
    }
    if text.starts_with("```") {
        return text.to_ascii_lowercase().starts_with("```json") || text.starts_with("```{");
    }
    if !text.starts_with('{') {
        return false;
    }
    let compact_prefix = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .take(48)
        .collect::<String>();
    "{\"narada_tool_call\"".starts_with(&compact_prefix)
        || compact_prefix.starts_with("{\"narada_tool_call\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{
        PROVIDER_OUTPUT_PAYLOAD_SCHEMA, PROVIDER_REQUEST_PAYLOAD_SCHEMA, parse_input_event,
    };
    use crate::provider_adapter_contract::provider_adapter_contract;
    use crate::test_env_lock::ENV_LOCK;
    use std::collections::BTreeMap;
    use std::fs::{create_dir_all, remove_dir_all, write};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    const INPUT_FIXTURE: &str = include_str!("../carrier-protocol/fixtures/input-event.json");
    fn set_test_env_var(key: &str, value: impl AsRef<std::ffi::OsStr>) {
        // Tests that mutate process environment hold ENV_LOCK, so no other test in
        // this module observes a partially-restored provider runtime environment.
        unsafe { env::set_var(key, value) };
    }

    fn remove_test_env_var(key: &str) {
        // Tests that mutate process environment hold ENV_LOCK, so removal is
        // serialized with restoration of the same keys.
        unsafe { env::remove_var(key) };
    }

    fn admitted_provider() -> &'static str {
        provider_adapter_contract()
            .admitted_providers
            .first()
            .expect("provider contract has at least one admitted provider")
            .as_str()
    }

    fn provider_process_input() -> InputEvent {
        let mut input = parse_input_event(INPUT_FIXTURE).expect("input parses");
        input.content = "answer with provider text".to_string();
        input
    }

    fn provider_runtime_env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        let contract = provider_adapter_contract();
        pairs
            .iter()
            .map(|(semantic_key, value)| {
                let env_key = match *semantic_key {
                    "execution_enabled" => &contract.provider_execution_env_var,
                    "provider" => &contract.intelligence_provider_env_var,
                    "model" => &contract.ai_model_env_var,
                    "thinking" => &contract.ai_thinking_env_var,
                    unexpected => panic!("unknown provider runtime env semantic key: {unexpected}"),
                };
                (env_key.clone(), value.to_string())
            })
            .collect()
    }

    #[derive(Clone)]
    struct RecordingOutputSink {
        outputs: Arc<Mutex<Vec<String>>>,
        emitted: Option<mpsc::Sender<()>>,
    }

    impl RecordingOutputSink {
        fn new() -> Self {
            Self {
                outputs: Arc::new(Mutex::new(Vec::new())),
                emitted: None,
            }
        }

        fn with_signal(emitted: mpsc::Sender<()>) -> Self {
            Self {
                outputs: Arc::new(Mutex::new(Vec::new())),
                emitted: Some(emitted),
            }
        }

        fn outputs(&self) -> Vec<String> {
            self.outputs.lock().expect("recording sink lock").clone()
        }
    }

    impl ProviderOutputSink for RecordingOutputSink {
        fn emit_provider_output(&mut self, output: ProviderOutputRecord) -> Result<(), String> {
            self.outputs.lock().expect("recording sink lock").push(
                output.payload["text_delta"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            );
            if let Some(emitted) = &self.emitted {
                let _ = emitted.send(());
            }
            Ok(())
        }
    }

    #[test]
    fn provider_dispatch_statuses_have_canonical_strings() {
        assert_eq!(
            ProviderDispatchStatus::RecordedNotDispatched.as_str(),
            "recorded_not_dispatched"
        );
        assert_eq!(ProviderDispatchStatus::Dispatched.as_str(), "dispatched");
        assert_eq!(ProviderDispatchStatus::Completed.as_str(), "completed");
        assert_eq!(ProviderDispatchStatus::Failed.as_str(), "failed");
        assert_eq!(ProviderDispatchStatus::Interrupted.as_str(), "interrupted");
    }

    #[test]
    fn large_provider_output_records_payload_ref() {
        let text = ProviderOutputRecord::text_delta("turn_1", &"x".repeat(5000), 1);
        assert_eq!(
            text.payload["text_delta"],
            "provider text delta omitted from transcript"
        );
        assert_eq!(
            text.payload["text_delta_ref"]["reader_tool"],
            "mcp_payload_show"
        );

        let tool = ProviderOutputRecord::sensitive_tool_call_request(
            "turn_1",
            "site_loop_run_once",
            "secret args",
            2,
        );
        assert_eq!(tool.payload["arguments_summary"], "secret args");
        assert!(tool.payload["arguments_ref"].is_null());
    }

    #[test]
    fn provider_output_records_map_to_session_event_kinds() {
        let text = ProviderOutputRecord::text_delta("turn_1", "hello", 1);
        assert_eq!(text.kind, ProviderOutputKind::TextDelta);
        assert_eq!(
            text.kind.session_event_kind(),
            SessionEventKind::ProviderTextDeltaRecorded
        );
        assert_eq!(text.payload["schema"], PROVIDER_OUTPUT_PAYLOAD_SCHEMA);
        assert_eq!(text.payload["provider_output_kind"], "text_delta");
        assert_eq!(text.payload["text_delta"], "hello");

        let tool = ProviderOutputRecord::tool_call_request("turn_1", "site_loop_run_once", "{}", 2);
        assert_eq!(tool.kind, ProviderOutputKind::ToolCallRequest);
        assert_eq!(
            tool.kind.session_event_kind(),
            SessionEventKind::ProviderToolCallRequested
        );
        assert_eq!(tool.payload["schema"], PROVIDER_OUTPUT_PAYLOAD_SCHEMA);
        assert_eq!(tool.payload["provider_output_kind"], "tool_call_request");
        assert_eq!(tool.payload["tool_name"], "site_loop_run_once");
    }

    #[test]
    fn parses_paged_output_reader_tool_call_envelope() {
        let (tool_name, arguments) = parse_narada_tool_call(
            r#"{"narada_tool_call":{"name":"mcp_output_show","arguments":{"output_ref":"mcp_output:o_6cd77433e384445e976c7fdf"}}}"#,
        )
        .expect("reader tool envelope parses");

        assert_eq!(tool_name, "mcp_output_show");
        assert_eq!(
            arguments,
            r#"{"output_ref":"mcp_output:o_6cd77433e384445e976c7fdf"}"#
        );
    }

    #[test]
    fn provider_adapter_request_has_stable_dispatch_payload_shape() {
        let input = parse_input_event(INPUT_FIXTURE).expect("input parses");
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
            ("thinking", "medium"),
        ]));
        let admission = ProviderAdapterAdmission::from_runtime_config(&runtime_config, None);
        let request = ProviderAdapterRequest::from_input(&input, "turn_1", &runtime_config);
        let payload =
            request.dispatch_payload(&ProviderDispatchStatus::RecordedNotDispatched, &admission);

        assert_eq!(request.turn_id, "turn_1");
        assert_eq!(request.input_event_id, input.event_id);
        assert_eq!(request.content_preview, input.content);
        assert_eq!(request.provider_runtime_status, "configured");
        assert_eq!(payload["schema"], PROVIDER_REQUEST_PAYLOAD_SCHEMA);
        assert_eq!(
            payload["provider_request_status"],
            "recorded_not_dispatched"
        );
        assert_eq!(payload["provider_runtime_status"], "configured");
        assert_eq!(payload["provider"], admitted_provider());
        assert_eq!(payload["model"], "gpt-5.5");
        assert_eq!(payload["thinking"], "medium");
        assert_eq!(
            payload["provider_adapter_admission_status"],
            "configured_without_adapter"
        );
        assert_eq!(payload["provider_execution_enabled"], false);
    }

    #[test]
    fn routes_startup_sequence_intent_directly_to_startup_tool() {
        assert_eq!(
            direct_operator_intent_tool_call("run startup sequence"),
            Some((
                "agent_context_startup_sequence".to_string(),
                "{}".to_string()
            ))
        );
        assert_eq!(
            direct_operator_intent_tool_call("  startup   sequence.  "),
            Some((
                "agent_context_startup_sequence".to_string(),
                "{}".to_string()
            ))
        );
        assert_eq!(direct_operator_intent_tool_call("check startup docs"), None);
    }

    #[test]
    fn stub_records_provider_request_without_dispatch() {
        let input = parse_input_event(INPUT_FIXTURE).expect("input parses");
        let dispatcher = ProviderDispatchStub::default();
        let adapter: &dyn ProviderAdapter = &dispatcher;
        let record = adapter.dispatch_request(&input, "turn_1", &ProviderCancellationToken::new());

        assert_eq!(record.status, ProviderDispatchStatus::RecordedNotDispatched);
        assert_eq!(record.provider_execution_enabled, false);
        assert_eq!(record.payload["turn_id"], "turn_1");
        assert_eq!(record.payload["input_event_id"], input.event_id);
        assert_eq!(
            record.payload["provider_request_status"],
            "recorded_not_dispatched"
        );
        assert_eq!(record.payload["provider_execution_enabled"], false);
        assert_eq!(record.payload["provider_runtime_status"], "disabled");
        assert_eq!(
            record.payload["provider_adapter_admission_status"],
            "disabled"
        );
        assert_eq!(record.payload["provider_adapter_kind"], Value::Null);
        assert_eq!(
            record.payload["provider_adapter_refusal_reason"],
            Value::Null
        );
        assert!(record.outputs.is_empty());
    }

    #[test]
    fn codex_adapter_routes_startup_sequence_without_provider_process() {
        let mut input = parse_input_event(INPUT_FIXTURE).expect("input parses");
        input.content = "run startup sequence".to_string();
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));
        let adapter = CodexSubscriptionProviderAdapter {
            runtime_config,
            adapter_admission: ProviderAdapterAdmission::from_runtime_config(
                &ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
                    ("execution_enabled", "true"),
                    ("provider", admitted_provider()),
                    ("model", "gpt-5.5"),
                ])),
                Some(ProviderAdapterKind::CodexSubscription.as_str()),
            ),
        };

        let record = adapter.dispatch_request(&input, "turn_1", &ProviderCancellationToken::new());

        assert_eq!(record.status, ProviderDispatchStatus::Completed);
        assert_eq!(record.outputs.len(), 1);
        assert_eq!(record.outputs[0].kind, ProviderOutputKind::ToolCallRequest);
        assert_eq!(
            record.outputs[0].payload["tool_name"],
            "agent_context_startup_sequence"
        );
        assert_eq!(record.outputs[0].payload["arguments_summary"], "{}");
    }

    #[test]
    fn stub_records_configured_provider_runtime_refusal_without_dispatch() {
        let input = parse_input_event(INPUT_FIXTURE).expect("input parses");
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));
        let dispatcher = ProviderDispatchStub::with_runtime_config(runtime_config);
        let record =
            dispatcher.dispatch_request(&input, "turn_1", &ProviderCancellationToken::new());

        assert_eq!(record.status, ProviderDispatchStatus::RecordedNotDispatched);
        assert_eq!(record.provider_execution_enabled, false);
        assert_eq!(record.payload["provider_runtime_status"], "configured");
        assert_eq!(
            record.payload["provider_adapter_admission_status"],
            "configured_without_adapter"
        );
        assert_eq!(record.payload["provider_adapter_kind"], Value::Null);
        assert_eq!(record.payload["provider"], admitted_provider());
        assert_eq!(record.payload["model"], "gpt-5.5");
        assert_eq!(
            record.payload["provider_adapter_refusal_reason"],
            "provider_adapter_not_admitted"
        );
    }

    #[test]
    fn provider_adapter_factory_dispatches_admitted_production_adapter() {
        let _guard = ENV_LOCK.lock().expect("provider env lock");
        let input = provider_process_input();
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));
        let adapter_admission = ProviderAdapterAdmission::from_runtime_config(
            &runtime_config,
            Some(
                provider_adapter_contract()
                    .production_provider_adapter_kind
                    .as_str(),
            ),
        );
        let adapter = provider_adapter_from_runtime_config(runtime_config, adapter_admission);
        let previous_codex_command = env::var("NARADA_AGENT_TUI_CODEX_COMMAND").ok();
        set_test_env_var(
            "NARADA_AGENT_TUI_CODEX_COMMAND",
            "definitely-missing-codex-fixture",
        );
        let record = adapter.dispatch_request(&input, "turn_1", &ProviderCancellationToken::new());
        if let Some(previous) = previous_codex_command {
            set_test_env_var("NARADA_AGENT_TUI_CODEX_COMMAND", previous);
        } else {
            remove_test_env_var("NARADA_AGENT_TUI_CODEX_COMMAND");
        }

        assert_eq!(record.status, ProviderDispatchStatus::Failed);
        assert!(record.provider_execution_enabled);
        assert_eq!(
            record.payload["provider_adapter_admission_status"],
            "admitted"
        );
        assert_eq!(
            record.payload["provider_adapter_kind"],
            provider_adapter_contract().production_provider_adapter_kind
        );
        assert_eq!(
            record.payload["provider_adapter_refusal_reason"],
            Value::Null
        );
        assert_eq!(record.outputs.len(), 1);
        assert!(
            record.outputs[0].payload["text_delta"]
                .as_str()
                .unwrap_or_default()
                .contains("provider dispatch failed:")
        );
    }

    #[cfg(windows)]
    #[test]
    fn codex_subscription_adapter_interrupts_spawned_provider_process() {
        let _guard = ENV_LOCK.lock().expect("provider env lock");
        let input = provider_process_input();
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));
        let adapter_admission = ProviderAdapterAdmission::from_runtime_config(
            &runtime_config,
            Some(
                provider_adapter_contract()
                    .production_provider_adapter_kind
                    .as_str(),
            ),
        );
        let adapter = provider_adapter_from_runtime_config(runtime_config, adapter_admission);
        let fixture_dir = env::temp_dir().join(format!(
            "narada-agent-tui-codex-cancel-{}",
            std::process::id()
        ));
        create_dir_all(&fixture_dir).expect("fixture dir created");
        let command_path = fixture_dir.join("codex.cmd");
        write(&command_path, "@echo off\r\nping -n 60 127.0.0.1 >nul\r\n")
            .expect("fixture command written");
        let previous_codex_command = env::var("NARADA_AGENT_TUI_CODEX_COMMAND").ok();
        let previous_site_root = env::var("NARADA_SITE_ROOT").ok();
        set_test_env_var("NARADA_AGENT_TUI_CODEX_COMMAND", &command_path);
        set_test_env_var("NARADA_SITE_ROOT", &fixture_dir);

        let cancellation = ProviderCancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let handle =
            thread::spawn(move || adapter.dispatch_request(&input, "turn_1", &worker_cancellation));
        thread::sleep(Duration::from_millis(100));
        cancellation.cancel();
        let record = handle.join().expect("provider worker joins");

        if let Some(previous) = previous_codex_command {
            set_test_env_var("NARADA_AGENT_TUI_CODEX_COMMAND", previous);
        } else {
            remove_test_env_var("NARADA_AGENT_TUI_CODEX_COMMAND");
        }
        if let Some(previous) = previous_site_root {
            set_test_env_var("NARADA_SITE_ROOT", previous);
        } else {
            remove_test_env_var("NARADA_SITE_ROOT");
        }
        remove_dir_all(fixture_dir).ok();

        assert_eq!(record.status, ProviderDispatchStatus::Interrupted);
        assert!(record.outputs.is_empty());
        assert!(
            record.payload["error_summary"]
                .as_str()
                .unwrap_or_default()
                .contains("provider_cancelled")
        );
    }

    #[cfg(windows)]
    #[test]
    fn codex_subscription_adapter_streams_json_line_text_deltas_to_sink() {
        let _guard = ENV_LOCK.lock().expect("provider env lock");
        let input = provider_process_input();
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));
        let adapter_admission = ProviderAdapterAdmission::from_runtime_config(
            &runtime_config,
            Some(
                provider_adapter_contract()
                    .production_provider_adapter_kind
                    .as_str(),
            ),
        );
        let adapter = provider_adapter_from_runtime_config(runtime_config, adapter_admission);
        let fixture_dir = env::temp_dir().join(format!(
            "narada-agent-tui-codex-stream-{}",
            std::process::id()
        ));
        create_dir_all(&fixture_dir).expect("fixture dir created");
        let command_path = fixture_dir.join("codex.cmd");
        write(
            &command_path,
            "@echo off\r\necho {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\r\nping -n 2 127.0.0.1 >nul\r\necho {\"type\":\"response.output_text.delta\",\"delta\":\" world\"}\r\n",
        )
        .expect("fixture command written");
        let previous_codex_command = env::var("NARADA_AGENT_TUI_CODEX_COMMAND").ok();
        let previous_site_root = env::var("NARADA_SITE_ROOT").ok();
        set_test_env_var("NARADA_AGENT_TUI_CODEX_COMMAND", &command_path);
        set_test_env_var("NARADA_SITE_ROOT", &fixture_dir);

        let mut sink = RecordingOutputSink::new();
        let record = adapter.dispatch_request_streaming(
            &input,
            "turn_1",
            &ProviderCancellationToken::new(),
            &mut sink,
        );

        if let Some(previous) = previous_codex_command {
            set_test_env_var("NARADA_AGENT_TUI_CODEX_COMMAND", previous);
        } else {
            remove_test_env_var("NARADA_AGENT_TUI_CODEX_COMMAND");
        }
        if let Some(previous) = previous_site_root {
            set_test_env_var("NARADA_SITE_ROOT", previous);
        } else {
            remove_test_env_var("NARADA_SITE_ROOT");
        }
        remove_dir_all(fixture_dir).ok();

        assert_eq!(record.status, ProviderDispatchStatus::Completed);
        assert!(record.outputs.is_empty());
        assert_eq!(
            sink.outputs(),
            vec!["hello".to_string(), " world".to_string()]
        );
    }

    #[cfg(windows)]
    #[test]
    fn codex_subscription_adapter_streams_item_completed_before_process_exit() {
        let _guard = ENV_LOCK.lock().expect("provider env lock");
        let input = provider_process_input();
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));
        let adapter_admission = ProviderAdapterAdmission::from_runtime_config(
            &runtime_config,
            Some(
                provider_adapter_contract()
                    .production_provider_adapter_kind
                    .as_str(),
            ),
        );
        let adapter = provider_adapter_from_runtime_config(runtime_config, adapter_admission);
        let fixture_dir = env::temp_dir().join(format!(
            "narada-agent-tui-codex-item-completed-stream-{}",
            std::process::id()
        ));
        create_dir_all(&fixture_dir).expect("fixture dir created");
        let command_path = fixture_dir.join("codex.cmd");
        write(
            &command_path,
            "@echo off\r\necho {\"type\":\"item.completed\",\"item\":{\"type\":\"agent_message\",\"text\":\"hello\"}}\r\nping -n 3 127.0.0.1 >nul\r\necho {\"type\":\"turn.completed\"}\r\n",
        )
        .expect("fixture command written");
        let previous_codex_command = env::var("NARADA_AGENT_TUI_CODEX_COMMAND").ok();
        let previous_site_root = env::var("NARADA_SITE_ROOT").ok();
        set_test_env_var("NARADA_AGENT_TUI_CODEX_COMMAND", &command_path);
        set_test_env_var("NARADA_SITE_ROOT", &fixture_dir);

        let (emitted_sender, emitted_receiver) = mpsc::channel();
        let mut sink = RecordingOutputSink::with_signal(emitted_sender);
        let sink_snapshot = sink.clone();
        let handle = thread::spawn(move || {
            adapter.dispatch_request_streaming(
                &input,
                "turn_1",
                &ProviderCancellationToken::new(),
                &mut sink,
            )
        });
        emitted_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("item.completed streamed before provider process exit");
        assert_eq!(sink_snapshot.outputs(), vec!["hello".to_string()]);
        let record = handle.join().expect("provider worker joins");

        if let Some(previous) = previous_codex_command {
            set_test_env_var("NARADA_AGENT_TUI_CODEX_COMMAND", previous);
        } else {
            remove_test_env_var("NARADA_AGENT_TUI_CODEX_COMMAND");
        }
        if let Some(previous) = previous_site_root {
            set_test_env_var("NARADA_SITE_ROOT", previous);
        } else {
            remove_test_env_var("NARADA_SITE_ROOT");
        }
        remove_dir_all(fixture_dir).ok();

        assert_eq!(record.status, ProviderDispatchStatus::Completed);
        assert!(record.outputs.is_empty());
    }

    #[test]
    fn scripted_adapter_records_admitted_completed_dispatch_with_outputs() {
        let input = parse_input_event(INPUT_FIXTURE).expect("input parses");
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));
        let dispatcher = ScriptedProviderAdapter::try_new(
            runtime_config,
            ProviderAdapterKind::Scripted,
            vec![ProviderOutputRecord::text_delta("turn_1", "hello", 1)],
        )
        .expect("scripted adapter admits configured runtime");
        let record =
            dispatcher.dispatch_request(&input, "turn_1", &ProviderCancellationToken::new());

        assert_eq!(record.status, ProviderDispatchStatus::Completed);
        assert!(record.provider_execution_enabled);
        assert_eq!(record.payload["provider_request_status"], "completed");
        assert_eq!(record.payload["provider_execution_enabled"], true);
        assert_eq!(
            record.payload["provider_adapter_admission_status"],
            "admitted"
        );
        assert_eq!(
            record.payload["provider_adapter_kind"],
            provider_adapter_contract().scripted_provider_adapter_kind
        );
        assert_eq!(
            record.payload["provider_adapter_refusal_reason"],
            Value::Null
        );
        assert_eq!(record.outputs.len(), 1);
        assert_eq!(record.outputs[0].payload["text_delta"], "hello");
    }
}
