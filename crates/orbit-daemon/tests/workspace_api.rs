use async_trait::async_trait;
use orbit_client::{ClientError, DaemonClient};
use orbit_daemon::{
    Daemon,
    runtime::{
        ContainerInfo, ContainerRuntime, ContainerSpec, MountInfo, RuntimeError,
        RuntimePrerequisite,
    },
    workspace_manager::{HealthPolicy, PINNED_WEBTOP_IMAGE, WorkspaceManager},
};
use orbit_domain::{Harness, LifecycleState, PermissionProfile, ResourceLimits};
use orbit_protocol::{Command, ErrorCode, ResponseBody};
use orbit_store::WorkspaceStore;
use std::time::Duration;
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex},
};
use tempfile::tempdir;
use tokio::task::JoinHandle;
use tokio::time::{Instant, sleep};
use uuid::Uuid;

#[derive(Default)]
struct FakeState {
    containers: HashMap<String, ContainerInfo>,
    creates: usize,
}
#[derive(Clone, Default)]
struct FakeRuntime(Arc<Mutex<FakeState>>);
impl FakeRuntime {
    fn creates(&self) -> usize {
        self.0.lock().unwrap().creates
    }
}
#[async_trait]
impl ContainerRuntime for FakeRuntime {
    async fn prerequisite(&self) -> Result<RuntimePrerequisite, RuntimeError> {
        Ok(RuntimePrerequisite {
            available: true,
            version: Some("fake".into()),
            message: String::new(),
        })
    }
    async fn ensure_image(&self, _: &str) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn create(&self, spec: &ContainerSpec) -> Result<ContainerInfo, RuntimeError> {
        let mut s = self.0.lock().unwrap();
        if let Some(existing) = s
            .containers
            .values()
            .find(|c| {
                c.labels.get("com.orbit.workspace-id") == Some(&spec.workspace_id.to_string())
            })
            .cloned()
        {
            return Ok(existing);
        }
        s.creates += 1;
        let mut labels = BTreeMap::new();
        labels.insert("com.orbit.managed".into(), "true".into());
        labels.insert(
            "com.orbit.workspace-id".into(),
            spec.workspace_id.to_string(),
        );
        labels.insert("com.orbit.schema".into(), "1".into());
        labels.insert("com.orbit.image".into(), "webtop-ubuntu-xfce-v2".into());
        let info = ContainerInfo {
            id: format!("fake-{}", spec.workspace_id),
            name: spec.container_name.clone(),
            status: "created".into(),
            running: false,
            published_port: Some(3000),
            image: PINNED_WEBTOP_IMAGE.into(),
            labels,
            mounts: vec![MountInfo {
                source: spec.host_path.display().to_string(),
                destination: "/workspace".into(),
                read_only: false,
            }],
        };
        s.containers.insert(info.id.clone(), info.clone());
        Ok(info)
    }
    async fn start(&self, id: &str) -> Result<(), RuntimeError> {
        if let Some(c) = self.0.lock().unwrap().containers.get_mut(id) {
            c.running = true;
            c.status = "running".into();
        }
        Ok(())
    }
    async fn exec(&self, _: &str, _: &str, _: &[&str]) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn stop(&self, id: &str) -> Result<(), RuntimeError> {
        if let Some(c) = self.0.lock().unwrap().containers.get_mut(id) {
            c.running = false;
            c.status = "exited".into();
        }
        Ok(())
    }
    async fn remove(&self, id: &str) -> Result<(), RuntimeError> {
        self.0.lock().unwrap().containers.remove(id);
        Ok(())
    }
    async fn remove_volume(&self, _: &str) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn inspect(&self, id: &str) -> Result<Option<ContainerInfo>, RuntimeError> {
        Ok(self.0.lock().unwrap().containers.get(id).cloned())
    }
    async fn managed(&self) -> Result<Vec<ContainerInfo>, RuntimeError> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .containers
            .values()
            .cloned()
            .collect())
    }
    async fn healthy(&self, _: u16, _: &str, _: &str) -> Result<bool, RuntimeError> {
        Ok(true)
    }
}

async fn start(
    name: &str,
    store: Arc<WorkspaceStore>,
    runtime: FakeRuntime,
) -> (DaemonClient, JoinHandle<anyhow::Result<()>>) {
    let token = "orbit-test-token".to_owned();
    let manager = Arc::new(WorkspaceManager::new_for_test(
        store,
        Arc::new(runtime),
        HealthPolicy {
            attempts: 1,
            delay: Duration::ZERO,
        },
    ));
    let daemon = Daemon::new(name.to_owned(), token.clone(), manager);
    let task = tokio::spawn(daemon.run());
    let client = DaemonClient::new(name.to_owned(), token);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match client.ping().await {
            Ok(ResponseBody::Pong) => return (client, task),
            Ok(other) => panic!("unexpected readiness response: {other:?}"),
            Err(_error) if Instant::now() < deadline => sleep(Duration::from_millis(10)).await,
            Err(error) => panic!("daemon did not become ready: {error:?}"),
        }
    }
}

fn name() -> String {
    format!("orbit-api-test-{}", Uuid::new_v4())
}

#[tokio::test]
async fn create_list_and_transition_workspace() {
    let store = Arc::new(WorkspaceStore::in_memory().unwrap());
    let project = tempdir().unwrap();
    let (client, daemon) = start(&name(), store, FakeRuntime::default()).await;
    let resources = ResourceLimits::new(2.0, 1_073_741_824, 128, 10_737_418_240).unwrap();
    let created = match client
        .send(Command::CreateWorkspace {
            name: "Demo".into(),
            host_path: project.path().into(),
            profile: PermissionProfile::Workspace,
            harness: Harness::Opencode,
            model: None,
            reasoning_effort: None,
            resources: resources.clone(),
        })
        .await
        .unwrap()
    {
        ResponseBody::Workspace(workspace) => workspace,
        other => panic!("{other:?}"),
    };
    assert_eq!(created.state, LifecycleState::Running);
    assert_eq!(created.harness, Harness::Opencode);
    let stopping = client
        .send(Command::StopWorkspace { id: created.id })
        .await
        .unwrap();
    assert!(
        matches!(stopping, ResponseBody::Workspace(w) if w.state == LifecycleState::Stopping || w.state == LifecycleState::Stopped)
    );
    let stopped = client
        .send(Command::StopWorkspace { id: created.id })
        .await
        .unwrap();
    assert!(matches!(stopped, ResponseBody::Workspace(w) if w.state == LifecycleState::Stopped));
    let listed = client.send(Command::ListWorkspaces).await.unwrap();
    assert!(
        matches!(listed, ResponseBody::Workspaces(ws) if ws.len() == 1 && ws[0].state == LifecycleState::Stopped)
    );
    daemon.abort();
    let _ = daemon.await;
}

#[tokio::test]
async fn workspace_survives_daemon_restart() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("workspaces.sqlite");
    let store = Arc::new(WorkspaceStore::open(&db).unwrap());
    let runtime = FakeRuntime::default();
    let project = tempdir().unwrap();
    let (client, daemon) = start(&name(), store, runtime.clone()).await;
    let created = client
        .send(Command::CreateWorkspace {
            name: "Persistent".into(),
            host_path: project.path().into(),
            profile: PermissionProfile::Observe,
            harness: Harness::default(),
            model: None,
            reasoning_effort: None,
            resources: ResourceLimits::new(1.5, 2_147_483_648, 64, 5_368_709_120).unwrap(),
        })
        .await
        .unwrap();
    assert!(matches!(created, ResponseBody::Workspace(_)));
    daemon.abort();
    let _ = daemon.await;
    let store = Arc::new(WorkspaceStore::open(&db).unwrap());
    let (client, daemon) = start(&name(), store, runtime.clone()).await;
    let listed = client.send(Command::ListWorkspaces).await.unwrap();
    assert!(
        matches!(listed, ResponseBody::Workspaces(ws) if ws.len() == 1 && ws[0].name == "Persistent")
    );
    daemon.abort();
    let _ = daemon.await;
}

#[tokio::test]
async fn rejects_invalid_resources_without_storing_workspace() {
    let store = Arc::new(WorkspaceStore::in_memory().unwrap());
    let project = tempdir().unwrap();
    let (client, daemon) = start(&name(), store, FakeRuntime::default()).await;
    let result = client
        .send(Command::CreateWorkspace {
            name: "Invalid".into(),
            host_path: project.path().into(),
            profile: PermissionProfile::Observe,
            harness: Harness::default(),
            model: None,
            reasoning_effort: None,
            resources: ResourceLimits {
                cpus: 0.0,
                memory_bytes: 1024,
                pids: 64,
                soft_disk_bytes: 2048,
            },
        })
        .await;
    assert!(
        matches!(result, Err(ClientError::Api(error)) if error.code == ErrorCode::InvalidRequest)
    );
    assert!(
        matches!(client.send(Command::ListWorkspaces).await.unwrap(), ResponseBody::Workspaces(ws) if ws.is_empty())
    );
    daemon.abort();
    let _ = daemon.await;
}

#[tokio::test]
async fn concurrent_start_and_stop_converge_idempotently() {
    let store = Arc::new(WorkspaceStore::in_memory().unwrap());
    let project = tempdir().unwrap();
    let runtime = FakeRuntime::default();
    let (client, daemon) = start(&name(), store, runtime.clone()).await;
    let created = match client
        .send(Command::CreateWorkspace {
            name: "Concurrent".into(),
            host_path: project.path().into(),
            profile: PermissionProfile::Workspace,
            harness: Harness::default(),
            model: None,
            reasoning_effort: None,
            resources: ResourceLimits::new(1.0, 1024, 1, 2048).unwrap(),
        })
        .await
        .unwrap()
    {
        ResponseBody::Workspace(w) => w,
        other => panic!("{other:?}"),
    };
    client
        .send(Command::StopWorkspace { id: created.id })
        .await
        .unwrap();
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let (a, b) = tokio::join!(
        {
            let c = client.clone();
            let barrier = barrier.clone();
            async move {
                barrier.wait().await;
                c.send(Command::StartWorkspace { id: created.id }).await
            }
        },
        {
            let c = client.clone();
            let barrier = barrier.clone();
            async move {
                barrier.wait().await;
                c.send(Command::StartWorkspace { id: created.id }).await
            }
        },
    );
    assert!(matches!(a, Ok(ResponseBody::Workspace(ref w)) if w.state == LifecycleState::Running));
    assert!(matches!(b, Ok(ResponseBody::Workspace(ref w)) if w.state == LifecycleState::Running));
    assert_eq!(runtime.creates(), 1);
    client
        .send(Command::StopWorkspace { id: created.id })
        .await
        .unwrap();
    client
        .send(Command::DeleteWorkspace { id: created.id })
        .await
        .unwrap();
    daemon.abort();
    let _ = daemon.await;
}
