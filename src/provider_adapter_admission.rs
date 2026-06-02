use crate::provider_adapter_contract::provider_adapter_contract;
use crate::provider_runtime_config::{ProviderRuntimeAdmissionStatus, ProviderRuntimeConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderAdapterKind {
    Scripted,
    CodexSubscription,
}

impl ProviderAdapterKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if value == scripted_provider_adapter_kind() {
            return Ok(Self::Scripted);
        }
        if value == production_provider_adapter_kind() {
            return Ok(Self::CodexSubscription);
        }
        Err(format!("unknown_provider_adapter:{value}"))
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Scripted => scripted_provider_adapter_kind(),
            Self::CodexSubscription => production_provider_adapter_kind(),
        }
    }

    pub fn execution_implemented(&self) -> bool {
        match self {
            Self::Scripted => true,
            Self::CodexSubscription => {
                provider_adapter_contract().production_provider_adapter_implemented
            }
        }
    }
}

pub fn scripted_provider_adapter_kind() -> &'static str {
    provider_adapter_contract()
        .scripted_provider_adapter_kind
        .as_str()
}

pub fn production_provider_adapter_kind() -> &'static str {
    provider_adapter_contract()
        .production_provider_adapter_kind
        .as_str()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderAdapterAdmissionStatus {
    Disabled,
    Refused,
    ConfiguredWithoutAdapter,
    Admitted,
}

impl ProviderAdapterAdmissionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Refused => "refused",
            Self::ConfiguredWithoutAdapter => "configured_without_adapter",
            Self::Admitted => "admitted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderAdapterAdmission {
    pub status: ProviderAdapterAdmissionStatus,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub adapter_kind: Option<String>,
    pub provider_execution_enabled: bool,
    pub refusal_reason: Option<String>,
}

impl ProviderAdapterAdmission {
    pub fn try_admit(
        runtime_config: &ProviderRuntimeConfig,
        adapter_kind: ProviderAdapterKind,
    ) -> Result<Self, String> {
        if runtime_config.status != ProviderRuntimeAdmissionStatus::Configured {
            return Err("provider_runtime_not_configured".to_string());
        }
        if !adapter_kind.execution_implemented() {
            return Err(format!(
                "provider_adapter_not_implemented:{}",
                adapter_kind.as_str()
            ));
        }
        Ok(Self {
            status: ProviderAdapterAdmissionStatus::Admitted,
            provider: runtime_config.provider.clone(),
            model: runtime_config.model.clone(),
            adapter_kind: Some(adapter_kind.as_str().to_string()),
            provider_execution_enabled: true,
            refusal_reason: None,
        })
    }

    pub fn from_runtime_config(
        runtime_config: &ProviderRuntimeConfig,
        adapter_kind: Option<&str>,
    ) -> Self {
        match runtime_config.status {
            ProviderRuntimeAdmissionStatus::Disabled => Self {
                status: ProviderAdapterAdmissionStatus::Disabled,
                provider: None,
                model: None,
                adapter_kind: None,
                provider_execution_enabled: false,
                refusal_reason: None,
            },
            ProviderRuntimeAdmissionStatus::Refused => Self {
                status: ProviderAdapterAdmissionStatus::Refused,
                provider: runtime_config.provider.clone(),
                model: runtime_config.model.clone(),
                adapter_kind: None,
                provider_execution_enabled: false,
                refusal_reason: runtime_config.refusal_reason.clone(),
            },
            ProviderRuntimeAdmissionStatus::Configured => {
                let adapter_kind = adapter_kind
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                if let Some(adapter_kind) = adapter_kind {
                    match ProviderAdapterKind::parse(adapter_kind) {
                        Ok(adapter_kind) if adapter_kind.execution_implemented() => Self {
                            status: ProviderAdapterAdmissionStatus::Admitted,
                            provider: runtime_config.provider.clone(),
                            model: runtime_config.model.clone(),
                            adapter_kind: Some(adapter_kind.as_str().to_string()),
                            provider_execution_enabled: true,
                            refusal_reason: None,
                        },
                        Ok(adapter_kind) => Self {
                            status: ProviderAdapterAdmissionStatus::Refused,
                            provider: runtime_config.provider.clone(),
                            model: runtime_config.model.clone(),
                            adapter_kind: Some(adapter_kind.as_str().to_string()),
                            provider_execution_enabled: false,
                            refusal_reason: Some(format!(
                                "provider_adapter_not_implemented:{}",
                                adapter_kind.as_str()
                            )),
                        },
                        Err(reason) => Self {
                            status: ProviderAdapterAdmissionStatus::Refused,
                            provider: runtime_config.provider.clone(),
                            model: runtime_config.model.clone(),
                            adapter_kind: Some(adapter_kind.to_string()),
                            provider_execution_enabled: false,
                            refusal_reason: Some(reason),
                        },
                    }
                } else {
                    Self {
                        status: ProviderAdapterAdmissionStatus::ConfiguredWithoutAdapter,
                        provider: runtime_config.provider.clone(),
                        model: runtime_config.model.clone(),
                        adapter_kind: None,
                        provider_execution_enabled: false,
                        refusal_reason: Some("provider_adapter_not_admitted".to_string()),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

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
                    unexpected => panic!("unknown provider runtime env semantic key: {unexpected}"),
                };
                (env_key.clone(), value.to_string())
            })
            .collect()
    }

    #[test]
    fn parses_known_provider_adapter_kind() {
        let scripted_input = format!(" {} ", scripted_provider_adapter_kind());
        let scripted = ProviderAdapterKind::parse(&scripted_input).expect("scripted kind parses");
        assert_eq!(scripted, ProviderAdapterKind::Scripted);
        assert_eq!(scripted.as_str(), scripted_provider_adapter_kind());
        assert!(scripted.execution_implemented());

        let codex_input = format!(" {} ", production_provider_adapter_kind());
        let codex = ProviderAdapterKind::parse(&codex_input).expect("codex kind parses");
        assert_eq!(codex, ProviderAdapterKind::CodexSubscription);
        assert_eq!(codex.as_str(), production_provider_adapter_kind());
        assert!(codex.execution_implemented());
        assert_eq!(
            ProviderAdapterKind::parse("unknown_adapter").unwrap_err(),
            "unknown_provider_adapter:unknown_adapter"
        );
    }

    #[test]
    fn disabled_runtime_disables_adapter_admission() {
        let runtime_config = ProviderRuntimeConfig::disabled();
        let admission = ProviderAdapterAdmission::from_runtime_config(
            &runtime_config,
            Some(production_provider_adapter_kind()),
        );

        assert_eq!(admission.status, ProviderAdapterAdmissionStatus::Disabled);
        assert_eq!(admission.status.as_str(), "disabled");
        assert!(!admission.provider_execution_enabled);
        assert_eq!(admission.adapter_kind, None);
    }

    #[test]
    fn refused_runtime_refuses_adapter_admission() {
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", UNKNOWN_PROVIDER_FIXTURE),
            ("model", "gpt-5.5"),
        ]));
        let admission = ProviderAdapterAdmission::from_runtime_config(
            &runtime_config,
            Some(production_provider_adapter_kind()),
        );

        assert_eq!(admission.status, ProviderAdapterAdmissionStatus::Refused);
        assert!(!admission.provider_execution_enabled);
        assert_eq!(
            admission.refusal_reason.as_deref(),
            Some(unknown_provider_refusal().as_str())
        );
    }

    #[test]
    fn configured_runtime_without_adapter_is_not_execution_admitted() {
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));
        let admission = ProviderAdapterAdmission::from_runtime_config(&runtime_config, None);

        assert_eq!(
            admission.status,
            ProviderAdapterAdmissionStatus::ConfiguredWithoutAdapter
        );
        assert_eq!(admission.status.as_str(), "configured_without_adapter");
        assert_eq!(admission.provider.as_deref(), Some(admitted_provider()));
        assert_eq!(admission.model.as_deref(), Some("gpt-5.5"));
        assert!(!admission.provider_execution_enabled);
        assert_eq!(
            admission.refusal_reason.as_deref(),
            Some("provider_adapter_not_admitted")
        );
    }

    #[test]
    fn configured_runtime_with_unknown_adapter_kind_is_refused_as_unknown() {
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));
        let admission =
            ProviderAdapterAdmission::from_runtime_config(&runtime_config, Some("unknown_adapter"));

        assert_eq!(admission.status, ProviderAdapterAdmissionStatus::Refused);
        assert!(!admission.provider_execution_enabled);
        assert_eq!(admission.adapter_kind.as_deref(), Some("unknown_adapter"));
        assert_eq!(
            admission.refusal_reason.as_deref(),
            Some("unknown_provider_adapter:unknown_adapter")
        );
    }

    #[test]
    fn configured_runtime_with_production_adapter_kind_is_admitted_when_implemented() {
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));
        let admission = ProviderAdapterAdmission::from_runtime_config(
            &runtime_config,
            Some(production_provider_adapter_kind()),
        );

        assert_eq!(admission.status, ProviderAdapterAdmissionStatus::Admitted);
        assert_eq!(admission.status.as_str(), "admitted");
        assert!(admission.provider_execution_enabled);
        assert_eq!(
            admission.adapter_kind.as_deref(),
            Some(production_provider_adapter_kind())
        );
        assert_eq!(admission.refusal_reason, None);
    }

    #[test]
    fn admitted_adapter_requires_configured_runtime_and_enables_scripted_and_production_execution()
    {
        let runtime_config = ProviderRuntimeConfig::from_env_map(&provider_runtime_env(&[
            ("execution_enabled", "true"),
            ("provider", admitted_provider()),
            ("model", "gpt-5.5"),
        ]));

        let admission =
            ProviderAdapterAdmission::try_admit(&runtime_config, ProviderAdapterKind::Scripted)
                .expect("configured runtime admits scripted adapter");

        assert_eq!(admission.status, ProviderAdapterAdmissionStatus::Admitted);
        assert!(admission.provider_execution_enabled);
        assert_eq!(admission.provider.as_deref(), Some(admitted_provider()));
        assert_eq!(
            admission.adapter_kind.as_deref(),
            Some(scripted_provider_adapter_kind())
        );
        assert_eq!(admission.refusal_reason, None);
        let codex_admission = ProviderAdapterAdmission::try_admit(
            &runtime_config,
            ProviderAdapterKind::CodexSubscription,
        )
        .expect("configured runtime admits production codex adapter");
        assert_eq!(
            codex_admission.status,
            ProviderAdapterAdmissionStatus::Admitted
        );
        assert!(codex_admission.provider_execution_enabled);
        assert_eq!(
            codex_admission.adapter_kind.as_deref(),
            Some(production_provider_adapter_kind())
        );
        let disabled = ProviderRuntimeConfig::disabled();
        assert_eq!(
            ProviderAdapterAdmission::try_admit(&disabled, ProviderAdapterKind::Scripted)
                .unwrap_err(),
            "provider_runtime_not_configured"
        );
    }
}
