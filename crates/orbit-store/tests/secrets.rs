use orbit_domain::{
    Harness, LifecycleState, PermissionProfile, ResourceLimits, SecretAssignment, SecretInput,
    Workspace,
};
use orbit_store::{CredentialStore, StoreError, WorkspaceStore};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tempfile::tempdir;
use uuid::Uuid;

#[derive(Default)]
struct MemoryCredentials {
    values: Mutex<HashMap<String, String>>,
    fail_delete: Mutex<bool>,
}
impl CredentialStore for MemoryCredentials {
    fn set(&self, id: &str, value: &SecretInput) -> Result<(), StoreError> {
        self.values
            .lock()
            .unwrap()
            .insert(id.into(), value.0.clone());
        Ok(())
    }
    fn get(&self, id: &str) -> Result<String, StoreError> {
        self.values
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or(StoreError::Credential)
    }
    fn delete(&self, id: &str) -> Result<(), StoreError> {
        if *self.fail_delete.lock().unwrap() {
            return Err(StoreError::Credential);
        }
        self.values.lock().unwrap().remove(id);
        Ok(())
    }
}

fn store(credentials: Arc<MemoryCredentials>) -> WorkspaceStore {
    WorkspaceStore::in_memory()
        .unwrap()
        .with_credentials(credentials)
}
fn workspace(id: Uuid) -> Workspace {
    Workspace {
        id,
        name: id.to_string(),
        host_path: "/tmp/test".into(),
        profile: PermissionProfile::Workspace,
        harness: Harness::Codex,
        model: None,
        reasoning_effort: None,
        resources: ResourceLimits {
            cpus: 1.0,
            memory_bytes: 1,
            pids: 1,
            soft_disk_bytes: 1,
        },
        state: LifecycleState::Stopped,
        apps: Vec::new(),
        runtime: None,
    }
}
fn add_workspace(store: &WorkspaceStore, id: Uuid) {
    store.upsert(&workspace(id)).unwrap();
}

#[test]
fn secret_crud_and_assignment_deletion() {
    let credentials = Arc::new(MemoryCredentials::default());
    let store = store(credentials.clone());
    let ws = Uuid::new_v4();
    add_workspace(&store, ws);
    let secret = store
        .create_secret("name".into(), "TOKEN".into(), SecretInput("value".into()))
        .unwrap();
    assert_eq!(store.list_secrets().unwrap(), vec![secret.clone()]);
    store
        .assign_secret(SecretAssignment {
            secret_id: secret.id,
            workspace_id: ws,
        })
        .unwrap();
    assert_eq!(store.list_assignments(ws).unwrap().len(), 1);
    store.delete_secret(secret.id).unwrap();
    assert!(store.list_secrets().unwrap().is_empty());
    assert!(store.list_assignments(ws).unwrap().is_empty());
}

#[test]
fn duplicate_env_is_rejected_per_workspace_but_allowed_elsewhere() {
    let credentials = Arc::new(MemoryCredentials::default());
    let store = store(credentials);
    let one = Uuid::new_v4();
    let two = Uuid::new_v4();
    add_workspace(&store, one);
    add_workspace(&store, two);
    let a = store
        .create_secret("a".into(), "TOKEN".into(), SecretInput("a".into()))
        .unwrap();
    let b = store
        .create_secret("b".into(), "TOKEN".into(), SecretInput("b".into()))
        .unwrap();
    store
        .assign_secret(SecretAssignment {
            secret_id: a.id,
            workspace_id: one,
        })
        .unwrap();
    assert!(matches!(
        store.assign_secret(SecretAssignment {
            secret_id: b.id,
            workspace_id: one
        }),
        Err(StoreError::DuplicateSecretEnv)
    ));
    store
        .assign_secret(SecretAssignment {
            secret_id: b.id,
            workspace_id: two,
        })
        .unwrap();
}

#[test]
fn unavailable_credentials_fail_resolution_and_failed_delete_preserves_metadata() {
    let credentials = Arc::new(MemoryCredentials::default());
    let store = store(credentials.clone());
    let ws = Uuid::new_v4();
    add_workspace(&store, ws);
    let secret = store
        .create_secret("name".into(), "TOKEN".into(), SecretInput("value".into()))
        .unwrap();
    store
        .assign_secret(SecretAssignment {
            secret_id: secret.id,
            workspace_id: ws,
        })
        .unwrap();
    credentials.values.lock().unwrap().clear();
    assert!(matches!(
        store.resolve_assigned_env(ws),
        Err(StoreError::Credential)
    ));
    let secret = store
        .create_secret("other".into(), "OTHER".into(), SecretInput("value".into()))
        .unwrap();
    store
        .assign_secret(SecretAssignment {
            secret_id: secret.id,
            workspace_id: ws,
        })
        .unwrap();
    *credentials.fail_delete.lock().unwrap() = true;
    assert!(store.delete_secret(secret.id).is_err());
    assert!(store.list_secrets().unwrap().contains(&secret));
    assert_eq!(store.list_assignments(ws).unwrap().len(), 2);
}

#[test]
fn reopen_keeps_metadata_without_secret_bytes_and_debug_is_redacted() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("store.sqlite");
    let credentials = Arc::new(MemoryCredentials::default());
    let store = WorkspaceStore::open(&path)
        .unwrap()
        .with_credentials(credentials.clone());
    let secret = store
        .create_secret(
            "name".into(),
            "TOKEN".into(),
            SecretInput("secret-value".into()),
        )
        .unwrap();
    drop(store);
    let reopened = WorkspaceStore::open(&path)
        .unwrap()
        .with_credentials(Arc::new(MemoryCredentials::default()));
    assert_eq!(reopened.list_secrets().unwrap(), vec![secret]);
    assert!(!format!("{:?}", SecretInput("secret-value".into())).contains("secret-value"));
    assert!(
        !format!(
            "{:?}",
            orbit_domain::SecretEnv {
                env_name: "TOKEN".into(),
                value: SecretInput("secret-value".into())
            }
        )
        .contains("secret-value")
    );
}

#[test]
fn invalid_variable_is_rejected() {
    let store = store(Arc::new(MemoryCredentials::default()));
    assert!(matches!(
        store.create_secret("name".into(), "bad-name".into(), SecretInput("v".into())),
        Err(StoreError::InvalidSecret)
    ));
}
