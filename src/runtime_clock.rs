use crate::runtime_coordinator::RuntimeCoordinatorClock;
use crate::runtime_step::RuntimeStepClock;
use crate::turn_coordinator::TurnCoordinatorClock;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct RuntimeClock {
    occurred_at: String,
    refresh_occurred_at_per_step: bool,
    step_index: u64,
    input_event_prefix: String,
    turn_event_prefix: String,
    turn_id_prefix: String,
}

impl RuntimeClock {
    pub fn fixed(occurred_at: impl Into<String>) -> Self {
        Self::with_occurred_at(occurred_at)
    }

    pub fn system_now() -> Result<Self, String> {
        let mut clock = Self::with_occurred_at(system_time_utc_millis(SystemTime::now())?);
        clock.refresh_occurred_at_per_step = true;
        Ok(clock)
    }

    fn with_occurred_at(occurred_at: impl Into<String>) -> Self {
        Self {
            occurred_at: occurred_at.into(),
            refresh_occurred_at_per_step: false,
            step_index: 1,
            input_event_prefix: "session_event_runtime".to_string(),
            turn_event_prefix: "session_event_turn".to_string(),
            turn_id_prefix: "turn".to_string(),
        }
    }

    pub fn next_step_clock(&mut self) -> RuntimeStepClock {
        if self.refresh_occurred_at_per_step {
            if let Ok(occurred_at) = system_time_utc_millis(SystemTime::now()) {
                self.occurred_at = occurred_at;
            }
        }
        let index = self.step_index;
        self.step_index += 1;
        RuntimeStepClock {
            input: RuntimeCoordinatorClock {
                occurred_at: self.occurred_at.clone(),
                event_id_prefix: format!("{}_step{}", self.input_event_prefix, index),
            },
            turn: TurnCoordinatorClock {
                occurred_at: self.occurred_at.clone(),
                event_id_prefix: format!("{}_step{}", self.turn_event_prefix, index),
                turn_id_prefix: format!("{}_step{}", self.turn_id_prefix, index),
            },
        }
    }
}

fn system_time_utc_millis(time: SystemTime) -> Result<String, String> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system_time_before_unix_epoch:{error}"))?;
    let total_seconds = duration.as_secs() as i64;
    let millis = duration.subsec_millis();
    let days = total_seconds.div_euclid(86_400);
    let seconds_of_day = total_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z"
    ))
}

fn civil_from_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_unix_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn advances_step_prefixes_without_changing_timestamp() {
        let mut clock = RuntimeClock::fixed("2026-05-30T00:00:02.000Z");
        let first = clock.next_step_clock();
        let second = clock.next_step_clock();

        assert_eq!(first.input.occurred_at, "2026-05-30T00:00:02.000Z");
        assert_eq!(first.input.event_id_prefix, "session_event_runtime_step1");
        assert_eq!(first.turn.event_id_prefix, "session_event_turn_step1");
        assert_eq!(first.turn.turn_id_prefix, "turn_step1");
        assert_eq!(second.input.event_id_prefix, "session_event_runtime_step2");
        assert_eq!(second.turn.turn_id_prefix, "turn_step2");
    }

    #[test]
    fn formats_unix_epoch_as_protocol_timestamp() {
        assert_eq!(
            system_time_utc_millis(UNIX_EPOCH).expect("epoch formats"),
            "1970-01-01T00:00:00.000Z"
        );
    }

    #[test]
    fn formats_known_utc_millisecond_timestamp() {
        let time = UNIX_EPOCH + Duration::from_millis(1_779_929_600_123);
        assert_eq!(
            system_time_utc_millis(time).expect("time formats"),
            "2026-05-28T00:53:20.123Z"
        );
    }

    #[test]
    fn system_now_constructs_protocol_shaped_clock() {
        let mut clock = RuntimeClock::system_now().expect("system time works");
        let step = clock.next_step_clock();
        assert_eq!(step.input.occurred_at.len(), 24);
        assert!(step.input.occurred_at.ends_with('Z'));
    }

    #[test]
    fn refreshing_clock_updates_timestamp_for_each_step() {
        let mut clock = RuntimeClock::with_occurred_at("1970-01-01T00:00:00.000Z");
        clock.refresh_occurred_at_per_step = true;

        let step = clock.next_step_clock();

        assert_ne!(step.input.occurred_at, "1970-01-01T00:00:00.000Z");
        assert_eq!(step.input.occurred_at, step.turn.occurred_at);
    }
}
