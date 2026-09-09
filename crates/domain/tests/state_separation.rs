//! QUAL-EV-0102 — "State-transition tests reject impossible conflation such as
//! command failure = thread failure." Session, task, run, turn and step each
//! have their own machine; a failure at one level never implies a failure at
//! another, and the reducers refuse the conflated transitions.

use modbit_domain::run::{OwnerLocation, Run, RunEvent, RunState};
use modbit_domain::session::{Session, SessionEvent, SessionState};
use modbit_domain::state::StateMachine;
use modbit_domain::step::{RunStep, StepEvent, StepState, StepType};
use modbit_domain::task::{Task, TaskEvent, TaskOrigin, TaskState};
use modbit_domain::turn::{Turn, TurnEvent, TurnState};
use modbit_domain::{
    RunId, RunStepId, SessionId, SpaceId, TaskId, TenantId, Timestamp, TurnId, UserId, WorkspaceId,
};

#[test]
fn a_failed_command_step_does_not_fail_the_turn_the_run_the_task_or_the_session() {
    let at = Timestamp(1);
    let session = Session::create(
        SessionId::new(),
        &SessionEvent::SessionCreated {
            tenant_id: TenantId::new(),
            user_id: UserId::new(),
            space_id: SpaceId::new(),
        },
        at,
    )
    .unwrap();
    let mut task = Task::create(
        TaskId::new(),
        &TaskEvent::TaskCreated {
            session_id: session.session_id,
            goal_text: "g".into(),
            workspace_id: WorkspaceId::new(),
            base_revision: None,
            execution_profile: "local_trusted".into(),
            policy_profile_id: None,
            origin: TaskOrigin::Cli,
        },
        at,
    )
    .unwrap();
    task.apply(&TaskEvent::TaskQueued, at).unwrap();
    task.apply(&TaskEvent::TaskStarted, at).unwrap();
    let mut run = Run::create(
        RunId::new(),
        &RunEvent::RunCreated {
            task_id: task.task_id,
            attempt: 1,
            owner_location: OwnerLocation::Local,
            kernel_lease_generation: 1,
        },
        at,
    )
    .unwrap();
    run.apply(&RunEvent::RunStarted, at).unwrap();
    let mut turn = Turn::create(
        TurnId::new(),
        &TurnEvent::TurnPrepared {
            run_id: run.run_id,
            ordinal: 1,
        },
        at,
    )
    .unwrap();
    turn.apply(
        &TurnEvent::ModelInvocationStarted {
            model_route: serde_json::json!({}),
        },
        at,
    )
    .unwrap();
    turn.apply(
        &TurnEvent::ModelInvocationCompleted {
            requested_actions: true,
        },
        at,
    )
    .unwrap();
    let mut step = RunStep::create(
        RunStepId::new(),
        &StepEvent::StepScheduled {
            turn_id: turn.turn_id,
            step_type: StepType::ToolCall,
            ordinal: 1,
            input_ref: None,
        },
        at,
    )
    .unwrap();
    step.apply(&StepEvent::StepStarted, at).unwrap();

    // The shell command fails (docs/02 MOD-EXEC-001: command failure is not turn failure).
    step.apply(
        &StepEvent::StepFailed {
            failure_code: "EXIT_1".into(),
            output_ref: None,
        },
        at,
    )
    .unwrap();
    assert_eq!(step.state, StepState::Failed);
    assert_eq!(
        turn.state,
        TurnState::Executing,
        "turn keeps executing: it can repair"
    );
    assert_eq!(run.state, RunState::Running);
    assert_eq!(task.state, TaskState::Running);
    assert_eq!(session.state, SessionState::Active);

    // The turn can go back to the model for a repair attempt.
    turn.apply(
        &TurnEvent::ModelInvocationStarted {
            model_route: serde_json::json!({}),
        },
        at,
    )
    .unwrap();
    assert_eq!(turn.state, TurnState::Streaming);

    // Conflations are rejected: a step cannot "complete the task"; a task
    // cannot be failed by a step event; a turn failure event applied to a
    // task is not even representable, and terminal turn state does not
    // propagate.
    turn.apply(
        &TurnEvent::TurnFailed {
            failure_code: "MODEL_ERROR".into(),
        },
        at,
    )
    .unwrap();
    assert_eq!(turn.state, TurnState::Failed);
    assert_eq!(
        run.state,
        RunState::Running,
        "run decides separately whether to retry the turn"
    );
    assert_eq!(task.state, TaskState::Running);
    assert!(
        !TaskState::Running.can_transition(TaskState::Completed),
        "a task cannot complete without review"
    );
    assert!(
        !StepState::Failed.can_transition(StepState::Succeeded),
        "a failed step stays failed; repair is a new step"
    );
    assert!(!RunState::Running.can_transition(RunState::Pending));
    assert!(!SessionState::Archived.can_transition(SessionState::Active));
}

#[test]
fn every_machine_has_exactly_its_own_terminal_states() {
    assert!(SessionState::Archived.is_terminal() && !SessionState::Suspended.is_terminal());
    assert!(TaskState::Cancelled.is_terminal() && !TaskState::ReadyForReview.is_terminal());
    assert!(RunState::Failed.is_terminal() && !RunState::Suspended.is_terminal());
    assert!(TurnState::Interrupted.is_terminal() && !TurnState::Verifying.is_terminal());
    assert!(StepState::UnknownOutcome.is_terminal() && !StepState::Running.is_terminal());
}
