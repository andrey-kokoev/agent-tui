use crate::app_view_model::{AppViewInput, AppViewModel, build_app_view};
use crate::carrier_protocol::{SessionEvent, SessionEventKind, session_event_schema};
use crate::composer_view_model::ComposerViewInput;
use crate::layout_model::{LayoutConfig, TerminalSize};
use crate::projection_state::TurnState;
use crate::status_view_model::{RuntimePostureState, StatusViewInput};
use crate::terminal_input_tick::{
    CrosstermTerminalInputReader, TerminalInputTickOutcome,
    run_textarea_composer_input_tick_with_wait,
};
use crate::terminal_lifecycle::TerminalSession;
use crate::textarea_composer::TextareaComposer;
use crate::transcript_store::TranscriptStore;
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const WEBSOCKET_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const MAX_WEBSOCKET_PAYLOAD: usize = 16 * 1024 * 1024;
const INPUT_IDLE_WAIT: Duration = Duration::from_millis(25);
const LAUNCH_BINDING_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(60);
const LAUNCH_BINDING_DISCOVERY_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchBindingResolution {
    pub event_endpoint: String,
    pub identity: Option<String>,
    pub session: Option<String>,
}

pub fn resolve_event_endpoint_from_launch_binding(
    binding_path: &str,
) -> Result<LaunchBindingResolution, String> {
    let binding_path = Path::new(binding_path);
    let started = Instant::now();
    loop {
        let binding = match read_launch_binding(binding_path) {
            Ok(binding) => binding,
            Err(_error) if !binding_path.exists() => {
                if started.elapsed() >= LAUNCH_BINDING_DISCOVERY_TIMEOUT {
                    return Err(format!(
                        "nars_attach_launch_binding_endpoint_unavailable:{}",
                        binding_path.display()
                    ));
                }
                std::thread::sleep(LAUNCH_BINDING_DISCOVERY_POLL);
                continue;
            }
            Err(error) => return Err(error),
        };
        if value_string(binding.get("status")).as_deref() == Some("failed") {
            return Err(format!(
                "nars_attach_launch_binding_failed:{}",
                value_string(binding.get("reason")).unwrap_or_else(|| "unknown".to_string())
            ));
        }
        let site_root = value_string(binding.get("site_root"));
        if let Some(site_root) = site_root.as_deref() {
            let candidate_ids = launch_binding_session_ids(&binding);
            if let Some(resolution) = discover_launch_binding_endpoint(
                Path::new(site_root),
                &candidate_ids,
                value_string(binding.get("launch_session_id")).as_deref(),
                value_string(binding.get("agent")).as_deref(),
            ) {
                return Ok(resolution);
            }
        }
        if started.elapsed() >= LAUNCH_BINDING_DISCOVERY_TIMEOUT {
            return Err(format!(
                "nars_attach_launch_binding_endpoint_unavailable:{}",
                binding_path.display()
            ));
        }
        std::thread::sleep(LAUNCH_BINDING_DISCOVERY_POLL);
    }
}

fn read_launch_binding(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("nars_attach_launch_binding_read_failed:{error}"))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("nars_attach_launch_binding_invalid_json:{error}"))?;
    if value.get("schema").and_then(Value::as_str)
        != Some("narada.operator_projection_launch_binding.v1")
    {
        return Err("nars_attach_launch_binding_schema_invalid".to_string());
    }
    Ok(value)
}

fn launch_binding_session_ids(binding: &Value) -> Vec<String> {
    [
        "nars_session_id",
        "runtime_session_id",
        "carrier_session_id",
        "session_ref",
    ]
    .into_iter()
    .filter_map(|field| {
        if field == "session_ref" {
            binding
                .get(field)
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str)
                .map(ToString::to_string)
        } else {
            value_string(binding.get(field))
        }
    })
    .chain(value_string(binding.get("launch_session_id")))
    .collect()
}

fn discover_launch_binding_endpoint(
    site_root: &Path,
    candidate_ids: &[String],
    launch_session_id: Option<&str>,
    expected_agent: Option<&str>,
) -> Option<LaunchBindingResolution> {
    let sessions_root = site_root.join(".narada").join("crew").join("nars-sessions");
    let mut record_paths = Vec::new();
    for session_id in candidate_ids {
        record_paths.push(
            sessions_root
                .join(session_id)
                .join("session-index-record.json"),
        );
    }
    if let Ok(entries) = fs::read_dir(&sessions_root) {
        for entry in entries.flatten() {
            let path = entry.path().join("session-index-record.json");
            if !record_paths.iter().any(|candidate| candidate == &path) {
                record_paths.push(path);
            }
        }
    }

    for record_path in record_paths {
        let Ok(text) = fs::read_to_string(&record_path) else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(event_endpoint) = value_string(record.get("event_endpoint")) else {
            continue;
        };
        let record_session_id = [
            "session_id",
            "runtime_session_id",
            "nars_session_id",
            "carrier_session_id",
        ]
        .into_iter()
        .find_map(|field| value_string(record.get(field)));
        let record_launch_session_id = value_string(record.get("launch_session_id"));
        let session_matches = record_session_id
            .as_deref()
            .is_some_and(|value| candidate_ids.iter().any(|candidate| candidate == value))
            || record_launch_session_id
                .as_deref()
                .zip(launch_session_id)
                .is_some_and(|(record_id, binding_id)| record_id == binding_id);
        if !session_matches {
            continue;
        }
        let record_agent = value_string(record.get("agent_id"));
        if expected_agent.is_some_and(|expected| record_agent.as_deref() != Some(expected)) {
            continue;
        }
        return Some(LaunchBindingResolution {
            event_endpoint,
            identity: record_agent.or_else(|| expected_agent.map(ToString::to_string)),
            session: record_session_id,
        });
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WebSocketEndpoint {
    host: String,
    port: u16,
    path: String,
}

fn parse_websocket_endpoint(endpoint: &str) -> Result<WebSocketEndpoint, String> {
    let remainder = endpoint
        .strip_prefix("ws://")
        .ok_or_else(|| "nars_attach_requires_ws_endpoint".to_string())?;
    if remainder.is_empty() {
        return Err("nars_attach_endpoint_empty".to_string());
    }
    let (authority, raw_path) = match remainder.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (remainder, "/events".to_string()),
    };
    let path = if raw_path.is_empty() {
        "/events"
    } else {
        raw_path.as_str()
    }
    .to_string();
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let (host, port_text) = rest
            .split_once(']')
            .ok_or_else(|| "nars_attach_endpoint_invalid_host".to_string())?;
        let port_text = port_text
            .strip_prefix(':')
            .ok_or_else(|| "nars_attach_endpoint_missing_port".to_string())?;
        (host.to_string(), port_text)
    } else {
        let (host, port_text) = authority
            .rsplit_once(':')
            .ok_or_else(|| "nars_attach_endpoint_missing_port".to_string())?;
        (host.to_string(), port_text)
    };
    if host.trim().is_empty() {
        return Err("nars_attach_endpoint_missing_host".to_string());
    }
    let port = port
        .parse::<u16>()
        .map_err(|_| "nars_attach_endpoint_invalid_port".to_string())?;
    if port == 0 {
        return Err("nars_attach_endpoint_invalid_port".to_string());
    }
    Ok(WebSocketEndpoint { host, port, path })
}

#[derive(Debug)]
struct WebSocket {
    stream: TcpStream,
    read_buffer: Vec<u8>,
}

impl WebSocket {
    fn connect(endpoint: &str) -> Result<Self, String> {
        let endpoint = parse_websocket_endpoint(endpoint)?;
        let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port))
            .map_err(|error| format!("nars_attach_connect_failed:{error}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| format!("nars_attach_socket_config_failed:{error}"))?;

        let key = websocket_client_key();
        let request = format!(
            "GET {} HTTP/1.1\r\nHost: {}:{}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {}\r\nSec-WebSocket-Version: 13\r\n\r\n",
            endpoint.path, endpoint.host, endpoint.port, key
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|error| format!("nars_attach_handshake_write_failed:{error}"))?;

        let mut response = Vec::new();
        let header_end = loop {
            let mut buffer = [0u8; 1024];
            let count = stream
                .read(&mut buffer)
                .map_err(|error| format!("nars_attach_handshake_read_failed:{error}"))?;
            if count == 0 {
                return Err("nars_attach_handshake_closed".to_string());
            }
            response.extend_from_slice(&buffer[..count]);
            if let Some(index) = find_bytes(&response, b"\r\n\r\n") {
                break index + 4;
            }
            if response.len() > 64 * 1024 {
                return Err("nars_attach_handshake_headers_too_large".to_string());
            }
        };
        let header = String::from_utf8_lossy(&response[..header_end]);
        if !header.lines().next().unwrap_or_default().contains(" 101 ") {
            return Err("nars_attach_handshake_not_switching_protocols".to_string());
        }
        let expected_accept = websocket_accept_value(&key);
        let actual_accept = header.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name.eq_ignore_ascii_case("sec-websocket-accept")).then_some(value.trim())
        });
        if actual_accept != Some(expected_accept.as_str()) {
            return Err("nars_attach_handshake_accept_mismatch".to_string());
        }

        stream
            .set_read_timeout(None)
            .map_err(|error| format!("nars_attach_socket_config_failed:{error}"))?;
        stream
            .set_nonblocking(true)
            .map_err(|error| format!("nars_attach_socket_nonblocking_failed:{error}"))?;
        Ok(Self {
            stream,
            read_buffer: response[header_end..].to_vec(),
        })
    }

    fn send_text(&mut self, text: &str) -> Result<(), String> {
        self.send_frame(0x1, text.as_bytes())
    }

    fn send_pong(&mut self, payload: &[u8]) -> Result<(), String> {
        self.send_frame(0xA, payload)
    }

    fn send_close(&mut self) -> Result<(), String> {
        self.send_frame(0x8, &[])
    }

    fn send_frame(&mut self, opcode: u8, payload: &[u8]) -> Result<(), String> {
        if payload.len() > MAX_WEBSOCKET_PAYLOAD {
            return Err("nars_attach_frame_too_large".to_string());
        }
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x80 | opcode);
        match payload.len() {
            0..=125 => frame.push(0x80 | payload.len() as u8),
            126..=65_535 => {
                frame.push(0x80 | 126);
                frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
            }
            length => {
                frame.push(0x80 | 127);
                frame.extend_from_slice(&(length as u64).to_be_bytes());
            }
        }
        let mask = websocket_mask_key();
        frame.extend_from_slice(&mask);
        frame.extend(
            payload
                .iter()
                .enumerate()
                .map(|(index, byte)| byte ^ mask[index % 4]),
        );
        self.stream
            .set_nonblocking(false)
            .map_err(|error| format!("nars_attach_socket_blocking_failed:{error}"))?;
        let write_result = self.stream.write_all(&frame);
        let nonblocking_result = self.stream.set_nonblocking(true);
        write_result.map_err(|error| format!("nars_attach_frame_write_failed:{error}"))?;
        nonblocking_result.map_err(|error| format!("nars_attach_socket_nonblocking_failed:{error}"))
    }

    fn receive_text(&mut self) -> Result<Option<String>, String> {
        self.read_available()?;
        loop {
            let Some((opcode, payload)) = self.next_frame()? else {
                return Ok(None);
            };
            match opcode {
                0x1 => {
                    let text = String::from_utf8(payload)
                        .map_err(|_| "nars_attach_text_frame_invalid_utf8".to_string())?;
                    return Ok(Some(text));
                }
                0x8 => {
                    let _ = self.send_close();
                    return Err("nars_attach_websocket_closed".to_string());
                }
                0x9 => self.send_pong(&payload)?,
                0xA => {}
                _ => {}
            }
        }
    }

    fn read_available(&mut self) -> Result<(), String> {
        loop {
            let mut buffer = [0u8; 8192];
            match self.stream.read(&mut buffer) {
                Ok(0) => return Err("nars_attach_websocket_eof".to_string()),
                Ok(count) => self.read_buffer.extend_from_slice(&buffer[..count]),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(format!("nars_attach_frame_read_failed:{error}")),
            }
        }
    }

    fn next_frame(&mut self) -> Result<Option<(u8, Vec<u8>)>, String> {
        if self.read_buffer.len() < 2 {
            return Ok(None);
        }
        let first = self.read_buffer[0];
        let second = self.read_buffer[1];
        let opcode = first & 0x0F;
        let masked = second & 0x80 != 0;
        let mut offset = 2usize;
        let payload_length = match (second & 0x7F) as usize {
            length @ 0..=125 => length,
            126 => {
                if self.read_buffer.len() < offset + 2 {
                    return Ok(None);
                }
                let length =
                    u16::from_be_bytes([self.read_buffer[offset], self.read_buffer[offset + 1]])
                        as usize;
                offset += 2;
                length
            }
            127 => {
                if self.read_buffer.len() < offset + 8 {
                    return Ok(None);
                }
                let length = u64::from_be_bytes([
                    self.read_buffer[offset],
                    self.read_buffer[offset + 1],
                    self.read_buffer[offset + 2],
                    self.read_buffer[offset + 3],
                    self.read_buffer[offset + 4],
                    self.read_buffer[offset + 5],
                    self.read_buffer[offset + 6],
                    self.read_buffer[offset + 7],
                ]);
                offset += 8;
                usize::try_from(length).map_err(|_| "nars_attach_frame_too_large".to_string())?
            }
            _ => unreachable!(),
        };
        if payload_length > MAX_WEBSOCKET_PAYLOAD {
            return Err("nars_attach_frame_too_large".to_string());
        }
        let mask = if masked {
            if self.read_buffer.len() < offset + 4 {
                return Ok(None);
            }
            let mask = [
                self.read_buffer[offset],
                self.read_buffer[offset + 1],
                self.read_buffer[offset + 2],
                self.read_buffer[offset + 3],
            ];
            offset += 4;
            Some(mask)
        } else {
            None
        };
        if self.read_buffer.len() < offset + payload_length {
            return Ok(None);
        }
        let mut payload = self.read_buffer[offset..offset + payload_length].to_vec();
        if let Some(mask) = mask {
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[index % 4];
            }
        }
        self.read_buffer.drain(..offset + payload_length);
        Ok(Some((opcode, payload)))
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn websocket_client_key() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut bytes = [0u8; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = ((now.rotate_left((index * 7) as u32) ^ u128::from(std::process::id()))
            >> ((index % 8) * 8)) as u8;
    }
    base64_encode(&bytes)
}

fn websocket_mask_key() -> [u8; 4] {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    [
        now as u8,
        (now >> 8) as u8,
        (now >> 32) as u8,
        (now >> 56) as u8,
    ]
}

fn websocket_accept_value(key: &str) -> String {
    let mut input = key.as_bytes().to_vec();
    input.extend_from_slice(WEBSOCKET_GUID.as_bytes());
    base64_encode(&sha1_digest(&input))
}

fn sha1_digest(input: &[u8]) -> [u8; 20] {
    let mut message = input.to_vec();
    let bit_length = (message.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());

    let mut h = [
        0x67452301u32,
        0xEFCDAB89,
        0x98BADCFE,
        0x10325476,
        0xC3D2E1F0,
    ];
    for chunk in message.chunks_exact(64) {
        let mut words = [0u32; 80];
        for index in 0..16 {
            let offset = index * 4;
            words[index] = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (index, word) in words.iter().enumerate() {
            let (function, constant) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(function)
                .wrapping_add(e)
                .wrapping_add(constant)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut output = [0u8; 20];
    for (index, word) in h.iter().enumerate() {
        output[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    output
}

fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        output.push(ALPHABET[(first >> 2) as usize] as char);
        output.push(ALPHABET[((first & 0x03) << 4 | second >> 4) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[((second & 0x0F) << 2 | third >> 6) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(third & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    output
}

#[derive(Debug)]
pub struct NarsProjectionClient {
    endpoint: String,
    socket: WebSocket,
    subscription_id: String,
    last_sequence: Option<u64>,
    oldest_sequence: Option<u64>,
    history_exhausted: bool,
    seen_event_ids: HashSet<String>,
    request_counter: u64,
}

#[derive(Debug, Default)]
struct HistoryReadResult {
    older: Vec<SessionEvent>,
    live: Vec<SessionEvent>,
}

impl NarsProjectionClient {
    pub fn connect(endpoint: &str) -> Result<Self, String> {
        let subscription_id = format!("agent-tui-{}", std::process::id());
        let mut client = Self {
            endpoint: endpoint.to_string(),
            socket: WebSocket::connect(endpoint)?,
            subscription_id,
            last_sequence: None,
            oldest_sequence: None,
            history_exhausted: false,
            seen_event_ids: HashSet::new(),
            request_counter: 0,
        };
        client.subscribe()?;
        Ok(client)
    }

    pub fn poll(&mut self) -> Result<Vec<SessionEvent>, String> {
        let mut events = Vec::new();
        loop {
            let message = match self.socket.receive_text() {
                Ok(message) => message,
                Err(error) => {
                    self.reconnect()?;
                    return Err(format!("nars_attach_stream_reset:{error}"));
                }
            };
            let Some(message) = message else { break };
            let frame: Value = serde_json::from_str(&message)
                .map_err(|error| format!("nars_attach_invalid_server_json:{error}"))?;
            if frame.get("event").and_then(Value::as_str) != Some("session_event") {
                if frame.get("event").and_then(Value::as_str) == Some("websocket_error") {
                    let code = frame
                        .get("code")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    return Err(format!("nars_attach_server_error:{code}"));
                }
                continue;
            }
            if let Some(event) = self.normalize_live_frame(&frame) {
                events.push(event);
            }
        }
        Ok(events)
    }

    fn normalize_live_frame(&mut self, frame: &Value) -> Option<SessionEvent> {
        let sequence = frame
            .get("cursor")
            .and_then(|cursor| cursor.get("sequence"))
            .and_then(Value::as_u64)
            .or_else(|| {
                frame
                    .get("payload")
                    .and_then(|payload| payload.get("event_sequence"))
                    .and_then(Value::as_u64)
            });
        if self
            .last_sequence
            .is_some_and(|last| sequence.is_some_and(|value| value <= last))
        {
            return None;
        }
        let payload = frame.get("payload").cloned().unwrap_or(Value::Null);
        self.normalize_event_value(&payload, sequence)
    }

    fn normalize_history_value(&mut self, raw: &Value) -> Option<SessionEvent> {
        let sequence = raw
            .get("event_sequence")
            .and_then(Value::as_u64)
            .or_else(|| raw.get("sequence").and_then(Value::as_u64));
        self.normalize_event_value(raw, sequence)
    }

    fn normalize_event_value(
        &mut self,
        raw: &Value,
        sequence: Option<u64>,
    ) -> Option<SessionEvent> {
        let event = normalize_nars_event(raw, sequence, None, None, None)?;
        if let Some(sequence) = sequence {
            self.last_sequence = Some(
                self.last_sequence
                    .map_or(sequence, |last| last.max(sequence)),
            );
            self.oldest_sequence = Some(
                self.oldest_sequence
                    .map_or(sequence, |oldest| oldest.min(sequence)),
            );
        }
        if !self.seen_event_ids.insert(event.event_id.clone()) {
            return None;
        }
        Some(event)
    }

    fn read_older_events(&mut self) -> Result<HistoryReadResult, String> {
        if self.history_exhausted {
            return Ok(HistoryReadResult::default());
        }
        let Some(before_sequence) = self.oldest_sequence else {
            self.history_exhausted = true;
            return Ok(HistoryReadResult::default());
        };
        let request_id = {
            self.request_counter = self.request_counter.saturating_add(1);
            format!("agent-tui-history-{}", self.request_counter)
        };
        let frame = json!({
            "id": request_id,
            "method": "session.events.read",
            "params": {
                "before_sequence": before_sequence,
                "direction": "backward",
                "limit": 100,
                "view": "raw",
            },
        });
        self.socket
            .send_text(&serde_json::to_string(&frame).map_err(|error| error.to_string())?)?;

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut result = HistoryReadResult::default();
        loop {
            if Instant::now() >= deadline {
                return Err("nars_attach_history_read_timeout".to_string());
            }
            let Some(message) = self.socket.receive_text()? else {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };
            let frame: Value = serde_json::from_str(&message)
                .map_err(|error| format!("nars_attach_invalid_server_json:{error}"))?;
            let event_name = frame.get("event").and_then(Value::as_str);
            if event_name == Some("websocket_error") {
                let code = frame
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                return Err(format!("nars_attach_server_error:{code}"));
            }
            if event_name == Some("session_event") {
                if let Some(event) = self.normalize_live_frame(&frame) {
                    result.live.push(event);
                }
                continue;
            }
            if event_name != Some("session_events_read")
                || frame.get("request_id").and_then(Value::as_str) != Some(request_id.as_str())
            {
                continue;
            }
            let events = frame
                .get("events")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for event in events.iter() {
                if let Some(event) = self.normalize_history_value(event) {
                    result.older.push(event);
                }
            }
            self.history_exhausted = !frame
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            return Ok(result);
        }
    }

    pub fn submit(&mut self, text: &str, active_turn_id: Option<&str>) -> Result<(), String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        let params = if active_turn_id.is_some() {
            json!({
                "content": text,
                "source": "operator_steering",
                "delivery_mode": "admit_after_active_turn",
                "active_turn_id": active_turn_id,
            })
        } else {
            json!({"content": text, "source": "manual_operator"})
        };
        self.send_request("session.submit", params)
    }

    pub fn cancel(&mut self) -> Result<(), String> {
        self.send_request("session.cancel", json!({"reason": "operator_interrupt"}))
    }

    pub fn health(&mut self) -> Result<(), String> {
        self.send_request("session.health", json!({}))
    }

    pub fn recovery(&mut self) -> Result<(), String> {
        self.send_request("session.recovery", json!({}))
    }

    pub fn close(&mut self) -> Result<(), String> {
        let result = self.send_request("session.close", json!({"reason": "operator_exit"}));
        let _ = self.socket.send_close();
        result
    }

    fn subscribe(&mut self) -> Result<(), String> {
        let mut params = json!({
            "include_replay": true,
            "page_size": 100,
            "view": "raw",
            "subscription_id": self.subscription_id,
        });
        if let Some(sequence) = self.last_sequence {
            params["since_sequence"] = json!(sequence);
        }
        self.send_request_with_params("session.events.subscribe", params)
    }

    fn send_request(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.send_request_with_params(method, params)
    }

    fn send_request_with_params(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.request_counter = self.request_counter.saturating_add(1);
        let frame = json!({
            "id": format!("agent-tui-{}", self.request_counter),
            "method": method,
            "params": params,
        });
        self.socket
            .send_text(&serde_json::to_string(&frame).map_err(|error| error.to_string())?)
    }

    fn reconnect(&mut self) -> Result<(), String> {
        self.socket = WebSocket::connect(&self.endpoint)?;
        self.subscribe()
    }
}

fn value_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(ToString::to_string)
}

fn event_name(event: &Map<String, Value>) -> String {
    ["event", "event_kind", "type", "kind"]
        .iter()
        .find_map(|key| event.get(*key).and_then(Value::as_str))
        .unwrap_or("diagnostic")
        .to_ascii_lowercase()
        .replace('-', "_")
}

fn event_kind(name: &str) -> SessionEventKind {
    match name {
        "session_started" | "carrier_session_started" => SessionEventKind::CarrierSessionStarted,
        "input_queued" | "input_queued_for_turn_boundary" | "operator_input_queued" => {
            SessionEventKind::InputQueuedForTurnBoundary
        }
        "user_message"
        | "operator_input_submitted"
        | "input_admitted"
        | "input_admitted_to_turn" => SessionEventKind::InputAdmittedToTurn,
        "input_completed" => SessionEventKind::InputCompleted,
        "input_dropped" | "input_dropped_by_operator" => SessionEventKind::InputDroppedByOperator,
        "input_abandoned" | "input_abandoned_on_session_end" => {
            SessionEventKind::InputAbandonedOnSessionEnd
        }
        "system_directive_held" => SessionEventKind::SystemDirectiveHeld,
        "system_directive_released" => SessionEventKind::SystemDirectiveReleased,
        "turn_started" | "carrier_turn_started" => SessionEventKind::TurnStarted,
        "provider_request" | "provider_request_recorded" => {
            SessionEventKind::ProviderRequestRecorded
        }
        "assistant_message"
        | "assistant_message_stream"
        | "provider_text_delta"
        | "provider_text_delta_recorded" => SessionEventKind::ProviderTextDeltaRecorded,
        "tool_call"
        | "carrier_tool_requested"
        | "provider_tool_call_requested"
        | "tool_call_requested" => SessionEventKind::ProviderToolCallRequested,
        "tool_result" | "carrier_tool_completed" | "tool_result_received" => {
            SessionEventKind::ToolResultReceived
        }
        "turn_completed" | "turn_complete" | "carrier_turn_completed" => {
            SessionEventKind::TurnCompleted
        }
        "turn_failed" | "carrier_turn_failed" => SessionEventKind::TurnFailed,
        "turn_interrupted" | "carrier_turn_interrupted" => SessionEventKind::TurnInterrupted,
        "session_closed" | "carrier_session_closed" => SessionEventKind::CarrierSessionClosed,
        _ => SessionEventKind::CarrierDiagnosticRecorded,
    }
}

fn normalize_nars_event(
    raw: &Value,
    sequence: Option<u64>,
    identity: Option<&str>,
    session: Option<&str>,
    site_root: Option<&str>,
) -> Option<SessionEvent> {
    let object = raw.as_object()?;
    let name = event_name(object);
    let mut payload = object
        .get("payload")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (key, value) in object {
        payload.entry(key.clone()).or_insert_with(|| value.clone());
    }
    let event_id = value_string(object.get("event_id"))
        .or_else(|| value_string(object.get("id")))
        .or_else(|| value_string(object.get("request_id")))
        .or_else(|| sequence.map(|value| format!("nars-sequence-{value}")))
        .unwrap_or_else(|| format!("nars-event-{}", payload.len()));
    let occurred_at = value_string(object.get("occurred_at"))
        .or_else(|| value_string(object.get("timestamp")))
        .or_else(|| value_string(object.get("created_at")))
        .unwrap_or_default();
    let carrier_session_id = value_string(object.get("session_id"))
        .or_else(|| value_string(object.get("carrier_session_id")))
        .or_else(|| session.map(ToString::to_string))
        .unwrap_or_default();
    let agent_id = value_string(object.get("agent_id"))
        .or_else(|| identity.map(ToString::to_string))
        .unwrap_or_default();
    let site_id = value_string(object.get("site_id")).unwrap_or_default();
    let site_root = value_string(object.get("site_root"))
        .or_else(|| site_root.map(ToString::to_string))
        .unwrap_or_default();

    if let Some(sequence) = sequence {
        payload
            .entry("sequence".to_string())
            .or_insert(json!(sequence));
    }
    let source_kind = value_string(payload.get("source_kind"))
        .or_else(|| value_string(payload.get("source")))
        .unwrap_or_else(|| "operator".to_string());
    if matches!(
        event_kind(&name),
        SessionEventKind::InputAdmittedToTurn | SessionEventKind::TurnStarted
    ) {
        payload
            .entry("input_event_id".to_string())
            .or_insert(json!(event_id));
        payload
            .entry("source_kind".to_string())
            .or_insert(json!(source_kind));
        let content = value_string(payload.get("content_preview"))
            .or_else(|| value_string(payload.get("content")))
            .or_else(|| value_string(payload.get("text")))
            .or_else(|| value_string(payload.get("message")));
        if let Some(content) = content {
            payload
                .entry("content_preview".to_string())
                .or_insert(json!(content));
        }
    }
    if matches!(
        event_kind(&name),
        SessionEventKind::ProviderTextDeltaRecorded
    ) {
        let text = value_string(payload.get("text_delta"))
            .or_else(|| value_string(payload.get("text")))
            .or_else(|| value_string(payload.get("content")))
            .or_else(|| value_string(payload.get("message")));
        if let Some(text) = text {
            payload
                .entry("text_delta".to_string())
                .or_insert(json!(text));
        }
        let turn_id = value_string(payload.get("turn_id")).unwrap_or_default();
        payload
            .entry("turn_id".to_string())
            .or_insert(json!(turn_id));
    }
    if matches!(
        event_kind(&name),
        SessionEventKind::ProviderToolCallRequested
    ) {
        let tool_name = value_string(payload.get("tool_name"))
            .or_else(|| value_string(payload.get("name")))
            .unwrap_or_else(|| "tool".to_string());
        payload
            .entry("tool_name".to_string())
            .or_insert(json!(tool_name));
        let turn_id = value_string(payload.get("turn_id")).unwrap_or_default();
        payload
            .entry("turn_id".to_string())
            .or_insert(json!(turn_id));
        if !payload.contains_key("arguments_summary") {
            if let Some(arguments) = payload.get("arguments").cloned() {
                payload.insert(
                    "arguments_summary".to_string(),
                    json!(compact_json(&arguments)),
                );
            }
        }
    }
    if matches!(event_kind(&name), SessionEventKind::ToolResultReceived) {
        let tool_name = value_string(payload.get("tool_name"))
            .or_else(|| value_string(payload.get("name")))
            .unwrap_or_else(|| "tool".to_string());
        payload
            .entry("tool_name".to_string())
            .or_insert(json!(tool_name));
        payload
            .entry("status".to_string())
            .or_insert(json!("completed"));
    }
    if matches!(
        event_kind(&name),
        SessionEventKind::TurnStarted | SessionEventKind::ProviderRequestRecorded
    ) {
        let turn_id = value_string(payload.get("turn_id")).unwrap_or_default();
        payload
            .entry("turn_id".to_string())
            .or_insert(json!(turn_id));
    }
    if matches!(
        event_kind(&name),
        SessionEventKind::TurnCompleted
            | SessionEventKind::TurnFailed
            | SessionEventKind::TurnInterrupted
    ) {
        let turn_id = value_string(payload.get("turn_id")).unwrap_or_default();
        payload
            .entry("turn_id".to_string())
            .or_insert(json!(turn_id));
    }
    Some(SessionEvent {
        schema: session_event_schema().to_string(),
        event_kind: event_kind(&name),
        event_id,
        occurred_at,
        carrier_session_id,
        agent_id,
        site_id,
        site_root,
        payload: Value::Object(payload),
    })
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

#[derive(Debug)]
struct NarsProjectionState {
    client: NarsProjectionClient,
    transcript: TranscriptStore,
    identity: String,
    session: String,
    turn_state: TurnState,
    active_phase: Option<String>,
    active_turn_id: Option<String>,
    queued_inputs: usize,
    held_system_directives: usize,
    last_error: Option<String>,
    transcript_scroll_offset: usize,
}

impl NarsProjectionState {
    fn new(
        client: NarsProjectionClient,
        identity: Option<String>,
        session: Option<String>,
    ) -> Self {
        Self {
            client,
            transcript: TranscriptStore::new(),
            identity: identity
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "agent-tui".to_string()),
            session: session
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "attached".to_string()),
            turn_state: TurnState::Idle,
            active_phase: None,
            active_turn_id: None,
            queued_inputs: 0,
            held_system_directives: 0,
            last_error: None,
            transcript_scroll_offset: 0,
        }
    }

    fn apply_live_event(&mut self, event: SessionEvent) {
        if !event.agent_id.is_empty() {
            self.identity = event.agent_id.clone();
        }
        if !event.carrier_session_id.is_empty() {
            self.session = event.carrier_session_id.clone();
        }
        match event.event_kind {
            SessionEventKind::InputQueuedForTurnBoundary => {
                self.queued_inputs = self.queued_inputs.saturating_add(1);
            }
            SessionEventKind::InputAdmittedToTurn => {
                self.queued_inputs = self.queued_inputs.saturating_sub(1);
            }
            SessionEventKind::InputCompleted
            | SessionEventKind::InputDroppedByOperator
            | SessionEventKind::InputAbandonedOnSessionEnd => {
                self.queued_inputs = self.queued_inputs.saturating_sub(1);
            }
            SessionEventKind::SystemDirectiveHeld => {
                self.held_system_directives = self.held_system_directives.saturating_add(1);
            }
            SessionEventKind::SystemDirectiveReleased => {
                self.held_system_directives = self.held_system_directives.saturating_sub(1);
            }
            SessionEventKind::TurnStarted => {
                self.turn_state = TurnState::Active;
                self.active_phase = Some("thinking".to_string());
                self.active_turn_id = event
                    .payload
                    .get("turn_id")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
            }
            SessionEventKind::ProviderRequestRecorded
            | SessionEventKind::ProviderToolCallRequested => {
                self.turn_state = TurnState::Active;
                self.active_phase = Some("calling".to_string());
            }
            SessionEventKind::TurnCompleted
            | SessionEventKind::TurnFailed
            | SessionEventKind::TurnInterrupted => {
                self.turn_state = TurnState::Idle;
                self.active_phase = None;
                self.active_turn_id = None;
            }
            SessionEventKind::CarrierDiagnosticRecorded => {
                self.last_error = value_string(event.payload.get("message"))
                    .or_else(|| value_string(event.payload.get("error")))
                    .or_else(|| value_string(event.payload.get("code")));
            }
            _ => {}
        }
        let _ = self.transcript.ingest_event(&event);
    }

    fn poll(&mut self) -> Result<(), String> {
        for event in self.client.poll()? {
            self.apply_live_event(event);
        }
        Ok(())
    }

    fn read_older_history(&mut self) -> Result<(), String> {
        let result = self.client.read_older_events()?;
        for event in result.live {
            self.apply_live_event(event);
        }
        self.transcript.ingest_history_events(&result.older);
        Ok(())
    }

    fn build_view(&self, terminal_size: TerminalSize, composer: &TextareaComposer) -> AppViewModel {
        let mut model = build_app_view(&AppViewInput {
            terminal_size,
            layout_config: LayoutConfig::default(),
            transcript_items: self.transcript.items().to_vec(),
            status: StatusViewInput {
                identity: self.identity.clone(),
                session: self.session.clone(),
                turn_state: self.turn_state,
                active_phase: self.active_phase.clone(),
                active_turn_age: None,
                queued_inputs: self.queued_inputs,
                held_system_directives: self.held_system_directives,
                oldest_held_age: None,
                transcript_items: self.transcript.len(),
                runtime_posture: RuntimePostureState::disabled(),
                last_error: self.last_error.clone(),
            },
            composer: ComposerViewInput {
                identity: self.identity.clone(),
                draft_text: composer.text(),
                turn_state: self.turn_state,
                queued_operator_notes: self.queued_inputs,
                held_system_directives: self.held_system_directives,
            },
        });
        model.set_transcript_scroll_offset(self.transcript_scroll_offset);
        model
    }
}

pub fn run_attached_projection(
    endpoint: &str,
    identity: Option<String>,
    session: Option<String>,
    max_steps: Option<u64>,
) -> Result<(), String> {
    let client = NarsProjectionClient::connect(endpoint)?;
    let mut state = NarsProjectionState::new(client, identity, session);
    let mut composer = TextareaComposer::default();
    let mut input_reader = CrosstermTerminalInputReader;
    let mut terminal_session = TerminalSession::enter()?;
    let mut steps = 0u64;
    let loop_result = loop {
        steps = steps.saturating_add(1);
        if let Err(error) = state.poll() {
            state.last_error = Some(error);
            std::thread::sleep(Duration::from_millis(100));
        }
        let size = terminal_session.terminal_size()?;
        let model = state.build_view(size, &composer);
        terminal_session.draw_once_with_composer(&model, &composer)?;
        if max_steps.is_some_and(|limit| steps >= limit) {
            break Ok(());
        }
        match run_textarea_composer_input_tick_with_wait(
            &mut input_reader,
            &mut composer,
            INPUT_IDLE_WAIT,
        ) {
            TerminalInputTickOutcome::NoInput
            | TerminalInputTickOutcome::NonKeyEventIgnored
            | TerminalInputTickOutcome::DraftEffect(
                crate::composer_draft::ComposerDraftEffect::DraftChanged,
            )
            | TerminalInputTickOutcome::DraftEffect(
                crate::composer_draft::ComposerDraftEffect::None,
            ) => {}
            TerminalInputTickOutcome::ScrollTranscriptUp => {
                if state.transcript_scroll_offset.saturating_add(8) >= state.transcript.len() {
                    if let Err(error) = state.read_older_history() {
                        state.last_error = Some(error);
                    }
                }
                state.transcript_scroll_offset = state.transcript_scroll_offset.saturating_add(1);
            }
            TerminalInputTickOutcome::ScrollTranscriptDown => {
                state.transcript_scroll_offset = state.transcript_scroll_offset.saturating_sub(1);
            }
            TerminalInputTickOutcome::DraftEffect(
                crate::composer_draft::ComposerDraftEffect::SubmitRequested { text },
            ) => {
                if let Err(error) = state.client.submit(&text, state.active_turn_id.as_deref()) {
                    state.last_error = Some(error);
                }
            }
            TerminalInputTickOutcome::DraftEffect(
                crate::composer_draft::ComposerDraftEffect::ClearOrInterruptRequested,
            ) => {
                if state.turn_state == TurnState::Active {
                    if let Err(error) = state.client.cancel() {
                        state.last_error = Some(error);
                    }
                } else {
                    composer = TextareaComposer::default();
                }
            }
            TerminalInputTickOutcome::DraftEffect(
                crate::composer_draft::ComposerDraftEffect::ExitRequested,
            ) => {
                let _ = state.client.close();
                break Ok(());
            }
            TerminalInputTickOutcome::ReadFailed(error) => {
                break Err(format!("terminal_input_read_failed:{error}"));
            }
        }
    };
    let leave_result = terminal_session.leave();
    match (loop_result, leave_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(leave_error)) => Err(format!("{error};{leave_error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_loopback_event_endpoint() {
        assert_eq!(
            parse_websocket_endpoint("ws://127.0.0.1:12345/events"),
            Ok(WebSocketEndpoint {
                host: "127.0.0.1".to_string(),
                port: 12345,
                path: "/events".to_string(),
            })
        );
    }

    #[test]
    fn computes_websocket_accept_value() {
        assert_eq!(
            websocket_accept_value("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn normalizes_conversation_events_for_existing_projection() {
        let user = normalize_nars_event(
            &json!({
                "event": "user_message",
                "event_id": "event-1",
                "session_id": "session-1",
                "agent_id": "sonar.resident",
                "content": "run startup sequence"
            }),
            Some(7),
            None,
            None,
            None,
        )
        .expect("user event");
        assert_eq!(user.event_kind, SessionEventKind::InputAdmittedToTurn);
        assert_eq!(user.payload["content_preview"], "run startup sequence");
        assert_eq!(user.payload["sequence"], 7);

        let assistant = normalize_nars_event(
            &json!({
                "event": "assistant_message_stream",
                "event_id": "event-2",
                "turn_id": "turn-1",
                "text": "done"
            }),
            Some(8),
            None,
            None,
            None,
        )
        .expect("assistant event");
        assert_eq!(
            assistant.event_kind,
            SessionEventKind::ProviderTextDeltaRecorded
        );
        assert_eq!(assistant.payload["text_delta"], "done");
        assert_eq!(assistant.payload["turn_id"], "turn-1");
    }

    #[test]
    fn normalizes_turn_tool_and_result_events_with_projection_fields() {
        let started = normalize_nars_event(
            &json!({
                "event": "turn_started",
                "event_id": "event-3",
                "session_id": "session-1",
                "turn_id": "turn-2",
                "agent_id": "sonar.resident"
            }),
            Some(9),
            None,
            None,
            None,
        )
        .expect("turn start event");
        assert_eq!(started.event_kind, SessionEventKind::TurnStarted);
        assert_eq!(started.payload["turn_id"], "turn-2");
        assert_eq!(started.payload["sequence"], 9);

        let tool = normalize_nars_event(
            &json!({
                "event": "tool_call",
                "event_id": "event-4",
                "turn_id": "turn-2",
                "tool_name": "mcp_output_show",
                "arguments": {"output_ref": "mcp_output:o_123"}
            }),
            Some(10),
            None,
            None,
            None,
        )
        .expect("tool call event");
        assert_eq!(tool.event_kind, SessionEventKind::ProviderToolCallRequested);
        assert_eq!(tool.payload["tool_name"], "mcp_output_show");
        assert_eq!(tool.payload["turn_id"], "turn-2");
        assert_eq!(
            tool.payload["arguments_summary"],
            "{\"output_ref\":\"mcp_output:o_123\"}"
        );

        let result = normalize_nars_event(
            &json!({
                "event": "tool_result",
                "event_id": "event-5",
                "turn_id": "turn-2",
                "tool_name": "mcp_output_show",
                "text": "full output"
            }),
            Some(11),
            None,
            None,
            None,
        )
        .expect("tool result event");
        assert_eq!(result.event_kind, SessionEventKind::ToolResultReceived);
        assert_eq!(result.payload["tool_name"], "mcp_output_show");
        assert_eq!(result.payload["status"], "completed");

        let completed = normalize_nars_event(
            &json!({
                "event": "turn_completed",
                "event_id": "event-6",
                "turn_id": "turn-2"
            }),
            Some(12),
            None,
            None,
            None,
        )
        .expect("turn completion event");
        assert_eq!(completed.event_kind, SessionEventKind::TurnCompleted);
        assert_eq!(completed.payload["turn_id"], "turn-2");
    }

    #[test]
    fn resolves_event_endpoint_from_nars_launch_binding() {
        let root = std::env::temp_dir().join(format!(
            "narada-agent-tui-launch-binding-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let record_dir = root
            .join(".narada")
            .join("crew")
            .join("nars-sessions")
            .join("nars-session-1");
        fs::create_dir_all(&record_dir).expect("create session record directory");
        fs::write(
            record_dir.join("session-index-record.json"),
            serde_json::to_vec(&json!({
                "session_id": "nars-session-1",
                "agent_id": "sonar.resident",
                "event_endpoint": "ws://127.0.0.1:4567/events"
            }))
            .expect("serialize session record"),
        )
        .expect("write session record");
        let binding_path = root.join("launch-binding.json");
        fs::write(
            &binding_path,
            serde_json::to_vec(&json!({
                "schema": "narada.operator_projection_launch_binding.v1",
                "status": "ready",
                "site_root": root,
                "nars_session_id": "nars-session-1",
                "agent": "sonar.resident"
            }))
            .expect("serialize launch binding"),
        )
        .expect("write launch binding");

        let resolution = resolve_event_endpoint_from_launch_binding(
            binding_path.to_str().expect("binding path is UTF-8"),
        )
        .expect("resolve launch binding");
        let _ = fs::remove_dir_all(&root);

        assert_eq!(resolution.event_endpoint, "ws://127.0.0.1:4567/events");
        assert_eq!(resolution.identity.as_deref(), Some("sonar.resident"));
        assert_eq!(resolution.session.as_deref(), Some("nars-session-1"));
    }

    #[test]
    fn rejects_failed_launch_binding_without_endpoint_discovery() {
        let path = std::env::temp_dir().join(format!(
            "narada-agent-tui-failed-binding-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::write(
            &path,
            serde_json::to_vec(&json!({
                "schema": "narada.operator_projection_launch_binding.v1",
                "status": "failed",
                "reason": "nars_start_failed"
            }))
            .expect("serialize failed binding"),
        )
        .expect("write failed binding");

        let result = resolve_event_endpoint_from_launch_binding(
            path.to_str().expect("binding path is UTF-8"),
        );
        let _ = fs::remove_file(&path);

        let error = result.expect_err("failed binding is rejected");
        assert_eq!(error, "nars_attach_launch_binding_failed:nars_start_failed");
    }

    #[test]
    fn base64_encodes_short_payloads() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }
}
