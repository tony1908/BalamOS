use orbit_domain::{
    Harness, LifecycleState, PermissionProfile, ResourceLimits, Routine, RoutineRunStatus,
    RuntimeMetadata, RuntimeSecret, Workspace,
};
use orbit_store::WorkspaceStore;
use rusqlite::Connection;
use std::path::PathBuf;
use uuid::Uuid;

fn demo_workspace() -> Workspace {
    Workspace::new(
        "Demo".to_owned(),
        PathBuf::from("/projects/demo"),
        PermissionProfile::Workspace,
        ResourceLimits::new(2.0, 1024, 64, 2048).expect("valid resource limits"),
    )
}

#[test]
fn upsert_and_get_workspace() {
    let store = WorkspaceStore::in_memory().expect("create in-memory store");
    let expected = demo_workspace();

    store.upsert(&expected).expect("upsert workspace");

    assert_eq!(
        store.get(expected.id).expect("get workspace"),
        Some(expected)
    );
}

#[test]
fn list_returns_inserted_workspaces() {
    let store = WorkspaceStore::in_memory().expect("create in-memory store");
    let first = demo_workspace();
    let second = demo_workspace();

    store.upsert(&first).expect("upsert first workspace");
    store.upsert(&second).expect("upsert second workspace");

    assert_eq!(store.list().expect("list workspaces").len(), 2);
}

#[test]
fn file_database_survives_reopen() {
    let directory = tempfile::tempdir().expect("create temporary directory");
    let path = directory.path().join("orbit.db");
    let expected = demo_workspace();

    {
        let store = WorkspaceStore::open(&path).expect("open database");
        store.upsert(&expected).expect("upsert workspace");
    }

    let store = WorkspaceStore::open(&path).expect("reopen database");
    assert_eq!(
        store.get(expected.id).expect("get workspace"),
        Some(expected)
    );
}

#[test]
fn workspace_ids_are_stored_as_text() {
    let directory = tempfile::tempdir().expect("create temporary directory");
    let path = directory.path().join("orbit.db");
    let workspace = demo_workspace();

    {
        let store = WorkspaceStore::open(&path).expect("open database");
        store.upsert(&workspace).expect("upsert workspace");
    }

    let connection = Connection::open(&path).expect("open database directly");
    let id_type: String = connection
        .query_row("SELECT typeof(id) FROM workspaces LIMIT 1", [], |row| {
            row.get(0)
        })
        .expect("read stored id type");
    assert_eq!(id_type, "text");
}

#[test]
fn compare_and_swap_only_updates_matching_snapshot() {
    let store = WorkspaceStore::in_memory().unwrap();
    let original = demo_workspace();
    store.upsert(&original).unwrap();
    let mut stopped = original.clone();
    stopped.transition(LifecycleState::Starting).unwrap();
    stopped.transition(LifecycleState::Running).unwrap();
    stopped.transition(LifecycleState::Stopping).unwrap();
    stopped.transition(LifecycleState::Stopped).unwrap();
    store.upsert(&stopped).unwrap();
    let mut a = stopped.clone();
    a.transition(LifecycleState::Starting).unwrap();
    let mut b = stopped.clone();
    b.transition(LifecycleState::Deleting).unwrap();
    assert!(store.compare_and_swap(&stopped, &a).unwrap());
    assert!(!store.compare_and_swap(&stopped, &b).unwrap());
    assert_eq!(store.get(original.id).unwrap(), Some(a));
}

#[test]
fn runtime_metadata_round_trips_through_workspace_operations() {
    let store = WorkspaceStore::in_memory().unwrap();
    let mut workspace = demo_workspace();
    workspace.runtime = Some(RuntimeMetadata {
        container_id: "cid".into(),
        container_name: "cname".into(),
        volume_name: "vname".into(),
        upstream_port: 8080,
        image_ref: "img:tag".into(),
    });
    store.upsert(&workspace).unwrap();
    assert_eq!(store.get(workspace.id).unwrap(), Some(workspace.clone()));
    assert_eq!(store.list().unwrap(), vec![workspace]);
}

#[test]
fn harness_round_trips_through_workspace_list() {
    let store = WorkspaceStore::in_memory().unwrap();
    let mut opencode_workspace = demo_workspace();
    opencode_workspace.harness = Harness::Opencode;
    let codex_workspace = demo_workspace();

    store.upsert(&opencode_workspace).unwrap();
    store.upsert(&codex_workspace).unwrap();

    let listed = store.list().unwrap();
    let opencode = listed
        .iter()
        .find(|workspace| workspace.id == opencode_workspace.id)
        .unwrap();
    let codex = listed
        .iter()
        .find(|workspace| workspace.id == codex_workspace.id)
        .unwrap();
    assert_eq!(opencode.harness, Harness::Opencode);
    assert_eq!(codex.harness, Harness::Codex);
}

#[test]
fn legacy_workspace_documents_without_harness_default_to_codex() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("legacy.db");
    let workspace = demo_workspace();
    let legacy = serde_json::to_string(&workspace)
        .unwrap()
        .replace(",\"harness\":\"codex\"", "");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "CREATE TABLE workspaces (id TEXT PRIMARY KEY NOT NULL, document TEXT NOT NULL)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO workspaces (id, document) VALUES (?1, ?2)",
            rusqlite::params![workspace.id.to_string(), legacy],
        )
        .unwrap();
    drop(connection);

    let store = WorkspaceStore::open(&path).unwrap();
    assert_eq!(
        store.get(workspace.id).unwrap().unwrap().harness,
        Harness::Codex
    );
}

#[test]
fn legacy_workspace_documents_without_runtime_remain_readable() {
    let document = r#"{"id":"00000000-0000-0000-0000-000000000001","name":"Demo","host_path":"/demo","profile":"workspace","resources":{"cpus":1.0,"memory_bytes":1,"pids":1,"soft_disk_bytes":1},"state":"creating"}"#;
    let workspace: Workspace = serde_json::from_str(document).unwrap();
    assert_eq!(workspace.runtime, None);
}

#[test]
fn compare_and_swap_updates_legacy_workspace_documents() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("legacy.db");
    let workspace = demo_workspace();
    let legacy = serde_json::to_string(&workspace)
        .unwrap()
        .replace(",\"runtime\":null", "");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "CREATE TABLE workspaces (id TEXT PRIMARY KEY NOT NULL, document TEXT NOT NULL)",
            [],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO workspaces (id, document) VALUES (?1, ?2)",
            rusqlite::params![workspace.id.to_string(), legacy],
        )
        .unwrap();
    drop(connection);
    let store = WorkspaceStore::open(&path).unwrap();
    let mut replacement = store.get(workspace.id).unwrap().unwrap();
    replacement.transition(LifecycleState::Starting).unwrap();
    assert!(store.compare_and_swap(&workspace, &replacement).unwrap());
    assert_eq!(store.get(workspace.id).unwrap(), Some(replacement));
}

#[test]
fn runtime_secret_round_trips_and_deletes() {
    let store = WorkspaceStore::in_memory().unwrap();
    let secret = RuntimeSecret {
        workspace_id: Uuid::new_v4(),
        webtop_password: "password".into(),
    };
    assert!(!format!("{secret:?}").contains("password"));
    assert_eq!(store.get_runtime_secret(secret.workspace_id).unwrap(), None);
    store.put_runtime_secret(&secret).unwrap();
    let workspace = Workspace {
        id: secret.workspace_id,
        ..demo_workspace()
    };
    store.upsert(&workspace).unwrap();
    let returned = store.get(workspace.id).unwrap().unwrap();
    assert!(
        !serde_json::to_string(&returned)
            .unwrap()
            .contains("password")
    );
    assert!(!format!("{returned:?}").contains("password"));
    let listed = store.list().unwrap();
    assert_eq!(listed, vec![workspace]);
    assert!(
        !serde_json::to_string(&listed[0])
            .unwrap()
            .contains("password")
    );
    assert!(!format!("{:?}", listed[0]).contains("password"));
    assert_eq!(
        store.get_runtime_secret(secret.workspace_id).unwrap(),
        Some(secret.clone())
    );
    store.delete_runtime_secret(secret.workspace_id).unwrap();
    assert_eq!(store.get_runtime_secret(secret.workspace_id).unwrap(), None);
}

#[test]
fn deleting_workspace_does_not_expose_or_implicitly_assume_secret_cascade() {
    let store = WorkspaceStore::in_memory().unwrap();
    let workspace = demo_workspace();
    let secret = RuntimeSecret {
        workspace_id: workspace.id,
        webtop_password: "pw".into(),
    };
    store.upsert(&workspace).unwrap();
    store.put_runtime_secret(&secret).unwrap();
    store.delete_workspace(workspace.id).unwrap();
    assert_eq!(store.get(workspace.id).unwrap(), None);
    assert_eq!(store.get_runtime_secret(workspace.id).unwrap(), None);
}

#[test]
fn routines_crud_by_workspace() {
    let store = WorkspaceStore::in_memory().unwrap();
    let workspace_id = Uuid::new_v4();
    let other_workspace_id = Uuid::new_v4();
    let first = Routine {
        id: Uuid::new_v4(),
        workspace_id,
        name: "first".into(),
        instruction: "run first".into(),
        interval_minutes: 15,
        enabled: true,
        last_run_unix_ms: Some(100),
        last_status: Some(RoutineRunStatus::Ok),
        last_skip_reason: None,
    };
    let second = Routine {
        id: Uuid::new_v4(),
        workspace_id,
        name: "second".into(),
        instruction: "run second".into(),
        interval_minutes: 30,
        enabled: true,
        last_run_unix_ms: None,
        last_status: None,
        last_skip_reason: None,
    };
    let other = Routine {
        id: Uuid::new_v4(),
        workspace_id: other_workspace_id,
        name: "other".into(),
        instruction: "run other".into(),
        interval_minutes: 60,
        enabled: true,
        last_run_unix_ms: None,
        last_status: None,
        last_skip_reason: None,
    };

    store.upsert_routine(&first).unwrap();
    store.upsert_routine(&second).unwrap();
    store.upsert_routine(&other).unwrap();
    assert_eq!(store.list_routines(workspace_id).unwrap().len(), 2);
    assert_eq!(store.get_routine(first.id).unwrap(), Some(first.clone()));

    let mut updated = first.clone();
    updated.enabled = false;
    store.upsert_routine(&updated).unwrap();
    assert_eq!(store.get_routine(first.id).unwrap(), Some(updated));

    store.delete_routine(second.id).unwrap();
    assert_eq!(store.list_routines(workspace_id).unwrap().len(), 1);
    store.delete_routines_for_workspace(workspace_id).unwrap();
    assert!(store.list_routines(workspace_id).unwrap().is_empty());
    assert_eq!(store.get_routine(other.id).unwrap(), Some(other));
}
