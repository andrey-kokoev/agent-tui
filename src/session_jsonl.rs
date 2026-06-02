use crate::carrier_protocol::{SessionEvent, serialize_session_event};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub fn append_session_event(path: &Path, event: &SessionEvent) -> Result<(), String> {
    let line = serialize_session_event(event)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("session_jsonl_open_failed:{error}"))?;
    file.write_all(line.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|error| format!("session_jsonl_write_failed:{error}"))
}

pub fn encode_session_jsonl_line(event: &SessionEvent) -> Result<String, String> {
    serialize_session_event(event).map(|line| format!("{line}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::parse_session_event;

    #[test]
    fn encodes_session_event_as_single_jsonl_line() {
        let event = parse_session_event(include_str!(
            "../carrier-protocol/fixtures/session-event.json"
        ))
        .expect("session fixture parses");

        let line = encode_session_jsonl_line(&event).expect("jsonl line encodes");
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1);
        let reparsed = parse_session_event(line.trim_end()).expect("jsonl event reparses");
        assert_eq!(reparsed.event_id, event.event_id);
    }
}
