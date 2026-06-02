use std::collections::BTreeMap;

use crate::terminal_runtime_contract::terminal_runtime_contract;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalRuntimeStatus {
    Disabled,
    Configured,
    Refused,
}

impl TerminalRuntimeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Configured => "configured",
            Self::Refused => "refused",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRuntimeConfig {
    pub status: TerminalRuntimeStatus,
    pub terminal_rendering_enabled: bool,
    pub mode: Option<String>,
    pub refusal_reason: Option<String>,
}

impl TerminalRuntimeConfig {
    pub fn disabled() -> Self {
        Self {
            status: TerminalRuntimeStatus::Disabled,
            terminal_rendering_enabled: false,
            mode: None,
            refusal_reason: None,
        }
    }

    pub fn configured_for_explicit_terminal_mode(mode: impl Into<String>) -> Self {
        Self {
            status: TerminalRuntimeStatus::Configured,
            terminal_rendering_enabled: true,
            mode: Some(mode.into()),
            refusal_reason: None,
        }
    }

    pub fn from_env_map(env: &BTreeMap<String, String>) -> Self {
        let contract = terminal_runtime_contract();
        if !env_flag_enabled(env.get(&contract.terminal_rendering_env_var)) {
            return Self::disabled();
        }

        let mode = trimmed_nonempty(env.get(&contract.terminal_mode_env_var));
        let Some(mode) = mode else {
            return Self::refused("missing_terminal_mode");
        };
        if mode != contract.required_terminal_mode {
            return Self::refused(format!("unsupported_terminal_mode:{mode}"));
        }

        Self {
            status: TerminalRuntimeStatus::Configured,
            terminal_rendering_enabled: true,
            mode: Some(mode),
            refusal_reason: None,
        }
    }

    fn refused(reason: impl Into<String>) -> Self {
        Self {
            status: TerminalRuntimeStatus::Refused,
            terminal_rendering_enabled: false,
            mode: None,
            refusal_reason: Some(reason.into()),
        }
    }
}

fn env_flag_enabled(value: Option<&String>) -> bool {
    matches!(
        value.map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if matches!(value.as_str(), "1" | "true" | "on" | "yes")
    )
}

fn trimmed_nonempty(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn terminal_runtime_is_disabled_without_explicit_rendering_flag() {
        let contract = terminal_runtime_contract();
        let config = TerminalRuntimeConfig::from_env_map(&env(&[(
            contract.terminal_mode_env_var.as_str(),
            contract.required_terminal_mode.as_str(),
        )]));

        assert_eq!(config.status, TerminalRuntimeStatus::Disabled);
        assert_eq!(config.status.as_str(), "disabled");
        assert!(!config.terminal_rendering_enabled);
        assert_eq!(config.mode, None);
    }

    #[test]
    fn terminal_runtime_refuses_missing_mode_when_enabled() {
        let contract = terminal_runtime_contract();
        let config = TerminalRuntimeConfig::from_env_map(&env(&[(
            contract.terminal_rendering_env_var.as_str(),
            "true",
        )]));

        assert_eq!(config.status, TerminalRuntimeStatus::Refused);
        assert!(!config.terminal_rendering_enabled);
        assert_eq!(
            config.refusal_reason.as_deref(),
            Some("missing_terminal_mode")
        );
    }

    #[test]
    fn terminal_runtime_refuses_unsupported_mode() {
        let contract = terminal_runtime_contract();
        let config = TerminalRuntimeConfig::from_env_map(&env(&[
            (contract.terminal_rendering_env_var.as_str(), "true"),
            (contract.terminal_mode_env_var.as_str(), "render_once"),
        ]));

        assert_eq!(config.status, TerminalRuntimeStatus::Refused);
        assert!(!config.terminal_rendering_enabled);
        assert_eq!(
            config.refusal_reason.as_deref(),
            Some("unsupported_terminal_mode:render_once")
        );
    }

    #[test]
    fn terminal_runtime_configures_interactive_loop_mode() {
        let contract = terminal_runtime_contract();
        let config = TerminalRuntimeConfig::from_env_map(&env(&[
            (contract.terminal_rendering_env_var.as_str(), "yes"),
            (
                contract.terminal_mode_env_var.as_str(),
                contract.required_terminal_mode.as_str(),
            ),
        ]));

        assert_eq!(config.status, TerminalRuntimeStatus::Configured);
        assert_eq!(config.status.as_str(), "configured");
        assert!(config.terminal_rendering_enabled);
        assert_eq!(
            config.mode.as_deref(),
            Some(contract.required_terminal_mode.as_str())
        );
    }

    #[test]
    fn explicit_terminal_mode_configures_terminal_without_env_gate() {
        let config = TerminalRuntimeConfig::configured_for_explicit_terminal_mode("render_once");

        assert_eq!(config.status, TerminalRuntimeStatus::Configured);
        assert!(config.terminal_rendering_enabled);
        assert_eq!(config.mode.as_deref(), Some("render_once"));
        assert_eq!(config.refusal_reason, None);
    }
}
