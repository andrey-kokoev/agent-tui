use crate::input_queue::TurnState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerViewInput {
    pub identity: String,
    pub draft_text: String,
    pub turn_state: TurnState,
    pub queued_operator_notes: usize,
    pub held_system_directives: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerViewModel {
    pub prompt_label: String,
    pub draft_text: String,
    pub draft_is_empty: bool,
    pub submit_hint: String,
    pub interrupt_hint: String,
    pub queued_note_affordance: String,
    pub held_directive_affordance: String,
}

pub fn build_composer_view(input: &ComposerViewInput) -> ComposerViewModel {
    let draft_is_empty = input.draft_text.trim().is_empty();
    let prompt_label = match input.turn_state {
        TurnState::Idle => format!("operator -> {}>", input.identity),
        TurnState::Active => format!("operator note -> {}>", input.identity),
    };
    let submit_hint = match input.turn_state {
        TurnState::Idle => "Enter submits turn".to_string(),
        TurnState::Active => "Enter queues note".to_string(),
    };
    let interrupt_hint = match input.turn_state {
        TurnState::Idle => "Esc interrupt".to_string(),
        TurnState::Active => "Esc interrupt".to_string(),
    };

    ComposerViewModel {
        prompt_label,
        draft_text: input.draft_text.clone(),
        draft_is_empty,
        submit_hint,
        interrupt_hint,
        queued_note_affordance: queued_note_affordance(input.queued_operator_notes),
        held_directive_affordance: held_directive_affordance(input.held_system_directives),
    }
}

fn queued_note_affordance(count: usize) -> String {
    match count {
        0 => "queued notes: 0".to_string(),
        1 => "queued notes: 1".to_string(),
        _ => format!("queued notes: {count}"),
    }
}

fn held_directive_affordance(count: usize) -> String {
    match count {
        0 => "held system directives: 0".to_string(),
        1 => "held system directives: 1".to_string(),
        _ => format!("held system directives: {count}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(turn_state: TurnState) -> ComposerViewInput {
        ComposerViewInput {
            identity: "sonar.resident".to_string(),
            draft_text: "run startup sequence".to_string(),
            turn_state,
            queued_operator_notes: 2,
            held_system_directives: 1,
        }
    }

    #[test]
    fn builds_idle_prompt_for_new_operator_turn() {
        let model = build_composer_view(&input(TurnState::Idle));

        assert_eq!(model.prompt_label, "operator -> sonar.resident>");
        assert_eq!(model.submit_hint, "Enter submits turn");
        assert_eq!(model.interrupt_hint, "Esc interrupt");
        assert_eq!(model.draft_text, "run startup sequence");
        assert!(!model.draft_is_empty);
    }

    #[test]
    fn builds_active_prompt_for_queued_operator_note() {
        let model = build_composer_view(&input(TurnState::Active));

        assert_eq!(model.prompt_label, "operator note -> sonar.resident>");
        assert_eq!(model.submit_hint, "Enter queues note");
        assert_eq!(model.interrupt_hint, "Esc interrupt");
    }

    #[test]
    fn reports_empty_draft_after_trimming_whitespace() {
        let model = build_composer_view(&ComposerViewInput {
            draft_text: "  \t  ".to_string(),
            ..input(TurnState::Idle)
        });

        assert!(model.draft_is_empty);
    }

    #[test]
    fn reports_queue_and_held_counts() {
        let model = build_composer_view(&input(TurnState::Active));

        assert_eq!(model.queued_note_affordance, "queued notes: 2");
        assert_eq!(model.held_directive_affordance, "held system directives: 1");
    }

    #[test]
    fn reports_zero_queue_and_held_counts() {
        let model = build_composer_view(&ComposerViewInput {
            queued_operator_notes: 0,
            held_system_directives: 0,
            ..input(TurnState::Idle)
        });

        assert_eq!(model.queued_note_affordance, "queued notes: 0");
        assert_eq!(model.held_directive_affordance, "held system directives: 0");
    }
}
