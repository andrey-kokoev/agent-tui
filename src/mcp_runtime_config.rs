use crate::mcp_runtime_contract::mcp_runtime_contract;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpRuntimeAdmissionStatus {
    Disabled,
    Configured,
    Refused,
}

impl McpRuntimeAdmissionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Configured => "configured",
            Self::Refused => "refused",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpRuntimeConfig {
    pub status: McpRuntimeAdmissionStatus,
    pub mcp_fabric_access_enabled: bool,
    pub config_path_policy: &'static str,
    pub config_path: Option<String>,
    pub site_mcp_fabric: Option<String>,
    pub refusal_reason: Option<String>,
}

impl McpRuntimeConfig {
    pub fn disabled() -> Self {
        Self {
            status: McpRuntimeAdmissionStatus::Disabled,
            mcp_fabric_access_enabled: false,
            config_path_policy: config_path_policy(),
            config_path: None,
            site_mcp_fabric: None,
            refusal_reason: None,
        }
    }

    pub fn from_env_map(env: &BTreeMap<String, String>) -> Self {
        Self::from_env_map_with_config_readiness(env, |_| Ok(()))
    }

    pub fn from_env_map_with_config_readiness(
        env: &BTreeMap<String, String>,
        config_readiness: impl Fn(&str) -> Result<(), String>,
    ) -> Self {
        if !env_flag_enabled(env.get(mcp_fabric_env_var())) {
            return Self::disabled();
        }

        let config_path = trimmed_nonempty(env.get(mcp_config_env_var()));
        let site_mcp_fabric = trimmed_nonempty(env.get(site_mcp_fabric_env_var()));

        let Some(config_path) = config_path else {
            return Self::refused("missing_mcp_config");
        };
        let Some(site_mcp_fabric) = site_mcp_fabric else {
            return Self::refused("missing_site_mcp_fabric");
        };

        if !config_path_is_within_fabric(&config_path, &site_mcp_fabric) {
            return Self::refused("mcp_config_outside_site_mcp_fabric");
        }
        if let Err(reason) = config_readiness(&config_path) {
            return Self::refused(reason);
        }

        Self {
            status: McpRuntimeAdmissionStatus::Configured,
            mcp_fabric_access_enabled: true,
            config_path_policy: config_path_policy(),
            config_path: Some(config_path),
            site_mcp_fabric: Some(site_mcp_fabric),
            refusal_reason: None,
        }
    }

    fn refused(reason: impl Into<String>) -> Self {
        Self {
            status: McpRuntimeAdmissionStatus::Refused,
            mcp_fabric_access_enabled: false,
            config_path_policy: config_path_policy(),
            config_path: None,
            site_mcp_fabric: None,
            refusal_reason: Some(reason.into()),
        }
    }
}

pub fn mcp_fabric_env_var() -> &'static str {
    mcp_runtime_contract().mcp_fabric_env_var.as_str()
}

pub fn config_path_policy() -> &'static str {
    mcp_runtime_contract().mcp_config_path_policy.as_str()
}

pub fn mcp_config_env_var() -> &'static str {
    mcp_runtime_contract().mcp_config_env_var.as_str()
}

pub fn site_mcp_fabric_env_var() -> &'static str {
    mcp_runtime_contract().site_mcp_fabric_env_var.as_str()
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

fn config_path_is_within_fabric(config_path: &str, site_mcp_fabric: &str) -> bool {
    let config_path = normalize_lexical_path(config_path);
    let site_mcp_fabric = normalize_lexical_path(site_mcp_fabric);
    if path_has_parent_component(&config_path) || path_has_parent_component(&site_mcp_fabric) {
        return false;
    }
    config_path
        .strip_prefix(&site_mcp_fabric)
        .is_some_and(|suffix| suffix.starts_with('/') && suffix.len() > 1)
}

fn path_has_parent_component(path: &str) -> bool {
    path.split('/').any(|component| component == "..")
}

fn normalize_lexical_path(path: &str) -> String {
    let mut normalized = path.trim().replace('\\', "/");
    while normalized.ends_with('/') && normalized.len() > 1 {
        normalized.pop();
    }
    if cfg!(windows) {
        normalized = normalized.to_ascii_lowercase();
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mcp_runtime_env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(semantic_key, value)| {
                let env_key = match *semantic_key {
                    "fabric_enabled" => mcp_fabric_env_var(),
                    "config" => mcp_config_env_var(),
                    "fabric" => site_mcp_fabric_env_var(),
                    unexpected => panic!("unknown MCP runtime env semantic key: {unexpected}"),
                };
                (env_key.to_string(), value.to_string())
            })
            .collect()
    }

    #[test]
    fn mcp_runtime_is_disabled_without_explicit_admission_flag() {
        let config = McpRuntimeConfig::from_env_map(&mcp_runtime_env(&[
            ("config", "D:/site/.ai/mcp/config.json"),
            ("fabric", "D:/site/.ai/mcp"),
        ]));

        assert_eq!(config.status, McpRuntimeAdmissionStatus::Disabled);
        assert_eq!(config.status.as_str(), "disabled");
        assert!(!config.mcp_fabric_access_enabled);
        assert_eq!(config.config_path, None);
    }

    #[test]
    fn mcp_runtime_requires_config_and_fabric_when_enabled() {
        let missing_config = McpRuntimeConfig::from_env_map(&mcp_runtime_env(&[
            ("fabric_enabled", "true"),
            ("fabric", "D:/site/.ai/mcp"),
        ]));
        assert_eq!(missing_config.status, McpRuntimeAdmissionStatus::Refused);
        assert_eq!(
            missing_config.refusal_reason.as_deref(),
            Some("missing_mcp_config")
        );

        let missing_fabric = McpRuntimeConfig::from_env_map(&mcp_runtime_env(&[
            ("fabric_enabled", "true"),
            ("config", "D:/site/.ai/mcp/config.json"),
        ]));
        assert_eq!(missing_fabric.status, McpRuntimeAdmissionStatus::Refused);
        assert_eq!(
            missing_fabric.refusal_reason.as_deref(),
            Some("missing_site_mcp_fabric")
        );
    }

    #[test]
    fn mcp_runtime_refuses_config_outside_site_mcp_fabric() {
        let config = McpRuntimeConfig::from_env_map(&mcp_runtime_env(&[
            ("fabric_enabled", "true"),
            ("config", "D:/other/.ai/mcp/config.json"),
            ("fabric", "D:/site/.ai/mcp"),
        ]));

        assert_eq!(config.status, McpRuntimeAdmissionStatus::Refused);
        assert_eq!(
            config.refusal_reason.as_deref(),
            Some("mcp_config_outside_site_mcp_fabric")
        );
        assert!(!config.mcp_fabric_access_enabled);
    }

    #[test]
    fn mcp_runtime_refuses_parent_traversal_inside_site_mcp_fabric_prefix() {
        let config = McpRuntimeConfig::from_env_map(&mcp_runtime_env(&[
            ("fabric_enabled", "true"),
            ("config", "D:/site/.ai/mcp/../outside/config.json"),
            ("fabric", "D:/site/.ai/mcp"),
        ]));

        assert_eq!(config.status, McpRuntimeAdmissionStatus::Refused);
        assert_eq!(
            config.refusal_reason.as_deref(),
            Some("mcp_config_outside_site_mcp_fabric")
        );
        assert!(!config.mcp_fabric_access_enabled);
    }

    #[test]
    fn mcp_runtime_refuses_unreadable_config_when_readiness_is_checked() {
        let config = McpRuntimeConfig::from_env_map_with_config_readiness(
            &mcp_runtime_env(&[
                ("fabric_enabled", "true"),
                ("config", "D:/site/.ai/mcp/config.json"),
                ("fabric", "D:/site/.ai/mcp"),
            ]),
            |_| Err("mcp_config_unreadable".to_string()),
        );

        assert_eq!(config.status, McpRuntimeAdmissionStatus::Refused);
        assert_eq!(
            config.refusal_reason.as_deref(),
            Some("mcp_config_unreadable")
        );
        assert!(!config.mcp_fabric_access_enabled);
    }

    #[test]
    fn mcp_runtime_configures_explicit_config_and_fabric() {
        let config = McpRuntimeConfig::from_env_map(&mcp_runtime_env(&[
            ("fabric_enabled", "yes"),
            ("config", "D:/site/.ai/mcp/config.json"),
            ("fabric", "D:/site/.ai/mcp"),
        ]));

        assert_eq!(config.status, McpRuntimeAdmissionStatus::Configured);
        assert_eq!(config.status.as_str(), "configured");
        assert!(config.mcp_fabric_access_enabled);
        assert_eq!(
            config.config_path.as_deref(),
            Some("D:/site/.ai/mcp/config.json")
        );
        assert_eq!(config.site_mcp_fabric.as_deref(), Some("D:/site/.ai/mcp"));
    }
}
