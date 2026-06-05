use crate::carrier_protocol::{
    ControlInputEvent, DeliveryMode, HoldCondition, InputEvent, SessionEvent, SessionEventKind,
    SourceKind, session_event_schema,
};
use serde_json::{Value, json};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnState {
    Idle,
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionDecision {
    AdmitNow {
        input_event_id: String,
    },
    QueueForTurnBoundary {
        input_event_id: String,
    },
    HoldForComposerClear {
        input_event_id: String,
        directive_id: Option<String>,
        original_delivery_mode: DeliveryMode,
    },
}

#[derive(Debug, Clone)]
pub struct SessionEvidenceContext {
    pub carrier_session_id: String,
    pub agent_id: String,
    pub site_id: String,
    pub site_root: String,
}

#[derive(Debug, Clone)]
pub struct QueuedInputRelease {
    pub input_event_id: String,
    pub directive_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedInputSummary {
    pub index: usize,
    pub input_event_id: String,
    pub source_kind: SourceKind,
    pub created_at: String,
    pub content_preview: String,
}

#[derive(Debug, Default)]
pub struct InputQueue {
    turn_state: TurnState,
    active_started_at: Option<String>,
    ready_for_current_turn: VecDeque<InputEvent>,
    queued_for_turn_boundary: VecDeque<InputEvent>,
    held_for_composer_clear: VecDeque<InputEvent>,
}

impl Default for TurnState {
    fn default() -> Self {
        Self::Idle
    }
}

impl InputQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn turn_state(&self) -> TurnState {
        self.turn_state
    }

    pub fn set_turn_state(&mut self, state: TurnState) {
        if state == TurnState::Idle {
            self.active_started_at = None;
        }
        self.turn_state = state;
    }

    pub fn set_turn_active_at(&mut self, started_at: impl Into<String>) {
        self.turn_state = TurnState::Active;
        self.active_started_at = Some(started_at.into());
    }

    pub fn set_turn_idle(&mut self) {
        self.turn_state = TurnState::Idle;
        self.active_started_at = None;
    }

    pub fn active_turn_age_label(&self, now: &str) -> Option<String> {
        if self.turn_state != TurnState::Active {
            return None;
        }
        let started_at = self.active_started_at.as_deref()?;
        elapsed_label_between(started_at, now)
    }

    pub fn queued_count(&self) -> usize {
        self.queued_for_turn_boundary.len()
    }

    pub fn queued_summaries(&self) -> Vec<QueuedInputSummary> {
        self.queued_for_turn_boundary
            .iter()
            .enumerate()
            .map(|(index, input)| QueuedInputSummary {
                index: index + 1,
                input_event_id: input.event_id.clone(),
                source_kind: input.source_kind.clone(),
                created_at: input.created_at.clone(),
                content_preview: input.content.lines().next().unwrap_or_default().to_string(),
            })
            .collect()
    }

    pub fn drop_queued_by_index(&mut self, index: usize) -> Option<InputEvent> {
        if index == 0 || index > self.queued_for_turn_boundary.len() {
            return None;
        }
        self.queued_for_turn_boundary.remove(index - 1)
    }

    pub fn clear_queued_operator_inputs(&mut self) -> Vec<InputEvent> {
        let mut retained = VecDeque::new();
        let mut dropped = Vec::new();
        while let Some(input) = self.queued_for_turn_boundary.pop_front() {
            if input.source_kind == SourceKind::Operator {
                dropped.push(input);
            } else {
                retained.push_back(input);
            }
        }
        self.queued_for_turn_boundary = retained;
        dropped
    }

    pub fn held_count(&self) -> usize {
        self.held_for_composer_clear.len()
    }

    pub fn oldest_held_age_label(&self, now: &str) -> Option<String> {
        let created_at = self.held_for_composer_clear.front()?.created_at.as_str();
        elapsed_label_between(created_at, now)
    }

    pub fn admit_control_event(
        &mut self,
        event: ControlInputEvent,
        composer_has_draft: bool,
    ) -> AdmissionDecision {
        self.admit_input_event(event.input, composer_has_draft)
    }

    pub fn admit_input_event(
        &mut self,
        input: InputEvent,
        composer_has_draft: bool,
    ) -> AdmissionDecision {
        let input_event_id = input.event_id.clone();

        if input.hold_condition == Some(HoldCondition::ComposerClearRequired) && composer_has_draft
        {
            let directive_id = input.directive_id.clone();
            let original_delivery_mode = input.delivery_mode.clone();
            self.held_for_composer_clear.push_back(input);
            return AdmissionDecision::HoldForComposerClear {
                input_event_id,
                directive_id,
                original_delivery_mode,
            };
        }

        if self.turn_state == TurnState::Active
            || input.delivery_mode == DeliveryMode::AdmitAfterActiveTurn
        {
            self.queued_for_turn_boundary.push_back(input);
            return AdmissionDecision::QueueForTurnBoundary { input_event_id };
        }

        self.ready_for_current_turn.push_back(input);
        AdmissionDecision::AdmitNow { input_event_id }
    }

    pub fn release_held_when_composer_clear(&mut self) -> Vec<QueuedInputRelease> {
        let mut released = Vec::new();
        while let Some(input) = self.held_for_composer_clear.pop_front() {
            released.push(QueuedInputRelease {
                input_event_id: input.event_id.clone(),
                directive_id: input.directive_id.clone(),
            });
            self.queued_for_turn_boundary.push_back(input);
        }
        released
    }

    pub fn next_ready_input(&mut self) -> Option<InputEvent> {
        if self.turn_state == TurnState::Active {
            return None;
        }
        self.ready_for_current_turn
            .pop_front()
            .or_else(|| self.queued_for_turn_boundary.pop_front())
    }
}

impl AdmissionDecision {
    pub fn to_session_event(
        &self,
        context: &SessionEvidenceContext,
        event_id: impl Into<String>,
        occurred_at: impl Into<String>,
    ) -> SessionEvent {
        let occurred_at = occurred_at.into();
        match self {
            AdmissionDecision::AdmitNow { input_event_id } => session_event(
                context,
                SessionEventKind::InputAdmittedToTurn,
                event_id,
                occurred_at,
                json!({ "input_event_id": input_event_id }),
            ),
            AdmissionDecision::QueueForTurnBoundary { input_event_id } => session_event(
                context,
                SessionEventKind::InputQueuedForTurnBoundary,
                event_id,
                occurred_at,
                json!({
                    "input_event_id": input_event_id,
                    "queue_state": "queued_for_turn_boundary"
                }),
            ),
            AdmissionDecision::HoldForComposerClear {
                input_event_id,
                directive_id,
                original_delivery_mode,
            } => {
                let event_occurred_at = occurred_at.clone();
                let mut payload = json!({
                    "input_event_id": input_event_id,
                    "held_at": occurred_at,
                    "held_reason": "composer_nonempty",
                    "original_delivery_mode": original_delivery_mode
                });
                insert_optional_directive_id(&mut payload, directive_id);
                session_event(
                    context,
                    SessionEventKind::SystemDirectiveHeld,
                    event_id,
                    event_occurred_at,
                    payload,
                )
            }
        }
    }
}

impl QueuedInputRelease {
    pub fn to_session_event(
        &self,
        context: &SessionEvidenceContext,
        event_id: impl Into<String>,
        occurred_at: impl Into<String>,
    ) -> SessionEvent {
        let occurred_at = occurred_at.into();
        let event_occurred_at = occurred_at.clone();
        let mut payload = json!({
            "input_event_id": self.input_event_id,
            "released_at": occurred_at,
            "released_reason": "composer_clear"
        });
        insert_optional_directive_id(&mut payload, &self.directive_id);
        session_event(
            context,
            SessionEventKind::SystemDirectiveReleased,
            event_id,
            event_occurred_at,
            payload,
        )
    }
}

pub(crate) fn elapsed_label_between(start: &str, end: &str) -> Option<String> {
    let elapsed = timestamp_elapsed_seconds(start, end)?;
    Some(format_elapsed_seconds(elapsed))
}

fn timestamp_elapsed_seconds(start: &str, end: &str) -> Option<u64> {
    let start = parse_utc_timestamp_seconds(start)?;
    let end = parse_utc_timestamp_seconds(end)?;
    Some(end.saturating_sub(start))
}

fn parse_utc_timestamp_seconds(value: &str) -> Option<u64> {
    if value.len() < 20 || !value.ends_with('Z') {
        return None;
    }
    let year = value.get(0..4)?.parse::<i64>().ok()?;
    let month = value.get(5..7)?.parse::<i64>().ok()?;
    let day = value.get(8..10)?.parse::<i64>().ok()?;
    let hour = value.get(11..13)?.parse::<i64>().ok()?;
    let minute = value.get(14..16)?.parse::<i64>().ok()?;
    let second = value.get(17..19)?.parse::<i64>().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=60).contains(&second)
    {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
    u64::try_from(seconds).ok()
}

fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * month_prime + 2) / 5 + day - 1;
    if !(0..=365).contains(&doy) {
        return None;
    }
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn format_elapsed_seconds(seconds: u64) -> String {
    if seconds < 60 {
        return format!("{seconds}s");
    }
    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;
    if minutes < 60 {
        return format!("{minutes}m {remaining_seconds}s");
    }
    let hours = minutes / 60;
    let remaining_minutes = minutes % 60;
    format!("{hours}h {remaining_minutes}m")
}

fn insert_optional_directive_id(payload: &mut Value, directive_id: &Option<String>) {
    if let (Some(map), Some(directive_id)) = (payload.as_object_mut(), directive_id) {
        map.insert(
            "directive_id".to_string(),
            Value::String(directive_id.clone()),
        );
    }
}

fn session_event(
    context: &SessionEvidenceContext,
    kind: SessionEventKind,
    event_id: impl Into<String>,
    occurred_at: impl Into<String>,
    payload: serde_json::Value,
) -> SessionEvent {
    SessionEvent {
        schema: session_event_schema().to_string(),
        event_kind: kind,
        event_id: event_id.into(),
        occurred_at: occurred_at.into(),
        carrier_session_id: context.carrier_session_id.clone(),
        agent_id: context.agent_id.clone(),
        site_id: context.site_id.clone(),
        site_root: context.site_root.clone(),
        payload,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::carrier_protocol::{
        parse_control_input_event, parse_input_event, parse_session_event,
    };

    const CONTROL_FIXTURE: &str =
        include_str!("../../narada/packages/carrier-protocol/fixtures/control-input-event.json");
    const INPUT_FIXTURE: &str =
        include_str!("../../narada/packages/carrier-protocol/fixtures/input-event.json");

    fn evidence_context() -> SessionEvidenceContext {
        SessionEvidenceContext {
            carrier_session_id: "carrier_fixture_1".to_string(),
            agent_id: "sonar.resident".to_string(),
            site_id: "narada-sonar".to_string(),
            site_root: "D:/code/narada.sonar".to_string(),
        }
    }

    #[test]
    fn admits_idle_current_turn_input_now() {
        let input = parse_input_event(INPUT_FIXTURE).expect("input fixture parses");
        let mut queue = InputQueue::new();

        let decision = queue.admit_input_event(input, false);
        assert!(matches!(decision, AdmissionDecision::AdmitNow { .. }));
        assert_eq!(queue.queued_count(), 0);
        assert_eq!(queue.held_count(), 0);
    }

    #[test]
    fn queues_input_when_turn_is_active() {
        let input = parse_input_event(INPUT_FIXTURE).expect("input fixture parses");
        let mut queue = InputQueue::new();
        queue.set_turn_state(TurnState::Active);

        let decision = queue.admit_input_event(input, false);
        assert!(matches!(
            decision,
            AdmissionDecision::QueueForTurnBoundary { .. }
        ));
        assert_eq!(queue.queued_count(), 1);
        assert!(queue.next_ready_input().is_none());

        queue.set_turn_state(TurnState::Idle);
        assert!(queue.next_ready_input().is_some());
    }

    #[test]
    fn reports_active_turn_age_when_started_at_is_known() {
        let mut queue = InputQueue::new();

        queue.set_turn_active_at("2026-05-30T00:00:00.000Z");

        assert_eq!(
            queue.active_turn_age_label("2026-05-30T00:01:12.000Z"),
            Some("1m 12s".to_string())
        );
        queue.set_turn_idle();
        assert_eq!(
            queue.active_turn_age_label("2026-05-30T00:01:13.000Z"),
            None
        );
    }

    #[test]
    fn holds_system_directive_until_composer_is_clear() {
        let event = parse_control_input_event(CONTROL_FIXTURE).expect("control fixture parses");
        assert_eq!(event.input.source_kind, SourceKind::System);
        let mut queue = InputQueue::new();

        let decision = queue.admit_control_event(event, true);
        assert!(matches!(
            decision,
            AdmissionDecision::HoldForComposerClear { .. }
        ));
        assert_eq!(queue.held_count(), 1);
        assert_eq!(queue.queued_count(), 0);

        let released = queue.release_held_when_composer_clear();
        assert_eq!(released.len(), 1);
        assert_eq!(queue.held_count(), 0);
        assert_eq!(queue.queued_count(), 1);
        assert!(queue.next_ready_input().is_some());
    }

    #[test]
    fn reports_oldest_held_system_directive_age() {
        let mut event = parse_control_input_event(CONTROL_FIXTURE).expect("control fixture parses");
        event.input.created_at = "2026-05-30T00:00:00.000Z".to_string();
        let mut queue = InputQueue::new();

        let _decision = queue.admit_control_event(event, true);

        assert_eq!(
            queue.oldest_held_age_label("2026-05-30T00:01:14.000Z"),
            Some("1m 14s".to_string())
        );
    }

    #[test]
    fn after_active_turn_delivery_queues_even_when_idle() {
        let mut input = parse_input_event(INPUT_FIXTURE).expect("input fixture parses");
        input.delivery_mode = DeliveryMode::AdmitAfterActiveTurn;
        let mut queue = InputQueue::new();

        let decision = queue.admit_input_event(input, false);
        assert!(matches!(
            decision,
            AdmissionDecision::QueueForTurnBoundary { .. }
        ));
        assert_eq!(queue.queued_count(), 1);
    }

    #[test]
    fn admission_decision_emits_valid_session_event() {
        let input = parse_input_event(INPUT_FIXTURE).expect("input fixture parses");
        let mut queue = InputQueue::new();
        queue.set_turn_state(TurnState::Active);
        let decision = queue.admit_input_event(input, false);

        let event = decision.to_session_event(
            &evidence_context(),
            "session_event_queue_1",
            "2026-05-30T00:00:02.000Z",
        );
        assert_eq!(
            event.event_kind,
            SessionEventKind::InputQueuedForTurnBoundary
        );
        assert_eq!(event.payload["queue_state"], "queued_for_turn_boundary");

        let serialized = serde_json::to_string(&event).expect("session event serializes");
        assert!(parse_session_event(&serialized).is_ok());
    }

    #[test]
    fn queued_summaries_are_one_based_and_preview_first_line() {
        let mut input = parse_input_event(INPUT_FIXTURE).expect("input fixture parses");
        input.content = "first line\nsecond line".to_string();
        let mut queue = InputQueue::new();
        queue.set_turn_state(TurnState::Active);
        let _decision = queue.admit_input_event(input, false);

        let summaries = queue.queued_summaries();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].index, 1);
        assert_eq!(summaries[0].content_preview, "first line");
    }

    #[test]
    fn drops_queued_input_by_one_based_index() {
        let input = parse_input_event(INPUT_FIXTURE).expect("input fixture parses");
        let mut queue = InputQueue::new();
        queue.set_turn_state(TurnState::Active);
        let _decision = queue.admit_input_event(input, false);

        let dropped = queue.drop_queued_by_index(1).expect("queued input drops");
        assert_eq!(dropped.event_id, "input_fixture_1");
        assert_eq!(queue.queued_count(), 0);
        assert!(queue.drop_queued_by_index(1).is_none());
    }

    #[test]
    fn clear_queued_operator_inputs_retains_system_inputs() {
        let operator_input = parse_input_event(INPUT_FIXTURE).expect("input fixture parses");
        let system_event =
            parse_control_input_event(CONTROL_FIXTURE).expect("control fixture parses");
        let mut queue = InputQueue::new();
        queue.set_turn_state(TurnState::Active);
        let _operator = queue.admit_input_event(operator_input, false);
        let _system = queue.admit_control_event(system_event, false);

        let dropped = queue.clear_queued_operator_inputs();
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].source_kind, SourceKind::Operator);
        assert_eq!(queue.queued_count(), 1);
    }

    #[test]
    fn release_emits_valid_session_event() {
        let event = parse_control_input_event(CONTROL_FIXTURE).expect("control fixture parses");
        let mut queue = InputQueue::new();
        let _decision = queue.admit_control_event(event, true);
        let released = queue.release_held_when_composer_clear();

        let event = released[0].to_session_event(
            &evidence_context(),
            "session_event_release_1",
            "2026-05-30T00:00:03.000Z",
        );
        assert_eq!(event.event_kind, SessionEventKind::SystemDirectiveReleased);
        assert_eq!(event.payload["released_reason"], "composer_clear");
        assert_eq!(event.payload["released_at"], "2026-05-30T00:00:03.000Z");

        let serialized = serde_json::to_string(&event).expect("session event serializes");
        assert!(parse_session_event(&serialized).is_ok());
    }
}
