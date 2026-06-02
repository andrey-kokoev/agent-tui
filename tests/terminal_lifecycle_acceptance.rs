use narada_agent_tui::terminal_lifecycle::{TerminalLifecycle, TerminalLifecycleState};

#[test]
fn lifecycle_acceptance_guard_leaves_after_normal_exit() {
    let mut lifecycle = TerminalLifecycle::new();

    let value = lifecycle
        .run_guarded(|| Ok("exit_requested".to_string()))
        .expect("normal guarded exit succeeds");

    assert_eq!(value, "exit_requested");
    assert_eq!(lifecycle.state(), TerminalLifecycleState::Normal);
}

#[test]
fn lifecycle_acceptance_guard_leaves_after_render_error() {
    let mut lifecycle = TerminalLifecycle::new();

    let error = lifecycle
        .run_guarded(|| Err::<(), String>("terminal_draw_failed:test".to_string()))
        .expect_err("render error is returned");

    assert_eq!(error, "terminal_draw_failed:test");
    assert_eq!(lifecycle.state(), TerminalLifecycleState::Normal);
}

#[test]
fn lifecycle_acceptance_guard_refuses_nested_tui_entry() {
    let mut lifecycle = TerminalLifecycle::new();
    lifecycle.mark_entered().expect("outer enter succeeds");

    let error = lifecycle
        .run_guarded(|| Ok(()))
        .expect_err("nested guarded enter is rejected");

    assert_eq!(error, "terminal_lifecycle_already_active");
    assert_eq!(lifecycle.state(), TerminalLifecycleState::TuiActive);
    lifecycle.mark_left().expect("outer leave succeeds");
}
