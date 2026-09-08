use crate::{agent_manager::AgentManager, workspace_manager::WorkspaceManager};
use orbit_domain::{LifecycleState, Routine, RoutineRunStatus};
use orbit_store::{StoreError, WorkspaceStore};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

pub fn routine_due(routine: &Routine, now_ms: u64) -> bool {
    match routine.last_run_unix_ms {
        None => true,
        Some(last) => now_ms.saturating_sub(last) >= (routine.interval_minutes as u64) * 60_000,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

pub fn spawn_scheduler(
    store: Arc<WorkspaceStore>,
    agents: Arc<AgentManager>,
    manager: Arc<WorkspaceManager>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        const TICK: Duration = Duration::from_secs(30);
        loop {
            tokio::time::sleep(TICK).await;
            let Ok(routines) = store.list_all_routines() else {
                continue;
            };
            for mut routine in routines {
                let now = now_ms();
                if !routine.enabled || !routine_due(&routine, now) {
                    continue;
                }
                match manager.get(routine.workspace_id).await {
                    Ok(workspace) if matches!(workspace.state, LifecycleState::Running) => {}
                    _ => continue,
                }
                let result = agents
                    .scheduled_prompt(routine.workspace_id, routine.instruction.clone())
                    .await;
                routine.last_run_unix_ms = Some(now_ms());
                routine.last_status = Some(match &result {
                    Ok(_) => RoutineRunStatus::Ok,
                    Err(crate::agent_manager::AgentManagerError::Governance(reasons))
                        if reasons.iter().any(|reason| {
                            matches!(
                                reason,
                                orbit_domain::GovernanceBlockedReason::ScheduledRunsDisabled
                            )
                        }) =>
                    {
                        RoutineRunStatus::Skipped
                    }
                    Err(_) => RoutineRunStatus::Failed,
                });
                routine.last_skip_reason = result.as_ref().err().map(ToString::to_string);
                let _ = store.upsert_routine(&routine);
            }
        }
    })
}

#[cfg(test)]
mod scheduler_tests_placeholder {
    use super::*;

    fn routine(last_run_unix_ms: Option<u64>, interval_minutes: u32) -> Routine {
        Routine {
            id: Uuid::new_v4(),
            workspace_id: Uuid::new_v4(),
            name: "routine".into(),
            instruction: "instruction".into(),
            interval_minutes,
            enabled: true,
            last_run_unix_ms,
            last_status: None,
            last_skip_reason: None,
        }
    }

    #[test]
    fn routine_without_previous_run_is_due() {
        assert!(routine_due(&routine(None, 1), 0));
    }

    #[test]
    fn routine_run_at_now_is_not_due() {
        assert!(!routine_due(&routine(Some(1_000), 1), 1_000));
    }

    #[test]
    fn routine_far_in_the_past_is_due() {
        assert!(routine_due(&routine(Some(0), 1), 60_000));
    }
}

#[derive(Debug, Error)]
pub enum RoutineManagerError {
    #[error("invalid routine request")]
    InvalidRequest,
    #[error("routine was not found")]
    NotFound,
    #[error("routine store failed: {0}")]
    Store(#[from] StoreError),
}

#[derive(Clone)]
pub struct RoutineManager {
    store: Arc<WorkspaceStore>,
}

impl RoutineManager {
    pub fn new(store: Arc<WorkspaceStore>) -> Self {
        Self { store }
    }

    fn validate(
        name: &str,
        instruction: &str,
        interval_minutes: u32,
    ) -> Result<(), RoutineManagerError> {
        if name.trim().is_empty()
            || name.trim().len() > 200
            || instruction.trim().is_empty()
            || instruction.len() > 8000
            || interval_minutes < 1
        {
            return Err(RoutineManagerError::InvalidRequest);
        }
        Ok(())
    }

    pub async fn create(
        &self,
        workspace_id: Uuid,
        name: String,
        instruction: String,
        interval_minutes: u32,
        enabled: bool,
    ) -> Result<Routine, RoutineManagerError> {
        Self::validate(&name, &instruction, interval_minutes)?;
        let routine = Routine {
            id: Uuid::new_v4(),
            workspace_id,
            name: name.trim().into(),
            instruction,
            interval_minutes,
            enabled,
            last_run_unix_ms: None,
            last_status: None,
            last_skip_reason: None,
        };
        self.store.upsert_routine(&routine)?;
        Ok(routine)
    }

    pub async fn list(&self, workspace_id: Uuid) -> Result<Vec<Routine>, RoutineManagerError> {
        Ok(self.store.list_routines(workspace_id)?)
    }

    pub async fn update(
        &self,
        id: Uuid,
        name: String,
        instruction: String,
        interval_minutes: u32,
        enabled: bool,
    ) -> Result<Routine, RoutineManagerError> {
        Self::validate(&name, &instruction, interval_minutes)?;
        let mut routine = self
            .store
            .get_routine(id)?
            .ok_or(RoutineManagerError::NotFound)?;
        routine.name = name.trim().into();
        routine.instruction = instruction;
        routine.interval_minutes = interval_minutes;
        routine.enabled = enabled;
        self.store.upsert_routine(&routine)?;
        Ok(routine)
    }

    pub async fn delete(&self, id: Uuid) -> Result<(), RoutineManagerError> {
        self.store.delete_routine(id)?;
        Ok(())
    }
}
