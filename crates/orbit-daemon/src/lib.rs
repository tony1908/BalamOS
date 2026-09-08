use crate::agent_manager::{AgentManager, AgentManagerError};
mod agents_md;
use crate::routine_manager::{RoutineManager, RoutineManagerError};
use anyhow::Context;
use fs2::FileExt;
#[cfg(windows)]
use interprocess::local_socket::{GenericNamespaced, ToNsName};
use interprocess::local_socket::{
    ListenerOptions,
    tokio::{Stream, prelude::*},
};
use orbit_protocol::{
    ApiError, Command, DesktopSession, ErrorCode, MAX_IPC_FRAME_BYTES, PROTOCOL_VERSION, Request,
    Response, ResponseBody,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
#[cfg(unix)]
use std::{
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::PathBuf,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tokio::time::{Duration, timeout};
use uuid::Uuid;
pub mod agent_manager;
pub mod antigravity_harness;
#[allow(dead_code)]
mod antigravity_protocol;
pub mod claude_harness;
mod claude_protocol;
pub mod codex_auth;
pub mod codex_harness;
pub mod codex_protocol;
pub mod desktop_gateway;
pub mod docker_cli;
pub mod opencode_harness;
#[allow(dead_code)]
mod opencode_protocol;
pub mod routine_manager;
pub mod runtime;
pub mod workspace_manager;
const MAX_FRAME_BYTES: usize = MAX_IPC_FRAME_BYTES;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONNECTIONS: usize = 64;
pub struct Daemon {
    socket_name: String,
    token: String,
    manager: Arc<workspace_manager::WorkspaceManager>,
    instance_lock_path: std::path::PathBuf,
}

/// The process-owned coordination point for runtime and desktop lifecycle.
/// Connections only retain a weak reference, so dropping the daemon root also
/// drops the gateway and its listener task.
struct DaemonService {
    manager: Arc<workspace_manager::WorkspaceManager>,
    agents: Arc<AgentManager>,
    routines: Arc<RoutineManager>,
    gateway: desktop_gateway::DesktopGateway,
    reconcile: RwLock<()>,
    workspace_locks: Mutex<HashMap<Uuid, Arc<Mutex<()>>>>,
}

impl DaemonService {
    async fn workspace_lock(&self, id: Uuid) -> Arc<Mutex<()>> {
        let mut locks = self.workspace_locks.lock().await;
        locks
            .entry(id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    async fn revoke_all(&self) {
        self.agents.stop_all().await;
        if let Ok(workspaces) = self.manager.list().await {
            for workspace in workspaces {
                self.gateway.revoke_workspace(workspace.id).await;
            }
        }
    }
}
impl Daemon {
    pub fn new(
        socket_name: String,
        token: String,
        manager: Arc<workspace_manager::WorkspaceManager>,
    ) -> Self {
        let instance_lock_path = lock_path(&socket_name).expect("valid socket lock path");
        Self {
            socket_name,
            token,
            manager,
            instance_lock_path,
        }
    }
    pub fn with_lock_path(
        socket_name: String,
        token: String,
        manager: Arc<workspace_manager::WorkspaceManager>,
        path: std::path::PathBuf,
    ) -> Self {
        Self {
            socket_name,
            token,
            manager,
            instance_lock_path: path,
        }
    }
    pub async fn run(self) -> anyhow::Result<()> {
        validate_socket_name(&self.socket_name)?;
        let _lock = acquire_instance_lock_path(&self.instance_lock_path)?;
        let name = endpoint(&self.socket_name)?;
        #[cfg(unix)]
        let socket_path = socket_path(&self.socket_name);
        let service = Arc::new(DaemonService {
            agents: Arc::new(AgentManager::new(Arc::clone(&self.manager))),
            routines: Arc::new(RoutineManager::new(Arc::clone(&self.manager.store))),
            manager: Arc::clone(&self.manager),
            gateway: desktop_gateway::DesktopGateway::bind_loopback().await?,
            reconcile: RwLock::new(()),
            workspace_locks: Mutex::new(HashMap::new()),
        });
        let _scheduler = routine_manager::spawn_scheduler(
            Arc::clone(&self.manager.store),
            Arc::clone(&service.agents),
            Arc::clone(&self.manager),
        );
        let startup_report = {
            let _guard = service.reconcile.write().await;
            service.revoke_all().await;
            service.manager.reconcile_and_migrate().await
        };
        match startup_report {
            Ok((report, upgraded)) => {
                eprintln!(
                    "startup reconciliation: workspaces={} containers={} running={} stopped={} failed={} orphans={} diagnostics={} upgrades={}",
                    report.examined_workspaces,
                    report.examined_containers,
                    report.running,
                    report.stopped,
                    report.failed,
                    report.orphan_container_ids.len(),
                    report.diagnostics.len(),
                    upgraded.len()
                )
            }
            Err(_) => eprintln!("startup reconciliation failed; serving read-only until retry"),
        }
        #[cfg(unix)]
        let mut _cleanup = SocketCleanup {
            path: socket_path.clone(),
            identity: None,
        };
        #[cfg(unix)]
        let _ = std::fs::remove_file(&socket_path);
        let options = ListenerOptions::new().name(name);
        #[cfg(target_os = "linux")]
        use interprocess::os::unix::local_socket::ListenerOptionsExt;
        #[cfg(target_os = "linux")]
        let options = options.mode(0o600);
        let listener = options.create_tokio()?;
        #[cfg(unix)]
        std::fs::set_permissions(&socket_path, PermissionsExt::from_mode(0o600))?;
        #[cfg(unix)]
        let metadata = std::fs::symlink_metadata(&socket_path)?;
        #[cfg(unix)]
        {
            // SAFETY: geteuid has no preconditions and only reads the effective user ID.
            let uid = unsafe { libc::geteuid() };
            anyhow::ensure!(
                metadata.file_type().is_socket()
                    && metadata.uid() == uid
                    && metadata.mode() & 0o077 == 0,
                "unsafe socket metadata"
            );
            _cleanup.identity = Some((metadata.dev(), metadata.ino(), uid));
        }
        let permits = Arc::new(Semaphore::new(MAX_CONNECTIONS));
        loop {
            let permit = permits.clone().acquire_owned().await?;
            let stream = listener.accept().await?;
            let token = self.token.clone();
            let service = Arc::downgrade(&service);
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(error) = handle_connection(stream, &token, service).await {
                    eprintln!("connection error: {error:#}");
                }
            });
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn oversized_response_is_replaced_with_bounded_sanitized_error() {
        let response = Response {
            protocol: PROTOCOL_VERSION,
            id: Uuid::new_v4(),
            body: ResponseBody::Error(ApiError {
                code: ErrorCode::Internal,
                message: "x".repeat(MAX_IPC_FRAME_BYTES),
                retryable: true,
            }),
        };
        let payload = encode_response(&response).unwrap();
        assert!(payload.len() <= MAX_IPC_FRAME_BYTES);
        assert!(
            String::from_utf8(payload)
                .unwrap()
                .contains("response exceeds IPC transport limit")
        );
    }

    #[test]
    fn unavailable_harness_error_is_invalid_request_and_not_retryable() {
        let message = "API key not configured (set ORBIT_EXAMPLE_API_KEY)";
        let response = agent_error(AgentManagerError::Harness(
            codex_harness::HarnessError::Unavailable(message.into()),
        ));

        match response {
            ResponseBody::Error(error) => {
                assert_eq!(error.code, ErrorCode::InvalidRequest);
                assert_eq!(error.message, message);
                assert!(!error.retryable);
            }
            other => panic!("expected error response, got {other:?}"),
        }
    }

    #[test]
    fn governance_error_maps_to_blocked_code_with_human_reason() {
        let response = agent_error(AgentManagerError::Governance(vec![
            orbit_domain::GovernanceBlockedReason::Disabled,
        ]));
        match response {
            ResponseBody::Error(error) => {
                assert_eq!(error.code, ErrorCode::GovernanceBlocked);
                assert!(!error.retryable);
                assert!(error.message.contains("governance is disabled"));
            }
            other => panic!("expected error response, got {other:?}"),
        }
    }

    #[test]
    fn rejects_runtime_directory_symlink_without_chmod_victim() {
        let root = tempfile::tempdir().unwrap();
        let victim = root.path().join("victim");
        std::fs::create_dir(&victim).unwrap();
        std::fs::set_permissions(&victim, PermissionsExt::from_mode(0o755)).unwrap();
        let link = root.path().join("runtime-link");
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        // SAFETY: geteuid has no preconditions.
        let uid = unsafe { libc::geteuid() };
        assert!(ensure_runtime_dir(&link, uid).is_err());
        assert_eq!(
            std::fs::symlink_metadata(&victim).unwrap().mode() & 0o777,
            0o755
        );
    }

    #[test]
    fn instance_lock_is_keyed_by_explicit_data_path_before_reconcile() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("daemon.lock");
        let first = acquire_instance_lock_path(&path).unwrap();
        assert!(acquire_instance_lock_path(&path).is_err());
        drop(first);
        assert!(acquire_instance_lock_path(&path).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn lock_file_permissions_are_private_and_symlinks_rejected() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("daemon.lock");
        let lock = acquire_instance_lock_path(&path).unwrap();
        assert_eq!(lock.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        drop(lock);
        let target = root.path().join("target");
        std::fs::write(&target, b"x").unwrap();
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(acquire_instance_lock_path(&path).is_err());
        assert_eq!(std::fs::metadata(&target).unwrap().uid(), unsafe {
            libc::geteuid()
        });
    }

    #[tokio::test]
    async fn desktop_session_is_loopback_only_and_credentials_are_redacted() {
        let gateway = desktop_gateway::DesktopGateway::bind_loopback()
            .await
            .unwrap();
        let credential = desktop_gateway::DesktopCredential::basic("agent", "unit-secret").unwrap();
        let session = gateway
            .create_session(
                Uuid::new_v4(),
                "127.0.0.1:3000".parse().unwrap(),
                credential.clone(),
                Duration::from_secs(300),
            )
            .await
            .unwrap();

        let parsed = reqwest::Url::parse(&session.url).unwrap();
        assert_eq!(parsed.host_str(), Some("127.0.0.1"));
        assert!(!format!("{session:?}").contains("unit-secret"));
        assert!(!format!("{credential:?}").contains("unit-secret"));

        assert!(
            gateway
                .create_session(
                    Uuid::new_v4(),
                    "192.0.2.1:3000".parse().unwrap(),
                    credential,
                    Duration::from_secs(300),
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn creating_a_second_desktop_session_revokes_the_first() {
        let gateway = desktop_gateway::DesktopGateway::bind_loopback()
            .await
            .unwrap();
        let workspace = Uuid::new_v4();
        let credential = desktop_gateway::DesktopCredential::basic("agent", "secret").unwrap();
        let first = gateway
            .create_session(
                workspace,
                "127.0.0.1:3000".parse().unwrap(),
                credential.clone(),
                Duration::from_secs(300),
            )
            .await
            .unwrap();
        assert_eq!(gateway.session_count().await, 1);
        gateway.revoke_workspace(workspace).await;
        assert_eq!(gateway.session_count().await, 0);
        let second = gateway
            .create_session(
                workspace,
                "127.0.0.1:3000".parse().unwrap(),
                credential,
                Duration::from_secs(300),
            )
            .await
            .unwrap();
        assert_ne!(first.url, second.url);
        assert_eq!(gateway.session_count().await, 1);
    }
}

#[cfg(unix)]
fn socket_path(logical: &str) -> PathBuf {
    // SAFETY: geteuid has no preconditions and only reads the effective user ID.
    let uid = unsafe { libc::geteuid() };
    PathBuf::from(format!("/tmp/orbit-runtime-{uid}/{logical}.sock"))
}

#[cfg(unix)]
struct SocketCleanup {
    path: PathBuf,
    identity: Option<(u64, u64, u32)>,
}
#[cfg(unix)]
impl Drop for SocketCleanup {
    fn drop(&mut self) {
        if let Some((dev, ino, uid)) = self.identity
            && let Ok(meta) = std::fs::symlink_metadata(&self.path)
            && meta.file_type().is_socket()
            && meta.dev() == dev
            && meta.ino() == ino
            && meta.uid() == uid
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(unix)]
fn endpoint(logical: &str) -> anyhow::Result<interprocess::local_socket::Name<'static>> {
    if logical.is_empty()
        || logical.len() > 64
        || !logical
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        anyhow::bail!("invalid local socket name");
    }
    // SAFETY: geteuid has no preconditions and only reads the effective user ID.
    let uid = unsafe { libc::geteuid() };
    let dir = PathBuf::from(format!("/tmp/orbit-runtime-{uid}"));
    ensure_runtime_dir(&dir, uid)?;
    interprocess::local_socket::ToFsName::to_fs_name::<interprocess::local_socket::GenericFilePath>(
        dir.join(format!("{logical}.sock")),
    )
    .map_err(Into::into)
}
#[cfg(unix)]
fn ensure_runtime_dir(dir: &std::path::Path, uid: u32) -> anyhow::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    let result = std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700).create(dir);
    if let Err(error) = result
        && error.kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(error.into());
    }
    let d = std::fs::symlink_metadata(dir)?;
    if !d.file_type().is_dir() || d.uid() != uid || d.mode() & 0o777 != 0o700 {
        anyhow::bail!("unsafe runtime directory");
    }
    Ok(())
}
#[cfg(windows)]
fn endpoint(logical: &str) -> anyhow::Result<interprocess::local_socket::Name<'static>> {
    // Windows current-user ACL hardening is Phase 5.
    logical
        .to_ns_name::<GenericNamespaced>()
        .map_err(Into::into)
}
async fn handle_connection(
    stream: Stream,
    token: &str,
    service: Weak<DaemonService>,
) -> anyhow::Result<()> {
    let (mut read, mut write) = stream.split();
    while let Some(line) = timeout(IO_TIMEOUT, read_frame(&mut read))
        .await
        .map_err(|_| anyhow::anyhow!("IPC read timed out"))??
    {
        let request: Request = serde_json::from_str(&line).context("invalid request JSON")?;
        let body = if request.protocol != PROTOCOL_VERSION {
            ResponseBody::Error(ApiError {
                code: ErrorCode::ProtocolMismatch,
                message: "unsupported protocol version".into(),
                retryable: false,
            })
        } else if request.token != token {
            ResponseBody::Error(ApiError {
                code: ErrorCode::Unauthorized,
                message: "invalid daemon capability token".into(),
                retryable: false,
            })
        } else {
            let Some(service) = service.upgrade() else {
                return Ok(());
            };
            dispatch(request.command, &service).await
        };
        let response = Response {
            protocol: PROTOCOL_VERSION,
            id: request.id,
            body,
        };
        let payload = encode_response(&response)?;
        timeout(IO_TIMEOUT, write.write_all(&payload))
            .await
            .map_err(|_| anyhow::anyhow!("IPC write timed out"))??;
    }
    Ok(())
}

fn encode_response(response: &Response) -> anyhow::Result<Vec<u8>> {
    let mut payload = serde_json::to_vec(response)?;
    payload.push(b'\n');
    if payload.len() > MAX_IPC_FRAME_BYTES {
        payload = serde_json::to_vec(&Response {
            protocol: response.protocol,
            id: response.id,
            body: ResponseBody::Error(ApiError {
                code: ErrorCode::Internal,
                message: "response exceeds IPC transport limit".into(),
                retryable: true,
            }),
        })?;
        payload.push(b'\n');
    }
    anyhow::ensure!(
        payload.len() <= MAX_IPC_FRAME_BYTES,
        "response error exceeds IPC transport limit"
    );
    Ok(payload)
}

fn internal_error(error: impl std::fmt::Display) -> ResponseBody {
    ResponseBody::Error(ApiError {
        code: ErrorCode::Internal,
        message: error.to_string(),
        retryable: true,
    })
}

async fn dispatch(command: Command, service: &DaemonService) -> ResponseBody {
    let manager = &service.manager;
    match command {
        Command::Ping => ResponseBody::Pong,
        Command::RuntimeStatus => ResponseBody::RuntimeStatus(manager.runtime_status().await),
        Command::ReconcileRuntime => {
            let _guard = service.reconcile.write().await;
            service.agents.stop_all().await;
            service.revoke_all().await;
            match manager.reconcile_and_migrate().await {
                Ok(_) => ResponseBody::RuntimeStatus(manager.runtime_status().await),
                Err(_) => ResponseBody::Error(ApiError {
                    code: ErrorCode::Internal,
                    message: "runtime reconciliation failed".into(),
                    retryable: true,
                }),
            }
        }
        Command::ListWorkspaces => match manager.list().await {
            Ok(workspaces) => {
                ResponseBody::Workspaces(workspaces.into_iter().map(Into::into).collect())
            }
            Err(error) => internal_error(error),
        },
        Command::GetGovernancePolicy => match service.agents.get_governance(None).await {
            Ok(view) => ResponseBody::Governance(view),
            Err(error) => agent_error(error),
        },
        Command::SaveGovernancePolicy { policy } => {
            match service.agents.save_global_governance(&policy).await {
                Ok(view) => ResponseBody::Governance(view),
                Err(error) => agent_error(error),
            }
        }
        Command::GetWorkspaceGovernance { workspace_id } => {
            match service.agents.get_governance(Some(workspace_id)).await {
                Ok(view) => ResponseBody::Governance(view),
                Err(error) => agent_error(error),
            }
        }
        Command::SaveWorkspaceGovernance {
            workspace_id,
            policy,
        } => match service
            .agents
            .save_workspace_governance(workspace_id, &policy)
            .await
        {
            Ok(view) => ResponseBody::Governance(view),
            Err(error) => agent_error(error),
        },
        Command::ListGovernancePresets => match service.agents.list_governance_presets().await {
            Ok(presets) => ResponseBody::GovernancePresets(presets),
            Err(error) => agent_error(error),
        },
        Command::CreateGovernancePreset { name, policy } => {
            match service.agents.create_governance_preset(name, &policy).await {
                Ok(preset) => ResponseBody::GovernancePreset(preset),
                Err(error) => agent_error(error),
            }
        }
        Command::DeleteGovernancePreset { id } => {
            match service.agents.delete_governance_preset(id.clone()).await {
                Ok(()) => ResponseBody::GovernancePresetDeleted { id },
                Err(error) => agent_error(error),
            }
        }
        Command::ListGovernanceRules => match service.agents.list_governance_rules().await {
            Ok(rules) => ResponseBody::GovernanceRules(rules),
            Err(error) => agent_error(error),
        },
        Command::CreateGovernanceRule { title, body } => {
            match service.agents.create_governance_rule(title, body).await {
                Ok(rule) => ResponseBody::GovernanceRule(rule),
                Err(error) => agent_error(error),
            }
        }
        Command::UpdateGovernanceRule { id, title, body } => {
            match service.agents.update_governance_rule(id, title, body).await {
                Ok(rule) => ResponseBody::GovernanceRule(rule),
                Err(error) => agent_error(error),
            }
        }
        Command::DeleteGovernanceRule { id } => {
            match service.agents.delete_governance_rule(id.clone()).await {
                Ok(()) => ResponseBody::GovernanceRuleDeleted { id },
                Err(error) => agent_error(error),
            }
        }
        Command::ListSecrets => service
            .agents
            .list_secrets()
            .await
            .map(ResponseBody::Secrets)
            .unwrap_or_else(agent_error),
        Command::CreateSecret {
            name,
            env_name,
            value,
        } => service
            .agents
            .create_secret(name, env_name, value)
            .await
            .map(ResponseBody::Secret)
            .unwrap_or_else(agent_error),
        Command::ReplaceSecret { id, value } => service
            .agents
            .replace_secret(id, value)
            .await
            .map(|_| ResponseBody::SecretDeleted { id })
            .unwrap_or_else(agent_error),
        Command::DeleteSecret { id } => service
            .agents
            .delete_secret(id)
            .await
            .map(|_| ResponseBody::SecretDeleted { id })
            .unwrap_or_else(agent_error),
        Command::AssignSecret {
            secret_id,
            workspace_id,
        } => service
            .agents
            .assign_secret(orbit_domain::SecretAssignment {
                secret_id,
                workspace_id,
            })
            .await
            .map(|_| {
                ResponseBody::SecretAssignment(orbit_domain::SecretAssignment {
                    secret_id,
                    workspace_id,
                })
            })
            .unwrap_or_else(agent_error),
        Command::ListSecretAssignments { workspace_id } => service
            .agents
            .list_assignments(workspace_id)
            .await
            .map(ResponseBody::SecretAssignments)
            .unwrap_or_else(agent_error),
        Command::UnassignSecret {
            secret_id,
            workspace_id,
        } => service
            .agents
            .unassign_secret(secret_id, workspace_id)
            .await
            .map(|_| ResponseBody::SecretDeleted { id: secret_id })
            .unwrap_or_else(agent_error),
        Command::CreateWorkspace {
            name,
            host_path,
            profile,
            harness,
            model,
            reasoning_effort,
            resources,
        } => match manager
            .create_with_harness(
                name,
                host_path,
                profile,
                harness,
                model,
                reasoning_effort,
                resources,
            )
            .await
        {
            Ok(w) => ResponseBody::Workspace(w.into()),
            Err(e) => manager_error(e),
        },
        Command::ListModels { harness } => ResponseBody::Models(manager.list_models(harness).await),
        Command::SetWorkspaceModel {
            workspace_id,
            model,
            reasoning_effort,
        } => manager
            .set_workspace_model(workspace_id, model, reasoning_effort)
            .await
            .map(|w| ResponseBody::Workspace(w.into()))
            .unwrap_or_else(manager_error),
        Command::ListApps => ResponseBody::Apps(manager.list_apps()),
        Command::SetWorkspaceApps { workspace_id, apps } => manager
            .set_workspace_apps(workspace_id, apps)
            .await
            .map(|w| ResponseBody::Workspace(w.into()))
            .unwrap_or_else(manager_error),
        Command::StartWorkspace { id } => {
            let _read = service.reconcile.read().await;
            let lock = service.workspace_lock(id).await;
            let _workspace = lock.lock().await;
            service.gateway.revoke_workspace(id).await;
            manager
                .start(id)
                .await
                .map(|w| ResponseBody::Workspace(w.into()))
                .unwrap_or_else(manager_error)
        }
        Command::StopWorkspace { id } => {
            let _read = service.reconcile.read().await;
            let lock = service.workspace_lock(id).await;
            let _workspace = lock.lock().await;
            service.agents.stop(id).await;
            service.gateway.revoke_workspace(id).await;
            manager
                .stop(id)
                .await
                .map(|w| ResponseBody::Workspace(w.into()))
                .unwrap_or_else(manager_error)
        }
        Command::ResetWorkspace { id } => {
            let _read = service.reconcile.read().await;
            let lock = service.workspace_lock(id).await;
            let _workspace = lock.lock().await;
            service.agents.stop(id).await;
            service.agents.clear(id).await;
            service.gateway.revoke_workspace(id).await;
            manager
                .reset(id)
                .await
                .map(|w| ResponseBody::Workspace(w.into()))
                .unwrap_or_else(manager_error)
        }
        Command::CreateDesktopSession { id } => {
            let _read = service.reconcile.read().await;
            let lock = service.workspace_lock(id).await;
            let _workspace = lock.lock().await;
            match manager.desktop_target(id).await {
                Ok((target, password)) => {
                    let Ok(upstream) = target.parse::<SocketAddr>() else {
                        return internal_error("invalid desktop target");
                    };
                    let Ok(credential) =
                        desktop_gateway::DesktopCredential::basic("agent", password)
                    else {
                        return internal_error("invalid desktop credential");
                    };
                    service.gateway.revoke_workspace(id).await;
                    match service
                        .gateway
                        .create_session(id, upstream, credential, Duration::from_secs(300))
                        .await
                    {
                        Ok(s) => ResponseBody::DesktopSession(DesktopSession {
                            url: s.url,
                            expires_at_unix_ms: s.expires_at_unix_ms,
                        }),
                        Err(e) => internal_error(e),
                    }
                }
                Err(e) => manager_error(e),
            }
        }
        Command::DeleteWorkspace { id } => {
            let _read = service.reconcile.read().await;
            let lock = service.workspace_lock(id).await;
            let _workspace = lock.lock().await;
            service.agents.stop(id).await;
            service.agents.clear(id).await;
            let _ = service.manager.store.delete_routines_for_workspace(id);
            let _ = service.manager.store.delete_bot_settings_for_workspace(id);
            let _ = service.manager.store.delete_skills_for_workspace(id);
            service.gateway.revoke_workspace(id).await;
            manager
                .delete(id)
                .await
                .map(|_| ResponseBody::Deleted { id })
                .unwrap_or_else(manager_error)
        }
        Command::CreateRoutine {
            workspace_id,
            name,
            instruction,
            interval_minutes,
            enabled,
        } => service
            .routines
            .create(workspace_id, name, instruction, interval_minutes, enabled)
            .await
            .map(ResponseBody::Routine)
            .unwrap_or_else(routine_error),
        Command::ListRoutines { workspace_id } => service
            .routines
            .list(workspace_id)
            .await
            .map(ResponseBody::Routines)
            .unwrap_or_else(routine_error),
        Command::SetBotSettings {
            workspace_id,
            display_name,
            label,
            description,
            avatar_color,
            notifications,
        } => service
            .manager
            .set_bot_settings(
                workspace_id,
                display_name,
                label,
                description,
                avatar_color,
                notifications,
            )
            .await
            .map(ResponseBody::BotSettings)
            .unwrap_or_else(manager_error),
        Command::ListBotSettings => service
            .manager
            .list_bot_settings()
            .await
            .map(ResponseBody::BotSettingsList)
            .unwrap_or_else(manager_error),
        Command::UpdateRoutine {
            id,
            name,
            instruction,
            interval_minutes,
            enabled,
        } => service
            .routines
            .update(id, name, instruction, interval_minutes, enabled)
            .await
            .map(ResponseBody::Routine)
            .unwrap_or_else(routine_error),
        Command::DeleteRoutine { id } => service
            .routines
            .delete(id)
            .await
            .map(|_| ResponseBody::RoutineDeleted { id })
            .unwrap_or_else(routine_error),
        Command::CreateSkill {
            workspace_id,
            name,
            instruction,
            enabled,
        } => service
            .manager
            .create_skill(workspace_id, name, instruction, enabled)
            .await
            .map(ResponseBody::Skill)
            .unwrap_or_else(manager_error),
        Command::ListSkills { workspace_id } => service
            .manager
            .list_skills(workspace_id)
            .await
            .map(ResponseBody::Skills)
            .unwrap_or_else(manager_error),
        Command::UpdateSkill {
            id,
            name,
            instruction,
            enabled,
        } => service
            .manager
            .update_skill(id, name, instruction, enabled)
            .await
            .map(ResponseBody::Skill)
            .unwrap_or_else(manager_error),
        Command::DeleteSkill { id } => service
            .manager
            .delete_skill(id)
            .await
            .map(|_| ResponseBody::SkillDeleted { id })
            .unwrap_or_else(manager_error),
        Command::SaveTemplate {
            name,
            settings,
            routines,
            skills,
        } => service
            .manager
            .save_template(name, settings, routines, skills)
            .await
            .map(ResponseBody::Template)
            .unwrap_or_else(manager_error),
        Command::ListTemplates => service
            .manager
            .list_templates()
            .await
            .map(ResponseBody::Templates)
            .unwrap_or_else(manager_error),
        Command::DeleteTemplate { id } => service
            .manager
            .delete_template(id)
            .await
            .map(|_| ResponseBody::TemplateDeleted { id })
            .unwrap_or_else(manager_error),
        Command::StartAgentSession { workspace_id } => service
            .agents
            .start(workspace_id)
            .await
            .map(ResponseBody::AgentSession)
            .unwrap_or_else(agent_error),
        Command::SendAgentPrompt {
            workspace_id,
            prompt,
        } => {
            let _read = service.reconcile.read().await;
            let lock = service.workspace_lock(workspace_id).await;
            let _workspace = lock.lock().await;
            service
                .agents
                .prompt(workspace_id, prompt)
                .await
                .map(ResponseBody::AgentSession)
                .unwrap_or_else(agent_error)
        }
        Command::PollAgentEvents {
            workspace_id,
            after,
        } => service
            .agents
            .poll(workspace_id, after)
            .await
            .map(ResponseBody::AgentEvents)
            .unwrap_or_else(agent_error),
        Command::ResolveAgentApproval {
            workspace_id,
            request_id,
            decision,
        } => service
            .agents
            .resolve(workspace_id, request_id, decision)
            .await
            .map(ResponseBody::AgentSession)
            .unwrap_or_else(agent_error),
        Command::InterruptAgent { workspace_id } => service
            .agents
            .interrupt(workspace_id)
            .await
            .map(ResponseBody::AgentSession)
            .unwrap_or_else(agent_error),
        Command::StopAgentSession { workspace_id } => {
            service.agents.stop(workspace_id).await;
            ResponseBody::AgentStopped { workspace_id }
        }
    }
}

fn agent_error(error: AgentManagerError) -> ResponseBody {
    // A missing/misconfigured harness credential is a user-fixable config
    // error, not an internal fault: surface the real reason and do not loop
    // Retry on it.
    if let AgentManagerError::Harness(crate::codex_harness::HarnessError::Unavailable(message)) =
        &error
    {
        return ResponseBody::Error(ApiError {
            code: ErrorCode::InvalidRequest,
            message: message.clone(),
            retryable: false,
        });
    }
    let (code, retryable) = match error {
        AgentManagerError::NotFound => (ErrorCode::NotFound, false),
        AgentManagerError::InvalidRequest(_) => (ErrorCode::InvalidRequest, false),
        AgentManagerError::Governance(_) => (ErrorCode::GovernanceBlocked, false),
        AgentManagerError::Workspace => (ErrorCode::InvalidTransition, false),
        AgentManagerError::Harness(_) => (ErrorCode::Internal, true),
    };
    ResponseBody::Error(ApiError {
        code,
        message: error.to_string(),
        retryable,
    })
}

fn routine_error(error: RoutineManagerError) -> ResponseBody {
    let (code, retryable) = match &error {
        RoutineManagerError::InvalidRequest => (ErrorCode::InvalidRequest, false),
        RoutineManagerError::NotFound => (ErrorCode::NotFound, false),
        RoutineManagerError::Store(_) => (ErrorCode::Internal, true),
    };
    ResponseBody::Error(ApiError {
        code,
        message: error.to_string(),
        retryable,
    })
}

fn validate_socket_name(logical: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !logical.is_empty()
            && logical.len() <= 64
            && logical
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid local socket name"
    );
    Ok(())
}

fn acquire_instance_lock_path(path: &std::path::Path) -> anyhow::Result<std::fs::File> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("lock path has no parent"))?;
    let metadata = std::fs::symlink_metadata(parent)?;
    anyhow::ensure!(
        metadata.file_type().is_dir(),
        "lock parent is not a directory"
    );
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let lock = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let m = lock.metadata()?;
        let uid = unsafe { libc::geteuid() };
        anyhow::ensure!(
            m.uid() == uid && m.permissions().mode() & 0o777 == 0o600,
            "unsafe lock file"
        );
    }
    lock.try_lock_exclusive()
        .map_err(|_| anyhow::anyhow!("daemon already running"))?;
    Ok(lock)
}

#[cfg(unix)]
fn lock_path(name: &str) -> anyhow::Result<PathBuf> {
    let dir = socket_path(name).parent().unwrap().to_path_buf();
    // SAFETY: geteuid only reads process credentials.
    let uid = unsafe { libc::geteuid() };
    ensure_runtime_dir(&dir, uid)?;
    Ok(dir.join(format!("{name}.lock")))
}
#[cfg(windows)]
fn lock_path(name: &str) -> anyhow::Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join("orbit-runtime-locks");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("{name}.lock")))
}

fn manager_error(e: workspace_manager::ManagerError) -> ResponseBody {
    let (code, retryable) = match e {
        workspace_manager::ManagerError::NotFound => (ErrorCode::NotFound, false),
        workspace_manager::ManagerError::Invalid(_) => (ErrorCode::InvalidRequest, false),
        workspace_manager::ManagerError::Transition(_) => (ErrorCode::InvalidTransition, false),
        workspace_manager::ManagerError::RuntimeUnavailable => {
            (ErrorCode::RuntimeUnavailable, true)
        }
        _ => (ErrorCode::Internal, true),
    };
    ResponseBody::Error(ApiError {
        code,
        message: match e {
            workspace_manager::ManagerError::RuntimeUnavailable => {
                "container runtime is unavailable; retry after reconciliation".into()
            }
            _ => e.to_string(),
        },
        retryable,
    })
}

async fn read_frame(read: &mut (impl AsyncRead + Unpin)) -> anyhow::Result<Option<String>> {
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if bytes.len() > MAX_FRAME_BYTES {
            anyhow::bail!("IPC frame exceeds 64 KiB");
        }
        let count = read.read(&mut byte).await?;
        if count == 0 {
            if bytes.is_empty() {
                return Ok(None);
            }
            anyhow::bail!("unexpected EOF in IPC frame");
        }
        if byte[0] == b'\n' {
            return Ok(Some(String::from_utf8(bytes)?));
        }
        bytes.push(byte[0]);
    }
}
