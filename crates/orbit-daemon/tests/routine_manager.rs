use orbit_daemon::routine_manager::{RoutineManager, RoutineManagerError};
use orbit_store::WorkspaceStore;
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn routine_manager_crud_and_validation() {
    let manager = RoutineManager::new(Arc::new(WorkspaceStore::in_memory().unwrap()));
    let workspace_id = Uuid::new_v4();

    assert!(matches!(
        manager
            .create(workspace_id, "   ".into(), "instruction".into(), 1, true)
            .await,
        Err(RoutineManagerError::InvalidRequest)
    ));
    assert!(matches!(
        manager
            .create(workspace_id, "name".into(), "   ".into(), 1, true)
            .await,
        Err(RoutineManagerError::InvalidRequest)
    ));
    assert!(matches!(
        manager
            .create(workspace_id, "name".into(), "instruction".into(), 0, true)
            .await,
        Err(RoutineManagerError::InvalidRequest)
    ));

    let routine = manager
        .create(
            workspace_id,
            "  Morning  ".into(),
            "instruction".into(),
            15,
            true,
        )
        .await
        .unwrap();
    assert_eq!(routine.name, "Morning");
    assert_eq!(
        manager.list(workspace_id).await.unwrap(),
        vec![routine.clone()]
    );

    let updated = manager
        .update(
            routine.id,
            "Evening".into(),
            "new instruction".into(),
            30,
            false,
        )
        .await
        .unwrap();
    assert_eq!(updated.name, "Evening");
    assert_eq!(updated.interval_minutes, 30);
    assert!(!updated.enabled);

    manager.delete(routine.id).await.unwrap();
    assert!(manager.list(workspace_id).await.unwrap().is_empty());
    assert!(matches!(
        manager
            .update(routine.id, "name".into(), "instruction".into(), 1, true)
            .await,
        Err(RoutineManagerError::NotFound)
    ));
}
