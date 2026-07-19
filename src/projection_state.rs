#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    Idle,
    Active,
}

impl Default for TurnState {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone)]
pub struct SessionEvidenceContext {
    pub carrier_session_id: String,
    pub agent_id: String,
    pub site_id: String,
    pub site_root: String,
}
