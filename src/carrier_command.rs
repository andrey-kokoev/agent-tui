#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarrierCommand {
    Help,
    Status,
    Stats { value: Option<String> },
    Model { value: Option<String> },
    Thinking { value: Option<String> },
    Clear,
    Exit,
    QueueShow,
    QueueClear,
    QueueDrop { index: usize },
    Unknown { command: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorSubmit {
    CarrierCommand(CarrierCommand),
    AgentInput(String),
    Empty,
}

pub fn parse_operator_submit(text: &str) -> OperatorSubmit {
    if text.trim().is_empty() {
        return OperatorSubmit::Empty;
    }

    if let Some(literal) = text.strip_prefix("//") {
        return OperatorSubmit::AgentInput(format!("/{literal}"));
    }

    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return OperatorSubmit::AgentInput(text.to_string());
    }

    let mut parts = trimmed.split_whitespace();
    let raw_command = parts.next().unwrap_or_default();
    let command = raw_command.to_ascii_lowercase();
    let value = parts.collect::<Vec<_>>().join(" ");
    match command.as_str() {
        "/help" => OperatorSubmit::CarrierCommand(CarrierCommand::Help),
        "/status" => OperatorSubmit::CarrierCommand(CarrierCommand::Status),
        "/stats" => OperatorSubmit::CarrierCommand(CarrierCommand::Stats {
            value: nonempty_value(value),
        }),
        "/model" => OperatorSubmit::CarrierCommand(CarrierCommand::Model {
            value: nonempty_value(value),
        }),
        "/thinking" => OperatorSubmit::CarrierCommand(CarrierCommand::Thinking {
            value: nonempty_value(value),
        }),
        "/clear" => OperatorSubmit::CarrierCommand(CarrierCommand::Clear),
        "/exit" | "/quit" => OperatorSubmit::CarrierCommand(CarrierCommand::Exit),
        "/queue" => parse_queue_command(&value),
        _ => OperatorSubmit::CarrierCommand(CarrierCommand::Unknown { command }),
    }
}

fn parse_queue_command(value: &str) -> OperatorSubmit {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return OperatorSubmit::CarrierCommand(CarrierCommand::QueueShow);
    }
    if trimmed == "clear" {
        return OperatorSubmit::CarrierCommand(CarrierCommand::QueueClear);
    }
    if let Some(index) = trimmed.strip_prefix("drop ") {
        if let Ok(index) = index.trim().parse::<usize>() {
            return OperatorSubmit::CarrierCommand(CarrierCommand::QueueDrop { index });
        }
    }
    OperatorSubmit::CarrierCommand(CarrierCommand::Unknown {
        command: format!("/queue {trimmed}"),
    })
}

fn nonempty_value(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_cli_parity_commands() {
        assert_eq!(
            parse_operator_submit("/help"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Help)
        );
        assert_eq!(
            parse_operator_submit("/status"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Status)
        );
        assert_eq!(
            parse_operator_submit("/stats --date 2026-06-01 --top 3"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Stats {
                value: Some("--date 2026-06-01 --top 3".to_string())
            })
        );
        assert_eq!(
            parse_operator_submit("/model gpt-5.5"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Model {
                value: Some("gpt-5.5".to_string())
            })
        );
        assert_eq!(
            parse_operator_submit("/thinking high"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Thinking {
                value: Some("high".to_string())
            })
        );
        assert_eq!(
            parse_operator_submit("/clear"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Clear)
        );
        assert_eq!(
            parse_operator_submit("/exit"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Exit)
        );
        assert_eq!(
            parse_operator_submit("/quit"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Exit)
        );
    }

    #[test]
    fn parses_queue_commands() {
        assert_eq!(
            parse_operator_submit("/queue"),
            OperatorSubmit::CarrierCommand(CarrierCommand::QueueShow)
        );
        assert_eq!(
            parse_operator_submit("/queue clear"),
            OperatorSubmit::CarrierCommand(CarrierCommand::QueueClear)
        );
        assert_eq!(
            parse_operator_submit("/queue drop 2"),
            OperatorSubmit::CarrierCommand(CarrierCommand::QueueDrop { index: 2 })
        );
    }

    #[test]
    fn double_slash_submits_literal_slash_to_agent() {
        assert_eq!(
            parse_operator_submit("//help"),
            OperatorSubmit::AgentInput("/help".to_string())
        );
    }

    #[test]
    fn unknown_slash_text_is_local_command_feedback_not_agent_input() {
        assert_eq!(
            parse_operator_submit("/wat"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Unknown {
                command: "/wat".to_string()
            })
        );
    }
}
