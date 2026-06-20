use crate::carrier_protocol::{
    InputEvent, SessionEventKind, create_provider_request_payload,
    create_provider_text_delta_payload, create_provider_tool_call_payload,
};
use crate::input_queue::SessionEvidenceContext;
use crate::mcp_fabric_transport::McpFabricTransportClient;
use crate::mcp_runtime_config::{McpRuntimeAdmissionStatus, McpRuntimeConfig};
use crate::operator_routing_contract::{DirectToolRoute, ReaderRoute, operator_routing_contract};
use crate::provider_adapter_admission::{ProviderAdapterAdmission, ProviderAdapterKind};
use crate::provider_process_tree::ProviderProcess;
use crate::provider_runtime_config::ProviderRuntimeConfig;
use crate::rendering_boundary::{
    InlinePayloadDecision, decide_payload_inline, default_payload_policy,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
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
    pub goal: Option<String>,
    pub goal_status: String,
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

    fn set_session_goal(&mut self, _goal: Option<String>, _status: String) {}

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
    codex_mcp_isolation: Option<CodexMcpIsolation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexMcpIsolation {
    pub codex_home: PathBuf,
    pub codex_config_dir: PathBuf,
    pub config_toml: String,
    pub auth_source_home: Option<PathBuf>,
}

const CODEX_AUTH_FILE_NAMES: &[&str] = &[
    "auth.json",
    "credentials.json",
    "credential.json",
    "token.json",
    "tokens.json",
    "session.json",
    "sessions.json",
];

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
            goal: runtime_config.goal.clone(),
            goal_status: runtime_config.goal_status.clone(),
            stream: runtime_config.stream,
        }
    }

    pub fn dispatch_payload(
        &self,
        status: &ProviderDispatchStatus,
        adapter_admission: &ProviderAdapterAdmission,
    ) -> Value {
        let mut payload = create_provider_request_payload(
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
        );
        payload["goal"] = json!(self.goal);
        payload["goal_status"] = json!(self.goal_status);
        payload
    }
}

fn set_runtime_session_goal(
    runtime_config: &mut ProviderRuntimeConfig,
    goal: Option<String>,
    status: String,
) {
    runtime_config.goal = goal.filter(|value| !value.trim().is_empty());
    runtime_config.goal_status = if runtime_config.goal.is_some() {
        match status.as_str() {
            "paused" => "paused".to_string(),
            "active" => "active".to_string(),
            _ => "active".to_string(),
        }
    } else {
        "unset".to_string()
    };
}

fn provider_streaming_contract(provider_execution_enabled: bool, stream: bool) -> &'static str {
    match (provider_execution_enabled, stream) {
        (true, true) => "streaming_text_delta_events",
        (true, false) => "single_provider_output_batch",
        (false, true) => "requested_but_not_dispatched",
        (false, false) => "not_requested",
    }
}

impl CodexMcpIsolation {
    pub fn from_mcp_runtime_config(
        config: &McpRuntimeConfig,
        context: &SessionEvidenceContext,
    ) -> Result<Option<Self>, String> {
        match config.status {
            McpRuntimeAdmissionStatus::Disabled => Ok(None),
            McpRuntimeAdmissionStatus::Refused => Ok(None),
            McpRuntimeAdmissionStatus::Configured => {
                let Some(config_path) = config.config_path.as_deref() else {
                    return Err("codex_mcp_isolation_missing_mcp_config".to_string());
                };
                let client = McpFabricTransportClient::from_path(config_path)?;
                let codex_home = codex_home_for_context(context);
                Ok(Some(Self::from_client(codex_home, &client)))
            }
        }
    }

    pub fn from_client(codex_home: impl Into<PathBuf>, client: &McpFabricTransportClient) -> Self {
        Self::from_client_with_auth_source(codex_home, client, default_codex_home())
    }

    pub fn from_client_with_auth_source(
        codex_home: impl Into<PathBuf>,
        client: &McpFabricTransportClient,
        auth_source_home: Option<PathBuf>,
    ) -> Self {
        let codex_home = codex_home.into();
        Self {
            codex_config_dir: codex_home.clone(),
            codex_home,
            config_toml: codex_config_toml(client),
            auth_source_home,
        }
    }

    pub fn materialize(&self) -> Result<(), String> {
        fs::create_dir_all(&self.codex_home).map_err(|error| {
            format!(
                "codex_mcp_isolation_home_create_failed:{}:{error}",
                self.codex_home.display()
            )
        })?;
        project_codex_auth_files(self.auth_source_home.as_deref(), &self.codex_home)?;
        fs::write(self.config_path(), &self.config_toml).map_err(|error| {
            format!(
                "codex_mcp_isolation_config_write_failed:{}:{error}",
                self.config_path().display()
            )
        })
    }

    pub fn env_overrides(&self) -> Result<BTreeMap<String, String>, String> {
        self.materialize()?;
        let codex_home = self.codex_home.display().to_string();
        Ok(BTreeMap::from([
            ("CODEX_HOME".to_string(), codex_home.clone()),
            ("CODEX_CONFIG_DIR".to_string(), codex_home),
        ]))
    }

    pub fn config_path(&self) -> PathBuf {
        self.codex_home.join("config.toml")
    }
}

fn codex_home_for_context(context: &SessionEvidenceContext) -> PathBuf {
    PathBuf::from(&context.site_root)
        .join(".narada")
        .join("crew")
        .join("nars-sessions")
        .join(&context.carrier_session_id)
        .join("codex-home")
}

fn default_codex_home() -> Option<PathBuf> {
    if let Some(value) = env::var_os("CODEX_HOME") {
        return Some(PathBuf::from(value));
    }
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(|user_root| PathBuf::from(user_root).join(".codex"))
}

fn project_codex_auth_files(source_home: Option<&Path>, target_home: &Path) -> Result<(), String> {
    let Some(source_home) = source_home else {
        return Ok(());
    };
    if source_home == target_home || !source_home.exists() {
        return Ok(());
    }
    for file_name in CODEX_AUTH_FILE_NAMES {
        let source_path = source_home.join(file_name);
        if !source_path.exists() {
            continue;
        }
        let metadata = match fs::metadata(&source_path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if !metadata.is_file() {
            continue;
        }
        let target_path = target_home.join(file_name);
        fs::copy(&source_path, &target_path).map_err(|error| {
            format!(
                "codex_mcp_isolation_auth_copy_failed:{}:{}:{error}",
                source_path.display(),
                target_path.display()
            )
        })?;
    }
    Ok(())
}

fn codex_config_toml(client: &McpFabricTransportClient) -> String {
    let mut lines = vec![
        "# Generated by narada-agent-tui for nested Codex subprocesses.".to_string(),
        "# Mirrors the target Site MCP fabric; does not import User Site MCP servers.".to_string(),
        String::new(),
    ];
    for (name, server) in &client.servers {
        lines.push(format!("[mcp_servers.\"{}\"]", toml_key(name)));
        lines.push(format!("command = {}", toml_string(&server.command)));
        lines.push(format!(
            "args = {}",
            serde_json::to_string(
                &server
                    .args
                    .iter()
                    .map(|arg| normalize_codex_path(arg))
                    .collect::<Vec<_>>()
            )
            .expect("MCP server args serialize")
        ));
        lines.push("default_tools_approval_mode = \"approve\"".to_string());
        lines.push(String::new());
    }
    lines.join("\n")
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(&normalize_codex_path(value)).expect("TOML string JSON escapes")
}

fn toml_key(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn normalize_codex_path(value: &str) -> String {
    value.replace('\\', "/")
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

    fn set_session_goal(&mut self, goal: Option<String>, status: String) {
        set_runtime_session_goal(&mut self.runtime_config, goal, status);
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
    provider_adapter_from_runtime_config_with_codex_mcp_isolation(
        runtime_config,
        adapter_admission,
        None,
    )
}

pub fn provider_adapter_from_runtime_config_with_codex_mcp_isolation(
    runtime_config: ProviderRuntimeConfig,
    adapter_admission: ProviderAdapterAdmission,
    codex_mcp_isolation: Option<CodexMcpIsolation>,
) -> Box<dyn ProviderAdapter> {
    if adapter_admission.provider_execution_enabled
        && adapter_admission.adapter_kind.as_deref()
            == Some(ProviderAdapterKind::CodexSubscription.as_str())
    {
        return Box::new(CodexSubscriptionProviderAdapter {
            runtime_config,
            adapter_admission,
            codex_mcp_isolation,
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

    fn set_session_goal(&mut self, goal: Option<String>, status: String) {
        set_runtime_session_goal(&mut self.runtime_config, goal, status);
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

    fn set_session_goal(&mut self, goal: Option<String>, status: String) {
        set_runtime_session_goal(&mut self.runtime_config, goal, status);
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
        match run_codex_subscription_request(
            request,
            cancellation,
            sink,
            self.codex_mcp_isolation.as_ref(),
        ) {
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
    if let Some(call) = parse_narada_tool_call(content) {
        return Some(call);
    }
    if let Some((route, output_ref)) = direct_mcp_output_reader_ref(content) {
        let mut arguments = route.arguments.clone();
        if let Some(arguments) = arguments.as_object_mut() {
            arguments.insert("ref".to_string(), json!(output_ref));
        } else {
            return None;
        }
        return Some((route.tool_name.clone(), canonical_json_string(&arguments)?));
    }
    let normalized = normalized_operator_phrase(content);
    operator_routing_contract()
        .direct_tool_routes
        .iter()
        .find(|route| direct_route_matches(route, &normalized))
        .and_then(|route| {
            Some((
                route.tool_name.clone(),
                canonical_json_string(&route.arguments)?,
            ))
        })
}

fn direct_mcp_output_reader_ref(content: &str) -> Option<(&'static ReaderRoute, String)> {
    let lower = content.to_ascii_lowercase();
    operator_routing_contract()
        .reader_routes
        .iter()
        .find(|route| {
            route
                .phrases
                .iter()
                .any(|phrase| lower.contains(&phrase.to_ascii_lowercase()))
        })
        .and_then(|route| {
            extract_ref_with_prefix(content, &route.ref_prefix).map(|value| (route, value))
        })
}

fn direct_route_matches(route: &DirectToolRoute, normalized: &str) -> bool {
    route
        .phrases
        .iter()
        .any(|phrase| normalized_operator_phrase(phrase) == normalized)
}

fn normalized_operator_phrase(content: &str) -> String {
    content
        .trim()
        .trim_end_matches('.')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn extract_ref_with_prefix(content: &str, prefix: &str) -> Option<String> {
    let start = content.find(prefix)?;
    let output_id = content[start + prefix.len()..]
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .collect::<String>();
    if !output_id.is_empty() {
        Some(format!("{prefix}{output_id}"))
    } else {
        None
    }
}

fn canonical_json_string(value: &Value) -> Option<String> {
    serde_json::to_string(value).ok()
}

fn run_codex_subscription_request(
    request: &ProviderAdapterRequest,
    cancellation: &ProviderCancellationToken,
    sink: &mut dyn ProviderOutputSink,
    codex_mcp_isolation: Option<&CodexMcpIsolation>,
) -> ProviderExecutionResult {
    let prompt = prompt_with_carrier_goal(request);
    if prompt.trim().is_empty() {
        return ProviderExecutionResult::Failed("codex_subscription_prompt_missing".to_string());
    }
    let Some(cwd) = env::var("NARADA_SITE_ROOT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
    else {
        return ProviderExecutionResult::Failed("codex_subscription_site_root_missing".to_string());
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

    let provider_env = match codex_mcp_isolation {
        Some(isolation) => match isolation.env_overrides() {
            Ok(env) => env,
            Err(error) => return ProviderExecutionResult::Failed(error),
        },
        None => BTreeMap::new(),
    };

    let mut child = match ProviderProcess::spawn_with_env(&command, &args, &cwd, &provider_env) {
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

fn prompt_with_carrier_goal(request: &ProviderAdapterRequest) -> String {
    let Some(goal) = request
        .goal
        .as_deref()
        .map(str::trim)
        .filter(|goal| !goal.is_empty())
    else {
        return request.content_preview.clone();
    };
    if request.goal_status != "active" {
        return request.content_preview.clone();
    }
    format!(
        "Active carrier session goal: {goal}\nUse this as the persistent task target and completion criterion while it remains active.\n\n{}",
        request.content_preview
    )
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
    let envelope = &operator_routing_contract().tool_call_envelope;
    let trimmed = content.trim();
    let without_fence = if envelope.fenced_json_admitted {
        let without_fence_prefix = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed)
            .trim();
        without_fence_prefix
            .strip_suffix("```")
            .unwrap_or(without_fence_prefix)
            .trim()
    } else {
        trimmed
    };
    let start = without_fence.find('{').unwrap_or(0);
    let end = without_fence
        .rfind('}')
        .map(|index| index + 1)
        .unwrap_or(without_fence.len());
    let candidate = without_fence[start..end].trim();
    let parsed: Value = serde_json::from_str(candidate).ok()?;
    let call = parsed.get(&envelope.field)?;
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
        return operator_routing_contract()
            .tool_call_envelope
            .fenced_json_admitted
            && (text.to_ascii_lowercase().starts_with("```json") || text.starts_with("```{"));
    }
    if !text.starts_with('{') {
        return false;
    }
    let compact_prefix = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .take(48)
        .collect::<String>();
    let envelope_prefix = format!(
        "{{\"{}\"",
        operator_routing_contract().tool_call_envelope.field
    );
    envelope_prefix.starts_with(&compact_prefix) || compact_prefix.starts_with(&envelope_prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{
        parse_input_event, provider_output_payload_schema, provider_request_payload_schema,
    };
    use crate::provider_adapter_contract::provider_adapter_contract;
    use crate::test_env_lock::ENV_LOCK;
    use std::collections::BTreeMap;
    use std::fs::{create_dir_all, read_to_string, remove_dir_all, write};
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    const INPUT_FIXTURE: &str =
        include_str!("../../narada/packages/carrier-protocol/fixtures/input-event.json");
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

    fn startup_route() -> &'static DirectToolRoute {
        operator_routing_contract()
            .direct_tool_routes
            .iter()
            .find(|route| route.id == "startup_sequence")
            .expect("startup route is present")
    }

    fn output_reader_route() -> &'static ReaderRoute {
        operator_routing_contract()
            .reader_routes
            .iter()
            .find(|route| route.id == "mcp_output_reader")
            .expect("output reader route is present")
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

    fn site_mcp_fabric_config_json() -> &'static str {
        r#"{
  "site_id": "narada-sonar",
  "carrier": "agent-tui",
  "mcpServers": {
    "narada-sonar-agent-context": {
      "transport": "stdio",
      "command": "node",
      "args": [
        "D:/code/mcp-surfaces/packages/agent-context-mcp/dist/src/main.js",
        "--site-root",
        "D:/code/narada.sonar"
      ],
      "tools": ["agent_context_startup_sequence"]
    },
    "narada-sonar-scheduler": {
      "transport": "stdio",
      "command": "node",
      "args": [
        "D:/code/mcp-surfaces/packages/scheduler-mcp/dist/src/main.js"
      ],
      "tools": ["scheduler_list"]
    }
  }
}"#
    }

    #[test]
    fn codex_mcp_isolation_config_contains_only_site_fabric_servers() {
        let client = McpFabricTransportClient::from_json_str(
            "D:/code/narada.sonar/.ai/mcp/agent-tui.json",
            site_mcp_fabric_config_json(),
        )
        .expect("site MCP fabric parses");
        let isolation =
            CodexMcpIsolation::from_client("D:/tmp/narada-agent-tui-codex-config-fixture", &client);

        assert!(
            isolation
                .config_toml
                .contains("[mcp_servers.\"narada-sonar-agent-context\"]")
        );
        assert!(
            isolation
                .config_toml
                .contains("[mcp_servers.\"narada-sonar-scheduler\"]")
        );
        assert!(isolation.config_toml.contains("D:/code/narada.sonar"));
        assert!(
            isolation
                .config_toml
                .contains("default_tools_approval_mode = \"approve\"")
        );
        assert!(!isolation.config_toml.contains("narada-andrey"));
        assert!(!isolation.config_toml.contains("C:/Users/Andrey/Narada"));
    }

    #[test]
    fn codex_mcp_isolation_materializes_session_scoped_config_dir_env() {
        let fixture_dir = env::temp_dir().join(format!(
            "narada-agent-tui-codex-isolation-{}",
            std::process::id()
        ));
        remove_dir_all(&fixture_dir).ok();
        let fabric_dir = fixture_dir.join(".narada").join("mcp");
        create_dir_all(&fabric_dir).expect("fabric dir created");
        let config_path = fabric_dir.join("agent-tui.json");
        write(&config_path, site_mcp_fabric_config_json()).expect("fabric config written");
        let context = SessionEvidenceContext {
            carrier_session_id: "carrier_fixture".to_string(),
            agent_id: "sonar.resident".to_string(),
            site_id: "sonar".to_string(),
            site_root: fixture_dir.display().to_string(),
        };
        let mcp_config = McpRuntimeConfig {
            status: McpRuntimeAdmissionStatus::Configured,
            mcp_fabric_access_enabled: true,
            config_path_policy: "site_mcp_fabric_child",
            config_path: Some(config_path.display().to_string()),
            site_mcp_fabric: Some(fabric_dir.display().to_string()),
            refusal_reason: None,
        };
        let mut isolation = CodexMcpIsolation::from_mcp_runtime_config(&mcp_config, &context)
            .expect("isolation admitted")
            .expect("isolation configured");
        isolation.auth_source_home = None;

        assert!(
            isolation.codex_home.ends_with(
                Path::new(".narada")
                    .join("crew")
                    .join("nars-sessions")
                    .join("carrier_fixture")
                    .join("codex-home")
            )
        );
        assert!(
            isolation.codex_config_dir.ends_with(
                Path::new(".narada")
                    .join("crew")
                    .join("nars-sessions")
                    .join("carrier_fixture")
                    .join("codex-home")
            )
        );
        let env = isolation
            .env_overrides()
            .expect("env overrides materialize");
        let codex_home = isolation.codex_home.display().to_string();
        assert_eq!(env.get("CODEX_HOME"), Some(&codex_home));
        assert_eq!(env.get("CODEX_CONFIG_DIR"), Some(&codex_home));
        let materialized_config =
            read_to_string(isolation.config_path()).expect("config materialized");
        assert!(materialized_config.contains("narada-sonar-agent-context"));
        assert!(!materialized_config.contains("narada-andrey"));

        remove_dir_all(fixture_dir).ok();
    }

    #[test]
    fn codex_mcp_isolation_projects_auth_but_not_ambient_config() {
        let fixture_dir = env::temp_dir().join(format!(
            "narada-agent-tui-codex-auth-isolation-{}",
            std::process::id()
        ));
        remove_dir_all(&fixture_dir).ok();
        let ambient_home = fixture_dir.join("ambient-codex-home");
        let isolated_home = fixture_dir.join("isolated-codex-home");
        create_dir_all(&ambient_home).expect("ambient codex home created");
        write(
            ambient_home.join("auth.json"),
            "{\"access_token\":\"fixture\"}\n",
        )
        .expect("auth fixture written");
        write(
            ambient_home.join("config.toml"),
            "[mcp_servers.\"narada-andrey-agent-context\"]\ncommand = \"node\"\n",
        )
        .expect("ambient config fixture written");
        let client = McpFabricTransportClient::from_json_str(
            "D:/code/narada.sonar/.ai/mcp/agent-tui.json",
            site_mcp_fabric_config_json(),
        )
        .expect("site MCP fabric parses");
        let isolation = CodexMcpIsolation::from_client_with_auth_source(
            isolated_home.clone(),
            &client,
            Some(ambient_home.clone()),
        );
        let env = isolation
            .env_overrides()
            .expect("env overrides materialize");

        assert_eq!(
            env.get("CODEX_HOME"),
            Some(&isolated_home.display().to_string())
        );
        assert_eq!(env.get("CODEX_CONFIG_DIR"), env.get("CODEX_HOME"));
        assert_eq!(
            read_to_string(isolated_home.join("auth.json")).expect("auth projected"),
            "{\"access_token\":\"fixture\"}\n"
        );
        let materialized_config =
            read_to_string(isolated_home.join("config.toml")).expect("config materialized");
        assert!(materialized_config.contains("narada-sonar-agent-context"));
        assert!(!materialized_config.contains("narada-andrey-agent-context"));

        remove_dir_all(fixture_dir).ok();
    }

    #[test]
    fn codex_subscription_requires_explicit_site_root() {
        let _guard = ENV_LOCK.lock().expect("provider env lock");
        let previous_site_root = env::var("NARADA_SITE_ROOT").ok();
        remove_test_env_var("NARADA_SITE_ROOT");
        let request = ProviderAdapterRequest {
            turn_id: "turn_1".to_string(),
            input_event_id: "input_1".to_string(),
            content_preview: "answer with provider text".to_string(),
            provider_runtime_status: "admitted".to_string(),
            provider: Some(admitted_provider().to_string()),
            model: Some("gpt-5.5".to_string()),
            thinking: None,
            goal: None,
            goal_status: "unset".to_string(),
            stream: true,
        };
        let mut sink = NoopProviderOutputSink;

        let result = run_codex_subscription_request(
            &request,
            &ProviderCancellationToken::new(),
            &mut sink,
            None,
        );

        if let Some(previous) = previous_site_root {
            set_test_env_var("NARADA_SITE_ROOT", previous);
        }

        assert!(matches!(
            result,
            ProviderExecutionResult::Failed(error) if error == "codex_subscription_site_root_missing"
        ));
    }

    #[test]
    fn active_carrier_goal_is_added_to_codex_prompt() {
        let request = ProviderAdapterRequest {
            turn_id: "turn_1".to_string(),
            input_event_id: "input_1".to_string(),
            content_preview: "answer with provider text".to_string(),
            provider_runtime_status: "admitted".to_string(),
            provider: Some(admitted_provider().to_string()),
            model: Some("gpt-5.5".to_string()),
            thinking: None,
            goal: Some("finish parity".to_string()),
            goal_status: "active".to_string(),
            stream: true,
        };

        assert_eq!(
            prompt_with_carrier_goal(&request),
            "Active carrier session goal: finish parity\nUse this as the persistent task target and completion criterion while it remains active.\n\nanswer with provider text"
        );

        let mut paused = request.clone();
        paused.goal_status = "paused".to_string();
        assert_eq!(
            prompt_with_carrier_goal(&paused),
            "answer with provider text"
        );
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
        assert_eq!(text.payload["schema"], provider_output_payload_schema());
        assert_eq!(text.payload["provider_output_kind"], "text_delta");
        assert_eq!(text.payload["text_delta"], "hello");

        let tool = ProviderOutputRecord::tool_call_request("turn_1", "site_loop_run_once", "{}", 2);
        assert_eq!(tool.kind, ProviderOutputKind::ToolCallRequest);
        assert_eq!(
            tool.kind.session_event_kind(),
            SessionEventKind::ProviderToolCallRequested
        );
        assert_eq!(tool.payload["schema"], provider_output_payload_schema());
        assert_eq!(tool.payload["provider_output_kind"], "tool_call_request");
        assert_eq!(tool.payload["tool_name"], "site_loop_run_once");
    }

    #[test]
    fn parses_paged_output_reader_tool_call_envelope() {
        let envelope_fixture: Value = serde_json::from_str(include_str!(
            "../../narada/packages/carrier-provider-contract/contracts/narada-tool-call-envelope.json"
        ))
        .expect("shared narada tool-call envelope fixture parses");
        let envelope = envelope_fixture["example"].to_string();
        let (tool_name, arguments) =
            parse_narada_tool_call(&envelope).expect("reader tool envelope parses");

        assert_eq!(tool_name, output_reader_route().tool_name);
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
        assert_eq!(payload["schema"], provider_request_payload_schema());
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
        let route = startup_route();
        let first_phrase = route.phrases[0].as_str();
        let second_phrase = route.phrases[1].replace(' ', "   ");
        assert_eq!(
            direct_operator_intent_tool_call(first_phrase),
            Some((route.tool_name.clone(), route.arguments.to_string()))
        );
        assert_eq!(
            direct_operator_intent_tool_call(&format!("  {second_phrase}.  ")),
            Some((route.tool_name.clone(), route.arguments.to_string()))
        );
        assert_eq!(direct_operator_intent_tool_call("check startup docs"), None);
    }

    #[test]
    fn routes_operator_pasted_narada_tool_call_directly() {
        let route = output_reader_route();
        let output_ref = "mcp_output:o_98b8292361cf4937a6282193";
        let envelope = json!({
            operator_routing_contract().tool_call_envelope.field.as_str(): {
                "name": route.tool_name,
                "arguments": {
                    "ref": output_ref,
                    "output_limit": route.arguments["output_limit"],
                },
            }
        })
        .to_string();
        assert_eq!(
            direct_operator_intent_tool_call(&envelope),
            Some((
                route.tool_name.clone(),
                r#"{"output_limit":10000,"ref":"mcp_output:o_98b8292361cf4937a6282193"}"#
                    .to_string()
            ))
        );
    }

    #[test]
    fn routes_operator_reader_request_with_output_ref_directly() {
        let route = output_reader_route();
        let phrase = route
            .phrases
            .iter()
            .find(|phrase| phrase.contains("reader"))
            .expect("reader phrase is present");
        let output_ref = "mcp_output:o_98b8292361cf4937a6282193";
        assert_eq!(
            direct_operator_intent_tool_call(&format!("Call the {phrase} now for {output_ref}.")),
            Some((
                route.tool_name.clone(),
                r#"{"output_limit":10000,"ref":"mcp_output:o_98b8292361cf4937a6282193"}"#
                    .to_string()
            ))
        );
        assert_eq!(
            direct_operator_intent_tool_call(&format!("Discuss {output_ref} generally")),
            None
        );
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
        let route = startup_route();
        input.content = route.phrases[0].clone();
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
            codex_mcp_isolation: None,
        };

        let record = adapter.dispatch_request(&input, "turn_1", &ProviderCancellationToken::new());

        assert_eq!(record.status, ProviderDispatchStatus::Completed);
        assert_eq!(record.outputs.len(), 1);
        assert_eq!(record.outputs[0].kind, ProviderOutputKind::ToolCallRequest);
        assert_eq!(record.outputs[0].payload["tool_name"], route.tool_name);
        assert_eq!(
            record.outputs[0].payload["arguments_summary"],
            route.arguments.to_string()
        );
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
