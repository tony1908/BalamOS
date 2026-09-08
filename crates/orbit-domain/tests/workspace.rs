use orbit_domain::{LifecycleState, PermissionProfile, ResourceLimits, Workspace, WorkspaceError};
use std::path::PathBuf;

fn workspace() -> Workspace {
    Workspace::new(
        "Demo".into(),
        PathBuf::from("/projects/demo"),
        PermissionProfile::Workspace,
        ResourceLimits::new(2.0, 3_221_225_472, 1_024, 20_000_000_000).unwrap(),
    )
}

#[test]
fn new_workspace_starts_creating() {
    assert_eq!(workspace().state, LifecycleState::Creating);
}

#[test]
fn workspace_defaults_model_and_reasoning_effort_to_unset() {
    let workspace = workspace();
    assert_eq!(workspace.model, None);
    assert_eq!(workspace.reasoning_effort, None);
}

#[test]
fn workspace_lifecycle_transitions_succeed() {
    let mut workspace = workspace();
    for state in [
        LifecycleState::Starting,
        LifecycleState::Running,
        LifecycleState::Stopping,
        LifecycleState::Stopped,
    ] {
        workspace.transition(state).unwrap();
    }
    assert_eq!(workspace.state, LifecycleState::Stopped);
}

#[test]
fn running_cannot_jump_to_deleted() {
    let mut workspace = workspace();
    workspace.transition(LifecycleState::Starting).unwrap();
    workspace.transition(LifecycleState::Running).unwrap();
    assert_eq!(
        workspace.transition(LifecycleState::Deleted),
        Err(WorkspaceError::InvalidTransition {
            from: LifecycleState::Running,
            to: LifecycleState::Deleted,
        })
    );
    assert_eq!(workspace.state, LifecycleState::Running);
}

#[test]
fn starting_can_transition_to_stopping() {
    let mut workspace = workspace();
    workspace.transition(LifecycleState::Starting).unwrap();
    workspace.transition(LifecycleState::Stopping).unwrap();
}

#[test]
fn starting_cannot_transition_to_stopped() {
    let mut workspace = workspace();
    workspace.transition(LifecycleState::Starting).unwrap();
    assert!(workspace.transition(LifecycleState::Stopped).is_err());
}

#[test]
fn resource_limits_reject_zero_for_each_field() {
    assert_eq!(
        ResourceLimits::new(0.0, 1, 1, 1),
        Err(WorkspaceError::InvalidResourceLimits)
    );
    assert_eq!(
        ResourceLimits::new(f64::NAN, 1, 1, 1),
        Err(WorkspaceError::InvalidResourceLimits)
    );
    assert_eq!(
        ResourceLimits::new(1.0, 0, 1, 1),
        Err(WorkspaceError::InvalidResourceLimits)
    );
    assert_eq!(
        ResourceLimits::new(1.0, 1, 0, 1),
        Err(WorkspaceError::InvalidResourceLimits)
    );
    assert_eq!(
        ResourceLimits::new(1.0, 1, 1, 0),
        Err(WorkspaceError::InvalidResourceLimits)
    );
}

#[test]
fn resource_limits_reject_non_finite_cpus() {
    for cpus in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        assert_eq!(
            ResourceLimits::new(cpus, 1, 1, 1),
            Err(WorkspaceError::InvalidResourceLimits)
        );
    }
}

#[test]
fn workspace_starts_without_runtime_metadata() {
    assert_eq!(workspace().runtime, None);
}
