//! Readiness is a hard prerequisite for every lifecycle mutation.
use async_trait::async_trait;
use orbit_daemon::{Daemon, runtime::*, workspace_manager::*};
use orbit_domain::{Harness, PermissionProfile, ResourceLimits};
use orbit_store::WorkspaceStore;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tempfile::tempdir;
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Clone)]
struct FakeContainerRuntime {
    available: Arc<AtomicBool>,
    managed_error: Arc<AtomicBool>,
    managed_calls: Arc<AtomicUsize>,
    create_calls: Arc<AtomicUsize>,
    start_calls: Arc<AtomicUsize>,
    block_managed: Arc<AtomicBool>,
    block_create: Arc<AtomicBool>,
    managed_entered: Arc<Semaphore>,
    managed_release: Arc<Semaphore>,
    create_entered: Arc<Semaphore>,
    create_release: Arc<Semaphore>,
    inspect: Arc<Mutex<Option<ContainerInfo>>>,
}
impl Default for FakeContainerRuntime {
    fn default() -> Self {
        Self {
            available: Default::default(),
            managed_error: Default::default(),
            managed_calls: Default::default(),
            create_calls: Default::default(),
            start_calls: Default::default(),
            block_managed: Default::default(),
            block_create: Default::default(),
            managed_entered: Arc::new(Semaphore::new(0)),
            managed_release: Arc::new(Semaphore::new(0)),
            create_entered: Arc::new(Semaphore::new(0)),
            create_release: Arc::new(Semaphore::new(0)),
            inspect: Default::default(),
        }
    }
}
impl FakeContainerRuntime {
    fn block_managed(&self) {
        self.block_managed.store(true, Ordering::Release);
    }
    fn release_managed(&self) {
        self.managed_release.add_permits(1);
    }
    async fn managed_entered(&self) {
        self.managed_entered.acquire().await.unwrap().forget();
    }
    fn block_create(&self) {
        self.block_create.store(true, Ordering::Release);
    }
    fn release_create(&self) {
        self.create_release.add_permits(1);
    }
    async fn create_entered(&self) {
        self.create_entered.acquire().await.unwrap().forget();
    }
}
impl FakeContainerRuntime {
    fn info(&self, id: Uuid) -> ContainerInfo {
        ContainerInfo {
            id: id.to_string(),
            name: format!("orbit-{id}"),
            status: "running".into(),
            running: true,
            published_port: Some(3000),
            image: PINNED_WEBTOP_IMAGE.into(),
            labels: [
                ("com.orbit.managed".into(), "true".into()),
                ("com.orbit.workspace-id".into(), id.to_string()),
                ("com.orbit.schema".into(), "1".into()),
                ("com.orbit.image".into(), "webtop-ubuntu-xfce-v2".into()),
            ]
            .into_iter()
            .collect(),
            mounts: vec![],
        }
    }
}
#[async_trait]
impl ContainerRuntime for FakeContainerRuntime {
    async fn prerequisite(&self) -> Result<RuntimePrerequisite, RuntimeError> {
        Ok(RuntimePrerequisite {
            available: self.available.load(Ordering::Acquire),
            version: Some("fake".into()),
            message: String::new(),
        })
    }
    async fn ensure_image(&self, _: &str) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn create(&self, s: &ContainerSpec) -> Result<ContainerInfo, RuntimeError> {
        self.create_calls.fetch_add(1, Ordering::AcqRel);
        if self.block_create.load(Ordering::Acquire) {
            self.create_entered.add_permits(1);
            let _ = self.create_release.acquire().await.unwrap();
        }
        let info = self.info(s.workspace_id);
        *self.inspect.lock().unwrap() = Some(info.clone());
        Ok(info)
    }
    async fn start(&self, _: &str) -> Result<(), RuntimeError> {
        self.start_calls.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
    async fn exec(&self, _: &str, _: &str, _: &[&str]) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn stop(&self, _: &str) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn remove(&self, _: &str) -> Result<(), RuntimeError> {
        *self.inspect.lock().unwrap() = None;
        Ok(())
    }
    async fn remove_volume(&self, _: &str) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn inspect(&self, _: &str) -> Result<Option<ContainerInfo>, RuntimeError> {
        Ok(self.inspect.lock().unwrap().clone())
    }
    async fn managed(&self) -> Result<Vec<ContainerInfo>, RuntimeError> {
        self.managed_calls.fetch_add(1, Ordering::AcqRel);
        if self.block_managed.load(Ordering::Acquire) {
            self.managed_entered.add_permits(1);
            let _ = self.managed_release.acquire().await.unwrap();
        }
        if self.managed_error.load(Ordering::Acquire) {
            Err(RuntimeError::Command("managed".into()))
        } else {
            Ok(vec![])
        }
    }
    async fn healthy(&self, _: u16, _: &str, _: &str) -> Result<bool, RuntimeError> {
        Ok(true)
    }
}
fn resources() -> ResourceLimits {
    ResourceLimits::new(1.0, 1024, 32, 2048).unwrap()
}

#[tokio::test]
async fn production_manager_rejects_mutations_until_successful_reconciliation() {
    let dir = tempdir().unwrap();
    let store = Arc::new(WorkspaceStore::in_memory().unwrap());
    let rt = FakeContainerRuntime::default();
    let manager = WorkspaceManager::new(store.clone(), Arc::new(rt.clone()));
    assert!(matches!(
        manager
            .create(
                "x".into(),
                dir.path().into(),
                PermissionProfile::Workspace,
                resources()
            )
            .await,
        Err(ManagerError::RuntimeUnavailable)
    ));
    assert_eq!(store.list().unwrap().len(), 0);
    assert_eq!(rt.create_calls.load(Ordering::Acquire), 0);
    assert!(!manager.reconcile().await.unwrap().diagnostics.is_empty());
    rt.available.store(true, Ordering::Release);
    rt.managed_error.store(true, Ordering::Release);
    assert!(manager.reconcile().await.is_err());
    assert!(!manager.mutations_ready());
    rt.managed_error.store(false, Ordering::Release);
    rt.available.store(true, Ordering::Release);
    assert!(manager.reconcile().await.is_ok());
    assert!(
        manager
            .create(
                "x".into(),
                dir.path().into(),
                PermissionProfile::Workspace,
                resources()
            )
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn reconciliation_write_lock_blocks_new_mutations() {
    let d = tempdir().unwrap();
    let store = Arc::new(WorkspaceStore::in_memory().unwrap());
    let rt = FakeContainerRuntime::default();
    rt.available.store(true, Ordering::Release);
    let m = Arc::new(WorkspaceManager::new(store.clone(), Arc::new(rt.clone())));
    m.reconcile().await.unwrap();
    rt.block_managed();
    let r = tokio::spawn({
        let m = m.clone();
        async move { m.reconcile().await }
    });
    rt.managed_entered().await;
    let c = tokio::spawn({
        let m = m.clone();
        let p = d.path().to_path_buf();
        async move {
            m.create("x".into(), p, PermissionProfile::Workspace, resources())
                .await
        }
    });
    for _ in 0..3 {
        tokio::task::yield_now().await;
    }
    assert!(!c.is_finished());
    assert!(store.list().unwrap().is_empty());
    assert_eq!(rt.create_calls.load(Ordering::Acquire), 0);
    rt.release_managed();
    r.await.unwrap().unwrap();
    c.await.unwrap().unwrap();
}

#[tokio::test]
async fn in_flight_mutation_read_lock_blocks_reconciliation_inventory() {
    let d = tempdir().unwrap();
    let store = Arc::new(WorkspaceStore::in_memory().unwrap());
    let rt = FakeContainerRuntime::default();
    rt.available.store(true, Ordering::Release);
    let m = Arc::new(WorkspaceManager::new(store, Arc::new(rt.clone())));
    m.reconcile().await.unwrap();
    rt.block_create();
    let c = tokio::spawn({
        let m = m.clone();
        let p = d.path().to_path_buf();
        async move {
            m.create("x".into(), p, PermissionProfile::Workspace, resources())
                .await
        }
    });
    rt.create_entered().await;
    let n = rt.managed_calls.load(Ordering::Acquire);
    let r = tokio::spawn({
        let m = m.clone();
        async move { m.reconcile().await }
    });
    for _ in 0..3 {
        tokio::task::yield_now().await;
    }
    assert!(!r.is_finished());
    assert_eq!(rt.managed_calls.load(Ordering::Acquire), n);
    rt.release_create();
    c.await.unwrap().unwrap();
    r.await.unwrap().unwrap();
    assert!(rt.managed_calls.load(Ordering::Acquire) > n);
}

#[tokio::test]
async fn authenticated_retry_enables_mutations_and_status_updates() {
    let dir = tempdir().unwrap();
    let store = Arc::new(WorkspaceStore::in_memory().unwrap());
    let rt = FakeContainerRuntime::default();
    let manager = Arc::new(WorkspaceManager::new(store, Arc::new(rt.clone())));
    let name = format!("orbit-readiness-{}", Uuid::new_v4());
    let token: String = "token".into();
    let daemon = Daemon::with_lock_path(
        name.clone(),
        token.clone(),
        manager,
        dir.path().join("daemon.lock"),
    );
    let task = tokio::spawn(daemon.run());
    let client = orbit_client::DaemonClient::new(name, token);
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if client.ping().await.is_ok() {
                break;
            }
            tokio::task::yield_now().await
        }
    })
    .await
    .unwrap();
    assert!(!client.runtime_status().await.unwrap().available);
    assert!(!client.runtime_status().await.unwrap().mutations_ready);
    let resources = resources();
    let request = orbit_protocol::Command::CreateWorkspace {
        name: "blocked".into(),
        host_path: dir.path().into(),
        profile: PermissionProfile::Workspace,
        harness: Harness::default(),
        model: None,
        reasoning_effort: None,
        resources: resources.clone(),
    };
    assert!(
        matches!(client.send(request).await, Err(orbit_client::ClientError::Api(e)) if e.code == orbit_protocol::ErrorCode::RuntimeUnavailable)
    );
    assert_eq!(rt.create_calls.load(Ordering::Acquire), 0);
    assert!(!client.reconcile_runtime().await.unwrap().available);
    rt.available.store(true, Ordering::Release);
    let status = client.reconcile_runtime().await.unwrap();
    assert_eq!((status.available, status.mutations_ready), (true, true));
    let request = orbit_protocol::Command::CreateWorkspace {
        name: "ready".into(),
        host_path: dir.path().into(),
        profile: PermissionProfile::Workspace,
        harness: Harness::default(),
        model: None,
        reasoning_effort: None,
        resources,
    };
    assert!(
        matches!(client.send(request).await.unwrap(), orbit_protocol::ResponseBody::Workspace(w) if w.state == orbit_domain::LifecycleState::Running)
    );
    assert_eq!(rt.create_calls.load(Ordering::Acquire), 1);
    task.abort();
}
