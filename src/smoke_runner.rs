use crate::composer_draft::ComposerDraftState;
use crate::input_queue::SessionEvidenceContext;
use crate::interactive_runtime::{
    AgentTuiInteractiveRuntime, InteractiveStepClock, InteractiveStepResult,
};
use crate::provider_adapter_admission::ProviderAdapterAdmission;
use crate::provider_dispatch::ProviderDispatchStub;
use crate::provider_runtime_config::ProviderRuntimeConfig;
use crate::runtime_clock::RuntimeClock;
use crate::transcript_store::TranscriptIngestSummary;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AgentTuiSmokeStepConfig {
    pub identity: String,
    pub session: String,
    pub site_root: PathBuf,
    pub control_jsonl: PathBuf,
    pub session_jsonl: PathBuf,
    pub composer_has_draft: bool,
}

pub struct AgentTuiSmokeSession {
    runtime: AgentTuiInteractiveRuntime,
    clock: RuntimeClock,
}

impl AgentTuiSmokeSession {
    pub fn new(config: &AgentTuiSmokeStepConfig) -> Result<Self, String> {
        Self::with_provider_runtime_config(config, ProviderRuntimeConfig::disabled())
    }

    pub fn with_provider_runtime_config(
        config: &AgentTuiSmokeStepConfig,
        provider_runtime_config: ProviderRuntimeConfig,
    ) -> Result<Self, String> {
        let provider_adapter_admission =
            ProviderAdapterAdmission::from_runtime_config(&provider_runtime_config, None);
        Self::with_provider_runtime_config_and_adapter_admission(
            config,
            provider_runtime_config,
            provider_adapter_admission,
        )
    }

    pub fn with_provider_runtime_config_and_adapter_admission(
        config: &AgentTuiSmokeStepConfig,
        provider_runtime_config: ProviderRuntimeConfig,
        provider_adapter_admission: ProviderAdapterAdmission,
    ) -> Result<Self, String> {
        let runtime_posture = crate::status_view_model::RuntimePostureState::from_runtime_configs(
            &provider_runtime_config,
            &provider_adapter_admission,
            &crate::mcp_runtime_config::McpRuntimeConfig::disabled(),
            &crate::terminal_runtime_config::TerminalRuntimeConfig::disabled(),
        );
        Ok(Self {
            runtime: AgentTuiInteractiveRuntime::with_provider_adapter_and_state(
                config.identity.clone(),
                config.session.clone(),
                config.control_jsonl.clone(),
                config.session_jsonl.clone(),
                evidence_context(config),
                Box::new(
                    ProviderDispatchStub::with_runtime_config_and_adapter_admission(
                        provider_runtime_config,
                        provider_adapter_admission,
                    ),
                ),
                runtime_posture,
            ),
            clock: RuntimeClock::system_now()?,
        })
    }

    pub fn run_step(&mut self, composer_has_draft: bool) -> Result<InteractiveStepResult, String> {
        let step_clock = InteractiveStepClock::from(self.clock.next_step_clock());
        let draft = smoke_step_draft(composer_has_draft);
        self.runtime.run_step(&draft, &step_clock)
    }

    pub fn record_interrupt(&mut self) -> Result<TranscriptIngestSummary, String> {
        let step_clock = InteractiveStepClock::from(self.clock.next_step_clock());
        self.runtime
            .coordinator_mut()
            .record_composer_interrupt(&step_clock.input)?;
        self.runtime.ingest_transcript()
    }
}

pub fn run_interactive_smoke_step(
    config: &AgentTuiSmokeStepConfig,
) -> Result<InteractiveStepResult, String> {
    let mut session = AgentTuiSmokeSession::new(config)?;
    session.run_step(config.composer_has_draft)
}

pub fn run_interactive_smoke_step_with_provider_runtime_config(
    config: &AgentTuiSmokeStepConfig,
    provider_runtime_config: ProviderRuntimeConfig,
) -> Result<InteractiveStepResult, String> {
    let mut session =
        AgentTuiSmokeSession::with_provider_runtime_config(config, provider_runtime_config)?;
    session.run_step(config.composer_has_draft)
}

pub fn run_interactive_smoke_step_with_provider_runtime_config_and_adapter_admission(
    config: &AgentTuiSmokeStepConfig,
    provider_runtime_config: ProviderRuntimeConfig,
    provider_adapter_admission: ProviderAdapterAdmission,
) -> Result<InteractiveStepResult, String> {
    let mut session = AgentTuiSmokeSession::with_provider_runtime_config_and_adapter_admission(
        config,
        provider_runtime_config,
        provider_adapter_admission,
    )?;
    session.run_step(config.composer_has_draft)
}

pub fn interactive_smoke_step_summary_lines(result: &InteractiveStepResult) -> Vec<String> {
    vec![
        format!(
            "control_evidence_written: {}",
            result.control_evidence_written
        ),
        format!("parse_errors: {}", result.parse_errors),
        format!("released_held: {}", result.released_held),
        format!(
            "completed_turn: {}",
            result
                .completed_turn
                .as_ref()
                .map(|turn| turn.turn_id.as_str())
                .unwrap_or("none")
        ),
        format!("transcript_projected: {}", result.transcript.projected),
        format!("transcript_ignored: {}", result.transcript.ignored),
        format!("transcript_duplicate: {}", result.transcript.duplicate),
        format!("transcript_total_items: {}", result.transcript.total_items),
    ]
}

fn smoke_step_draft(composer_has_draft: bool) -> ComposerDraftState {
    if composer_has_draft {
        ComposerDraftState {
            text: "draft".to_string(),
        }
    } else {
        ComposerDraftState::default()
    }
}

fn evidence_context(config: &AgentTuiSmokeStepConfig) -> SessionEvidenceContext {
    SessionEvidenceContext {
        carrier_session_id: config.session.clone(),
        agent_id: config.identity.clone(),
        site_id: derive_site_id(&config.identity),
        site_root: config.site_root.display().to_string(),
    }
}

fn derive_site_id(identity: &str) -> String {
    identity
        .rsplit_once('.')
        .map(|(site, _)| site.to_string())
        .unwrap_or_else(|| "unknown-site".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interactive_runtime::CompletedTurnSummary;

    #[test]
    fn summary_lines_are_stable() {
        let result = InteractiveStepResult {
            control_evidence_written: 1,
            parse_errors: 0,
            released_held: 0,
            completed_turn: Some(CompletedTurnSummary {
                turn_id: "turn_1".to_string(),
                input_event_id: "input_fixture_1".to_string(),
                evidence_written: 3,
            }),
            transcript: TranscriptIngestSummary {
                projected: 2,
                ignored: 3,
                duplicate: 1,
                total_items: 2,
            },
        };

        assert_eq!(
            interactive_smoke_step_summary_lines(&result),
            vec![
                "control_evidence_written: 1".to_string(),
                "parse_errors: 0".to_string(),
                "released_held: 0".to_string(),
                "completed_turn: turn_1".to_string(),
                "transcript_projected: 2".to_string(),
                "transcript_ignored: 3".to_string(),
                "transcript_duplicate: 1".to_string(),
                "transcript_total_items: 2".to_string(),
            ]
        );
    }

    #[test]
    fn derives_site_id_from_agent_identity() {
        let config = AgentTuiSmokeStepConfig {
            identity: "sonar.resident".to_string(),
            session: "carrier_fixture_1".to_string(),
            site_root: PathBuf::from("D:/code/narada.sonar"),
            control_jsonl: PathBuf::from("control.jsonl"),
            session_jsonl: PathBuf::from("session.jsonl"),
            composer_has_draft: false,
        };

        assert_eq!(evidence_context(&config).site_id, "sonar");
    }
}
