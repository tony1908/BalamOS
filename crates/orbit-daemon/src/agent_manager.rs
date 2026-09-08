use crate::{
    codex_auth::bootstrap_codex_auth,
    codex_harness::{CodexHarness, HarnessError},
    opencode_harness::OpencodeHarness,
    workspace_manager::{ManagerError, WorkspaceManager},
};
use async_trait::async_trait;
use orbit_domain::{
    AgentEvent, AgentEventBatch, AgentEventKind, AgentSessionView, ApprovalDecision,
    GovernanceBlockedReason, GovernancePolicy, GovernanceView, Harness, PermissionProfile,
    SecretEnv, Skill, WorkspaceGovernancePolicy, resolve_governance,
};
use orbit_protocol::{MAX_AGENT_APPROVAL_ID_BYTES, MAX_AGENT_PROMPT_BYTES};
use std::{collections::HashMap, ffi::OsString, path::Path, sync::Arc};
use thiserror::Error;
use tokio::{
    process::Command,
    sync::{Mutex, RwLock},
};
use uuid::Uuid;

/// Cap on `ToolStarted`/`ToolCompleted` `detail` for poll responses only; the
/// persisted conversation keeps the full detail (see `AgentManager::poll`).
const TOOL_DETAIL_VIEW_CAP: usize = 256;

/// Truncates tool `detail` for the wire response. Never touches persisted history.
fn view_event(event: &AgentEvent) -> AgentEvent {
    use AgentEventKind::*;
    let capped = |detail: &Option<String>| -> Option<String> {
        detail.as_ref().map(|s| {
            if s.len() > TOOL_DETAIL_VIEW_CAP {
                let mut end = TOOL_DETAIL_VIEW_CAP;
                while !s.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}… (truncated)", &s[..end])
            } else {
                s.clone()
            }
        })
    };
    let kind = match &event.kind {
        ToolStarted {
            item_id,
            title,
            detail,
        } => ToolStarted {
            item_id: item_id.clone(),
            title: title.clone(),
            detail: capped(detail),
        },
        ToolCompleted {
            item_id,
            success,
            detail,
        } => ToolCompleted {
            item_id: item_id.clone(),
            success: *success,
            detail: capped(detail),
        },
        other => other.clone(),
    };
    AgentEvent {
        cursor: event.cursor,
        kind,
    }
}

#[derive(Debug, Error)]
pub enum AgentManagerError {
    #[error("workspace is not available for an agent")]
    Workspace,
    #[error("agent session was not found")]
    NotFound,
    #[error("invalid agent request{0}")]
    InvalidRequest(String),
    #[error("governance blocked agent operation: {reasons}", reasons = format_governance(.0))]
    Governance(Vec<GovernanceBlockedReason>),
    #[error("agent harness failed: {0}")]
    Harness(#[from] HarnessError),
}

fn format_governance(reasons: &[GovernanceBlockedReason]) -> String {
    reasons
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

impl From<ManagerError> for AgentManagerError {
    fn from(_: ManagerError) -> Self {
        Self::Workspace
    }
}

#[async_trait]
pub trait AgentHarness: Send + Sync {
    async fn view(&self) -> AgentSessionView;
    async fn send_prompt(&self, prompt: String) -> Result<AgentSessionView, HarnessError>;
    async fn poll(&self, after: u64) -> AgentEventBatch;
    async fn restore(
        &self,
        _events: Vec<AgentEvent>,
        _next_cursor: u64,
        _session_id: Option<String>,
    ) {
    }
    async fn resolve_approval(
        &self,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), HarnessError>;
    async fn interrupt(&self) -> Result<AgentSessionView, HarnessError>;
    async fn stop(&self);
}

struct ManagedSession {
    harness: Arc<dyn AgentHarness>,
    kind: Harness,
    profile: PermissionProfile,
}

#[async_trait]
impl AgentHarness for CodexHarness {
    async fn view(&self) -> AgentSessionView {
        self.view().await
    }
    async fn send_prompt(&self, prompt: String) -> Result<AgentSessionView, HarnessError> {
        self.send_prompt(prompt).await
    }
    async fn poll(&self, after: u64) -> AgentEventBatch {
        self.poll(after).await
    }
    async fn resolve_approval(
        &self,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), HarnessError> {
        self.resolve_approval(request_id, decision).await
    }
    async fn interrupt(&self) -> Result<AgentSessionView, HarnessError> {
        self.interrupt().await
    }
    async fn stop(&self) {
        self.stop().await;
    }
}

#[async_trait]
impl AgentHarness for OpencodeHarness {
    async fn view(&self) -> AgentSessionView {
        self.view().await
    }
    async fn send_prompt(&self, prompt: String) -> Result<AgentSessionView, HarnessError> {
        self.send_prompt(prompt).await
    }
    async fn poll(&self, after: u64) -> AgentEventBatch {
        self.poll(after).await
    }
    async fn restore(&self, events: Vec<AgentEvent>, next_cursor: u64, session_id: Option<String>) {
        self.restore(events, next_cursor, session_id).await
    }
    async fn resolve_approval(
        &self,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), HarnessError> {
        self.resolve_approval(request_id, decision).await
    }
    async fn interrupt(&self) -> Result<AgentSessionView, HarnessError> {
        self.interrupt().await
    }
    async fn stop(&self) {
        self.stop().await
    }
}

#[async_trait]
pub trait HarnessSpawner: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    async fn spawn(
        &self,
        workspace_id: Uuid,
        container_id: &str,
        host_path: &Path,
        profile: PermissionProfile,
        harness: Harness,
        model: Option<&str>,
        reasoning_effort: Option<&str>,
        assigned_env: Vec<SecretEnv>,
    ) -> Result<Arc<dyn AgentHarness>, HarnessError>;
}

pub struct DockerHarnessSpawner {
    binary: OsString,
}

impl DockerHarnessSpawner {
    pub fn new(binary: impl AsRef<Path>) -> Self {
        Self {
            binary: binary.as_ref().as_os_str().to_owned(),
        }
    }
}

pub fn docker_exec_args(container_id: &str, profile: PermissionProfile) -> Vec<OsString> {
    docker_exec_args_with_overrides(container_id, profile, None, None, &[])
}

pub fn docker_exec_args_with_overrides(
    container_id: &str,
    profile: PermissionProfile,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
    assigned_env: &[SecretEnv],
) -> Vec<OsString> {
    let user = if matches!(profile, PermissionProfile::FullControl) {
        "root"
    } else {
        "abc"
    };
    let mut args: Vec<OsString> = [
        "exec",
        "-i",
        "-u",
        user,
        "-w",
        "/workspace",
        "-e",
        "HOME=/config",
        "-e",
        "CODEX_HOME=/config/.codex",
        container_id,
        "codex",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    if let Some(model) = model.filter(|value| !value.is_empty()) {
        args.push("-c".into());
        args.push(format!("model=\"{model}\"").into());
    }
    if let Some(reasoning_effort) = reasoning_effort.filter(|value| !value.is_empty()) {
        args.push("-c".into());
        args.push(format!("model_reasoning_effort=\"{reasoning_effort}\"").into());
    }
    let insert_at = args.iter().position(|arg| arg == container_id).unwrap();
    for secret in assigned_env.iter().rev() {
        args.insert(insert_at, OsString::from("-e"));
        args.insert(insert_at + 1, OsString::from(&secret.env_name));
    }
    args.push("app-server".into());
    args.push("--stdio".into());
    args
}

#[async_trait]
impl HarnessSpawner for DockerHarnessSpawner {
    #[allow(clippy::too_many_arguments)]
    async fn spawn(
        &self,
        workspace_id: Uuid,
        container_id: &str,
        host_path: &Path,
        profile: PermissionProfile,
        harness: Harness,
        model: Option<&str>,
        reasoning_effort: Option<&str>,
        assigned_env: Vec<SecretEnv>,
    ) -> Result<Arc<dyn AgentHarness>, HarnessError> {
        match harness {
            Harness::Opencode => Ok(OpencodeHarness::spawn(
                self.binary.clone(),
                container_id.to_owned(),
                workspace_id,
                profile,
                std::env::var("ORBIT_OPENCODE_API_KEY").ok(),
                model.map(str::to_owned),
                reasoning_effort.map(str::to_owned),
                assigned_env.clone(),
            )
            .await?),
            Harness::ClaudeCode => {
                let credential = std::env::var("ORBIT_CLAUDE_CODE_TOKEN")
                    .ok()
                    .map(crate::claude_harness::ClaudeCredential::OauthToken)
                    .or_else(|| {
                        std::env::var("ORBIT_ANTHROPIC_API_KEY")
                            .ok()
                            .map(crate::claude_harness::ClaudeCredential::ApiKey)
                    });
                Ok(crate::claude_harness::ClaudeHarness::spawn(
                    self.binary.clone(),
                    container_id.to_owned(),
                    workspace_id,
                    profile,
                    credential,
                    model.map(str::to_owned),
                    reasoning_effort.map(str::to_owned),
                    assigned_env.clone(),
                )
                .await?)
            }
            Harness::Codex => {
                bootstrap_codex_auth(self.binary.as_os_str(), container_id)
                    .await
                    .map_err(HarnessError::Protocol)?;
                let mut command = Command::new(&self.binary);
                command.args(docker_exec_args_with_overrides(
                    container_id,
                    profile,
                    model,
                    reasoning_effort,
                    &assigned_env,
                ));
                for secret in &assigned_env {
                    command.env(&secret.env_name, &secret.value.0);
                }
                Ok(CodexHarness::spawn(command, workspace_id, profile, assigned_env).await?)
            }
            Harness::Antigravity => Ok(crate::antigravity_harness::AntigravityHarness::spawn(
                std::env::var("ORBIT_ANTIGRAVITY_BIN")
                    .unwrap_or_else(|_| "antigravity".to_string())
                    .into(),
                host_path.to_path_buf(),
                workspace_id,
                profile,
                model.map(str::to_owned),
                reasoning_effort.map(str::to_owned),
                assigned_env,
            )
            .await?),
        }
    }
}

pub struct AgentManager {
    workspace_manager: Arc<WorkspaceManager>,
    spawner: Arc<dyn HarnessSpawner>,
    sessions: Mutex<HashMap<Uuid, ManagedSession>>,
    policy_gate: RwLock<()>,
}

impl AgentManager {
    fn sync_agents_md(&self, workspace_id: Uuid) {
        let host_path = match self.workspace_manager.store.get(workspace_id) {
            Ok(Some(workspace)) => workspace.host_path,
            _ => return,
        };
        let enabled: Vec<Skill> = self
            .workspace_manager
            .store
            .list_skills(workspace_id)
            .unwrap_or_default()
            .into_iter()
            .filter(|skill| skill.enabled)
            .collect();
        let orbit_dir = host_path.join(".orbit");
        let _ = std::fs::create_dir_all(&orbit_dir);
        let memory_path = orbit_dir.join("MEMORY.md");
        let memory = std::fs::read_to_string(&memory_path).unwrap_or_default();
        if !memory_path.exists() {
            let _ = std::fs::write(&memory_path, "");
        }
        let path = host_path.join("AGENTS.md");
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let next = crate::agents_md::compose_agents_md(&existing, &enabled, &memory);
        if next != existing {
            let _ = std::fs::write(path, next);
        }
    }

    pub fn new(workspace_manager: Arc<WorkspaceManager>) -> Self {
        Self::with_spawner(
            workspace_manager,
            Arc::new(DockerHarnessSpawner::new("docker")),
        )
    }

    pub fn with_spawner(
        workspace_manager: Arc<WorkspaceManager>,
        spawner: Arc<dyn HarnessSpawner>,
    ) -> Self {
        Self {
            workspace_manager,
            spawner,
            sessions: Mutex::new(HashMap::new()),
            policy_gate: RwLock::new(()),
        }
    }

    pub async fn save_global_governance(
        &self,
        policy: &GovernancePolicy,
    ) -> Result<GovernanceView, AgentManagerError> {
        let _gate = self.policy_gate.write().await;
        self.workspace_manager
            .store
            .save_governance_policy(policy)
            .map_err(|e| match e {
                orbit_store::StoreError::GuidanceTooLong => AgentManagerError::InvalidRequest(
                    ": governance guidance exceeds 8000 bytes".into(),
                ),
                _ => AgentManagerError::Workspace,
            })?;
        self.get_governance_inner(None).await
    }

    pub async fn save_workspace_governance(
        &self,
        id: Uuid,
        policy: &WorkspaceGovernancePolicy,
    ) -> Result<GovernanceView, AgentManagerError> {
        let _gate = self.policy_gate.write().await;
        self.workspace_manager
            .store
            .save_workspace_governance(id, policy)
            .map_err(|e| match e {
                orbit_store::StoreError::NotFound => AgentManagerError::NotFound,
                orbit_store::StoreError::GuidanceTooLong => AgentManagerError::InvalidRequest(
                    ": governance guidance exceeds 8000 bytes".into(),
                ),
                _ => AgentManagerError::Workspace,
            })?;
        self.get_governance_inner(Some(id)).await
    }

    pub async fn list_governance_presets(
        &self,
    ) -> Result<Vec<orbit_domain::GovernancePreset>, AgentManagerError> {
        self.workspace_manager
            .store
            .list_governance_presets()
            .map_err(|_| AgentManagerError::Workspace)
    }
    pub async fn create_governance_preset(
        &self,
        name: String,
        policy: &GovernancePolicy,
    ) -> Result<orbit_domain::GovernancePreset, AgentManagerError> {
        self.workspace_manager
            .store
            .create_governance_preset(name, policy.clone())
            .map_err(|error| match error {
                orbit_store::StoreError::GuidanceTooLong
                | orbit_store::StoreError::InvalidPresetName => {
                    AgentManagerError::InvalidRequest(error.to_string())
                }
                _ => AgentManagerError::Workspace,
            })
    }
    pub async fn delete_governance_preset(&self, id: String) -> Result<(), AgentManagerError> {
        self.workspace_manager
            .store
            .delete_governance_preset(&id)
            .map_err(|error| match error {
                orbit_store::StoreError::BuiltinPreset => {
                    AgentManagerError::InvalidRequest(error.to_string())
                }
                orbit_store::StoreError::NotFound => AgentManagerError::NotFound,
                _ => AgentManagerError::Workspace,
            })
    }
    pub async fn list_governance_rules(
        &self,
    ) -> Result<Vec<orbit_domain::GovernanceRule>, AgentManagerError> {
        self.workspace_manager
            .store
            .list_governance_rules()
            .map_err(|_| AgentManagerError::Workspace)
    }
    pub async fn create_governance_rule(
        &self,
        title: String,
        body: String,
    ) -> Result<orbit_domain::GovernanceRule, AgentManagerError> {
        self.workspace_manager
            .store
            .create_governance_rule(title, body)
            .map_err(|error| match error {
                orbit_store::StoreError::GuidanceTooLong
                | orbit_store::StoreError::InvalidPresetName => {
                    AgentManagerError::InvalidRequest(error.to_string())
                }
                _ => AgentManagerError::Workspace,
            })
    }
    pub async fn update_governance_rule(
        &self,
        id: String,
        title: String,
        body: String,
    ) -> Result<orbit_domain::GovernanceRule, AgentManagerError> {
        self.workspace_manager
            .store
            .update_governance_rule(&id, title, body)
            .map_err(|error| match error {
                orbit_store::StoreError::GuidanceTooLong
                | orbit_store::StoreError::InvalidPresetName => {
                    AgentManagerError::InvalidRequest(error.to_string())
                }
                orbit_store::StoreError::NotFound => AgentManagerError::NotFound,
                _ => AgentManagerError::Workspace,
            })
    }
    pub async fn delete_governance_rule(&self, id: String) -> Result<(), AgentManagerError> {
        self.workspace_manager
            .store
            .delete_governance_rule(&id)
            .map_err(|error| match error {
                orbit_store::StoreError::NotFound => AgentManagerError::NotFound,
                _ => AgentManagerError::Workspace,
            })
    }
    pub async fn list_secrets(
        &self,
    ) -> Result<Vec<orbit_domain::SecretMetadata>, AgentManagerError> {
        self.workspace_manager
            .store
            .list_secrets()
            .map_err(|_| AgentManagerError::Workspace)
    }
    pub async fn create_secret(
        &self,
        name: String,
        env_name: String,
        value: orbit_domain::SecretInput,
    ) -> Result<orbit_domain::SecretMetadata, AgentManagerError> {
        let store = Arc::clone(&self.workspace_manager.store);
        tokio::task::spawn_blocking(move || store.create_secret(name, env_name, value))
            .await
            .map_err(|_| AgentManagerError::Workspace)?
            .map_err(|e| AgentManagerError::InvalidRequest(e.to_string()))
    }
    pub async fn replace_secret(
        &self,
        id: Uuid,
        value: orbit_domain::SecretInput,
    ) -> Result<(), AgentManagerError> {
        let store = Arc::clone(&self.workspace_manager.store);
        tokio::task::spawn_blocking(move || store.replace_secret(id, value))
            .await
            .map_err(|_| AgentManagerError::Workspace)?
            .map_err(|e| AgentManagerError::InvalidRequest(e.to_string()))
    }
    pub async fn delete_secret(&self, id: Uuid) -> Result<(), AgentManagerError> {
        let store = Arc::clone(&self.workspace_manager.store);
        tokio::task::spawn_blocking(move || store.delete_secret(id))
            .await
            .map_err(|_| AgentManagerError::Workspace)?
            .map_err(|e| AgentManagerError::InvalidRequest(e.to_string()))
    }
    pub async fn assign_secret(
        &self,
        a: orbit_domain::SecretAssignment,
    ) -> Result<(), AgentManagerError> {
        let store = Arc::clone(&self.workspace_manager.store);
        tokio::task::spawn_blocking(move || store.assign_secret(a))
            .await
            .map_err(|_| AgentManagerError::Workspace)?
            .map_err(|e| AgentManagerError::InvalidRequest(e.to_string()))
    }
    pub async fn list_assignments(
        &self,
        id: Uuid,
    ) -> Result<Vec<orbit_domain::SecretAssignment>, AgentManagerError> {
        self.workspace_manager
            .store
            .list_assignments(id)
            .map_err(|e| AgentManagerError::InvalidRequest(e.to_string()))
    }
    pub async fn unassign_secret(
        &self,
        secret_id: Uuid,
        workspace_id: Uuid,
    ) -> Result<(), AgentManagerError> {
        let store = Arc::clone(&self.workspace_manager.store);
        tokio::task::spawn_blocking(move || store.unassign_secret(secret_id, workspace_id))
            .await
            .map_err(|_| AgentManagerError::Workspace)?
            .map_err(|e| AgentManagerError::InvalidRequest(e.to_string()))
    }

    async fn get_governance_inner(
        &self,
        id: Option<Uuid>,
    ) -> Result<GovernanceView, AgentManagerError> {
        let (global, local, workspace) = match id {
            Some(id) => {
                let workspace = self
                    .workspace_manager
                    .store
                    .get(id)
                    .map_err(|_| AgentManagerError::Workspace)?
                    .ok_or(AgentManagerError::NotFound)?;
                let (global, local) = self
                    .workspace_manager
                    .store
                    .get_governance_pair(id)
                    .map_err(|_| AgentManagerError::Workspace)?;
                (global, local, Some(workspace))
            }
            None => {
                let (global, local) = self
                    .workspace_manager
                    .store
                    .get_governance_pair(Uuid::nil())
                    .map_err(|_| AgentManagerError::Workspace)?;
                (global, local, None)
            }
        };
        let effective = resolve_governance(&global, &local);
        let blocked_reasons = if let Some(w) = workspace {
            let sessions = self.sessions.lock().await;
            if let Some(session) = sessions.get(&w.id) {
                effective.blocked_reasons(session.kind, session.profile, true, false)
            } else {
                effective.blocked_reasons(w.harness, w.profile, true, false)
            }
        } else {
            Vec::new()
        };
        let library = self
            .workspace_manager
            .store
            .list_governance_rules()
            .map_err(|_| AgentManagerError::Workspace)?;
        Ok(GovernanceView {
            global,
            local,
            effective,
            blocked_reasons,
            library,
        })
    }

    pub async fn get_governance(
        &self,
        id: Option<Uuid>,
    ) -> Result<GovernanceView, AgentManagerError> {
        let _gate = self.policy_gate.read().await;
        self.get_governance_inner(id).await
    }

    pub async fn start(&self, workspace_id: Uuid) -> Result<AgentSessionView, AgentManagerError> {
        let _gate = self.policy_gate.read().await;
        self.start_guarded(workspace_id).await
    }

    pub(crate) async fn start_guarded(
        &self,
        workspace_id: Uuid,
    ) -> Result<AgentSessionView, AgentManagerError> {
        let workspace = self
            .workspace_manager
            .store
            .get(workspace_id)
            .map_err(|_| AgentManagerError::Workspace)?
            .ok_or(AgentManagerError::Workspace)?;
        let (global, local) = self
            .workspace_manager
            .store
            .get_governance_pair(workspace_id)
            .map_err(|_| AgentManagerError::Workspace)?;
        let effective = resolve_governance(&global, &local);
        let reasons = effective.blocked_reasons(workspace.harness, workspace.profile, true, false);
        if !reasons.is_empty() {
            return Err(AgentManagerError::Governance(reasons));
        }
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(&workspace_id) {
            let reasons = effective.blocked_reasons(session.kind, session.profile, true, false);
            if !reasons.is_empty() {
                return Err(AgentManagerError::Governance(reasons));
            }
            let view = session.harness.view().await;
            if view.state != orbit_domain::AgentSessionState::Failed {
                return Ok(view);
            }
            let failed = sessions.remove(&workspace_id).expect("session exists");
            failed.harness.stop().await;
        }
        let store = Arc::clone(&self.workspace_manager.store);
        let assigned_env =
            tokio::task::spawn_blocking(move || store.resolve_assigned_env(workspace_id))
                .await
                .map_err(|_| AgentManagerError::Workspace)?
                .map_err(|_| {
                    AgentManagerError::InvalidRequest("assigned credential is unavailable".into())
                })?;
        let (container_id, profile, harness, model, reasoning_effort) =
            self.workspace_manager.agent_target(workspace_id).await?;
        self.sync_agents_md(workspace_id);
        let host_path = self
            .workspace_manager
            .store
            .get(workspace_id)
            .ok()
            .flatten()
            .map(|workspace| workspace.host_path)
            .unwrap_or_default();
        let session = self
            .spawner
            .spawn(
                workspace_id,
                &container_id,
                &host_path,
                profile,
                harness,
                model.as_deref(),
                reasoning_effort.as_deref(),
                assigned_env,
            )
            .await?;
        if let Ok(Some((events, session_id))) =
            self.workspace_manager.store.load_conversation(workspace_id)
        {
            let next = events.last().map(|e| e.cursor + 1).unwrap_or(0);
            session.restore(events, next, session_id).await;
        }
        let view = session.view().await;
        sessions.insert(
            workspace_id,
            ManagedSession {
                harness: session,
                kind: harness,
                profile,
            },
        );
        Ok(view)
    }

    pub async fn prompt(
        &self,
        workspace_id: Uuid,
        prompt: String,
    ) -> Result<AgentSessionView, AgentManagerError> {
        let _gate = self.policy_gate.read().await;
        self.prompt_guarded(workspace_id, prompt).await
    }

    pub(crate) async fn prompt_guarded(
        &self,
        workspace_id: Uuid,
        prompt: String,
    ) -> Result<AgentSessionView, AgentManagerError> {
        if prompt.trim().is_empty() || prompt.len() > MAX_AGENT_PROMPT_BYTES {
            return Err(AgentManagerError::InvalidRequest("".into()));
        }
        let _workspace = self
            .workspace_manager
            .store
            .get(workspace_id)
            .map_err(|_| AgentManagerError::Workspace)?
            .ok_or(AgentManagerError::Workspace)?;
        let (global, local) = self
            .workspace_manager
            .store
            .get_governance_pair(workspace_id)
            .map_err(|_| AgentManagerError::Workspace)?;
        let effective = resolve_governance(&global, &local);
        let session = self.session(workspace_id).await?;
        let reasons = effective.blocked_reasons(session.kind, session.profile, true, false);
        if !reasons.is_empty() {
            return Err(AgentManagerError::Governance(reasons));
        }
        let library = self
            .workspace_manager
            .store
            .list_governance_rules()
            .map_err(|_| AgentManagerError::Workspace)?;
        let rules_text = orbit_domain::compose_rule_guidance(
            &library,
            &local.applied_rule_ids,
            &local.custom_rules,
        );
        let combined = match (effective.guidance.is_empty(), rules_text.is_empty()) {
            (true, true) => String::new(),
            (false, true) => effective.guidance.clone(),
            (true, false) => rules_text,
            (false, false) => format!("{}\n\n{}", effective.guidance, rules_text),
        };
        let prompt = if combined.is_empty() {
            prompt
        } else {
            format!(
                "[Orbit governance guidance]\n{}\n[/Orbit governance guidance]\n\n{}",
                combined, prompt
            )
        };
        session
            .harness
            .send_prompt(prompt)
            .await
            .map_err(Into::into)
    }

    pub async fn scheduled_prompt(
        &self,
        workspace_id: Uuid,
        prompt: String,
    ) -> Result<AgentSessionView, AgentManagerError> {
        let _gate = self.policy_gate.read().await;
        let workspace = self
            .workspace_manager
            .store
            .get(workspace_id)
            .map_err(|_| AgentManagerError::Workspace)?
            .ok_or(AgentManagerError::NotFound)?;
        let (global, local) = self
            .workspace_manager
            .store
            .get_governance_pair(workspace_id)
            .map_err(|_| AgentManagerError::Workspace)?;
        let effective = resolve_governance(&global, &local);
        let reasons = effective.blocked_reasons(workspace.harness, workspace.profile, true, true);
        if !reasons.is_empty() {
            return Err(AgentManagerError::Governance(reasons));
        }
        self.start_guarded(workspace_id).await?;
        self.prompt_guarded(workspace_id, prompt).await
    }

    pub async fn poll(
        &self,
        workspace_id: Uuid,
        after: u64,
    ) -> Result<AgentEventBatch, AgentManagerError> {
        let batch = self.session(workspace_id).await?.harness.poll(after).await;
        let view_events: Vec<AgentEvent> = batch.events.iter().map(view_event).collect();
        let paged = crate::codex_harness::pack_event_page(&view_events, after);
        if !paged.has_more && !batch.events.is_empty() {
            let session = self.session(workspace_id).await?;
            let all = session.harness.poll(0).await; // full events, untruncated
            let view = session.harness.view().await;
            let _ = self.workspace_manager.store.save_conversation(
                workspace_id,
                &all.events,
                view.thread_id.as_deref(),
            );
        }
        Ok(AgentEventBatch {
            workspace_id,
            after,
            next_cursor: paged.next_cursor,
            events: paged.events,
            has_more: paged.has_more,
        })
    }

    pub async fn resolve(
        &self,
        workspace_id: Uuid,
        request_id: String,
        decision: ApprovalDecision,
    ) -> Result<AgentSessionView, AgentManagerError> {
        if request_id.is_empty() || request_id.len() > MAX_AGENT_APPROVAL_ID_BYTES {
            return Err(AgentManagerError::InvalidRequest("".into()));
        }
        let session = self.session(workspace_id).await?;
        session
            .harness
            .resolve_approval(&request_id, decision)
            .await?;
        Ok(session.harness.view().await)
    }

    pub async fn interrupt(
        &self,
        workspace_id: Uuid,
    ) -> Result<AgentSessionView, AgentManagerError> {
        self.session(workspace_id)
            .await?
            .harness
            .interrupt()
            .await
            .map_err(Into::into)
    }

    pub async fn stop(&self, workspace_id: Uuid) -> bool {
        let session = self.sessions.lock().await.remove(&workspace_id);
        if let Some(session) = session {
            session.harness.stop().await;
            true
        } else {
            false
        }
    }

    pub async fn clear(&self, workspace_id: Uuid) {
        let _ = self
            .workspace_manager
            .store
            .delete_conversation(workspace_id);
    }

    pub async fn stop_all(&self) {
        let sessions = {
            let mut sessions = self.sessions.lock().await;
            sessions
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>()
        };
        for session in sessions {
            session.harness.stop().await;
        }
    }

    async fn session(&self, workspace_id: Uuid) -> Result<ManagedSession, AgentManagerError> {
        self.sessions
            .lock()
            .await
            .get(&workspace_id)
            .map(|session| ManagedSession {
                harness: session.harness.clone(),
                kind: session.kind,
                profile: session.profile,
            })
            .ok_or(AgentManagerError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_domain::{AgentSessionState, ResourceLimits, Workspace};
    use orbit_store::{CredentialStore, StoreError, WorkspaceStore};
    use std::sync::Mutex as StdMutex;
    use tempfile::tempdir;

    struct AgentHarnessMock {
        workspace_id: Uuid,
        prompts: StdMutex<Vec<String>>,
    }

    impl AgentHarnessMock {
        fn view_for(&self) -> AgentSessionView {
            AgentSessionView {
                workspace_id: self.workspace_id,
                thread_id: None,
                state: AgentSessionState::Idle,
                next_cursor: 0,
            }
        }
    }

    #[async_trait]
    impl AgentHarness for AgentHarnessMock {
        async fn view(&self) -> AgentSessionView {
            self.view_for()
        }
        async fn send_prompt(&self, prompt: String) -> Result<AgentSessionView, HarnessError> {
            self.prompts.lock().unwrap().push(prompt);
            Ok(self.view_for())
        }
        async fn poll(&self, _after: u64) -> AgentEventBatch {
            AgentEventBatch {
                workspace_id: self.workspace_id,
                after: 0,
                next_cursor: 0,
                events: vec![],
                has_more: false,
            }
        }
        async fn resolve_approval(
            &self,
            _request_id: &str,
            _decision: ApprovalDecision,
        ) -> Result<(), HarnessError> {
            Ok(())
        }
        async fn interrupt(&self) -> Result<AgentSessionView, HarnessError> {
            Ok(self.view_for())
        }
        async fn stop(&self) {}
    }

    async fn fixture(harness: Harness) -> (AgentManager, Uuid, Arc<AgentHarnessMock>) {
        let (manager, id, mock, _) =
            fixture_with_credentials(harness, Arc::new(TestCredentials)).await;
        (manager, id, mock)
    }

    async fn fixture_with_credentials(
        harness: Harness,
        credentials: Arc<dyn CredentialStore>,
    ) -> (
        AgentManager,
        Uuid,
        Arc<AgentHarnessMock>,
        Arc<WorkspaceStore>,
    ) {
        let store = Arc::new(
            WorkspaceStore::in_memory()
                .unwrap()
                .with_credentials(credentials),
        );
        let root = tempdir().unwrap();
        let resources =
            ResourceLimits::new(1.0, 512 * 1024 * 1024, 128, 1024 * 1024 * 1024).unwrap();
        let mut workspace = Workspace::new(
            "test".into(),
            root.path().to_path_buf(),
            PermissionProfile::Workspace,
            resources,
        );
        workspace.harness = harness;
        let id = workspace.id;
        store.upsert(&workspace).unwrap();
        let manager = Arc::new(WorkspaceManager::new(
            store.clone(),
            Arc::new(crate::docker_cli::DockerCliRuntime::new(
                "/nonexistent/orbit-test",
            )),
        ));
        let mock = Arc::new(AgentHarnessMock {
            workspace_id: id,
            prompts: StdMutex::new(vec![]),
        });
        let agents = AgentManager::with_spawner(
            manager,
            Arc::new(TestSpawner {
                harness: mock.clone(),
            }),
        );
        (agents, id, mock, store)
    }

    struct TestCredentials;
    impl CredentialStore for TestCredentials {
        fn set(&self, _id: &str, _value: &orbit_domain::SecretInput) -> Result<(), StoreError> {
            Ok(())
        }
        fn get(&self, _id: &str) -> Result<String, StoreError> {
            Err(StoreError::Credential)
        }
        fn delete(&self, _id: &str) -> Result<(), StoreError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn antigravity_start_reports_unavailable_assigned_credential_before_runtime() {
        let (manager, id, _, store) =
            fixture_with_credentials(Harness::Antigravity, Arc::new(TestCredentials)).await;
        let secret = store
            .create_secret(
                "token".into(),
                "TOKEN".into(),
                orbit_domain::SecretInput("value".into()),
            )
            .unwrap();
        store
            .assign_secret(orbit_domain::SecretAssignment {
                secret_id: secret.id,
                workspace_id: id,
            })
            .unwrap();
        let error = manager.start(id).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("assigned credential is unavailable")
        );
    }

    struct TestSpawner {
        harness: Arc<AgentHarnessMock>,
    }
    #[async_trait]
    impl HarnessSpawner for TestSpawner {
        async fn spawn(
            &self,
            _workspace_id: Uuid,
            _container_id: &str,
            _host_path: &Path,
            _profile: PermissionProfile,
            _harness: Harness,
            _model: Option<&str>,
            _reasoning_effort: Option<&str>,
            _assigned_env: Vec<SecretEnv>,
        ) -> Result<Arc<dyn AgentHarness>, HarnessError> {
            Ok(self.harness.clone())
        }
    }

    async fn cache_session(
        manager: &AgentManager,
        id: Uuid,
        harness: Harness,
    ) -> Arc<AgentHarnessMock> {
        let mock = Arc::new(AgentHarnessMock {
            workspace_id: id,
            prompts: StdMutex::new(vec![]),
        });
        manager.sessions.lock().await.insert(
            id,
            ManagedSession {
                harness: mock.clone(),
                kind: harness,
                profile: PermissionProfile::Workspace,
            },
        );
        mock
    }

    #[tokio::test]
    async fn governance_global_disabled_blocks_cached_start_and_prompt() {
        let (manager, id, _) = fixture(Harness::Codex).await;
        let mock = cache_session(&manager, id, Harness::Codex).await;
        manager
            .save_global_governance(&GovernancePolicy {
                enabled: false,
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(
            matches!(manager.start(id).await, Err(AgentManagerError::Governance(reasons)) if reasons == vec![GovernanceBlockedReason::Disabled])
        );
        assert!(matches!(
            manager.prompt(id, "hello".into()).await,
            Err(AgentManagerError::Governance(_))
        ));
        assert!(mock.prompts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn governance_scheduled_disabled_blocks_before_runtime_access() {
        let (manager, id, mock) = fixture(Harness::Codex).await;
        manager
            .save_global_governance(&GovernancePolicy {
                allow_scheduled: false,
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(
            matches!(manager.scheduled_prompt(id, "hello".into()).await, Err(AgentManagerError::Governance(reasons)) if reasons == vec![GovernanceBlockedReason::ScheduledRunsDisabled])
        );
        assert!(mock.prompts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn governance_global_and_local_guidance_is_ordered_and_global_disable_wins() {
        let (manager, id, _) = fixture(Harness::Codex).await;
        let mock = cache_session(&manager, id, Harness::Codex).await;
        manager
            .save_global_governance(&GovernancePolicy {
                guidance: "global".into(),
                enabled: false,
                ..Default::default()
            })
            .await
            .unwrap();
        manager
            .save_workspace_governance(
                id,
                &WorkspaceGovernancePolicy {
                    enabled: Some(true),
                    guidance: Some("local".into()),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(
            matches!(manager.prompt(id, "hello".into()).await, Err(AgentManagerError::Governance(reasons)) if reasons == vec![GovernanceBlockedReason::Disabled])
        );
        assert!(mock.prompts.lock().unwrap().is_empty());
        let view = manager.get_governance(Some(id)).await.unwrap();
        assert!(!view.effective.enabled);
        assert_eq!(view.effective.guidance, "[global]\nglobal\n[local]\nlocal");
    }

    #[tokio::test]
    async fn governance_antigravity_container_requirement_blocks_cached_session() {
        let (manager, id, _) = fixture(Harness::Codex).await;
        manager
            .save_global_governance(&GovernancePolicy {
                require_container: true,
                ..Default::default()
            })
            .await
            .unwrap();
        let mock = cache_session(&manager, id, Harness::Antigravity).await;
        for result in [
            manager.start(id).await.map(|_| ()),
            manager.prompt(id, "hello".into()).await.map(|_| ()),
        ] {
            assert!(
                matches!(result, Err(AgentManagerError::Governance(reasons)) if reasons == vec![GovernanceBlockedReason::ContainerRequired])
            );
        }
        assert!(matches!(
            manager
                .get_governance(Some(id))
                .await
                .unwrap()
                .blocked_reasons
                .as_slice(),
            [GovernanceBlockedReason::ContainerRequired]
        ));
        assert!(mock.prompts.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn governance_get_stopped_workspace_is_disabled_and_policy_is_reread() {
        let (manager, id, _mock) = fixture(Harness::Codex).await;
        manager
            .save_global_governance(&GovernancePolicy {
                enabled: false,
                ..Default::default()
            })
            .await
            .unwrap();
        let view = manager.get_governance(Some(id)).await.unwrap();
        assert_eq!(
            view.blocked_reasons,
            vec![GovernanceBlockedReason::Disabled]
        );
        assert!(!view.effective.enabled);
        assert_eq!(
            manager
                .workspace_manager
                .store
                .get_governance_policy()
                .unwrap()
                .enabled,
            false
        );
    }

    #[test]
    fn view_event_truncates_long_tool_detail_but_leaves_other_events_alone() {
        let long_detail = "x".repeat(10_000);

        let completed = AgentEvent {
            cursor: 1,
            kind: AgentEventKind::ToolCompleted {
                item_id: "tool-1".into(),
                success: true,
                detail: Some(long_detail.clone()),
            },
        };
        let viewed = view_event(&completed);
        match viewed.kind {
            AgentEventKind::ToolCompleted { detail, .. } => {
                let detail = detail.expect("detail present");
                assert!(detail.len() <= TOOL_DETAIL_VIEW_CAP + "… (truncated)".len());
                assert!(detail.ends_with("… (truncated)"));
            }
            other => panic!("expected ToolCompleted, got {other:?}"),
        }

        let short_detail = "short output";
        let short_completed = AgentEvent {
            cursor: 2,
            kind: AgentEventKind::ToolCompleted {
                item_id: "tool-2".into(),
                success: false,
                detail: Some(short_detail.into()),
            },
        };
        assert_eq!(short_completed, view_event(&short_completed));

        let long_assistant_text = "y".repeat(10_000);
        let assistant = AgentEvent {
            cursor: 3,
            kind: AgentEventKind::AssistantDelta {
                text: long_assistant_text,
            },
        };
        assert_eq!(assistant, view_event(&assistant));
    }

    #[test]
    fn docker_exec_is_exact_and_full_control_is_the_only_root_profile() {
        for (profile, user) in [
            (PermissionProfile::Observe, "abc"),
            (PermissionProfile::Workspace, "abc"),
            (PermissionProfile::FullControl, "root"),
        ] {
            let args = docker_exec_args("owned-container", profile);
            let args = args
                .iter()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>();
            assert_eq!(args[3], user);
            assert!(args.contains(&"HOME=/config".into()));
            assert!(args.contains(&"CODEX_HOME=/config/.codex".into()));
            assert_eq!(
                &args[10..],
                &["owned-container", "codex", "app-server", "--stdio"]
            );
            assert!(!args.iter().any(|arg| arg.contains("docker.sock")));
        }
    }

    #[test]
    fn codex_overrides_are_appended_only_for_non_empty_values() {
        let args = docker_exec_args_with_overrides(
            "owned-container",
            PermissionProfile::Workspace,
            Some("gpt-5-codex"),
            Some("high"),
            &[],
        );
        let args = args
            .iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>();
        assert_eq!(
            &args[10..],
            &[
                "owned-container",
                "codex",
                "-c",
                "model=\"gpt-5-codex\"",
                "-c",
                "model_reasoning_effort=\"high\"",
                "app-server",
                "--stdio",
            ]
        );

        assert_eq!(
            docker_exec_args_with_overrides(
                "owned-container",
                PermissionProfile::Workspace,
                Some(""),
                Some(""),
                &[]
            ),
            docker_exec_args("owned-container", PermissionProfile::Workspace)
        );
    }

    #[test]
    fn assigned_secret_argv_contains_only_the_environment_name_before_container() {
        let args = docker_exec_args_with_overrides(
            "owned-container",
            PermissionProfile::Workspace,
            None,
            None,
            &[SecretEnv {
                env_name: "API_TOKEN".into(),
                value: orbit_domain::SecretInput("sentinel-value".into()),
            }],
        );
        let args = args
            .iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>();
        let container = args
            .iter()
            .position(|arg| arg == "owned-container")
            .unwrap();
        assert_eq!(&args[container - 2..container], &["-e", "API_TOKEN"]);
        assert!(!args.iter().any(|arg| arg == "sentinel-value"));
    }

    #[tokio::test]
    async fn opencode_is_rejected_before_constructing_a_process() {
        let spawner = DockerHarnessSpawner::new("binary-that-must-not-run");

        let result = spawner
            .spawn(
                Uuid::nil(),
                "owned-container",
                Path::new("/tmp/ws"),
                PermissionProfile::Workspace,
                orbit_domain::Harness::Opencode,
                None,
                None,
                vec![],
            )
            .await;

        assert!(
            matches!(result, Err(HarnessError::Unavailable(ref message)) if message == "opencode API key not configured (set ORBIT_OPENCODE_API_KEY)")
        );
    }

    #[tokio::test]
    async fn claude_code_requires_credentials_before_constructing_a_process() {
        unsafe {
            std::env::remove_var("ORBIT_CLAUDE_CODE_TOKEN");
            std::env::remove_var("ORBIT_ANTHROPIC_API_KEY");
        }
        let spawner = DockerHarnessSpawner::new("binary-that-must-not-run");

        let result = spawner
            .spawn(
                Uuid::nil(),
                "owned-container",
                Path::new("/tmp/ws"),
                PermissionProfile::Workspace,
                orbit_domain::Harness::ClaudeCode,
                None,
                None,
                vec![],
            )
            .await;

        assert!(
            matches!(result, Err(HarnessError::Unavailable(ref message)) if message == "Claude Code credentials not configured (set ORBIT_CLAUDE_CODE_TOKEN from `claude setup-token`, or ORBIT_ANTHROPIC_API_KEY)")
        );
    }
}
