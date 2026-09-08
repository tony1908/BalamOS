use crate::runtime::{ContainerInfo, ContainerRuntime, ContainerSpec, RuntimeError};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use orbit_domain::{
    AppOption, BotSettings, Harness, LifecycleState, ModelOption, PermissionProfile,
    ResourceLimits, RuntimeMetadata, RuntimeSecret, Skill, Template, TemplateRoutine,
    TemplateSettings, Workspace,
};
use orbit_protocol::{
    MAX_BOT_DESCRIPTION_BYTES, MAX_BOT_LABEL_BYTES, MAX_BOT_NAME_BYTES,
    MAX_SKILL_INSTRUCTION_BYTES, MAX_SKILL_NAME_BYTES, RuntimeStatus,
};
use orbit_store::{StoreError, WorkspaceStore};
use rand::RngCore;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileCode {
    RuntimeUnavailable,
    MissingContainer,
    DuplicateContainers,
    OwnershipMismatch,
    MissingSecret,
    HealthFailed,
    OrphanContainer,
    InterruptedLifecycle,
    UpgradeRequired,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileDiagnostic {
    pub workspace_id: Option<Uuid>,
    pub code: ReconcileCode,
    pub message: String,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    pub examined_workspaces: usize,
    pub examined_containers: usize,
    pub running: usize,
    pub stopped: usize,
    pub failed: usize,
    pub orphan_container_ids: Vec<String>,
    pub diagnostics: Vec<ReconcileDiagnostic>,
}

pub const PINNED_WEBTOP_IMAGE: &str = "orbit-webtop:0.4.0";
const CURRENT_WEBTOP_IDENTITY: &str = "webtop-ubuntu-xfce-v2";
const LEGACY_WEBTOP_IMAGE: &str = "orbit-webtop:0.1.0";
const LEGACY_WEBTOP_IDENTITY: &str = "webtop-ubuntu-xfce-v1";
#[derive(Debug, Clone, Copy)]
pub struct HealthPolicy {
    pub attempts: usize,
    pub delay: Duration,
}
impl Default for HealthPolicy {
    fn default() -> Self {
        Self {
            attempts: 30,
            delay: Duration::from_secs(1),
        }
    }
}
#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("workspace not found")]
    NotFound,
    #[error("invalid workspace: {0}")]
    Invalid(String),
    #[error("invalid lifecycle transition: {0}")]
    Transition(String),
    #[error("runtime operation failed: {0}")]
    Runtime(String),
    #[error("workspace store failed: {0}")]
    Store(String),
    #[error("container ownership mismatch")]
    Ownership,
    #[error("container runtime is unavailable; retry after reconciliation")]
    RuntimeUnavailable,
}
impl From<StoreError> for ManagerError {
    fn from(e: StoreError) -> Self {
        Self::Store(e.to_string())
    }
}
impl From<RuntimeError> for ManagerError {
    fn from(e: RuntimeError) -> Self {
        ManagerError::Runtime(e.to_string())
    }
}

const MODEL_LIST_TIMEOUT: Duration = Duration::from_secs(20);

/// `docker run --rm --entrypoint sh <image> -c "codex debug models"` stdout
/// is `{"models":[{"slug":..,"display_name":..}, ...]}`; auth-free.
async fn list_codex_models() -> Vec<ModelOption> {
    let image =
        std::env::var("ORBIT_WEBTOP_IMAGE").unwrap_or_else(|_| PINNED_WEBTOP_IMAGE.to_string());
    let output = tokio::process::Command::new("docker")
        .args([
            "run",
            "--rm",
            "--entrypoint",
            "sh",
            &image,
            "-c",
            "codex debug models",
        ])
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(MODEL_LIST_TIMEOUT, output).await {
        Ok(Ok(out)) if out.status.success() => {
            parse_codex_models_json(&String::from_utf8_lossy(&out.stdout))
        }
        _ => Vec::new(),
    }
}

fn parse_codex_models_json(stdout: &str) -> Vec<ModelOption> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout.trim()) else {
        return Vec::new();
    };
    let Some(models) = value.get("models").and_then(|m| m.as_array()) else {
        return Vec::new();
    };
    models
        .iter()
        .filter_map(|m| {
            let slug = m.get("slug")?.as_str()?.to_string();
            let label = m
                .get("display_name")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| slug.clone());
            Some(ModelOption { id: slug, label })
        })
        .collect()
}

/// The host `antigravity models` binary prints tab-separated `slug<TAB>label`
/// lines, plus the odd human-readable status line (e.g. "Fetching available
/// models..."); those have no tab and are silently skipped.
async fn list_antigravity_models() -> Vec<ModelOption> {
    let binary = std::env::var("ORBIT_ANTIGRAVITY_BIN").unwrap_or_else(|_| "antigravity".into());
    let output = tokio::process::Command::new(binary)
        .arg("models")
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(MODEL_LIST_TIMEOUT, output).await {
        Ok(Ok(out)) if out.status.success() => {
            parse_antigravity_models_lines(&String::from_utf8_lossy(&out.stdout))
        }
        _ => Vec::new(),
    }
}

fn parse_antigravity_models_lines(stdout: &str) -> Vec<ModelOption> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(2, '\t');
            let id = parts.next()?.trim();
            let label = parts.next()?.trim();
            (!id.is_empty() && !label.is_empty()).then(|| ModelOption {
                id: id.to_string(),
                label: label.to_string(),
            })
        })
        .collect()
}

/// Claude Code has no non-interactive model-list command; this is a curated
/// constant.
fn claude_code_models() -> Vec<ModelOption> {
    [
        ("default", "Default"),
        ("sonnet", "Sonnet"),
        ("opus", "Opus"),
        ("haiku", "Haiku"),
        ("fable", "Fable"),
        ("opusplan", "Opus Plan"),
        ("claude-sonnet-5", "Claude Sonnet 5"),
        ("claude-opus-5", "Claude Opus 5"),
        ("claude-haiku-4-5", "Claude Haiku 4.5"),
    ]
    .into_iter()
    .map(|(id, label)| ModelOption {
        id: id.to_string(),
        label: label.to_string(),
    })
    .collect()
}

#[derive(Clone)]
pub struct WorkspaceManager {
    pub store: Arc<WorkspaceStore>,
    runtime: Arc<dyn ContainerRuntime>,
    workspaces_root: PathBuf,
    policy: HealthPolicy,
    locks: Arc<Mutex<BTreeMap<Uuid, Arc<tokio::sync::Mutex<()>>>>>,
    mutations_ready: Arc<AtomicBool>,
    runtime_available: Arc<AtomicBool>,
    lifecycle_gate: Arc<tokio::sync::RwLock<()>>,
}
impl WorkspaceManager {
    pub async fn save_template(
        &self,
        name: String,
        settings: TemplateSettings,
        routines: Vec<TemplateRoutine>,
        skills: Vec<orbit_domain::TemplateSkill>,
    ) -> Result<Template, ManagerError> {
        let name = name.trim();
        if name.is_empty() || name.len() > 200 {
            return Err(ManagerError::Invalid(
                "template name must be non-empty and at most 200 bytes".into(),
            ));
        }
        let template = Template {
            id: Uuid::new_v4(),
            name: name.into(),
            settings,
            routines,
            skills,
        };
        self.store.upsert_template(&template)?;
        Ok(template)
    }
    pub async fn list_templates(&self) -> Result<Vec<Template>, ManagerError> {
        Ok(self.store.list_all_templates()?)
    }
    pub async fn delete_template(&self, id: Uuid) -> Result<(), ManagerError> {
        Ok(self.store.delete_template(id)?)
    }
    fn default_workspaces_root() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Orbit/workspaces"))
            .unwrap_or_else(|| std::env::temp_dir().join("orbit/workspaces"))
    }

    fn slug(name: &str) -> String {
        let mut slug = String::new();
        for c in name.chars() {
            if c.is_alphanumeric() {
                slug.extend(c.to_lowercase());
            } else if !slug.ends_with('-') {
                slug.push('-');
            }
        }
        let slug = slug.trim_matches('-').to_owned();
        if slug.is_empty() {
            "workspace".into()
        } else {
            slug
        }
    }

    pub async fn runtime_status(&self) -> RuntimeStatus {
        match self.runtime.prerequisite().await {
            Ok(p) => RuntimeStatus {
                available: p.available,
                mutations_ready: self.mutations_ready(),
                version: p.version,
                message: p.message,
            },
            Err(e) => RuntimeStatus {
                available: false,
                mutations_ready: false,
                version: None,
                message: e.to_string(),
            },
        }
    }
    /// Resolve a fresh, authenticated loopback target. Persisted ports are
    /// intentionally ignored; the runtime inspection is authoritative.
    pub(crate) async fn desktop_target(&self, id: Uuid) -> Result<(String, String), ManagerError> {
        let _guard = self.lock(id).await;
        let w = self.get(id).await?;
        if w.state != LifecycleState::Running {
            return Err(ManagerError::Invalid("workspace is not running".into()));
        }
        let r = w
            .runtime
            .as_ref()
            .ok_or_else(|| ManagerError::Runtime("runtime metadata missing".into()))?;
        let secret = self
            .store
            .get_runtime_secret(id)?
            .ok_or_else(|| ManagerError::Runtime("runtime secret missing".into()))?;
        let i = self
            .runtime
            .inspect(&r.container_id)
            .await?
            .ok_or_else(|| ManagerError::Runtime("container missing".into()))?;
        if !Self::owned(&i, id) || i.image != PINNED_WEBTOP_IMAGE || !i.running {
            return Err(ManagerError::Ownership);
        }
        let port = i
            .published_port
            .filter(|p| *p != 0)
            .ok_or_else(|| ManagerError::Runtime("container port missing".into()))?;
        Ok((format!("127.0.0.1:{port}"), secret.webtop_password))
    }

    /// Resolve the owned running container used to host the agent. The
    /// container identity is re-inspected instead of trusting persisted data.
    pub(crate) async fn agent_target(
        &self,
        id: Uuid,
    ) -> Result<
        (
            String,
            PermissionProfile,
            Harness,
            Option<String>,
            Option<String>,
        ),
        ManagerError,
    > {
        let _guard = self.lock(id).await;
        let workspace = self.get(id).await?;
        if workspace.state != LifecycleState::Running {
            return Err(ManagerError::Invalid("workspace is not running".into()));
        }
        let runtime = workspace
            .runtime
            .as_ref()
            .ok_or_else(|| ManagerError::Runtime("runtime metadata missing".into()))?;
        let container = self
            .runtime
            .inspect(&runtime.container_id)
            .await?
            .ok_or_else(|| ManagerError::Runtime("container missing".into()))?;
        if !Self::owned(&container, id)
            || container.image != PINNED_WEBTOP_IMAGE
            || !container.running
        {
            return Err(ManagerError::Ownership);
        }
        Ok((
            container.id,
            workspace.profile,
            workspace.harness,
            workspace.model,
            workspace.reasoning_effort,
        ))
    }
    pub fn new(store: Arc<WorkspaceStore>, runtime: Arc<dyn ContainerRuntime>) -> Self {
        let workspaces_root = Self::default_workspaces_root();
        // Create the managed root eagerly so Docker Desktop's file-sharing layer
        // has synced it before the first bot's bind-mount (a brand-new ~/Orbit
        // tree otherwise races: "bind source path does not exist").
        let _ = std::fs::create_dir_all(&workspaces_root);
        Self {
            store,
            runtime,
            workspaces_root,
            policy: Default::default(),
            locks: Arc::new(Mutex::new(BTreeMap::new())),
            mutations_ready: Arc::new(AtomicBool::new(false)),
            runtime_available: Arc::new(AtomicBool::new(false)),
            lifecycle_gate: Arc::new(tokio::sync::RwLock::new(())),
        }
    }
    pub fn new_for_test(
        store: Arc<WorkspaceStore>,
        runtime: Arc<dyn ContainerRuntime>,
        policy: HealthPolicy,
    ) -> Self {
        Self {
            store,
            runtime,
            workspaces_root: std::env::temp_dir().join(format!("orbit-test-ws-{}", Uuid::new_v4())),
            policy,
            locks: Arc::new(Mutex::new(BTreeMap::new())),
            mutations_ready: Arc::new(AtomicBool::new(true)),
            runtime_available: Arc::new(AtomicBool::new(true)),
            lifecycle_gate: Arc::new(tokio::sync::RwLock::new(())),
        }
    }
    async fn lock(&self, id: Uuid) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.locks.lock().expect("workspace lock map");
            locks
                .entry(id)
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        lock.lock_owned().await
    }
    pub fn with_policy(mut self, p: HealthPolicy) -> Self {
        self.policy = p;
        self
    }
    pub async fn list(&self) -> Result<Vec<Workspace>, ManagerError> {
        Ok(self.store.list()?)
    }

    pub async fn get_bot_settings(&self, workspace_id: Uuid) -> Result<BotSettings, ManagerError> {
        Ok(self
            .store
            .get_bot_settings(workspace_id)?
            .unwrap_or(BotSettings {
                workspace_id,
                display_name: None,
                label: None,
                description: None,
                avatar_color: None,
                notifications: true,
            }))
    }

    pub async fn set_bot_settings(
        &self,
        workspace_id: Uuid,
        display_name: Option<String>,
        label: Option<String>,
        description: Option<String>,
        avatar_color: Option<String>,
        notifications: bool,
    ) -> Result<BotSettings, ManagerError> {
        let normalize = |value: Option<String>| {
            value.and_then(|value| {
                let value = value.trim().to_string();
                (!value.is_empty()).then_some(value)
            })
        };
        let display_name = normalize(display_name);
        let label = normalize(label);
        let description = normalize(description);
        let avatar_color = normalize(avatar_color);
        if display_name
            .as_ref()
            .is_some_and(|value| value.len() > MAX_BOT_NAME_BYTES)
        {
            return Err(ManagerError::Invalid("display name is too long".into()));
        }
        if label
            .as_ref()
            .is_some_and(|value| value.len() > MAX_BOT_LABEL_BYTES)
        {
            return Err(ManagerError::Invalid("label is too long".into()));
        }
        if description
            .as_ref()
            .is_some_and(|value| value.len() > MAX_BOT_DESCRIPTION_BYTES)
        {
            return Err(ManagerError::Invalid("description is too long".into()));
        }
        let settings = BotSettings {
            workspace_id,
            display_name,
            label,
            description,
            avatar_color,
            notifications,
        };
        self.store.upsert_bot_settings(&settings)?;
        Ok(settings)
    }

    pub async fn list_bot_settings(&self) -> Result<Vec<BotSettings>, ManagerError> {
        Ok(self.store.list_all_bot_settings()?)
    }

    pub async fn set_workspace_model(
        &self,
        workspace_id: Uuid,
        model: Option<String>,
        reasoning_effort: Option<String>,
    ) -> Result<Workspace, ManagerError> {
        let mut w = self.get(workspace_id).await?;
        w.model = model.filter(|value| !value.is_empty());
        w.reasoning_effort = reasoning_effort.filter(|value| !value.is_empty());
        self.store.upsert(&w)?;
        Ok(w)
    }

    pub async fn set_workspace_apps(
        &self,
        workspace_id: Uuid,
        apps: Vec<String>,
    ) -> Result<Workspace, ManagerError> {
        let mut w = self.get(workspace_id).await?;
        w.apps = apps;
        self.store.upsert(&w)?;
        Ok(w)
    }

    pub fn list_apps(&self) -> Vec<AppOption> {
        vec![
            AppOption {
                id: "metamask".into(),
                label: "MetaMask".into(),
                description: "Ethereum wallet (browser extension)".into(),
            },
            AppOption {
                id: "obsidian".into(),
                label: "Obsidian".into(),
                description: "Markdown knowledge base (AppImage)".into(),
            },
        ]
    }

    /// Models a harness offers. Defensive by design: any spawn/exec/parse
    /// failure yields an empty Vec rather than an error, since this only
    /// feeds an optional picker.
    pub async fn list_models(&self, harness: Harness) -> Vec<ModelOption> {
        match harness {
            Harness::Codex => list_codex_models().await,
            Harness::Antigravity => list_antigravity_models().await,
            // ponytail: opencode dynamic listing needs an authed container; deferred, frontend uses free-text.
            Harness::Opencode => Vec::new(),
            Harness::ClaudeCode => claude_code_models(),
        }
    }

    fn validate_skill(name: &str, instruction: &str) -> Result<(), ManagerError> {
        if name.trim().is_empty()
            || name.trim().len() > MAX_SKILL_NAME_BYTES
            || instruction.trim().is_empty()
            || instruction.len() > MAX_SKILL_INSTRUCTION_BYTES
        {
            return Err(ManagerError::Invalid("invalid skill".into()));
        }
        Ok(())
    }
    pub async fn create_skill(
        &self,
        workspace_id: Uuid,
        name: String,
        instruction: String,
        enabled: bool,
    ) -> Result<Skill, ManagerError> {
        Self::validate_skill(&name, &instruction)?;
        let skill = Skill {
            id: Uuid::new_v4(),
            workspace_id,
            name: name.trim().into(),
            instruction,
            enabled,
        };
        self.store.upsert_skill(&skill)?;
        Ok(skill)
    }
    pub async fn list_skills(&self, workspace_id: Uuid) -> Result<Vec<Skill>, ManagerError> {
        Ok(self.store.list_skills(workspace_id)?)
    }
    pub async fn update_skill(
        &self,
        id: Uuid,
        name: String,
        instruction: String,
        enabled: bool,
    ) -> Result<Skill, ManagerError> {
        Self::validate_skill(&name, &instruction)?;
        let mut skill = self.store.get_skill(id)?.ok_or(ManagerError::NotFound)?;
        skill.name = name.trim().into();
        skill.instruction = instruction;
        skill.enabled = enabled;
        self.store.upsert_skill(&skill)?;
        Ok(skill)
    }
    pub async fn delete_skill(&self, id: Uuid) -> Result<(), ManagerError> {
        Ok(self.store.delete_skill(id)?)
    }

    /// Reconcile durable workspace state with the existing runtime inventory. This deliberately
    /// never creates, removes, or starts containers during daemon startup.
    pub async fn reconcile(&self) -> Result<ReconcileReport, ManagerError> {
        let _lifecycle_guard = self.lifecycle_gate.write().await;
        self.mutations_ready.store(false, Ordering::Release);
        self.runtime_available.store(false, Ordering::Release);
        let mut report = ReconcileReport::default();
        let prerequisite = match self.runtime.prerequisite().await {
            Ok(p) => p,
            Err(e) => {
                report.diagnostics.push(ReconcileDiagnostic {
                    workspace_id: None,
                    code: ReconcileCode::RuntimeUnavailable,
                    message: "container runtime unavailable".into(),
                });
                let _ = e;
                return Ok(report);
            }
        };
        if !prerequisite.available {
            report.diagnostics.push(ReconcileDiagnostic {
                workspace_id: None,
                code: ReconcileCode::RuntimeUnavailable,
                message: "container runtime unavailable".into(),
            });
            return Ok(report);
        }
        self.runtime_available.store(true, Ordering::Release);
        let mut containers = self.runtime.managed().await?;
        report.examined_containers = containers.len();
        let workspaces = self.store.list()?;
        report.examined_workspaces = workspaces.len();
        let mut associated = std::collections::BTreeSet::new();
        for mut w in workspaces {
            let _guard = self.lock(w.id).await;
            let id_text = w.id.to_string();
            let recorded = w
                .runtime
                .as_ref()
                .and_then(|r| containers.iter().find(|c| c.id == r.container_id).cloned());
            let labeled: Vec<_> = containers
                .iter()
                .filter(|c| c.labels.get("com.orbit.workspace-id") == Some(&id_text))
                .cloned()
                .collect();
            let mut candidates = labeled;
            if let Some(r) = &recorded {
                candidates.push(r.clone());
            }
            candidates.sort_by(|a, b| a.id.cmp(&b.id));
            candidates.dedup_by(|a, b| a.id == b.id);
            associated.extend(candidates.iter().map(|c| c.id.clone()));
            if let Some(r) = &recorded
                && !Self::owned(r, w.id)
                && !Self::legacy_owned(r, w.id)
            {
                self.fail(&mut w)?;
                report.diagnostics.push(ReconcileDiagnostic {
                    workspace_id: Some(w.id),
                    code: ReconcileCode::OwnershipMismatch,
                    message: "container labels or image do not match workspace".into(),
                });
                continue;
            }
            if candidates.len() > 1 {
                self.fail(&mut w)?;
                report.diagnostics.push(ReconcileDiagnostic {
                    workspace_id: Some(w.id),
                    code: ReconcileCode::DuplicateContainers,
                    message: "multiple matching containers".into(),
                });
                continue;
            }
            let candidate = candidates.first();
            if let Some(c) = candidate
                && !Self::owned(c, w.id)
                && !Self::legacy_owned(c, w.id)
            {
                self.fail(&mut w)?;
                report.diagnostics.push(ReconcileDiagnostic {
                    workspace_id: Some(w.id),
                    code: ReconcileCode::OwnershipMismatch,
                    message: "container labels or image do not match workspace".into(),
                });
                continue;
            }
            let Some(c) = candidate else {
                w.runtime = None;
                match w.state {
                    LifecycleState::Running | LifecycleState::Starting => {
                        self.fail(&mut w)?;
                        report.diagnostics.push(ReconcileDiagnostic {
                            workspace_id: Some(w.id),
                            code: ReconcileCode::MissingContainer,
                            message: "managed container is missing".into(),
                        });
                    }
                    LifecycleState::Stopped | LifecycleState::Stopping => {
                        w.state = LifecycleState::Stopped;
                        self.store.upsert(&w)?;
                    }
                    LifecycleState::Creating
                    | LifecycleState::Resetting
                    | LifecycleState::Deleting => {
                        self.fail(&mut w)?;
                        report.diagnostics.push(ReconcileDiagnostic {
                            workspace_id: Some(w.id),
                            code: ReconcileCode::InterruptedLifecycle,
                            message:
                                "workspace was interrupted during a mutating lifecycle operation"
                                    .into(),
                        });
                    }
                    LifecycleState::Failed => {
                        self.store.upsert(&w)?;
                    }
                    LifecycleState::Deleted => {}
                }
                continue;
            };
            if Self::legacy_owned(c, w.id) {
                report.diagnostics.push(ReconcileDiagnostic {
                    workspace_id: Some(w.id),
                    code: ReconcileCode::UpgradeRequired,
                    message: "managed container requires an Orbit image upgrade".into(),
                });
                continue;
            }
            let port = c.published_port.unwrap_or(0);
            w.runtime = Some(RuntimeMetadata {
                container_id: c.id.clone(),
                container_name: c.name.clone(),
                volume_name: w
                    .runtime
                    .as_ref()
                    .map(|r| r.volume_name.clone())
                    .unwrap_or_else(|| format!("orbit-config-{}", w.id)),
                upstream_port: port,
                image_ref: PINNED_WEBTOP_IMAGE.into(),
            });
            match w.state {
                LifecycleState::Running | LifecycleState::Starting => {
                    if !c.running {
                        w.state = LifecycleState::Stopped;
                        self.store.upsert(&w)?;
                        continue;
                    }
                    let Some(secret) = self.store.get_runtime_secret(w.id)? else {
                        self.fail(&mut w)?;
                        report.diagnostics.push(ReconcileDiagnostic {
                            workspace_id: Some(w.id),
                            code: ReconcileCode::MissingSecret,
                            message: "runtime secret is missing".into(),
                        });
                        continue;
                    };
                    if port == 0
                        || !self
                            .runtime
                            .healthy(port, "agent", &secret.webtop_password)
                            .await
                            .unwrap_or(false)
                    {
                        self.fail(&mut w)?;
                        report.diagnostics.push(ReconcileDiagnostic {
                            workspace_id: Some(w.id),
                            code: ReconcileCode::HealthFailed,
                            message: "container health check failed".into(),
                        });
                    } else {
                        w.state = LifecycleState::Running;
                        self.store.upsert(&w)?;
                    }
                }
                LifecycleState::Stopping | LifecycleState::Stopped => {
                    if c.running {
                        self.runtime.stop(&c.id).await?;
                    }
                    w.state = LifecycleState::Stopped;
                    self.store.upsert(&w)?;
                }
                LifecycleState::Resetting | LifecycleState::Creating | LifecycleState::Deleting => {
                    self.fail(&mut w)?;
                    report.diagnostics.push(ReconcileDiagnostic {
                        workspace_id: Some(w.id),
                        code: ReconcileCode::InterruptedLifecycle,
                        message: "workspace was interrupted during a mutating lifecycle operation"
                            .into(),
                    });
                }
                LifecycleState::Failed => {
                    self.store.upsert(&w)?;
                }
                LifecycleState::Deleted => {}
            }
        }
        for c in containers.drain(..) {
            if !associated.contains(&c.id) {
                report.orphan_container_ids.push(c.id.clone());
                report.diagnostics.push(ReconcileDiagnostic {
                    workspace_id: None,
                    code: ReconcileCode::OrphanContainer,
                    message: "managed container is not associated with a workspace".into(),
                });
            }
        }
        for w in self.store.list()? {
            match w.state {
                LifecycleState::Running => report.running += 1,
                LifecycleState::Stopped => report.stopped += 1,
                LifecycleState::Failed => report.failed += 1,
                _ => {}
            }
        }
        self.mutations_ready.store(true, Ordering::Release);
        Ok(report)
    }

    pub async fn reconcile_and_migrate(
        &self,
    ) -> Result<(ReconcileReport, Vec<Uuid>), ManagerError> {
        let report = self.reconcile().await?;
        let eligible = report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == ReconcileCode::UpgradeRequired)
            .filter_map(|diagnostic| diagnostic.workspace_id)
            .collect::<BTreeSet<_>>();
        let migrated = self.migrate_legacy_images(&eligible).await?;
        Ok((report, migrated))
    }
    pub async fn get(&self, id: Uuid) -> Result<Workspace, ManagerError> {
        self.store.get(id)?.ok_or(ManagerError::NotFound)
    }
    fn secret() -> String {
        let mut b = [0u8; 32];
        rand::rng().fill_bytes(&mut b);
        URL_SAFE_NO_PAD.encode(b)
    }
    fn persist_state(&self, w: &mut Workspace, n: LifecycleState) -> Result<(), ManagerError> {
        w.transition(n)
            .map_err(|e| ManagerError::Transition(e.to_string()))?;
        self.store.upsert(w)?;
        Ok(())
    }
    fn fail(&self, w: &mut Workspace) -> Result<(), ManagerError> {
        w.state = LifecycleState::Failed;
        self.store.upsert(w).map_err(ManagerError::from)
    }
    pub fn mutations_ready(&self) -> bool {
        self.mutations_ready.load(Ordering::Acquire)
    }
    pub fn runtime_available(&self) -> bool {
        self.runtime_available.load(Ordering::Acquire)
    }
    pub fn ensure_mutations_ready(&self) -> Result<(), ManagerError> {
        if self.mutations_ready() {
            Ok(())
        } else {
            Err(ManagerError::RuntimeUnavailable)
        }
    }
    fn err(e: RuntimeError, p: Option<&str>) -> ManagerError {
        let mut s = e.to_string();
        if let Some(p) = p.filter(|x| !x.is_empty()) {
            s = s.replace(p, "[REDACTED]");
        }
        for sensitive in ["password", "secret"] {
            s = s.replace(sensitive, "[REDACTED]");
        }
        ManagerError::Runtime(s)
    }
    fn owned(i: &ContainerInfo, id: Uuid) -> bool {
        Self::owned_version(i, id, PINNED_WEBTOP_IMAGE, CURRENT_WEBTOP_IDENTITY)
    }
    fn legacy_owned(i: &ContainerInfo, id: Uuid) -> bool {
        Self::owned_version(i, id, LEGACY_WEBTOP_IMAGE, LEGACY_WEBTOP_IDENTITY)
    }
    fn owned_version(i: &ContainerInfo, id: Uuid, image: &str, identity: &str) -> bool {
        [
            ("com.orbit.managed", "true".into()),
            ("com.orbit.workspace-id", id.to_string()),
            ("com.orbit.schema", "1".into()),
            ("com.orbit.image", identity.into()),
        ]
        .iter()
        .all(|(k, v)| i.labels.get(*k) == Some(v))
            && i.image == image
    }
    fn spec(w: &Workspace, p: &str) -> ContainerSpec {
        Self::spec_with_volume(w, p, format!("orbit-config-{}", w.id))
    }
    fn spec_with_volume(w: &Workspace, p: &str, volume_name: String) -> ContainerSpec {
        let mut l = BTreeMap::new();
        l.insert("com.orbit.managed".into(), "true".into());
        l.insert("com.orbit.workspace-id".into(), w.id.to_string());
        l.insert("com.orbit.schema".into(), "1".into());
        l.insert("com.orbit.image".into(), CURRENT_WEBTOP_IDENTITY.into());
        ContainerSpec {
            workspace_id: w.id,
            container_name: format!("orbit-{}", w.id),
            volume_name,
            host_path: w.host_path.clone(),
            profile: w.profile,
            resources: w.resources.clone(),
            webtop_password: p.into(),
            image_ref: PINNED_WEBTOP_IMAGE.into(),
            labels: l,
        }
    }

    /// Replace the one explicitly recognized legacy Orbit image while retaining the named
    /// config volume and runtime secret. Unknown or modified containers are never touched.
    async fn migrate_legacy_images(
        &self,
        eligible: &BTreeSet<Uuid>,
    ) -> Result<Vec<Uuid>, ManagerError> {
        if eligible.is_empty() {
            return Ok(Vec::new());
        }
        let _lifecycle_guard = self.lifecycle_gate.read().await;
        self.ensure_mutations_ready()?;
        let mut migrated = Vec::new();
        for workspace in self.store.list()? {
            if !eligible.contains(&workspace.id) {
                continue;
            }
            let _guard = self.lock(workspace.id).await;
            let mut w = self.get(workspace.id).await?;
            let candidates = self
                .runtime
                .managed()
                .await
                .map_err(|e| Self::err(e, None))?
                .into_iter()
                .filter(|container| {
                    container.labels.get("com.orbit.workspace-id") == Some(&w.id.to_string())
                })
                .collect::<Vec<_>>();
            if candidates.len() != 1 {
                continue;
            }
            let container = &candidates[0];
            if !Self::legacy_owned(container, w.id) {
                continue;
            }
            if let Some(recorded) = &w.runtime
                && recorded.container_id != container.id
                && self
                    .runtime
                    .inspect(&recorded.container_id)
                    .await?
                    .is_some()
            {
                continue;
            }
            let volume_name = format!("orbit-config-{}", w.id);
            let secret = self
                .store
                .get_runtime_secret(w.id)?
                .ok_or_else(|| ManagerError::Runtime("runtime secret missing".into()))?;
            self.runtime
                .ensure_image(PINNED_WEBTOP_IMAGE)
                .await
                .map_err(|e| Self::err(e, Some(&secret.webtop_password)))?;

            let should_run = matches!(w.state, LifecycleState::Running | LifecycleState::Starting);
            w.state = if should_run {
                LifecycleState::Starting
            } else {
                LifecycleState::Stopping
            };
            self.store.upsert(&w)?;
            if container.running {
                self.runtime
                    .stop(&container.id)
                    .await
                    .map_err(|e| Self::err(e, Some(&secret.webtop_password)))?;
            }
            self.runtime
                .remove(&container.id)
                .await
                .map_err(|e| Self::err(e, Some(&secret.webtop_password)))?;
            w.runtime = None;
            self.store.upsert(&w)?;

            let created = self
                .runtime
                .create(&Self::spec_with_volume(
                    &w,
                    &secret.webtop_password,
                    volume_name.clone(),
                ))
                .await
                .map_err(|e| Self::err(e, Some(&secret.webtop_password)))?;
            if !Self::owned(&created, w.id) {
                self.fail(&mut w)?;
                return Err(ManagerError::Ownership);
            }
            w.runtime = Some(RuntimeMetadata {
                container_id: created.id.clone(),
                container_name: created.name.clone(),
                volume_name,
                upstream_port: created.published_port.unwrap_or(0),
                image_ref: PINNED_WEBTOP_IMAGE.into(),
            });
            self.store.upsert(&w)?;

            if should_run {
                self.runtime
                    .start(&created.id)
                    .await
                    .map_err(|e| Self::err(e, Some(&secret.webtop_password)))?;
                let started = self
                    .runtime
                    .inspect(&created.id)
                    .await
                    .map_err(|e| Self::err(e, Some(&secret.webtop_password)))?
                    .ok_or_else(|| ManagerError::Runtime("started container missing".into()))?;
                if !Self::owned(&started, w.id) {
                    self.fail(&mut w)?;
                    return Err(ManagerError::Ownership);
                }
                let port = started
                    .published_port
                    .filter(|port| *port != 0)
                    .ok_or_else(|| {
                        ManagerError::Runtime("container did not publish a port".into())
                    })?;
                if let Some(runtime) = &mut w.runtime {
                    runtime.container_name = started.name;
                    runtime.upstream_port = port;
                }
                self.store.upsert(&w)?;
                self.gate(port, &secret.webtop_password).await?;
                w.state = LifecycleState::Running;
            } else {
                w.state = LifecycleState::Stopped;
            }
            self.store.upsert(&w)?;
            migrated.push(w.id);
        }
        Ok(migrated)
    }
    async fn gate(&self, port: u16, p: &str) -> Result<(), ManagerError> {
        for n in 0..self.policy.attempts.max(1) {
            if self
                .runtime
                .healthy(port, "agent", p)
                .await
                .map_err(|e| Self::err(e, Some(p)))?
            {
                return Ok(());
            }
            if n + 1 < self.policy.attempts {
                tokio::time::sleep(self.policy.delay).await
            }
        }
        Err(ManagerError::Runtime("health check failed".into()))
    }
    async fn provision(&self, w: &mut Workspace, p: &str) -> Result<(), ManagerError> {
        // The state is durable before any external mutation, allowing crash recovery.
        if w.state != LifecycleState::Starting {
            self.persist_state(w, LifecycleState::Starting)?;
        } else {
            self.store.upsert(w)?;
        }
        self.runtime
            .ensure_image(PINNED_WEBTOP_IMAGE)
            .await
            .map_err(|e| Self::err(e, Some(p)))?;
        let candidates = self
            .runtime
            .managed()
            .await
            .map_err(|e| Self::err(e, Some(p)))?
            .into_iter()
            .filter(|i| i.labels.get("com.orbit.workspace-id") == Some(&w.id.to_string()))
            .collect::<Vec<_>>();
        let (i, created) = match candidates.len() {
            0 => (
                self.runtime
                    .create(&Self::spec(w, p))
                    .await
                    .map_err(|e| Self::err(e, Some(p)))?,
                true,
            ),
            1 if Self::owned(&candidates[0], w.id) => (candidates[0].clone(), false),
            _ => return Err(ManagerError::Ownership),
        };
        // Docker assigns an ephemeral host port when the container starts, not
        // necessarily when it is created. Persist the identity first for crash
        // recovery, then inspect again after start for the authoritative port.
        w.runtime = Some(RuntimeMetadata {
            container_id: i.id.clone(),
            container_name: i.name,
            volume_name: format!("orbit-config-{}", w.id),
            upstream_port: i.published_port.unwrap_or(0),
            image_ref: PINNED_WEBTOP_IMAGE.into(),
        });
        if let Err(e) = self.store.upsert(w) {
            if created {
                let _ = self.runtime.remove(&i.id).await;
                let _ = self
                    .runtime
                    .remove_volume(&format!("orbit-config-{}", w.id))
                    .await;
            }
            return Err(e.into());
        }
        self.runtime
            .start(&i.id)
            .await
            .map_err(|e| Self::err(e, Some(p)))?;
        let started = self
            .runtime
            .inspect(&i.id)
            .await
            .map_err(|e| Self::err(e, Some(p)))?
            .ok_or_else(|| ManagerError::Runtime("started container missing".into()))?;
        if !Self::owned(&started, w.id) {
            return Err(ManagerError::Ownership);
        }
        let port = started
            .published_port
            .filter(|port| *port != 0)
            .ok_or_else(|| ManagerError::Runtime("container did not publish a port".into()))?;
        if let Some(runtime) = &mut w.runtime {
            runtime.container_name = started.name;
            runtime.upstream_port = port;
        }
        self.store.upsert(w)?;
        self.gate(port, p).await?;
        self.persist_state(w, LifecycleState::Running)
    }
    pub async fn create(
        &self,
        name: String,
        path: PathBuf,
        profile: PermissionProfile,
        r: ResourceLimits,
    ) -> Result<Workspace, ManagerError> {
        self.create_with_harness(name, path, profile, Harness::default(), None, None, r)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_with_harness(
        &self,
        name: String,
        path: PathBuf,
        profile: PermissionProfile,
        harness: Harness,
        model: Option<String>,
        reasoning_effort: Option<String>,
        r: ResourceLimits,
    ) -> Result<Workspace, ManagerError> {
        let _lifecycle_guard = self.lifecycle_gate.read().await;
        self.ensure_mutations_ready()?;
        if name.trim().is_empty() {
            return Err(ManagerError::Invalid("name must not be empty".into()));
        }
        let path = if path.as_os_str().is_empty() {
            let path = self.workspaces_root.join(Self::slug(&name));
            std::fs::create_dir_all(&path).map_err(|_| {
                ManagerError::Invalid("could not create the default workspace folder".into())
            })?;
            path
        } else {
            path
        };
        let path = std::fs::canonicalize(path)
            .map_err(|_| ManagerError::Invalid("host path must be an existing directory".into()))?;
        if !path.is_dir() {
            return Err(ManagerError::Invalid(
                "host path must be an existing directory".into(),
            ));
        }
        let r = ResourceLimits::new(r.cpus, r.memory_bytes, r.pids, r.soft_disk_bytes)
            .map_err(|e| ManagerError::Invalid(e.to_string()))?;
        let mut w = Workspace::new(name, path, profile, r);
        w.harness = harness;
        w.model = model.filter(|value| !value.is_empty());
        w.reasoning_effort = reasoning_effort.filter(|value| !value.is_empty());
        let p = Self::secret();
        self.store.insert_workspace_with_secret(
            &w,
            &RuntimeSecret {
                workspace_id: w.id,
                webtop_password: p.clone(),
            },
        )?;
        if let Err(e) = self.provision(&mut w, &p).await {
            self.fail(&mut w)?;
            return Err(e);
        }
        Ok(w)
    }
    pub async fn start(&self, id: Uuid) -> Result<Workspace, ManagerError> {
        let _lifecycle_guard = self.lifecycle_gate.read().await;
        self.ensure_mutations_ready()?;
        let _guard = self.lock(id).await;
        let mut w = self.get(id).await?;
        let Some(s) = self.store.get_runtime_secret(id)? else {
            if matches!(
                w.state,
                LifecycleState::Creating | LifecycleState::Starting | LifecycleState::Resetting
            ) {
                self.persist_state(&mut w, LifecycleState::Failed)?;
            }
            return Err(ManagerError::Runtime("missing runtime secret".into()));
        };
        let recovering_starting = w.state == LifecycleState::Starting;
        let i = if let Some(r) = &w.runtime {
            self.runtime
                .inspect(&r.container_id)
                .await
                .map_err(|e| Self::err(e, Some(&s.webtop_password)))?
        } else {
            None
        };
        let container_id = if let Some(i) = i {
            if !Self::owned(&i, id) {
                self.fail(&mut w)?;
                return Err(ManagerError::Ownership);
            }
            if i.running && w.state == LifecycleState::Running {
                return Ok(w);
            }
            if w.state == LifecycleState::Running {
                w.state = LifecycleState::Failed;
                self.store.upsert(&w)?;
            }
            if w.state != LifecycleState::Starting {
                self.persist_state(&mut w, LifecycleState::Starting)?;
            }
            if (!i.running || !recovering_starting)
                && let Err(e) = self.runtime.start(&i.id).await
            {
                let e = Self::err(e, Some(&s.webtop_password));
                self.fail(&mut w)?;
                return Err(e);
            }
            i.id
        } else {
            w.runtime = None;
            if let Err(e) = self.provision(&mut w, &s.webtop_password).await {
                self.fail(&mut w)?;
                return Err(e);
            }
            return Ok(w);
        };
        let refreshed = self
            .runtime
            .inspect(&container_id)
            .await
            .map_err(|e| Self::err(e, Some(&s.webtop_password)))?
            .ok_or_else(|| ManagerError::Runtime("started container missing".into()))?;
        if !Self::owned(&refreshed, id) {
            self.fail(&mut w)?;
            return Err(ManagerError::Ownership);
        }
        let Some(port) = refreshed.published_port.filter(|port| *port != 0) else {
            self.fail(&mut w)?;
            return Err(ManagerError::Runtime(
                "container did not publish a port".into(),
            ));
        };
        if let Some(runtime) = &mut w.runtime {
            runtime.container_name = refreshed.name;
            runtime.upstream_port = port;
        }
        self.store.upsert(&w)?;
        if let Err(e) = self.gate(port, &s.webtop_password).await {
            self.fail(&mut w)?;
            return Err(e);
        }
        self.persist_state(&mut w, LifecycleState::Running)?;
        if !w.apps.is_empty() {
            let profile = match w.profile {
                PermissionProfile::Observe => "observe",
                PermissionProfile::Workspace => "workspace",
                PermissionProfile::FullControl => "full_control",
            };
            let mut args: Vec<&str> = vec!["orbit-install-apps", "--profile", profile];
            for a in &w.apps {
                args.push(a.as_str());
            }
            let _ = self.runtime.exec(&container_id, "root", &args).await;
        }
        Ok(w)
    }
    pub async fn stop(&self, id: Uuid) -> Result<Workspace, ManagerError> {
        let _lifecycle_guard = self.lifecycle_gate.read().await;
        self.ensure_mutations_ready()?;
        let _guard = self.lock(id).await;
        let mut w = self.get(id).await?;
        let Some(r) = w.runtime.clone() else {
            if w.state == LifecycleState::Stopped {
                return Ok(w);
            }
            if matches!(
                w.state,
                LifecycleState::Creating | LifecycleState::Starting | LifecycleState::Resetting
            ) {
                self.persist_state(&mut w, LifecycleState::Failed)?;
            }
            if w.state == LifecycleState::Failed || w.state == LifecycleState::Running {
                self.persist_state(&mut w, LifecycleState::Stopping)?;
            }
            if w.state == LifecycleState::Stopping {
                self.persist_state(&mut w, LifecycleState::Stopped)?;
            }
            return Ok(w);
        };
        let inspected = match self.runtime.inspect(&r.container_id).await {
            Ok(i) => i,
            Err(e) => {
                self.fail(&mut w)?;
                return Err(Self::err(
                    e,
                    self.store
                        .get_runtime_secret(id)?
                        .as_ref()
                        .map(|s| s.webtop_password.as_str()),
                ));
            }
        };
        if w.state != LifecycleState::Stopping && w.state != LifecycleState::Stopped {
            self.persist_state(&mut w, LifecycleState::Stopping)?;
        }
        if let Some(i) = inspected.as_ref()
            && !Self::owned(i, id)
        {
            self.fail(&mut w)?;
            return Err(ManagerError::Ownership);
        }
        if inspected.is_none() {
            w.runtime = None;
            if w.state == LifecycleState::Stopped {
                self.store.upsert(&w)?;
                return Ok(w);
            }
            self.persist_state(&mut w, LifecycleState::Stopped)?;
            return Ok(w);
        }
        if inspected.as_ref().is_some_and(|i| !i.running) {
            if w.state != LifecycleState::Stopped {
                self.persist_state(&mut w, LifecycleState::Stopped)?;
            }
            return Ok(w);
        }
        if let Err(e) = self.runtime.stop(&r.container_id).await {
            self.fail(&mut w)?;
            return Err(Self::err(
                e,
                self.store
                    .get_runtime_secret(id)?
                    .as_ref()
                    .map(|s| s.webtop_password.as_str()),
            ));
        }
        if w.state == LifecycleState::Stopped {
            return Ok(w);
        }
        self.persist_state(&mut w, LifecycleState::Stopped)?;
        Ok(w)
    }
    pub async fn reset(&self, id: Uuid) -> Result<Workspace, ManagerError> {
        let _lifecycle_guard = self.lifecycle_gate.read().await;
        self.ensure_mutations_ready()?;
        let _guard = self.lock(id).await;
        let mut w = self.get(id).await?;
        if w.runtime.is_none() && self.store.get_runtime_secret(id)?.is_none() {
            if matches!(
                w.state,
                LifecycleState::Creating | LifecycleState::Starting | LifecycleState::Resetting
            ) {
                self.persist_state(&mut w, LifecycleState::Failed)?;
            }
            return Err(ManagerError::Runtime("missing runtime secret".into()));
        }
        if matches!(
            w.state,
            LifecycleState::Starting | LifecycleState::Stopping | LifecycleState::Resetting
        ) {
            self.persist_state(&mut w, LifecycleState::Failed)?;
        }
        if w.state == LifecycleState::Running {
            if let Some(r) = w.runtime.clone()
                && let Some(i) = self.runtime.inspect(&r.container_id).await?
            {
                if !Self::owned(&i, id) {
                    self.fail(&mut w)?;
                    return Err(ManagerError::Ownership);
                }
                if i.running {
                    self.runtime.stop(&r.container_id).await?;
                }
            }
            w.state = LifecycleState::Stopped;
            self.store.upsert(&w)?;
        }
        if !matches!(w.state, LifecycleState::Stopped | LifecycleState::Failed) {
            return Err(ManagerError::Transition(
                "workspace must be stopped or failed".into(),
            ));
        }
        self.persist_state(&mut w, LifecycleState::Resetting)?;
        if let Some(r) = w.runtime.clone() {
            let inspected = match self.runtime.inspect(&r.container_id).await {
                Ok(i) => i,
                Err(e) => {
                    self.fail(&mut w)?;
                    return Err(Self::err(
                        e,
                        self.store
                            .get_runtime_secret(id)?
                            .as_ref()
                            .map(|s| s.webtop_password.as_str()),
                    ));
                }
            };
            if let Some(i) = inspected
                && !Self::owned(&i, id)
            {
                self.fail(&mut w)?;
                return Err(ManagerError::Ownership);
            }
            if let Err(e) = self.runtime.remove(&r.container_id).await {
                self.fail(&mut w)?;
                return Err(Self::err(
                    e,
                    self.store
                        .get_runtime_secret(id)?
                        .as_ref()
                        .map(|s| s.webtop_password.as_str()),
                ));
            }
            if let Err(e) = self.runtime.remove_volume(&r.volume_name).await {
                self.fail(&mut w)?;
                return Err(Self::err(
                    e,
                    self.store
                        .get_runtime_secret(id)?
                        .as_ref()
                        .map(|s| s.webtop_password.as_str()),
                ));
            }
        }
        let p = Self::secret();
        self.store.put_runtime_secret(&RuntimeSecret {
            workspace_id: id,
            webtop_password: p.clone(),
        })?;
        w.runtime = None;
        if let Err(e) = self.provision(&mut w, &p).await {
            self.fail(&mut w)?;
            return Err(e);
        }
        Ok(w)
    }
    pub async fn delete(&self, id: Uuid) -> Result<(), ManagerError> {
        let _lifecycle_guard = self.lifecycle_gate.read().await;
        self.ensure_mutations_ready()?;
        let _guard = self.lock(id).await;
        let Some(mut w) = self.store.get(id)? else {
            return Ok(());
        };
        if matches!(
            w.state,
            LifecycleState::Starting | LifecycleState::Stopping | LifecycleState::Resetting
        ) {
            self.persist_state(&mut w, LifecycleState::Failed)?;
        }
        if !matches!(
            w.state,
            LifecycleState::Stopped | LifecycleState::Failed | LifecycleState::Deleting
        ) {
            return Err(ManagerError::Transition(
                "workspace must be stopped or failed".into(),
            ));
        }
        if w.state != LifecycleState::Deleting {
            self.persist_state(&mut w, LifecycleState::Deleting)?;
        }
        if let Some(r) = w.runtime.clone() {
            let inspected = match self.runtime.inspect(&r.container_id).await {
                Ok(i) => i,
                Err(e) => {
                    self.fail(&mut w)?;
                    return Err(Self::err(
                        e,
                        self.store
                            .get_runtime_secret(id)?
                            .as_ref()
                            .map(|s| s.webtop_password.as_str()),
                    ));
                }
            };
            if let Some(i) = inspected
                && !Self::owned(&i, id)
            {
                self.fail(&mut w)?;
                return Err(ManagerError::Ownership);
            }
            if let Err(e) = self.runtime.remove(&r.container_id).await {
                self.fail(&mut w)?;
                return Err(Self::err(
                    e,
                    self.store
                        .get_runtime_secret(id)?
                        .as_ref()
                        .map(|s| s.webtop_password.as_str()),
                ));
            }
            if let Err(e) = self.runtime.remove_volume(&r.volume_name).await {
                self.fail(&mut w)?;
                return Err(Self::err(
                    e,
                    self.store
                        .get_runtime_secret(id)?
                        .as_ref()
                        .map(|s| s.webtop_password.as_str()),
                ));
            }
        }
        self.store.delete_workspace(id)?;
        Ok(())
    }
}

#[cfg(test)]
mod desktop_target_tests {
    use super::*;
    use crate::runtime::{MountInfo, RuntimePrerequisite};
    use async_trait::async_trait;

    #[test]
    fn slug_normalizes_names_and_falls_back_for_empty_results() {
        assert_eq!(WorkspaceManager::slug("My Bot!"), "my-bot");
        assert_eq!(WorkspaceManager::slug("  ***  "), "workspace");
    }

    type ExecCall = (String, String, Vec<String>);

    #[derive(Clone)]
    struct TargetRuntime(Arc<Mutex<Option<ContainerInfo>>>, Arc<Mutex<Vec<ExecCall>>>);

    impl TargetRuntime {
        fn set(&self, info: ContainerInfo) {
            *self.0.lock().unwrap() = Some(info);
        }
    }

    #[async_trait]
    impl ContainerRuntime for TargetRuntime {
        async fn prerequisite(&self) -> Result<RuntimePrerequisite, RuntimeError> {
            Ok(RuntimePrerequisite {
                available: true,
                version: Some("test".into()),
                message: "ready".into(),
            })
        }
        async fn ensure_image(&self, _: &str) -> Result<(), RuntimeError> {
            unreachable!()
        }
        async fn create(&self, _: &ContainerSpec) -> Result<ContainerInfo, RuntimeError> {
            unreachable!()
        }
        async fn start(&self, _: &str) -> Result<(), RuntimeError> {
            Ok(())
        }
        async fn exec(&self, id: &str, user: &str, args: &[&str]) -> Result<(), RuntimeError> {
            self.1.lock().unwrap().push((
                id.to_string(),
                user.to_string(),
                args.iter().map(|s| s.to_string()).collect(),
            ));
            Ok(())
        }
        async fn stop(&self, _: &str) -> Result<(), RuntimeError> {
            unreachable!()
        }
        async fn remove(&self, _: &str) -> Result<(), RuntimeError> {
            unreachable!()
        }
        async fn remove_volume(&self, _: &str) -> Result<(), RuntimeError> {
            unreachable!()
        }
        async fn inspect(&self, _: &str) -> Result<Option<ContainerInfo>, RuntimeError> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn managed(&self) -> Result<Vec<ContainerInfo>, RuntimeError> {
            unreachable!()
        }
        async fn healthy(&self, _: u16, _: &str, _: &str) -> Result<bool, RuntimeError> {
            Ok(true)
        }
    }

    fn inspected(workspace_id: Uuid, port: Option<u16>) -> ContainerInfo {
        let labels = BTreeMap::from([
            ("com.orbit.managed".into(), "true".into()),
            ("com.orbit.workspace-id".into(), workspace_id.to_string()),
            ("com.orbit.schema".into(), "1".into()),
            ("com.orbit.image".into(), "webtop-ubuntu-xfce-v2".into()),
        ]);
        ContainerInfo {
            id: "container".into(),
            name: format!("orbit-{workspace_id}"),
            status: "running".into(),
            running: true,
            published_port: port,
            image: PINNED_WEBTOP_IMAGE.into(),
            labels,
            mounts: vec![MountInfo {
                source: "/host".into(),
                destination: "/workspace".into(),
                read_only: false,
            }],
        }
    }

    fn manager_with_target() -> (WorkspaceManager, TargetRuntime, Workspace) {
        let id = Uuid::new_v4();
        let workspace = Workspace {
            id,
            name: "target".into(),
            host_path: "/host".into(),
            profile: PermissionProfile::Workspace,
            harness: orbit_domain::Harness::default(),
            model: None,
            reasoning_effort: None,
            apps: Vec::new(),
            resources: ResourceLimits::new(1.0, 1024, 16, 2048).unwrap(),
            state: LifecycleState::Running,
            runtime: Some(RuntimeMetadata {
                container_id: "container".into(),
                container_name: format!("orbit-{id}"),
                volume_name: format!("orbit-{id}-home"),
                upstream_port: 1111,
                image_ref: PINNED_WEBTOP_IMAGE.into(),
            }),
        };
        let secret = RuntimeSecret {
            workspace_id: id,
            webtop_password: "target-secret".into(),
        };
        let store = Arc::new(WorkspaceStore::in_memory().unwrap());
        store
            .insert_workspace_with_secret(&workspace, &secret)
            .unwrap();
        let runtime = TargetRuntime(
            Arc::new(Mutex::new(Some(inspected(id, Some(4321))))),
            Arc::new(Mutex::new(Vec::new())),
        );
        let manager = WorkspaceManager::new_for_test(
            store,
            Arc::new(runtime.clone()),
            HealthPolicy {
                attempts: 1,
                delay: Duration::ZERO,
            },
        );
        (manager, runtime, workspace)
    }

    #[tokio::test]
    async fn start_installs_workspace_apps_and_skips_empty_apps() {
        let (manager, runtime, mut workspace) = manager_with_target();
        let mut info = inspected(workspace.id, Some(4321));
        info.running = false;
        runtime.set(info);
        workspace.apps = vec!["metamask".into()];
        manager.store.upsert(&workspace).unwrap();

        manager.start(workspace.id).await.unwrap();
        assert_eq!(
            *runtime.1.lock().unwrap(),
            vec![(
                "container".into(),
                "root".into(),
                vec![
                    "orbit-install-apps".into(),
                    "--profile".into(),
                    "workspace".into(),
                    "metamask".into(),
                ],
            )]
        );

        workspace.apps.clear();
        manager.store.upsert(&workspace).unwrap();
        runtime.1.lock().unwrap().clear();
        manager.start(workspace.id).await.unwrap();
        assert!(runtime.1.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn desktop_target_uses_fresh_inspection_and_keeps_secret_internal() {
        let (manager, _, workspace) = manager_with_target();
        let (target, password) = manager.desktop_target(workspace.id).await.unwrap();
        assert_eq!(target, "127.0.0.1:4321");
        assert_eq!(password, "target-secret");
        assert_ne!(target, "127.0.0.1:1111");
    }

    #[tokio::test]
    async fn desktop_target_rejects_stopped_missing_secret_and_unsafe_inspection() {
        let (manager, runtime, mut workspace) = manager_with_target();
        workspace.state = LifecycleState::Stopped;
        manager.store.upsert(&workspace).unwrap();
        assert!(matches!(
            manager.desktop_target(workspace.id).await,
            Err(ManagerError::Invalid(_))
        ));

        workspace.state = LifecycleState::Running;
        manager.store.upsert(&workspace).unwrap();
        manager.store.delete_runtime_secret(workspace.id).unwrap();
        assert!(matches!(
            manager.desktop_target(workspace.id).await,
            Err(ManagerError::Runtime(message)) if message == "runtime secret missing"
        ));
        manager
            .store
            .put_runtime_secret(&RuntimeSecret {
                workspace_id: workspace.id,
                webtop_password: "target-secret".into(),
            })
            .unwrap();

        let mut bad = inspected(workspace.id, Some(4321));
        bad.running = false;
        runtime.set(bad);
        assert!(matches!(
            manager.desktop_target(workspace.id).await,
            Err(ManagerError::Ownership)
        ));

        let mut bad = inspected(workspace.id, None);
        runtime.set(bad.clone());
        assert!(matches!(
            manager.desktop_target(workspace.id).await,
            Err(ManagerError::Runtime(message)) if message == "container port missing"
        ));

        bad.published_port = Some(4321);
        bad.image = "untrusted-image".into();
        runtime.set(bad);
        assert!(matches!(
            manager.desktop_target(workspace.id).await,
            Err(ManagerError::Ownership)
        ));
    }

    #[tokio::test]
    async fn bot_settings_default_set_validation_and_list() {
        let (manager, _, workspace) = manager_with_target();
        let defaults = manager.get_bot_settings(workspace.id).await.unwrap();
        assert!(defaults.notifications);
        assert_eq!(defaults.display_name, None);
        let saved = manager
            .set_bot_settings(
                workspace.id,
                Some("  Orbit Bot ".into()),
                Some("  helper ".into()),
                Some(" desc ".into()),
                Some(" #529e85 ".into()),
                false,
            )
            .await
            .unwrap();
        assert_eq!(saved.display_name.as_deref(), Some("Orbit Bot"));
        assert_eq!(saved.avatar_color.as_deref(), Some("#529e85"));
        assert_eq!(manager.get_bot_settings(workspace.id).await.unwrap(), saved);
        assert!(matches!(
            manager
                .set_bot_settings(workspace.id, None, None, Some("x".repeat(8001)), None, true)
                .await,
            Err(ManagerError::Invalid(_))
        ));
        assert_eq!(manager.list_bot_settings().await.unwrap(), vec![saved]);
    }

    #[tokio::test]
    async fn skills_create_validate_list_update_and_delete() {
        let (manager, _, workspace) = manager_with_target();
        assert!(matches!(
            manager
                .create_skill(workspace.id, " ".into(), "do it".into(), true)
                .await,
            Err(ManagerError::Invalid(_))
        ));
        assert!(matches!(
            manager
                .create_skill(workspace.id, "skill".into(), " ".into(), true)
                .await,
            Err(ManagerError::Invalid(_))
        ));
        let skill = manager
            .create_skill(workspace.id, "  helper ".into(), "do it".into(), true)
            .await
            .unwrap();
        assert_eq!(
            manager.list_skills(workspace.id).await.unwrap(),
            vec![skill.clone()]
        );
        let updated = manager
            .update_skill(skill.id, "updated".into(), "do more".into(), false)
            .await
            .unwrap();
        assert_eq!(updated.name, "updated");
        assert!(!updated.enabled);
        manager.delete_skill(skill.id).await.unwrap();
        assert!(manager.list_skills(workspace.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn set_workspace_model_persists_then_blanks_on_reload() {
        let (manager, _, workspace) = manager_with_target();
        let updated = manager
            .set_workspace_model(
                workspace.id,
                Some("gpt-5-codex".into()),
                Some("high".into()),
            )
            .await
            .unwrap();
        assert_eq!(updated.model.as_deref(), Some("gpt-5-codex"));
        assert_eq!(updated.reasoning_effort.as_deref(), Some("high"));
        let reloaded = manager.get(workspace.id).await.unwrap();
        assert_eq!(reloaded.model.as_deref(), Some("gpt-5-codex"));
        assert_eq!(reloaded.reasoning_effort.as_deref(), Some("high"));

        let blanked = manager
            .set_workspace_model(workspace.id, Some("".into()), Some("".into()))
            .await
            .unwrap();
        assert_eq!(blanked.model, None);
        assert_eq!(blanked.reasoning_effort, None);
        let reloaded = manager.get(workspace.id).await.unwrap();
        assert_eq!(reloaded.model, None);
        assert_eq!(reloaded.reasoning_effort, None);
    }

    #[tokio::test]
    async fn set_workspace_apps_persists_then_blanks_on_reload() {
        let (manager, _, workspace) = manager_with_target();
        let updated = manager
            .set_workspace_apps(workspace.id, vec!["metamask".into()])
            .await
            .unwrap();
        assert_eq!(updated.apps, vec!["metamask"]);
        assert_eq!(
            manager.get(workspace.id).await.unwrap().apps,
            vec!["metamask"]
        );
        let blanked = manager
            .set_workspace_apps(workspace.id, Vec::new())
            .await
            .unwrap();
        assert!(blanked.apps.is_empty());
        assert!(manager.get(workspace.id).await.unwrap().apps.is_empty());
    }

    #[test]
    fn parses_codex_models_json_sample_and_falls_back_to_slug() {
        let stdout =
            r#"{"models":[{"slug":"gpt-5-codex","display_name":"GPT-5 Codex"},{"slug":"gpt-5"}]}"#;
        assert_eq!(
            parse_codex_models_json(stdout),
            vec![
                ModelOption {
                    id: "gpt-5-codex".into(),
                    label: "GPT-5 Codex".into()
                },
                ModelOption {
                    id: "gpt-5".into(),
                    label: "gpt-5".into()
                },
            ]
        );
        assert!(parse_codex_models_json("not json").is_empty());
        assert!(parse_codex_models_json("{}").is_empty());
    }

    #[test]
    fn parses_antigravity_models_lines_and_skips_noise() {
        let stdout = "Fetching available models...\ngemini-3-pro\tGemini 3 Pro\ngemini-3-flash\tGemini 3 Flash\n";
        assert_eq!(
            parse_antigravity_models_lines(stdout),
            vec![
                ModelOption {
                    id: "gemini-3-pro".into(),
                    label: "Gemini 3 Pro".into()
                },
                ModelOption {
                    id: "gemini-3-flash".into(),
                    label: "Gemini 3 Flash".into()
                },
            ]
        );
    }
}
