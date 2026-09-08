use interprocess::local_socket::{
    GenericFilePath, ToFsName,
    tokio::{Stream, prelude::*},
};
use orbit_domain::{
    AgentEventBatch, AgentSessionView, AppOption, ApprovalDecision, BotSettings, GovernancePolicy,
    GovernancePreset, GovernanceRule, GovernanceView, Harness, ModelOption, Routine,
    SecretAssignment, SecretInput, SecretMetadata, Skill, Template, TemplateRoutine,
    TemplateSettings, TemplateSkill, WorkspaceGovernancePolicy, WorkspaceView,
};
use orbit_protocol::{
    ApiError, Command, DesktopSession, ErrorCode, MAX_IPC_FRAME_BYTES, PROTOCOL_VERSION, Request,
    Response, ResponseBody, RuntimeStatus,
};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};
pub const MAX_FRAME_BYTES: usize = MAX_IPC_FRAME_BYTES;
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const RUNTIME_MUTATION_TIMEOUT: Duration = Duration::from_secs(35 * 60);
const RECONCILE_TIMEOUT: Duration = Duration::from_secs(2 * 60);
#[cfg(windows)]
use interprocess::local_socket::{GenericNamespaced, ToNsName};
#[cfg(unix)]
use std::{
    os::unix::fs::{FileTypeExt, MetadataExt},
    path::PathBuf,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct DaemonClient {
    socket_name: String,
    token: String,
    io_timeout: Duration,
}
impl DaemonClient {
    pub async fn list_secrets(&self) -> Result<Vec<SecretMetadata>, ClientError> {
        match self.send(Command::ListSecrets).await? {
            ResponseBody::Secrets(v) => Ok(v),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn create_secret(
        &self,
        name: String,
        env_name: String,
        value: String,
    ) -> Result<SecretMetadata, ClientError> {
        match self
            .send(Command::CreateSecret {
                name,
                env_name,
                value: SecretInput(value),
            })
            .await?
        {
            ResponseBody::Secret(v) => Ok(v),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn replace_secret(&self, id: Uuid, value: String) -> Result<(), ClientError> {
        match self
            .send(Command::ReplaceSecret {
                id,
                value: SecretInput(value),
            })
            .await?
        {
            ResponseBody::Deleted { .. } | ResponseBody::SecretDeleted { .. } => Ok(()),
            ResponseBody::Secret(_) => Ok(()),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn delete_secret(&self, id: Uuid) -> Result<Uuid, ClientError> {
        match self.send(Command::DeleteSecret { id }).await? {
            ResponseBody::SecretDeleted { id } => Ok(id),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn list_secret_assignments(
        &self,
        workspace_id: Uuid,
    ) -> Result<Vec<SecretAssignment>, ClientError> {
        match self
            .send(Command::ListSecretAssignments { workspace_id })
            .await?
        {
            ResponseBody::SecretAssignments(v) => Ok(v),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn assign_secret(
        &self,
        secret_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<SecretAssignment, ClientError> {
        match self
            .send(Command::AssignSecret {
                secret_id,
                workspace_id,
            })
            .await?
        {
            ResponseBody::SecretAssignment(v) => Ok(v),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn unassign_secret(
        &self,
        secret_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<(), ClientError> {
        match self
            .send(Command::UnassignSecret {
                secret_id,
                workspace_id,
            })
            .await?
        {
            ResponseBody::Deleted { .. } | ResponseBody::SecretAssignment(_) => Ok(()),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn governance_policy(&self) -> Result<GovernanceView, ClientError> {
        match self.send(Command::GetGovernancePolicy).await? {
            ResponseBody::Governance(p) => Ok(p),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn save_governance_policy(
        &self,
        policy: GovernancePolicy,
    ) -> Result<GovernanceView, ClientError> {
        match self.send(Command::SaveGovernancePolicy { policy }).await? {
            ResponseBody::Governance(p) => Ok(p),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn workspace_governance(
        &self,
        workspace_id: Uuid,
    ) -> Result<GovernanceView, ClientError> {
        match self
            .send(Command::GetWorkspaceGovernance { workspace_id })
            .await?
        {
            ResponseBody::Governance(p) => Ok(p),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn save_workspace_governance(
        &self,
        workspace_id: Uuid,
        policy: WorkspaceGovernancePolicy,
    ) -> Result<GovernanceView, ClientError> {
        match self
            .send(Command::SaveWorkspaceGovernance {
                workspace_id,
                policy,
            })
            .await?
        {
            ResponseBody::Governance(p) => Ok(p),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn list_governance_presets(&self) -> Result<Vec<GovernancePreset>, ClientError> {
        match self.send(Command::ListGovernancePresets).await? {
            ResponseBody::GovernancePresets(p) => Ok(p),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn create_governance_preset(
        &self,
        name: String,
        policy: GovernancePolicy,
    ) -> Result<GovernancePreset, ClientError> {
        match self
            .send(Command::CreateGovernancePreset { name, policy })
            .await?
        {
            ResponseBody::GovernancePreset(p) => Ok(p),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn delete_governance_preset(&self, id: String) -> Result<String, ClientError> {
        match self.send(Command::DeleteGovernancePreset { id }).await? {
            ResponseBody::GovernancePresetDeleted { id } => Ok(id),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn list_governance_rules(&self) -> Result<Vec<GovernanceRule>, ClientError> {
        match self.send(Command::ListGovernanceRules).await? {
            ResponseBody::GovernanceRules(rules) => Ok(rules),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn create_governance_rule(
        &self,
        title: String,
        body: String,
    ) -> Result<GovernanceRule, ClientError> {
        match self
            .send(Command::CreateGovernanceRule { title, body })
            .await?
        {
            ResponseBody::GovernanceRule(rule) => Ok(rule),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn update_governance_rule(
        &self,
        id: String,
        title: String,
        body: String,
    ) -> Result<GovernanceRule, ClientError> {
        match self
            .send(Command::UpdateGovernanceRule { id, title, body })
            .await?
        {
            ResponseBody::GovernanceRule(rule) => Ok(rule),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn delete_governance_rule(&self, id: String) -> Result<String, ClientError> {
        match self.send(Command::DeleteGovernanceRule { id }).await? {
            ResponseBody::GovernanceRuleDeleted { id } => Ok(id),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn create_routine(
        &self,
        workspace_id: Uuid,
        name: String,
        instruction: String,
        interval_minutes: u32,
        enabled: bool,
    ) -> Result<Routine, ClientError> {
        match self
            .send(Command::CreateRoutine {
                workspace_id,
                name,
                instruction,
                interval_minutes,
                enabled,
            })
            .await?
        {
            ResponseBody::Routine(r) => Ok(r),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn list_routines(&self, workspace_id: Uuid) -> Result<Vec<Routine>, ClientError> {
        match self.send(Command::ListRoutines { workspace_id }).await? {
            ResponseBody::Routines(r) => Ok(r),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn set_bot_settings(
        &self,
        workspace_id: Uuid,
        display_name: Option<String>,
        label: Option<String>,
        description: Option<String>,
        avatar_color: Option<String>,
        notifications: bool,
    ) -> Result<BotSettings, ClientError> {
        match self
            .send(Command::SetBotSettings {
                workspace_id,
                display_name,
                label,
                description,
                avatar_color,
                notifications,
            })
            .await?
        {
            ResponseBody::BotSettings(settings) => Ok(settings),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn list_bot_settings(&self) -> Result<Vec<BotSettings>, ClientError> {
        match self.send(Command::ListBotSettings).await? {
            ResponseBody::BotSettingsList(settings) => Ok(settings),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn update_routine(
        &self,
        id: Uuid,
        name: String,
        instruction: String,
        interval_minutes: u32,
        enabled: bool,
    ) -> Result<Routine, ClientError> {
        match self
            .send(Command::UpdateRoutine {
                id,
                name,
                instruction,
                interval_minutes,
                enabled,
            })
            .await?
        {
            ResponseBody::Routine(r) => Ok(r),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn delete_routine(&self, id: Uuid) -> Result<Uuid, ClientError> {
        match self.send(Command::DeleteRoutine { id }).await? {
            ResponseBody::RoutineDeleted { id } => Ok(id),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn create_skill(
        &self,
        workspace_id: Uuid,
        name: String,
        instruction: String,
        enabled: bool,
    ) -> Result<Skill, ClientError> {
        match self
            .send(Command::CreateSkill {
                workspace_id,
                name,
                instruction,
                enabled,
            })
            .await?
        {
            ResponseBody::Skill(s) => Ok(s),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn list_skills(&self, workspace_id: Uuid) -> Result<Vec<Skill>, ClientError> {
        match self.send(Command::ListSkills { workspace_id }).await? {
            ResponseBody::Skills(s) => Ok(s),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn update_skill(
        &self,
        id: Uuid,
        name: String,
        instruction: String,
        enabled: bool,
    ) -> Result<Skill, ClientError> {
        match self
            .send(Command::UpdateSkill {
                id,
                name,
                instruction,
                enabled,
            })
            .await?
        {
            ResponseBody::Skill(s) => Ok(s),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn delete_skill(&self, id: Uuid) -> Result<Uuid, ClientError> {
        match self.send(Command::DeleteSkill { id }).await? {
            ResponseBody::SkillDeleted { id } => Ok(id),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn save_template(
        &self,
        name: String,
        settings: TemplateSettings,
        routines: Vec<TemplateRoutine>,
        skills: Vec<TemplateSkill>,
    ) -> Result<Template, ClientError> {
        match self
            .send(Command::SaveTemplate {
                name,
                settings,
                routines,
                skills,
            })
            .await?
        {
            ResponseBody::Template(t) => Ok(t),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn list_templates(&self) -> Result<Vec<Template>, ClientError> {
        match self.send(Command::ListTemplates).await? {
            ResponseBody::Templates(t) => Ok(t),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn delete_template(&self, id: Uuid) -> Result<Uuid, ClientError> {
        match self.send(Command::DeleteTemplate { id }).await? {
            ResponseBody::TemplateDeleted { id } => Ok(id),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn runtime_status(&self) -> Result<RuntimeStatus, ClientError> {
        match self.send(Command::RuntimeStatus).await? {
            ResponseBody::RuntimeStatus(status) => Ok(status),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn reconcile_runtime(&self) -> Result<RuntimeStatus, ClientError> {
        match self.send(Command::ReconcileRuntime).await? {
            ResponseBody::RuntimeStatus(status) => Ok(status),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn list_models(&self, harness: Harness) -> Result<Vec<ModelOption>, ClientError> {
        match self.send(Command::ListModels { harness }).await? {
            ResponseBody::Models(models) => Ok(models),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn set_workspace_model(
        &self,
        workspace_id: Uuid,
        model: Option<String>,
        reasoning_effort: Option<String>,
    ) -> Result<WorkspaceView, ClientError> {
        workspace_response(
            self.send(Command::SetWorkspaceModel {
                workspace_id,
                model,
                reasoning_effort,
            })
            .await?,
        )
    }
    pub async fn list_apps(&self) -> Result<Vec<AppOption>, ClientError> {
        match self.send(Command::ListApps).await? {
            ResponseBody::Apps(apps) => Ok(apps),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn set_workspace_apps(
        &self,
        workspace_id: Uuid,
        apps: Vec<String>,
    ) -> Result<WorkspaceView, ClientError> {
        workspace_response(
            self.send(Command::SetWorkspaceApps { workspace_id, apps })
                .await?,
        )
    }
    pub async fn start_workspace(&self, id: Uuid) -> Result<WorkspaceView, ClientError> {
        workspace_response(self.send(Command::StartWorkspace { id }).await?)
    }
    pub async fn stop_workspace(&self, id: Uuid) -> Result<WorkspaceView, ClientError> {
        workspace_response(self.send(Command::StopWorkspace { id }).await?)
    }
    pub async fn reset_workspace(&self, id: Uuid) -> Result<WorkspaceView, ClientError> {
        workspace_response(self.send(Command::ResetWorkspace { id }).await?)
    }
    pub async fn delete_workspace(&self, id: Uuid) -> Result<Uuid, ClientError> {
        match self.send(Command::DeleteWorkspace { id }).await? {
            ResponseBody::Deleted { id } => Ok(id),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn create_desktop_session(&self, id: Uuid) -> Result<DesktopSession, ClientError> {
        match self.send(Command::CreateDesktopSession { id }).await? {
            ResponseBody::DesktopSession(s) => Ok(s),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn start_agent_session(
        &self,
        workspace_id: Uuid,
    ) -> Result<AgentSessionView, ClientError> {
        agent_session_response(
            self.send(Command::StartAgentSession { workspace_id })
                .await?,
        )
    }
    pub async fn send_agent_prompt(
        &self,
        workspace_id: Uuid,
        prompt: String,
    ) -> Result<AgentSessionView, ClientError> {
        agent_session_response(
            self.send(Command::SendAgentPrompt {
                workspace_id,
                prompt,
            })
            .await?,
        )
    }
    pub async fn poll_agent_events(
        &self,
        workspace_id: Uuid,
        after: u64,
    ) -> Result<AgentEventBatch, ClientError> {
        match self
            .send(Command::PollAgentEvents {
                workspace_id,
                after,
            })
            .await?
        {
            ResponseBody::AgentEvents(events) => Ok(events),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub async fn resolve_agent_approval(
        &self,
        workspace_id: Uuid,
        request_id: String,
        decision: ApprovalDecision,
    ) -> Result<AgentSessionView, ClientError> {
        agent_session_response(
            self.send(Command::ResolveAgentApproval {
                workspace_id,
                request_id,
                decision,
            })
            .await?,
        )
    }
    pub async fn interrupt_agent(
        &self,
        workspace_id: Uuid,
    ) -> Result<AgentSessionView, ClientError> {
        agent_session_response(self.send(Command::InterruptAgent { workspace_id }).await?)
    }
    pub async fn stop_agent_session(&self, workspace_id: Uuid) -> Result<Uuid, ClientError> {
        match self
            .send(Command::StopAgentSession { workspace_id })
            .await?
        {
            ResponseBody::AgentStopped { workspace_id } => Ok(workspace_id),
            _ => Err(ClientError::EmptyResponse),
        }
    }
    pub fn new(socket_name: String, token: String) -> Self {
        Self {
            socket_name,
            token,
            io_timeout: IO_TIMEOUT,
        }
    }
    pub async fn ping(&self) -> Result<ResponseBody, ClientError> {
        self.send(Command::Ping).await
    }
    pub async fn send(&self, command: Command) -> Result<ResponseBody, ClientError> {
        let response_timeout = response_timeout(&command, self.io_timeout);
        let name = endpoint(&self.socket_name)?;
        let stream = timeout(self.io_timeout, Stream::connect(name))
            .await
            .map_err(|_| ClientError::Timeout)??;
        let (read, mut write) = stream.split();
        let request = Request {
            protocol: PROTOCOL_VERSION,
            id: Uuid::new_v4(),
            token: self.token.clone(),
            command,
        };
        let request_id = request.id;
        let payload = format!("{}\n", serde_json::to_string(&request)?);
        timeout(self.io_timeout, write.write_all(payload.as_bytes()))
            .await
            .map_err(|_| ClientError::Timeout)??;
        let line = timeout(response_timeout, read_frame(read))
            .await
            .map_err(|_| ClientError::Timeout)??;
        let response: Response = serde_json::from_str(&line)?;
        if response.protocol != PROTOCOL_VERSION {
            return Err(ClientError::ProtocolMismatch {
                expected: PROTOCOL_VERSION,
                actual: response.protocol,
            });
        }
        if response.id != request_id {
            return Err(ClientError::CorrelationMismatch {
                expected: request_id,
                actual: response.id,
            });
        }
        match response.body {
            ResponseBody::Error(error) => Err(ClientError::Api(error)),
            body => Ok(body),
        }
    }
}

fn response_timeout(command: &Command, default: Duration) -> Duration {
    match command {
        Command::StartWorkspace { .. }
        | Command::StopWorkspace { .. }
        | Command::ResetWorkspace { .. }
        | Command::DeleteWorkspace { .. }
        | Command::SendAgentPrompt { .. } => RUNTIME_MUTATION_TIMEOUT,
        Command::ReconcileRuntime | Command::StartAgentSession { .. } => RECONCILE_TIMEOUT,
        _ => default,
    }
}
fn agent_session_response(body: ResponseBody) -> Result<AgentSessionView, ClientError> {
    match body {
        ResponseBody::AgentSession(session) => Ok(session),
        _ => Err(ClientError::EmptyResponse),
    }
}
fn workspace_response(body: ResponseBody) -> Result<WorkspaceView, ClientError> {
    match body {
        ResponseBody::Workspace(w) => Ok(w),
        _ => Err(ClientError::EmptyResponse),
    }
}
#[cfg(test)]
impl DaemonClient {
    #[allow(dead_code)]
    fn with_timeout(socket_name: String, token: String, io_timeout: Duration) -> Self {
        Self {
            socket_name,
            token,
            io_timeout,
        }
    }
}

#[cfg(unix)]
fn endpoint(logical: &str) -> Result<interprocess::local_socket::Name<'static>, ClientError> {
    if logical.is_empty()
        || logical.len() > 64
        || !logical
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(ClientError::SocketName);
    }
    // SAFETY: geteuid has no preconditions and only reads the effective user ID.
    let uid = unsafe { libc::geteuid() };
    let dir = PathBuf::from(format!("/tmp/orbit-runtime-{uid}"));
    let d = std::fs::symlink_metadata(&dir).map_err(|_| ClientError::SocketName)?;
    if !d.is_dir() || d.uid() != uid || d.mode() & 0o777 != 0o700 {
        return Err(ClientError::SocketName);
    }
    let socket = dir.join(format!("{logical}.sock"));
    let s = std::fs::symlink_metadata(&socket).map_err(ClientError::Io)?;
    if !s.file_type().is_socket() || s.uid() != uid || s.mode() & 0o077 != 0 {
        return Err(ClientError::SocketName);
    }
    socket
        .to_fs_name::<GenericFilePath>()
        .map_err(|_| ClientError::SocketName)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use interprocess::local_socket::{ListenerOptions, ToFsName};
    use orbit_protocol::{Response, ResponseBody};
    use std::{os::unix::fs::PermissionsExt, path::PathBuf};

    #[test]
    fn runtime_mutations_use_a_longer_response_deadline() {
        let id = Uuid::new_v4();
        assert_eq!(
            response_timeout(&Command::StartWorkspace { id }, IO_TIMEOUT),
            RUNTIME_MUTATION_TIMEOUT
        );
        assert_eq!(
            response_timeout(&Command::ReconcileRuntime, IO_TIMEOUT),
            RECONCILE_TIMEOUT
        );
        assert_eq!(
            response_timeout(
                &Command::SendAgentPrompt {
                    workspace_id: id,
                    prompt: "work".into(),
                },
                IO_TIMEOUT,
            ),
            RUNTIME_MUTATION_TIMEOUT
        );
        assert_eq!(response_timeout(&Command::Ping, IO_TIMEOUT), IO_TIMEOUT);
    }

    struct Peer {
        path: PathBuf,
        task: tokio::task::JoinHandle<()>,
    }
    impl Drop for Peer {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
            self.task.abort();
        }
    }

    async fn peer<F, Fut>(logical: &str, handler: F) -> Peer
    where
        F: FnOnce(Stream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        // SAFETY: geteuid has no preconditions and only reads the effective user ID.
        let uid = unsafe { libc::geteuid() };
        let dir = PathBuf::from(format!("/tmp/orbit-runtime-{uid}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.join(format!("{logical}.sock"));
        let _ = std::fs::remove_file(&path);
        let name = path.clone().to_fs_name::<GenericFilePath>().unwrap();
        let listener = ListenerOptions::new().name(name).create_tokio().unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let task = tokio::spawn(async move {
            let stream = listener.accept().await.unwrap();
            handler(stream).await;
        });
        Peer { path, task }
    }
    fn client(name: &str, timeout_ms: u64) -> DaemonClient {
        DaemonClient::with_timeout(
            name.into(),
            "token".into(),
            Duration::from_millis(timeout_ms),
        )
    }
    async fn request_id(mut read: impl AsyncRead + Unpin) -> Uuid {
        let line = read_frame(&mut read).await.unwrap();
        serde_json::from_str::<Request>(&line).unwrap().id
    }

    #[tokio::test]
    async fn rejects_response_protocol_mismatch() {
        let name = "orbit-client-test-protocol";
        let p = peer(name, |stream| async move {
            let (r, mut w) = stream.split();
            let id = request_id(r).await;
            let r = Response {
                protocol: PROTOCOL_VERSION + 1,
                id,
                body: ResponseBody::Pong,
            };
            w.write_all(format!("{}\n", serde_json::to_string(&r).unwrap()).as_bytes())
                .await
                .unwrap();
        })
        .await;
        assert!(matches!(
            client(name, 100).ping().await,
            Err(ClientError::ProtocolMismatch { .. })
        ));
        drop(p);
    }
    #[tokio::test]
    async fn rejects_response_correlation_mismatch() {
        let name = "orbit-client-test-correlation";
        let p = peer(name, |stream| async move {
            let (r, mut w) = stream.split();
            let _ = request_id(r).await;
            let r = Response {
                protocol: PROTOCOL_VERSION,
                id: Uuid::new_v4(),
                body: ResponseBody::Pong,
            };
            w.write_all(format!("{}\n", serde_json::to_string(&r).unwrap()).as_bytes())
                .await
                .unwrap();
        })
        .await;
        assert!(matches!(
            client(name, 100).ping().await,
            Err(ClientError::CorrelationMismatch { .. })
        ));
        drop(p);
    }
    #[tokio::test]
    async fn rejects_oversized_frame() {
        let name = "orbit-client-test-frame";
        let p = peer(name, |stream| async move {
            let (_, mut w) = stream.split();
            let _ = w.write_all(&vec![b'x'; MAX_FRAME_BYTES + 1]).await;
            let _ = w.write_all(b"\n").await;
        })
        .await;
        assert!(matches!(
            client(name, 2_000).ping().await,
            Err(ClientError::FrameTooLarge)
        ));
        drop(p);
    }
    #[tokio::test]
    async fn times_out_stalled_peer() {
        let name = "orbit-client-test-timeout";
        let p = peer(name, |stream| async move {
            let (mut r, _w) = stream.split();
            let _ = request_id(&mut r).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
        })
        .await;
        let result = client(name, 30).ping().await;
        assert!(
            matches!(result, Err(ClientError::Timeout)),
            "got {result:?}"
        );
        drop(p);
    }
}
#[cfg(windows)]
fn endpoint(logical: &str) -> Result<interprocess::local_socket::Name<'static>, ClientError> {
    // Windows current-user ACL hardening is Phase 5.
    logical
        .to_ns_name::<GenericNamespaced>()
        .map_err(|_| ClientError::SocketName)
}
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("local socket names are unsupported on this host")]
    SocketName,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("daemon returned no response")]
    EmptyResponse,
    #[error("daemon error: {0:?}")]
    Api(ApiError),
    #[error("daemon response protocol mismatch (expected {expected}, got {actual})")]
    ProtocolMismatch { expected: u16, actual: u16 },
    #[error("daemon response correlation mismatch (expected {expected}, got {actual})")]
    CorrelationMismatch { expected: Uuid, actual: Uuid },
    #[error("IPC operation timed out")]
    Timeout,
    #[error("IPC frame exceeds 64 KiB")]
    FrameTooLarge,
}

async fn read_frame(mut read: impl AsyncRead + Unpin) -> Result<String, ClientError> {
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(ClientError::FrameTooLarge);
        }
        let count = read.read(&mut byte).await?;
        if count == 0 {
            if bytes.is_empty() {
                return Err(ClientError::EmptyResponse);
            }
            return Err(ClientError::Json(serde_json::Error::io(
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "missing newline"),
            )));
        }
        if byte[0] == b'\n' {
            return String::from_utf8(bytes).map_err(|_| {
                ClientError::Json(serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid UTF-8",
                )))
            });
        }
        bytes.push(byte[0]);
    }
}
impl ClientError {
    pub fn code(&self) -> Option<ErrorCode> {
        match self {
            Self::Api(error) => Some(error.code),
            _ => None,
        }
    }
}
