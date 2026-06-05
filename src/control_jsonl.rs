use crate::carrier_protocol::{ControlInputEvent, parse_control_input_event};

#[derive(Debug)]
pub struct ControlJsonlEntry {
    pub line_number: usize,
    pub event: ControlInputEvent,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ControlJsonlError {
    pub line_number: usize,
    pub message: String,
}

pub fn parse_control_jsonl(content: &str) -> (Vec<ControlJsonlEntry>, Vec<ControlJsonlError>) {
    let mut entries = Vec::new();
    let mut errors = Vec::new();

    for (index, raw_line) in content.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        match parse_control_input_event(line) {
            Ok(event) => entries.push(ControlJsonlEntry { line_number, event }),
            Err(message) => errors.push(ControlJsonlError {
                line_number,
                message,
            }),
        }
    }

    (entries, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTROL_FIXTURE: &str =
        include_str!("../../narada/packages/carrier-protocol/fixtures/control-input-event.json");

    #[test]
    fn parses_valid_control_jsonl_lines() {
        let content = format!("\n{}\n", CONTROL_FIXTURE.trim_end());
        let (entries, errors) = parse_control_jsonl(&content);

        assert!(errors.is_empty());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].line_number, 2);
        assert_eq!(entries[0].event.input.content, "run startup sequence");
    }

    #[test]
    fn reports_malformed_line_with_line_number() {
        let content = format!("{}\nnot json\n", CONTROL_FIXTURE.trim_end());
        let (entries, errors) = parse_control_jsonl(&content);

        assert_eq!(entries.len(), 1);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line_number, 2);
        assert!(!errors[0].message.trim().is_empty());
    }

    #[test]
    fn reports_unsupported_schema_with_line_number() {
        let content = r#"{"schema":"legacy.system_directive.deliver"}"#;
        let (entries, errors) = parse_control_jsonl(content);

        assert!(entries.is_empty());
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line_number, 1);
        assert!(
            errors[0].message.contains("missing field")
                || errors[0].message.contains("invalid_schema")
        );
    }
}
