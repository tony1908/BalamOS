use async_trait::async_trait;
use orbit_daemon::runtime::{
    ContainerInfo, ContainerRuntime, ContainerSpec, MountInfo, RuntimeError, RuntimePrerequisite,
};
use orbit_daemon::workspace_manager::{
    HealthPolicy, ManagerError, PINNED_WEBTOP_IMAGE, WorkspaceManager,
};
use orbit_domain::{
    LifecycleState, PermissionProfile, ResourceLimits, RuntimeMetadata, RuntimeSecret,
    TemplateSettings, Workspace,
};
use orbit_store::WorkspaceStore;
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

#[derive(Default)]
struct State {
    calls: Vec<String>,
    specs: Vec<ContainerSpec>,
    inspect: Option<ContainerInfo>,
    managed: Vec<ContainerInfo>,
    health: VecDeque<Result<bool, RuntimeError>>,
    error: Option<RuntimeError>,
    start_hook: Option<Arc<dyn Fn() + Send + Sync>>,
    start_ids: Vec<String>,
    stop_ids: Vec<String>,
    remove_ids: Vec<String>,
    volume_names: Vec<String>,
    managed_calls: usize,
    create_result: Option<ContainerInfo>,
    created_port: Option<Option<u16>>,
}
#[derive(Clone, Default)]
struct FakeContainerRuntime(Arc<Mutex<State>>);
#[allow(dead_code)]
impl FakeContainerRuntime {
    fn with_inspect(self, info: Option<ContainerInfo>) -> Self {
        self.0.lock().unwrap().inspect = info;
        self
    }
    fn with_managed(self, infos: Vec<ContainerInfo>) -> Self {
        self.0.lock().unwrap().managed = infos;
        self
    }
    fn with_create_result(self, info: ContainerInfo) -> Self {
        self.0.lock().unwrap().create_result = Some(info);
        self
    }
    fn with_created_port(self, port: Option<u16>) -> Self {
        self.0.lock().unwrap().created_port = Some(port);
        self
    }
    fn set_inspect_running(&self, running: bool) {
        if let Some(info) = self.0.lock().unwrap().inspect.as_mut() {
            info.running = running;
            info.status = if running { "running" } else { "exited" }.into();
        }
    }
    fn set_inspect_port(&self, port: Option<u16>) {
        if let Some(info) = self.0.lock().unwrap().inspect.as_mut() {
            info.published_port = port;
        }
    }
    fn managed_calls(&self) -> usize {
        self.0.lock().unwrap().managed_calls
    }
    fn calls(&self) -> Vec<String> {
        self.0.lock().unwrap().calls.clone()
    }
    fn specs(&self) -> Vec<ContainerSpec> {
        self.0.lock().unwrap().specs.clone()
    }
    fn push_health(&self, result: Result<bool, RuntimeError>) {
        self.0.lock().unwrap().health.push_back(result);
    }
    fn fail(&self, e: RuntimeError) {
        self.0.lock().unwrap().error = Some(e);
    }
    fn on_start(&self, hook: impl Fn() + Send + Sync + 'static) {
        self.0.lock().unwrap().start_hook = Some(Arc::new(hook));
    }
    fn reset_calls(&self) {
        let mut s = self.0.lock().unwrap();
        s.calls.clear();
        s.start_ids.clear();
        s.stop_ids.clear();
        s.remove_ids.clear();
        s.volume_names.clear();
    }
    fn start_ids(&self) -> Vec<String> {
        self.0.lock().unwrap().start_ids.clone()
    }
    fn stop_ids(&self) -> Vec<String> {
        self.0.lock().unwrap().stop_ids.clone()
    }
    fn remove_ids(&self) -> Vec<String> {
        self.0.lock().unwrap().remove_ids.clone()
    }
    fn volume_names(&self) -> Vec<String> {
        self.0.lock().unwrap().volume_names.clone()
    }
    fn take_error(s: &mut State) -> Result<(), RuntimeError> {
        s.error.take().map_or(Ok(()), Err)
    }
}
fn info(id: &str, workspace: uuid::Uuid) -> ContainerInfo {
    let mut labels = BTreeMap::new();
    labels.insert("com.orbit.managed".into(), "true".into());
    labels.insert("com.orbit.workspace-id".into(), workspace.to_string());
    labels.insert("com.orbit.schema".into(), "1".into());
    labels.insert("com.orbit.image".into(), "webtop-ubuntu-xfce-v2".into());
    ContainerInfo {
        id: id.into(),
        name: format!("orbit-{workspace}"),
        status: "running".into(),
        running: true,
        published_port: Some(3000),
        image: PINNED_WEBTOP_IMAGE.into(),
        labels,
        mounts: vec![MountInfo {
            source: "/host".into(),
            destination: "/workspace".into(),
            read_only: false,
        }],
    }
}

fn legacy_info(id: &str, workspace: uuid::Uuid) -> ContainerInfo {
    let mut legacy = info(id, workspace);
    legacy.image = "orbit-webtop:0.1.0".into();
    legacy
        .labels
        .insert("com.orbit.image".into(), "webtop-ubuntu-xfce-v1".into());
    legacy
}
#[async_trait]
impl ContainerRuntime for FakeContainerRuntime {
    async fn prerequisite(&self) -> Result<RuntimePrerequisite, RuntimeError> {
        Ok(RuntimePrerequisite {
            available: true,
            version: Some("fake".into()),
            message: String::new(),
        })
    }
    async fn ensure_image(&self, _: &str) -> Result<(), RuntimeError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("ensure_image".into());
        Self::take_error(&mut s)
    }
    async fn create(&self, spec: &ContainerSpec) -> Result<ContainerInfo, RuntimeError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("create".into());
        s.specs.push(spec.clone());
        Self::take_error(&mut s).map(|_| {
            let mut created = s
                .create_result
                .take()
                .unwrap_or_else(|| info(&spec.workspace_id.to_string(), spec.workspace_id));
            if let Some(port) = s.created_port {
                created.published_port = port;
            }
            s.inspect = Some(created.clone());
            created
        })
    }
    async fn start(&self, id: &str) -> Result<(), RuntimeError> {
        let s = self.0.lock().unwrap();
        let hook = s.start_hook.clone();
        drop(s);
        if let Some(hook) = hook {
            hook();
        }
        let mut s = self.0.lock().unwrap();
        s.calls.push("start".into());
        s.start_ids.push(id.into());
        if let Some(info) = s.inspect.as_mut() {
            if info.running {
                return Self::take_error(&mut s);
            }
            info.running = true;
            info.status = "running".into();
        }
        Self::take_error(&mut s)
    }
    async fn exec(&self, _: &str, _: &str, _: &[&str]) -> Result<(), RuntimeError> {
        Ok(())
    }
    async fn stop(&self, id: &str) -> Result<(), RuntimeError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("stop".into());
        s.stop_ids.push(id.into());
        if let Some(info) = s.inspect.as_mut() {
            info.running = false;
            info.status = "exited".into();
        }
        Self::take_error(&mut s)
    }
    async fn remove(&self, id: &str) -> Result<(), RuntimeError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("remove".into());
        s.remove_ids.push(id.into());
        if s.inspect.as_ref().is_some_and(|info| info.id == id) {
            s.inspect = None;
        }
        Self::take_error(&mut s)
    }
    async fn remove_volume(&self, name: &str) -> Result<(), RuntimeError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("remove_volume".into());
        s.volume_names.push(name.into());
        Self::take_error(&mut s)
    }
    async fn inspect(&self, _id: &str) -> Result<Option<ContainerInfo>, RuntimeError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("inspect".into());
        Ok(s.inspect.clone())
    }
    async fn managed(&self) -> Result<Vec<ContainerInfo>, RuntimeError> {
        let mut s = self.0.lock().unwrap();
        s.managed_calls += 1;
        Ok(s.managed.clone())
    }
    async fn healthy(&self, _: u16, _: &str, _: &str) -> Result<bool, RuntimeError> {
        let mut s = self.0.lock().unwrap();
        s.calls.push("healthy".into());
        s.health.pop_front().unwrap_or(Ok(true))
    }
}
fn resources() -> ResourceLimits {
    ResourceLimits::new(1.0, 1024, 32, 2048).unwrap()
}
async fn manager(rt: FakeContainerRuntime) -> (WorkspaceManager, tempfile::TempDir) {
    let d = tempdir().unwrap();
    let store = Arc::new(WorkspaceStore::in_memory().unwrap());
    let m = WorkspaceManager::new_for_test(
        store,
        Arc::new(rt),
        HealthPolicy {
            attempts: 3,
            delay: std::time::Duration::ZERO,
        },
    );
    (m, d)
}

#[tokio::test]
async fn templates_save_reject_empty_list_and_delete() {
    let (manager, _tempdir) = manager(FakeContainerRuntime::default()).await;
    let settings = TemplateSettings {
        display_name: None,
        label: None,
        description: None,
        notifications: true,
    };
    assert!(matches!(
        manager
            .save_template("  ".into(), settings.clone(), vec![], vec![])
            .await,
        Err(ManagerError::Invalid(_))
    ));
    let template = manager
        .save_template("  Starter  ".into(), settings, vec![], vec![])
        .await
        .unwrap();
    assert_eq!(
        manager.list_templates().await.unwrap(),
        vec![template.clone()]
    );
    manager.delete_template(template.id).await.unwrap();
    assert!(manager.list_templates().await.unwrap().is_empty());
}
async fn created(
    rt: FakeContainerRuntime,
) -> (WorkspaceManager, tempfile::TempDir, orbit_domain::Workspace) {
    let (m, d) = manager(rt).await;
    let w = m
        .create(
            "demo".into(),
            d.path().into(),
            PermissionProfile::Workspace,
            resources(),
        )
        .await
        .unwrap();
    (m, d, w)
}

fn set_stored_state(m: &WorkspaceManager, id: uuid::Uuid, state: LifecycleState) {
    let mut w = m.store.get(id).unwrap().unwrap();
    w.state = state;
    m.store.upsert(&w).unwrap();
}

#[tokio::test]
async fn create_canonicalizes_path_pins_image_sets_all_labels_and_waits_for_health() {
    let rt = FakeContainerRuntime::default();
    rt.push_health(Ok(false));
    rt.push_health(Ok(false));
    rt.push_health(Ok(true));
    let (_m, d, w) = created(rt.clone()).await;
    assert_eq!(w.host_path, std::fs::canonicalize(d.path()).unwrap());
    let s = &rt.specs()[0];
    assert_eq!(s.image_ref, PINNED_WEBTOP_IMAGE);
    assert_eq!(s.container_name, format!("orbit-{}", w.id));
    assert_eq!(s.volume_name, format!("orbit-config-{}", w.id));
    assert_eq!(s.workspace_id, w.id);
    assert_eq!(s.profile, PermissionProfile::Workspace);
    assert_eq!(s.resources, resources());
    assert_eq!(s.labels.get("com.orbit.managed"), Some(&"true".into()));
    assert_eq!(
        s.labels.get("com.orbit.workspace-id"),
        Some(&w.id.to_string())
    );
    assert_eq!(s.labels.get("com.orbit.schema"), Some(&"1".into()));
    assert_eq!(
        s.labels.get("com.orbit.image"),
        Some(&"webtop-ubuntu-xfce-v2".into())
    );
    assert_eq!(s.webtop_password.len(), 43);
    assert!(!s.webtop_password.contains('='));
    assert!(
        s.webtop_password
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    );
    let credential = s.webtop_password.clone();
    assert!(!format!("{w:?}").contains(&credential));
    assert!(!serde_json::to_string(&w).unwrap().contains(&credential));
    assert!(!format!("{:?}", w.runtime.as_ref().unwrap()).contains(&credential));
    assert_eq!(w.runtime.as_ref().unwrap().image_ref, PINNED_WEBTOP_IMAGE);
    assert_eq!(
        rt.calls(),
        [
            "ensure_image",
            "create",
            "start",
            "inspect",
            "healthy",
            "healthy",
            "healthy"
        ]
    );
}

#[tokio::test]
async fn create_with_empty_path_uses_a_managed_slugged_workspace_folder() {
    let rt = FakeContainerRuntime::default();
    let (m, _d) = manager(rt).await;
    let w = m
        .create(
            "My Bot!".into(),
            PathBuf::new(),
            PermissionProfile::Workspace,
            resources(),
        )
        .await
        .unwrap();

    let host_path = PathBuf::from(&w.host_path);
    assert_eq!(host_path.file_name().unwrap(), "my-bot");
    assert!(
        host_path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("orbit-test-ws-")
    );
    assert!(host_path.is_dir());
}
#[tokio::test]
async fn create_rejects_blank_name_missing_path_non_directory_and_invalid_resources_without_runtime_calls()
 {
    let rt = FakeContainerRuntime::default();
    let (m, d) = manager(rt.clone()).await;
    for (n, p, r) in [
        (" ", d.path().to_path_buf(), resources()),
        ("x", PathBuf::from("/missing"), resources()),
        ("x", d.path().join("f"), resources()),
    ] {
        if n == "x" && p.ends_with("f") {
            std::fs::write(&p, b"").unwrap();
        }
        assert!(
            m.create(n.into(), p, PermissionProfile::Observe, r)
                .await
                .is_err()
        );
    }
    assert!(
        m.create(
            "x".into(),
            d.path().into(),
            PermissionProfile::Observe,
            ResourceLimits {
                cpus: 0.0,
                ..resources()
            }
        )
        .await
        .is_err()
    );
    assert!(rt.calls().is_empty());
}
#[tokio::test]
async fn create_failure_persists_failed_and_redacts_secret() {
    let rt = FakeContainerRuntime::default();
    rt.fail(RuntimeError::Command("password leaked".into()));
    let (m, d) = manager(rt).await;
    let e = m
        .create(
            "x".into(),
            d.path().into(),
            PermissionProfile::Observe,
            resources(),
        )
        .await
        .unwrap_err();
    assert!(!format!("{e:?}").contains("password"));
    assert_eq!(m.list().await.unwrap()[0].state, LifecycleState::Failed);
}
#[tokio::test]
async fn permission_profiles_produce_expected_policy() {
    let rt = FakeContainerRuntime::default();
    let (m, d) = manager(rt.clone()).await;
    for p in [
        PermissionProfile::Observe,
        PermissionProfile::Workspace,
        PermissionProfile::FullControl,
    ] {
        let _ = m
            .create(format!("{p:?}"), d.path().into(), p, resources())
            .await
            .unwrap();
    }
    let specs = rt.specs();
    assert_eq!(specs.len(), 3);
    assert_eq!(
        specs.iter().map(|s| s.profile).collect::<Vec<_>>(),
        [
            PermissionProfile::Observe,
            PermissionProfile::Workspace,
            PermissionProfile::FullControl
        ]
    );
    assert_eq!(
        specs
            .iter()
            .map(|s| (s.workspace_read_only(), s.network_mode()))
            .collect::<Vec<_>>(),
        [
            (true, orbit_daemon::runtime::NetworkMode::None),
            (false, orbit_daemon::runtime::NetworkMode::Bridge),
            (false, orbit_daemon::runtime::NetworkMode::Bridge)
        ]
    );
}
#[tokio::test]
async fn repeated_start_and_stop_are_idempotent_without_duplicate_calls() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    rt.clone()
        .with_inspect(Some(info(&w.runtime.as_ref().unwrap().container_id, w.id)));
    m.stop(w.id).await.unwrap();
    rt.reset_calls();
    let restarted = m.start(w.id).await.unwrap();
    assert_eq!(restarted.state, LifecycleState::Running);
    assert_eq!(
        rt.start_ids(),
        vec![w.runtime.as_ref().unwrap().container_id.clone()]
    );
    assert_eq!(rt.calls().iter().filter(|x| *x == "create").count(), 0);
    assert!(rt.calls().contains(&"inspect".into()));
    assert!(rt.calls().contains(&"healthy".into()));
    let mutations = rt.start_ids().len() + rt.stop_ids().len() + rt.specs().len();
    m.start(w.id).await.unwrap();
    assert_eq!(
        rt.start_ids().len() + rt.stop_ids().len() + rt.specs().len(),
        mutations
    );
    m.stop(w.id).await.unwrap();
    m.stop(w.id).await.unwrap();
    assert_eq!(rt.stop_ids().len(), 1);
}

#[tokio::test]
async fn stop_missing_container_converges_stopped_state_without_runtime_stop() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    m.stop(w.id).await.unwrap();
    rt.reset_calls();
    rt.clone().with_inspect(None);

    let first = m.stop(w.id).await.unwrap();
    assert_eq!(first.state, LifecycleState::Stopped);
    assert!(first.runtime.is_none());
    assert_eq!(rt.stop_ids().len(), 0);
    assert!(m.store.get(w.id).unwrap().unwrap().runtime.is_none());

    let second = m.stop(w.id).await.unwrap();
    assert_eq!(second.state, LifecycleState::Stopped);
    assert!(second.runtime.is_none());
    assert_eq!(rt.stop_ids().len(), 0);
}

#[tokio::test]
async fn start_recreates_a_missing_container_exactly_once() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    rt.0.lock().unwrap().inspect = None;
    m.stop(w.id).await.unwrap();
    let restarted = m.start(w.id).await.unwrap();
    assert_eq!(restarted.state, LifecycleState::Running);
    assert!(restarted.runtime.is_some());
    assert_eq!(rt.calls().iter().filter(|x| *x == "create").count(), 2);
}
#[tokio::test]
async fn partial_start_and_stop_errors_leave_failed_not_transient() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    m.stop(w.id).await.unwrap();
    let credential = m
        .store
        .get_runtime_secret(w.id)
        .unwrap()
        .unwrap()
        .webtop_password;
    rt.clone().with_inspect(Some(info("existing", w.id)));
    rt.fail(RuntimeError::Command(format!("leaked {credential}")));
    let e = m.start(w.id).await.unwrap_err();
    assert!(!format!("{e:?}{e}").contains(&credential));
    assert_eq!(
        m.store.get(w.id).unwrap().unwrap().state,
        LifecycleState::Failed
    );
}

#[tokio::test]
async fn start_hook_observes_starting_state_before_runtime_mutation() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    m.stop(w.id).await.unwrap();
    let store = m.store.clone();
    rt.on_start(move || {
        assert_eq!(
            store.get(w.id).unwrap().unwrap().state,
            LifecycleState::Starting
        )
    });
    m.start(w.id).await.unwrap();
}
#[tokio::test]
async fn reset_removes_only_owned_container_and_volume_rotates_secret_and_preserves_host_marker() {
    let rt = FakeContainerRuntime::default();
    let (m, d, w) = created(rt.clone()).await;
    let marker = d.path().join("marker");
    std::fs::write(&marker, "keep").unwrap();
    let old_runtime = w.runtime.clone().unwrap();
    rt.clone()
        .with_inspect(Some(info(&old_runtime.container_id, w.id)));
    m.stop(w.id).await.unwrap();
    let old = m
        .store
        .get_runtime_secret(w.id)
        .unwrap()
        .unwrap()
        .webtop_password;
    m.reset(w.id).await.unwrap();
    assert_ne!(
        old,
        m.store
            .get_runtime_secret(w.id)
            .unwrap()
            .unwrap()
            .webtop_password
    );
    assert_eq!(rt.stop_ids(), vec![old_runtime.container_id.clone()]);
    assert_eq!(rt.remove_ids(), vec![old_runtime.container_id.clone()]);
    assert_eq!(rt.volume_names(), vec![old_runtime.volume_name.clone()]);
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "keep");
    assert_eq!(m.get(w.id).await.unwrap().state, LifecycleState::Running);
}

#[tokio::test]
async fn legacy_image_upgrade_recreates_only_the_container_and_preserves_config_volume_and_secret()
{
    let rt = FakeContainerRuntime::default();
    let (m, d) = manager(rt.clone()).await;
    let mut workspace = Workspace::new(
        "legacy".into(),
        d.path().to_path_buf(),
        PermissionProfile::Workspace,
        resources(),
    );
    let id = workspace.id;
    let volume = format!("orbit-config-{id}");
    workspace.state = LifecycleState::Running;
    workspace.runtime = Some(RuntimeMetadata {
        container_id: "legacy-container".into(),
        container_name: format!("orbit-{id}"),
        volume_name: volume.clone(),
        upstream_port: 3000,
        image_ref: "orbit-webtop:0.1.0".into(),
    });
    let secret = RuntimeSecret {
        workspace_id: id,
        webtop_password: "keep-this-secret".into(),
    };
    m.store
        .insert_workspace_with_secret(&workspace, &secret)
        .unwrap();
    let legacy = legacy_info("legacy-container", id);
    rt.clone()
        .with_inspect(Some(legacy.clone()))
        .with_managed(vec![legacy]);

    assert_eq!(m.reconcile_and_migrate().await.unwrap().1, vec![id]);

    let upgraded = m.get(id).await.unwrap();
    assert_eq!(upgraded.state, LifecycleState::Running);
    assert_eq!(upgraded.runtime.unwrap().volume_name, volume);
    assert_eq!(rt.remove_ids(), vec!["legacy-container"]);
    assert!(rt.volume_names().is_empty());
    assert_eq!(rt.specs()[0].image_ref, "orbit-webtop:0.4.0");
    assert_eq!(
        rt.specs()[0].labels.get("com.orbit.image").unwrap(),
        "webtop-ubuntu-xfce-v2"
    );
    assert_eq!(
        m.store
            .get_runtime_secret(id)
            .unwrap()
            .unwrap()
            .webtop_password,
        "keep-this-secret"
    );
}

#[tokio::test]
async fn legacy_image_upgrade_recovers_unique_container_when_runtime_metadata_is_missing() {
    let rt = FakeContainerRuntime::default();
    let (m, d) = manager(rt.clone()).await;
    let mut workspace = Workspace::new(
        "legacy crash".into(),
        d.path().to_path_buf(),
        PermissionProfile::Workspace,
        resources(),
    );
    let id = workspace.id;
    workspace.state = LifecycleState::Starting;
    let secret = RuntimeSecret {
        workspace_id: id,
        webtop_password: "preserved-after-crash".into(),
    };
    m.store
        .insert_workspace_with_secret(&workspace, &secret)
        .unwrap();
    let legacy = legacy_info("unrecorded-legacy", id);
    rt.clone()
        .with_inspect(Some(legacy.clone()))
        .with_managed(vec![legacy]);

    assert_eq!(m.reconcile_and_migrate().await.unwrap().1, vec![id]);

    let upgraded = m.get(id).await.unwrap();
    assert_eq!(upgraded.state, LifecycleState::Running);
    assert_eq!(
        upgraded.runtime.unwrap().volume_name,
        format!("orbit-config-{id}")
    );
    assert_eq!(rt.remove_ids(), vec!["unrecorded-legacy"]);
    assert!(rt.volume_names().is_empty());
}
#[tokio::test]
async fn delete_is_idempotent_removes_db_and_secret_and_preserves_host_marker() {
    let rt = FakeContainerRuntime::default();
    let (m, d, w) = created(rt.clone()).await;
    let marker = d.path().join("marker");
    std::fs::write(&marker, "keep").unwrap();
    let old = w.runtime.clone().unwrap();
    rt.clone().with_inspect(Some(info(&old.container_id, w.id)));
    m.stop(w.id).await.unwrap();
    m.delete(w.id).await.unwrap();
    m.delete(w.id).await.unwrap();
    assert!(m.get(w.id).await.is_err());
    assert_eq!(rt.remove_ids(), vec![old.container_id]);
    assert_eq!(rt.volume_names(), vec![old.volume_name]);
    assert!(m.store.get_runtime_secret(w.id).unwrap().is_none());
    assert!(m.store.get(w.id).unwrap().is_none());
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "keep");
    let calls = rt.calls();
    m.delete(w.id).await.unwrap();
    assert_eq!(rt.calls(), calls);
}
#[tokio::test]
async fn ownership_mismatch_for_each_required_label_and_image_blocks_start_stop_reset_delete_removal()
 {
    for field in [
        "managed",
        "workspace-id",
        "schema",
        "image-label",
        "actual-image",
    ] {
        for op in ["start", "stop", "reset", "delete"] {
            let rt = FakeContainerRuntime::default();
            let (m, _, w) = created(rt.clone()).await;
            rt.clone().with_inspect(Some(info("id", w.id)));
            if op != "stop" {
                m.stop(w.id).await.unwrap();
            }
            let mut i = info("id", w.id);
            match field {
                "managed" => {
                    i.labels.insert("com.orbit.managed".into(), "false".into());
                }
                "workspace-id" => {
                    i.labels.insert(
                        "com.orbit.workspace-id".into(),
                        uuid::Uuid::new_v4().to_string(),
                    );
                }
                "schema" => {
                    i.labels.insert("com.orbit.schema".into(), "99".into());
                }
                "image-label" => {
                    i.labels.insert("com.orbit.image".into(), "wrong".into());
                }
                _ => i.image = "wrong".into(),
            }
            rt.clone().with_inspect(Some(i));
            rt.reset_calls();
            let result = match op {
                "start" => m.start(w.id).await.map(|_| ()),
                "stop" => m.stop(w.id).await.map(|_| ()),
                "reset" => m.reset(w.id).await.map(|_| ()),
                _ => m.delete(w.id).await,
            };
            assert!(
                matches!(result, Err(ManagerError::Ownership)),
                "{field} {op}"
            );
            assert!(
                !rt.calls()
                    .iter()
                    .any(|c| ["start", "stop", "remove", "remove_volume"].contains(&c.as_str())),
                "{field} {op}"
            );
        }
    }
}
#[tokio::test]
async fn workspace_and_manager_errors_debug_display_never_contain_password() {
    let rt = FakeContainerRuntime::default();
    let (m, _d, w) = created(rt.clone()).await;
    let credential = m
        .store
        .get_runtime_secret(w.id)
        .unwrap()
        .unwrap()
        .webtop_password;
    m.stop(w.id).await.unwrap();
    rt.fail(RuntimeError::Command(format!(
        "credential leaked: {credential}"
    )));
    let e = m.start(w.id).await.unwrap_err();
    assert!(!format!("{e:?}{e}").contains(&credential));
    assert_eq!(m.get(w.id).await.unwrap().state, LifecycleState::Failed);
}

#[tokio::test]
async fn concurrent_starts_are_serialized_and_start_once() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    m.stop(w.id).await.unwrap();
    rt.set_inspect_running(false);
    rt.reset_calls();
    let (a, b) = tokio::join!(m.start(w.id), m.start(w.id));
    assert!(a.is_ok() && b.is_ok());
    assert_eq!(rt.start_ids().len(), 1);
}

#[tokio::test]
async fn provision_adopts_one_managed_orphan() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    let r = w.runtime.clone().unwrap();
    m.stop(w.id).await.unwrap();
    let orphan = info(&r.container_id, w.id);
    let started_orphan = orphan.clone();
    let on_start = rt.clone();
    rt.on_start(move || {
        on_start.clone().with_inspect(Some(started_orphan.clone()));
    });
    rt.clone().with_inspect(None).with_managed(vec![orphan]);
    let started = m.start(w.id).await.unwrap();
    assert_eq!(started.state, LifecycleState::Running);
    assert!(rt.managed_calls() > 0);
    assert_eq!(rt.specs().len(), 1);
}

#[tokio::test]
async fn provision_rejects_multiple_or_mismatched_orphans_without_mutation() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    m.stop(w.id).await.unwrap();
    let mut wrong = info("wrong", w.id);
    wrong
        .labels
        .insert("com.orbit.managed".into(), "false".into());
    rt.clone()
        .with_inspect(None)
        .with_managed(vec![info("one", w.id), info("two", w.id)]);
    rt.reset_calls();
    let _ = m.start(w.id).await;
    assert_eq!(rt.start_ids().len(), 0);
    assert_eq!(rt.remove_ids().len(), 0);
    rt.clone().with_managed(vec![wrong]);
    let _ = m.start(w.id).await;
    assert_eq!(rt.start_ids().len(), 0);
}

#[tokio::test]
async fn missing_port_persists_identity_and_failed() {
    let rt = FakeContainerRuntime::default();
    rt.clone().with_created_port(None);
    let (m, d) = manager(rt.clone()).await;
    let result = m
        .create(
            "portless".into(),
            d.path().into(),
            PermissionProfile::Workspace,
            resources(),
        )
        .await;
    let error = result.expect_err("missing published port must fail safely");
    assert!(error.to_string().contains("did not publish a port"));
    let workspaces = m.list().await.unwrap();
    assert_eq!(workspaces.len(), 1);
    let failed = &workspaces[0];
    assert_eq!(failed.state, LifecycleState::Failed);
    let runtime = failed.runtime.as_ref().expect("identity is persisted");
    assert_eq!(runtime.container_id, format!("{}", failed.id));
    assert_eq!(runtime.container_name, format!("orbit-{}", failed.id));
    assert_eq!(runtime.volume_name, format!("orbit-config-{}", failed.id));
    assert_eq!(runtime.upstream_port, 0);
    assert_eq!(
        rt.calls().iter().filter(|call| *call == "create").count(),
        1
    );
    let start = m
        .start(failed.id)
        .await
        .expect_err("start must retain the missing-port failure");
    assert!(start.to_string().contains("did not publish a port"));
    let reset = m
        .reset(failed.id)
        .await
        .expect_err("reset reprovision must retain port failure");
    assert!(reset.to_string().contains("did not publish a port"));
    m.delete(failed.id).await.unwrap();
    assert!(rt.remove_ids().contains(&runtime.container_id));
}

#[tokio::test]
async fn provision_discovers_ephemeral_port_after_container_start() {
    let rt = FakeContainerRuntime::default();
    rt.clone().with_created_port(None);
    let on_start = rt.clone();
    rt.on_start(move || on_start.set_inspect_port(Some(4321)));
    let (m, d) = manager(rt.clone()).await;

    let workspace = m
        .create(
            "ephemeral-port".into(),
            d.path().into(),
            PermissionProfile::Workspace,
            resources(),
        )
        .await
        .unwrap();

    assert_eq!(workspace.state, LifecycleState::Running);
    assert_eq!(workspace.runtime.unwrap().upstream_port, 4321);
    assert_eq!(rt.start_ids().len(), 1);
}

#[tokio::test]
async fn runtime_state_drift_converges() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    m.stop(w.id).await.unwrap();
    rt.set_inspect_running(false);
    rt.reset_calls();
    let _ = m.start(w.id).await.unwrap();
    assert_eq!(rt.start_ids().len(), 1);
    rt.reset_calls();
    let _ = m.start(w.id).await.unwrap();
    assert_eq!(rt.start_ids().len(), 0);
    rt.set_inspect_running(true);
    rt.reset_calls();
    m.stop(w.id).await.unwrap();
    assert_eq!(rt.stop_ids().len(), 1);
}

#[tokio::test]
async fn reset_and_delete_are_serialized_without_resurrection() {
    let rt = FakeContainerRuntime::default();
    let (m, d, w) = created(rt.clone()).await;
    m.stop(w.id).await.unwrap();
    let marker = d.path().join("marker");
    std::fs::write(&marker, "keep").unwrap();
    let (reset, delete) = tokio::join!(m.reset(w.id), m.delete(w.id));
    assert!(reset.is_ok() || delete.is_ok());
    if delete.is_ok() {
        assert!(m.get(w.id).await.is_err());
    }
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "keep");
}

#[tokio::test]
async fn durable_starting_with_stopped_container_start_recovers_running() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    let r = w.runtime.clone().unwrap();
    rt.clone().with_inspect(Some(info(&r.container_id, w.id)));
    rt.set_inspect_running(false);
    set_stored_state(&m, w.id, LifecycleState::Starting);
    rt.reset_calls();
    let result = m.start(w.id).await.unwrap();
    assert_eq!(result.state, LifecycleState::Running);
    assert_eq!(rt.start_ids(), vec![r.container_id]);
    assert_eq!(rt.calls().iter().filter(|c| *c == "create").count(), 0);
}

#[tokio::test]
async fn durable_starting_with_running_container_start_health_checks_without_start() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    let r = w.runtime.clone().unwrap();
    rt.clone().with_inspect(Some(info(&r.container_id, w.id)));
    set_stored_state(&m, w.id, LifecycleState::Starting);
    rt.reset_calls();
    let result = m.start(w.id).await.unwrap();
    assert_eq!(result.state, LifecycleState::Running);
    assert!(rt.start_ids().is_empty());
    assert!(rt.calls().contains(&"healthy".into()));
}

#[tokio::test]
async fn durable_starting_stop_recovers_stopped() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    let r = w.runtime.clone().unwrap();
    rt.clone().with_inspect(Some(info(&r.container_id, w.id)));
    rt.set_inspect_running(false);
    set_stored_state(&m, w.id, LifecycleState::Starting);
    rt.reset_calls();
    let result = m.stop(w.id).await.unwrap();
    assert_eq!(result.state, LifecycleState::Stopped);
    assert!(rt.stop_ids().is_empty());
}

#[tokio::test]
async fn durable_starting_reset_recovers_and_recreates_once() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    let r = w.runtime.clone().unwrap();
    rt.clone().with_inspect(Some(info(&r.container_id, w.id)));
    set_stored_state(&m, w.id, LifecycleState::Starting);
    rt.reset_calls();
    let result = m.reset(w.id).await.unwrap();
    assert_eq!(result.state, LifecycleState::Running);
    assert_eq!(rt.remove_ids(), vec![r.container_id]);
    assert_eq!(rt.calls().iter().filter(|c| *c == "create").count(), 1);
}

#[tokio::test]
async fn durable_starting_delete_cleans_row_and_secret() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    let r = w.runtime.clone().unwrap();
    rt.clone().with_inspect(Some(info(&r.container_id, w.id)));
    set_stored_state(&m, w.id, LifecycleState::Starting);
    rt.reset_calls();
    m.delete(w.id).await.unwrap();
    assert!(m.store.get(w.id).unwrap().is_none());
    assert!(m.store.get_runtime_secret(w.id).unwrap().is_none());
    assert_eq!(rt.remove_ids(), vec![r.container_id]);
    assert_eq!(rt.volume_names(), vec![r.volume_name]);
}

#[tokio::test]
async fn durable_deleting_delete_retries_cleanup() {
    let rt = FakeContainerRuntime::default();
    let (m, _, w) = created(rt.clone()).await;
    let r = w.runtime.clone().unwrap();
    rt.clone().with_inspect(Some(info(&r.container_id, w.id)));
    set_stored_state(&m, w.id, LifecycleState::Deleting);
    rt.reset_calls();
    m.delete(w.id).await.unwrap();
    assert!(m.store.get(w.id).unwrap().is_none());
    assert_eq!(rt.remove_ids(), vec![r.container_id]);
    assert_eq!(rt.volume_names(), vec![r.volume_name]);
}
