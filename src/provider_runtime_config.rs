use std::collections::BTreeMap;

use crate::provider_adapter_contract::provider_adapter_contract;

fn provider_execution_env_var() -> &'static str {
    provider_adapter_contract()
        .provider_execution_env_var
        .as_str()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderRuntimeAdmissionStatus {
    Disabled,
    Configured,
    Refused,
}

impl ProviderRuntimeAdmissionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Configured => "configured",
            Self::Refused => "refused",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRuntimeConfig {
    pub status: ProviderRuntimeAdmissionStatus,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub stream: bool,
    pub refusal_reason: Option<String>,
}

impl ProviderRuntimeConfig {
    pub fn disabled() -> Self {
        Self {
            status: ProviderRuntimeAdmissionStatus::Disabled,
            provider: None,
            model: None,
            thinking: None,
            stream: false,
            refusal_reason: None,
        }
    }

    pub fn from_env_map(env: &BTreeMap<String, String>) -> Self {
        let contract = provider_adapter_contract();
        if !env_flag_enabled(env.get(provider_execution_env_var())) {
            return Self::disabled();
        }

        let provider = trimmed_nonempty(env.get(&contract.intelligence_provider_env_var));
        let model = trimmed_nonempty(env.get(&contract.ai_model_env_var));
        let thinking = trimmed_nonempty(env.get(&contract.ai_thinking_env_var));
        let stream = !matches!(
            env.get(&contract.ai_stream_env_var)
                .map(|value| value.trim().to_ascii_lowercase()),
            Some(value) if matches!(value.as_str(), "0" | "false" | "off" | "no")
        );

        let Some(provider) = provider else {
            return Self::refused("missing_provider");
        };
        let Some(model) = model else {
            return Self::refused("missing_model");
        };
        if !contract
            .admitted_providers
            .iter()
            .any(|admitted| admitted == &provider)
        {
            return Self::refused(format!("provider_not_admitted:{provider}"));
        }

        Self {
            status: ProviderRuntimeAdmissionStatus::Configured,
            provider: Some(provider),
            model: Some(model),
            thinking,
            stream,
            refusal_reason: None,
        }
    }

    fn refused(reason: impl Into<String>) -> Self {
        Self {
            status: ProviderRuntimeAdmissionStatus::Refused,
            provider: None,
            model: None,
            thinking: None,
            stream: false,
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

    fn admitted_provider() -> &'static str {
        provider_adapter_contract()
            .admitted_providers
            .first()
            .expect("provider contract has at least one admitted provider")
            .as_str()
    }

    const UNKNOWN_PROVIDER_FIXTURE: &str = "unknown-provider";

    fn unknown_provider_refusal() -> String {
        format!("provider_not_admitted:{UNKNOWN_PROVIDER_FIXTURE}")
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
                    "stream" => &contract.ai_stream_env_var,
                    unexpected => panic!("unknown provider runtime env semantic key: {unexpected}"),
                };
                (env_key.clone(), value.to_string())
            })
            .collect()
    }

    #[test]
    fn provider_runtime_is_disabled_without_explicit_admission_flag() {
        let config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));

        assert_eq!(config.status, ProviderRuntimeAdmissionStatus::Disabled);
        assert_eq!(config.status.as_str(), "disabled");
        assert_eq!(config.provider, None);
        assert_eq!(config.model, None);
    }

    #[test]
    fn provider_runtime_requires_provider_and_model_when_enabled() {
        let missing_provider = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("model", "gpt-5.5"),
        ]));
        assert_eq!(
            missing_provider.status,
            ProviderRuntimeAdmissionStatus::Refused
        );
        assert_eq!(
            missing_provider.refusal_reason.as_deref(),
            Some("missing_provider")
        );

        let missing_model = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
        ]));
        assert_eq!(
            missing_model.status,
            ProviderRuntimeAdmissionStatus::Refused
        );
        assert_eq!(
            missing_model.refusal_reason.as_deref(),
            Some("missing_model")
        );
    }

    #[test]
    fn provider_runtime_rejects_unknown_provider() {
        let config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", UNKNOWN_PROVIDER_FIXTURE),
            ("model", "gpt-5.5"),
        ]));

        assert_eq!(config.status, ProviderRuntimeAdmissionStatus::Refused);
        assert_eq!(
            config.refusal_reason.as_deref(),
            Some(unknown_provider_refusal().as_str())
        );
    }

    #[test]
    fn provider_runtime_configures_explicit_provider_model_without_admitting_execution() {
        let config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "yes"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
            ("thinking", "medium"),
            ("stream", "off"),
        ]));

        assert_eq!(config.status, ProviderRuntimeAdmissionStatus::Configured);
        assert_eq!(config.status.as_str(), "configured");
        assert_eq!(config.refusal_reason, None);
        assert_eq!(config.provider.as_deref(), Some(admitted_provider()));
        assert_eq!(config.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(config.thinking.as_deref(), Some("medium"));
        assert!(!config.stream);
    }
}
