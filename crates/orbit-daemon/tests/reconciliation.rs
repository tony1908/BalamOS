use async_trait::async_trait;
use orbit_daemon::runtime::{
    ContainerInfo, ContainerRuntime, ContainerSpec, MountInfo, RuntimeError, RuntimePrerequisite,
};
use orbit_daemon::workspace_manager::{
    HealthPolicy, PINNED_WEBTOP_IMAGE, ReconcileCode, ReconcileReport, WorkspaceManager,
};
use orbit_domain::{
    LifecycleState, PermissionProfile, ResourceLimits, RuntimeMetadata, RuntimeSecret, Workspace,
};
use orbit_store::WorkspaceStore;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tempfile::tempdir;
use uuid::Uuid;

const SECRET: &str = "reconcile-secret-DO-NOT-LEAK";
#[derive(Default)]
struct State {
    containers: Vec<ContainerInfo>,
    unavailable: bool,
    managed_error: bool,
    prerequisite_calls: usize,
    managed_calls: usize,
    healthy_calls: Vec<String>,
    starts: Vec<String>,
    stops: Vec<String>,
    removes: Vec<String>,
    volume_removals: Vec<String>,
    creates: usize,
}
#[derive(Clone, Default)]
struct FakeRuntime(Arc<Mutex<State>>);
impl FakeRuntime {
    fn new(containers: Vec<ContainerInfo>) -> Self {
        Self(Arc::new(Mutex::new(State {
            containers,
            ..Default::default()
        })))
    }
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap()
    }
    fn inventory(&self) -> Vec<ContainerInfo> {
        self.state().containers.clone()
    }
}
#[async_trait]
impl ContainerRuntime for FakeRuntime {
    async fn prerequisite(&self) -> Result<RuntimePrerequisite, RuntimeError> {
        let mut s = self.state();
        s.prerequisite_calls += 1;
        if s.unavailable {
            Err(RuntimeError::Unavailable("private".into()))
        } else {
            Ok(RuntimePrerequisite {
                available: true,
                version: Some("fake".into()),
                message: String::new(),
            })
        }
    }
    async fn ensure_image(&self, _: &str) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn create(&self, _: &ContainerSpec) -> Result<ContainerInfo, RuntimeError> {
        self.state().creates += 1;
        Err(RuntimeError::Command("unexpected".into()))
    }
    async fn start(&self, id: &str) -> Result<(), RuntimeError> {
        self.state().starts.push(id.into());
        Ok(())
    }
    async fn exec(&self, _: &str, _: &str, _: &[&str]) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn stop(&self, id: &str) -> Result<(), RuntimeError> {
        let mut s = self.state();
        s.stops.push(id.into());
        if let Some(c) = s.containers.iter_mut().find(|c| c.id == id) {
            c.running = false;
            c.status = "exited".into();
        }
        Ok(())
    }
    async fn remove(&self, id: &str) -> Result<(), RuntimeError> {
        self.state().removes.push(id.into());
        Ok(())
    }
    async fn remove_volume(&self, n: &str) -> Result<(), RuntimeError> {
        self.state().volume_removals.push(n.into());
        Ok(())
    }
    async fn inspect(&self, id: &str) -> Result<Option<ContainerInfo>, RuntimeError> {
        Ok(self.state().containers.iter().find(|c| c.id == id).cloned())
    }
    async fn managed(&self) -> Result<Vec<ContainerInfo>, RuntimeError> {
        let mut s = self.state();
        s.managed_calls += 1;
        if s.managed_error {
            Err(RuntimeError::Command("private".into()))
        } else {
            Ok(s.containers.clone())
        }
    }
    async fn healthy(&self, _: u16, _: &str, p: &str) -> Result<bool, RuntimeError> {
        self.state().healthy_calls.push(p.into());
        Ok(true)
    }
}
fn info(id: &str, w: Uuid) -> ContainerInfo {
    let mut l = BTreeMap::new();
    l.insert("com.orbit.managed".into(), "true".into());
    l.insert("com.orbit.workspace-id".into(), w.to_string());
    l.insert("com.orbit.schema".into(), "1".into());
    l.insert("com.orbit.image".into(), "webtop-ubuntu-xfce-v2".into());
    ContainerInfo {
        id: id.into(),
        name: format!("orbit-{w}"),
        status: "running".into(),
        running: true,
        published_port: Some(3000),
        image: PINNED_WEBTOP_IMAGE.into(),
        labels: l,
        mounts: vec![MountInfo {
            source: "/host".into(),
            destination: "/workspace".into(),
            read_only: false,
        }],
    }
}
fn new_workspace(state: LifecycleState) -> Workspace {
    let d = tempdir().unwrap();
    let mut w = Workspace::new(
        "demo".into(),
        PathBuf::from(d.path()),
        PermissionProfile::Workspace,
        ResourceLimits::new(1., 1024, 32, 2048).unwrap(),
    );
    w.state = state;
    w
}
async fn setup(
    state: LifecycleState,
    candidate: Option<ContainerInfo>,
    secret: bool,
    metadata: bool,
) -> (WorkspaceManager, FakeRuntime, Uuid) {
    let store = Arc::new(WorkspaceStore::in_memory().unwrap());
    let mut w = new_workspace(state);
    let id = w.id;
    if metadata {
        w.runtime = Some(RuntimeMetadata {
            container_id: "stale".into(),
            container_name: "stale".into(),
            volume_name: "kept".into(),
            upstream_port: 9,
            image_ref: "stale".into(),
        });
    }
    let mut candidate = candidate;
    if let Some(c) = candidate.as_mut()
        && c.labels.get("com.orbit.workspace-id") == Some(&Uuid::nil().to_string())
    {
        c.labels
            .insert("com.orbit.workspace-id".into(), id.to_string());
    }
    let rt = FakeRuntime::new(candidate.into_iter().collect());
    if secret {
        store
            .insert_workspace_with_secret(
                &w,
                &RuntimeSecret {
                    workspace_id: id,
                    webtop_password: SECRET.into(),
                },
            )
            .unwrap()
    } else {
        store.upsert(&w).unwrap()
    }
    (
        WorkspaceManager::new_for_test(
            store,
            Arc::new(rt.clone()),
            HealthPolicy {
                attempts: 1,
                delay: Duration::ZERO,
            },
        ),
        rt,
        id,
    )
}
fn no_mutations(s: &State) {
    assert!(s.starts.is_empty());
    assert!(s.stops.is_empty());
    assert!(s.removes.is_empty());
    assert!(s.volume_removals.is_empty());
    assert_eq!(s.creates, 0)
}
fn codes(r: &ReconcileReport) -> Vec<ReconcileCode> {
    r.diagnostics.iter().map(|d| d.code).collect()
}

#[tokio::test]
async fn running_db_and_running_healthy_stays_running_refreshes_port() {
    let (m, rt, id) = setup(
        LifecycleState::Running,
        Some(info("actual", Uuid::nil())),
        true,
        true,
    )
    .await;
    let r = m.reconcile().await.unwrap();
    let w = m.get(id).await.unwrap();
    assert_eq!(w.state, LifecycleState::Running);
    let x = w.runtime.unwrap();
    assert_eq!(
        (x.container_id, x.upstream_port, x.volume_name),
        ("actual".into(), 3000, "kept".into())
    );
    assert_eq!(rt.state().healthy_calls, vec![SECRET]);
    assert_eq!(r.running, 1)
}

#[tokio::test]
async fn legacy_owned_container_is_flagged_for_upgrade_without_reconcile_mutation() {
    let mut legacy = info("legacy", Uuid::nil());
    legacy.image = "orbit-webtop:0.1.0".into();
    legacy
        .labels
        .insert("com.orbit.image".into(), "webtop-ubuntu-xfce-v1".into());
    let (m, rt, id) = setup(LifecycleState::Running, Some(legacy), true, true).await;

    let report = m.reconcile().await.unwrap();

    assert_eq!(codes(&report), vec![ReconcileCode::UpgradeRequired]);
    assert_eq!(m.get(id).await.unwrap().state, LifecycleState::Running);
    no_mutations(&rt.state());
}
#[tokio::test]
async fn running_db_stopped_container_becomes_stopped() {
    let mut c = info("stopped", Uuid::nil());
    c.running = false;
    let (m, rt, id) = setup(LifecycleState::Running, Some(c), false, true).await;
    m.reconcile().await.unwrap();
    let w = m.get(id).await.unwrap();
    assert_eq!(w.state, LifecycleState::Stopped);
    assert!(rt.state().healthy_calls.is_empty());
    let (m, _, id) = setup(LifecycleState::Stopped, None, false, true).await;
    m.reconcile().await.unwrap();
    assert!(m.get(id).await.unwrap().runtime.is_none())
}
#[tokio::test]
async fn starting_db_running_healthy_becomes_running() {
    let (m, rt, id) = setup(
        LifecycleState::Starting,
        Some(info("start", Uuid::nil())),
        true,
        false,
    )
    .await;
    m.reconcile().await.unwrap();
    assert_eq!(m.get(id).await.unwrap().state, LifecycleState::Running);
    assert_eq!(rt.state().healthy_calls.len(), 1)
}
#[tokio::test]
async fn missing_container_running_becomes_failed_but_stopped_clears_metadata() {
    let (m, _, id) = setup(LifecycleState::Running, None, true, true).await;
    let r = m.reconcile().await.unwrap();
    assert_eq!(m.get(id).await.unwrap().state, LifecycleState::Failed);
    assert_eq!(codes(&r), vec![ReconcileCode::MissingContainer]);
    let (m, _, id) = setup(LifecycleState::Stopping, None, false, true).await;
    m.reconcile().await.unwrap();
    assert_eq!(m.get(id).await.unwrap().state, LifecycleState::Stopped)
}
#[tokio::test]
async fn duplicate_labeled_containers_becomes_failed_without_deletion() {
    let mut legacy = info("a", Uuid::nil());
    legacy.image = "orbit-webtop:0.1.0".into();
    legacy
        .labels
        .insert("com.orbit.image".into(), "webtop-ubuntu-xfce-v1".into());
    let (m, rt, id) = setup(LifecycleState::Running, Some(legacy), true, false).await;
    let mut duplicate = info("b", id);
    duplicate.image = "orbit-webtop:0.1.0".into();
    duplicate
        .labels
        .insert("com.orbit.image".into(), "webtop-ubuntu-xfce-v1".into());
    rt.state().containers.push(duplicate);
    let before = rt.inventory();
    let (r, migrated) = m.reconcile_and_migrate().await.unwrap();
    assert_eq!(codes(&r), vec![ReconcileCode::DuplicateContainers]);
    assert!(migrated.is_empty());
    assert_eq!(format!("{before:?}"), format!("{:?}", rt.inventory()));
    no_mutations(&rt.state())
}
#[tokio::test]
async fn mismatched_schema_or_image_becomes_failed_untouched() {
    for bad_image in [false, true] {
        let (m, rt, id) = setup(
            LifecycleState::Running,
            Some(info("bad", Uuid::nil())),
            true,
            false,
        )
        .await;
        if bad_image {
            rt.state().containers[0].image = "wrong".into()
        } else {
            rt.state().containers[0]
                .labels
                .insert("com.orbit.schema".into(), "99".into());
        }
        let before = rt.inventory();
        let r = m.reconcile().await.unwrap();
        assert_eq!(codes(&r), vec![ReconcileCode::OwnershipMismatch]);
        assert_eq!(m.get(id).await.unwrap().state, LifecycleState::Failed);
        assert_eq!(format!("{before:?}"), format!("{:?}", rt.inventory()));
        no_mutations(&rt.state())
    }
}
#[tokio::test]
async fn orphan_container_is_reported_and_untouched() {
    let (m, rt, _) = setup(
        LifecycleState::Stopped,
        Some(info("orphan", Uuid::new_v4())),
        false,
        false,
    )
    .await;
    let r = m.reconcile().await.unwrap();
    assert_eq!(r.orphan_container_ids, vec!["orphan"]);
    assert_eq!(codes(&r), vec![ReconcileCode::OrphanContainer]);
    no_mutations(&rt.state())
}
#[tokio::test]
async fn stopped_db_running_container_is_stopped_once() {
    let (m, rt, id) = setup(
        LifecycleState::Stopped,
        Some(info("stop", Uuid::nil())),
        false,
        false,
    )
    .await;
    m.reconcile().await.unwrap();
    assert_eq!(rt.state().stops, vec!["stop"]);
    m.reconcile().await.unwrap();
    assert_eq!(rt.state().stops, vec!["stop"]);
    assert_eq!(m.get(id).await.unwrap().state, LifecycleState::Stopped)
}
#[tokio::test]
async fn missing_secret_health_path_failed_without_secret_leak() {
    let (m, rt, id) = setup(
        LifecycleState::Running,
        Some(info("secret", Uuid::nil())),
        false,
        false,
    )
    .await;
    let r = m.reconcile().await.unwrap();
    assert_eq!(codes(&r), vec![ReconcileCode::MissingSecret]);
    assert_eq!(m.get(id).await.unwrap().state, LifecycleState::Failed);
    assert!(!format!("{r:?}").contains(SECRET));
    assert!(rt.state().healthy_calls.is_empty())
}
#[tokio::test]
async fn reconciliation_is_repeatable_no_duplicate_mutations() {
    let (m, rt, _) = setup(
        LifecycleState::Stopped,
        Some(info("repeat", Uuid::nil())),
        false,
        false,
    )
    .await;
    let a = m.reconcile().await.unwrap();
    let b = m.reconcile().await.unwrap();
    assert_eq!(
        (a.running, a.stopped, a.failed, codes(&a)),
        (b.running, b.stopped, b.failed, codes(&b))
    );
    assert_eq!(rt.state().stops, vec!["repeat"])
}
#[tokio::test]
async fn runtime_unavailable_returns_diagnostic_no_mutation() {
    let (m, rt, id) = setup(
        LifecycleState::Running,
        Some(info("x", Uuid::nil())),
        true,
        true,
    )
    .await;
    let before = m.get(id).await.unwrap();
    rt.state().unavailable = true;
    let r = m.reconcile().await.unwrap();
    assert_eq!(codes(&r), vec![ReconcileCode::RuntimeUnavailable]);
    assert_eq!(m.get(id).await.unwrap(), before);
    assert_eq!(rt.state().prerequisite_calls, 1);
    assert_eq!(rt.state().managed_calls, 0);
    no_mutations(&rt.state());
    assert!(!format!("{r:?}").contains("private"))
}
