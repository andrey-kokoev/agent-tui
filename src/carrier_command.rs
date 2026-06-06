#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CarrierCommand {
    Help,
    Status,
    Goal { value: Option<String> },
    Stats { value: Option<String> },
    Model { value: Option<String> },
    Thinking { value: Option<String> },
    ToolOutput { value: Option<String> },
    Tools { value: Option<String> },
    Observers,
    ObserverMute,
    ObserverUnmute,
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
    let normalized_full = normalize_command_token(trimmed);
    if !trimmed.starts_with('/') && command_name_for_token(&normalized_full).is_none() {
        return OperatorSubmit::AgentInput(text.to_string());
    }

    let mut parts = trimmed.split_whitespace();
    let raw_command = parts.next().unwrap_or_default();
    let command = normalize_command_token(raw_command);
    let value = parts.collect::<Vec<_>>().join(" ");
    let command_name = command_name_for_token(&normalized_full)
        .or_else(|| command_name_for_token(&command))
        .unwrap_or("unknown");
    match command_name {
        "help" => OperatorSubmit::CarrierCommand(CarrierCommand::Help),
        "status" => OperatorSubmit::CarrierCommand(CarrierCommand::Status),
        "goal" => OperatorSubmit::CarrierCommand(CarrierCommand::Goal {
            value: nonempty_value(value),
        }),
        "stats" => OperatorSubmit::CarrierCommand(CarrierCommand::Stats {
            value: nonempty_value(value),
        }),
        "model" => OperatorSubmit::CarrierCommand(CarrierCommand::Model {
            value: nonempty_value(value),
        }),
        "thinking" => OperatorSubmit::CarrierCommand(CarrierCommand::Thinking {
            value: nonempty_value(value),
        }),
        "tool_output" => OperatorSubmit::CarrierCommand(CarrierCommand::ToolOutput {
            value: nonempty_value(value),
        }),
        "tools" => OperatorSubmit::CarrierCommand(CarrierCommand::Tools {
            value: nonempty_value(value),
        }),
        "observers" => OperatorSubmit::CarrierCommand(CarrierCommand::Observers),
        "observer_mute" => OperatorSubmit::CarrierCommand(CarrierCommand::ObserverMute),
        "observer_unmute" => OperatorSubmit::CarrierCommand(CarrierCommand::ObserverUnmute),
        "clear" => OperatorSubmit::CarrierCommand(CarrierCommand::Clear),
        "exit" => OperatorSubmit::CarrierCommand(CarrierCommand::Exit),
        "queue_show" => parse_queue_command(&value),
        "queue_clear" => OperatorSubmit::CarrierCommand(CarrierCommand::QueueClear),
        "queue_drop" => parse_queue_command(
            normalized_full
                .strip_prefix(queue_command_prefix("queue_show"))
                .unwrap_or(value.as_str()),
        ),
        _ => OperatorSubmit::CarrierCommand(CarrierCommand::Unknown { command }),
    }
}

fn parse_queue_command(value: &str) -> OperatorSubmit {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return OperatorSubmit::CarrierCommand(CarrierCommand::QueueShow);
    }
    if trimmed == queue_command_suffix("queue_clear") {
        return OperatorSubmit::CarrierCommand(CarrierCommand::QueueClear);
    }
    if let Some(index) = trimmed.strip_prefix(&format!("{} ", queue_command_suffix("queue_drop"))) {
        if let Ok(index) = index.trim().parse::<usize>() {
            return OperatorSubmit::CarrierCommand(CarrierCommand::QueueDrop { index });
        }
    }
    OperatorSubmit::CarrierCommand(CarrierCommand::Unknown {
        command: format!("{} {trimmed}", queue_command_prefix("queue_show")),
    })
}

fn queue_command_prefix(name: &str) -> &'static str {
    command_named(name)
        .and_then(|command| command.primary.split_whitespace().next())
        .expect("bundled carrier command contract must define queue command prefix")
}

fn queue_command_suffix(name: &str) -> String {
    command_named(name)
        .map(|command| {
            command
                .primary
                .split_whitespace()
                .skip(1)
                .take_while(|part| !part.starts_with('<'))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| name.trim_start_matches("queue_").to_string())
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
        let command_contract: serde_json::Value = serde_json::from_str(include_str!(
            "../../narada/packages/carrier-command-contract/contracts/commands.json"
        ))
        .expect("shared command contract parses");
        assert_eq!(
            command_contract
                .get("schema")
                .and_then(serde_json::Value::as_str),
            Some("narada.carrier.command_contract.v1")
        );
        let command_tokens: Vec<&str> = command_contract
            .get("commands")
            .and_then(serde_json::Value::as_array)
            .expect("commands are listed")
            .iter()
            .flat_map(|command| {
                let primary = command
                    .get("primary")
                    .and_then(serde_json::Value::as_str)
                    .into_iter();
                let aliases = command
                    .get("aliases")
                    .and_then(serde_json::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_str);
                primary.chain(aliases)
            })
            .collect();
        assert_eq!(
            command_tokens,
            vec![
                "/help",
                "/status",
                "/goal",
                "/stats",
                "/model",
                "/thinking",
                "/tool-output",
                "/tool-outputs",
                "/tools",
                "/tool",
                "/observers",
                "/observer mute",
                "/observer unmute",
                "/queue",
                "/queue clear",
                "/queue drop <index>",
                "/clear",
                "/exit",
                "/quit",
                "exit",
            ]
        );

        assert_eq!(
            parse_operator_submit("/help"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Help)
        );
        assert_eq!(
            parse_operator_submit("/status"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Status)
        );
        assert_eq!(
            parse_operator_submit("/goal finish the carrier contract"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Goal {
                value: Some("finish the carrier contract".to_string())
            })
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
            parse_operator_submit("/tool-output off"),
            OperatorSubmit::CarrierCommand(CarrierCommand::ToolOutput {
                value: Some("off".to_string())
            })
        );
        assert_eq!(
            parse_operator_submit("/tool-outputs"),
            OperatorSubmit::CarrierCommand(CarrierCommand::ToolOutput { value: None })
        );
        assert_eq!(
            parse_operator_submit("/tools mcp"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Tools {
                value: Some("mcp".to_string())
            })
        );
        assert_eq!(
            parse_operator_submit("/tool"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Tools { value: None })
        );
        assert_eq!(
            parse_operator_submit("/observers"),
            OperatorSubmit::CarrierCommand(CarrierCommand::Observers)
        );
        assert_eq!(
            parse_operator_submit("/observer mute"),
            OperatorSubmit::CarrierCommand(CarrierCommand::ObserverMute)
        );
        assert_eq!(
            parse_operator_submit("/observer unmute"),
            OperatorSubmit::CarrierCommand(CarrierCommand::ObserverUnmute)
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
        assert_eq!(
            parse_operator_submit("exit"),
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
use crate::carrier_command_contract::{
    command_name_for_token, command_named, normalize_command_token,
};
