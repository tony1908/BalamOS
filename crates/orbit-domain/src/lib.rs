use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use thiserror::Error;
use uuid::Uuid;

mod agent;
pub use agent::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionProfile {
    Observe,
    Workspace,
    FullControl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Harness {
    #[default]
    Codex,
    Opencode,
    ClaudeCode,
    Antigravity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernancePolicy {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub require_approval: bool,
    #[serde(default)]
    pub require_container: bool,
    #[serde(default = "default_true")]
    pub allow_scheduled: bool,
    #[serde(default)]
    pub guidance: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernancePreset {
    pub id: String,
    pub name: String,
    pub policy: GovernancePolicy,
    pub builtin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceRule {
    pub id: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretMetadata {
    pub id: Uuid,
    pub name: String,
    pub env_name: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretInput(pub String);
impl fmt::Debug for SecretInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretAssignment {
    pub secret_id: Uuid,
    pub workspace_id: Uuid,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretEnv {
    pub env_name: String,
    pub value: SecretInput,
}

impl fmt::Debug for SecretEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretEnv")
            .field("env_name", &self.env_name)
            .field("value", &self.value)
            .finish()
    }
}

pub fn builtin_governance_presets() -> Vec<GovernancePreset> {
    vec![
        GovernancePreset {
            id: "container".into(),
            name: "Container only".into(),
            policy: GovernancePolicy {
                require_container: true,
                ..Default::default()
            },
            builtin: true,
        },
        GovernancePreset {
            id: "approval".into(),
            name: "Approval required".into(),
            policy: GovernancePolicy {
                require_approval: true,
                require_container: true,
                ..Default::default()
            },
            builtin: true,
        },
        GovernancePreset {
            id: "paused".into(),
            name: "Paused".into(),
            policy: GovernancePolicy {
                enabled: false,
                allow_scheduled: false,
                ..Default::default()
            },
            builtin: true,
        },
    ]
}

fn default_true() -> bool {
    true
}

impl Default for GovernancePolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            require_approval: false,
            require_container: false,
            allow_scheduled: true,
            guidance: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceGovernancePolicy {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub require_approval: Option<bool>,
    #[serde(default)]
    pub require_container: Option<bool>,
    #[serde(default)]
    pub allow_scheduled: Option<bool>,
    #[serde(default)]
    pub guidance: Option<String>,
    #[serde(default)]
    pub applied_rule_ids: Vec<String>,
    #[serde(default)]
    pub custom_rules: Vec<GovernanceRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GovernanceSource {
    Global,
    Local,
    Both,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveGovernancePolicy {
    pub enabled: bool,
    pub require_approval: bool,
    pub require_container: bool,
    pub allow_scheduled: bool,
    pub guidance: String,
    pub sources: std::collections::BTreeMap<String, GovernanceSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernanceView {
    pub global: GovernancePolicy,
    pub local: WorkspaceGovernancePolicy,
    pub effective: EffectiveGovernancePolicy,
    pub blocked_reasons: Vec<GovernanceBlockedReason>,
    pub library: Vec<GovernanceRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GovernanceBlockedReason {
    Disabled,
    ContainerRequired,
    HarnessUnsupportedApproval,
    ScheduledRunsDisabled,
}

impl fmt::Display for GovernanceBlockedReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Disabled => "governance is disabled",
            Self::ContainerRequired => "a container-backed harness is required",
            Self::HarnessUnsupportedApproval => {
                "the selected harness/profile cannot enforce approval"
            }
            Self::ScheduledRunsDisabled => "scheduled runs are disabled",
        })
    }
}

pub fn resolve_governance(
    global: &GovernancePolicy,
    local: &WorkspaceGovernancePolicy,
) -> EffectiveGovernancePolicy {
    let mut sources = std::collections::BTreeMap::new();
    let mut value = |name: &str, local: Option<bool>, global: bool, and: bool| {
        let restrictive = |value: bool| if and { !value } else { value };
        let source = match (restrictive(global), local.map(restrictive).unwrap_or(false)) {
            (true, true) => GovernanceSource::Both,
            (true, false) => GovernanceSource::Global,
            (false, true) => GovernanceSource::Local,
            (false, false) => GovernanceSource::Default,
        };
        sources.insert(name.into(), source);
        if and {
            global && local.unwrap_or(true)
        } else {
            global || local.unwrap_or(false)
        }
    };
    let guidance = match (&local.guidance, global.guidance.is_empty()) {
        (Some(local), _) if !local.is_empty() && !global.guidance.is_empty() => {
            format!("[global]\n{}\n[local]\n{}", global.guidance, local)
        }
        (Some(local), _) if !local.is_empty() => format!("[local]\n{}", local),
        _ if !global.guidance.is_empty() => format!("[global]\n{}", global.guidance),
        _ => String::new(),
    };
    let enabled = value("enabled", local.enabled, global.enabled, true);
    let require_approval = value(
        "require_approval",
        local.require_approval,
        global.require_approval,
        false,
    );
    let require_container = value(
        "require_container",
        local.require_container,
        global.require_container,
        false,
    );
    let allow_scheduled = value(
        "allow_scheduled",
        local.allow_scheduled,
        global.allow_scheduled,
        true,
    );
    drop(value);
    if local.guidance.is_some() {
        sources.insert("guidance_local".into(), GovernanceSource::Local);
    }
    if !global.guidance.is_empty() {
        sources.insert("guidance_global".into(), GovernanceSource::Global);
    }
    EffectiveGovernancePolicy {
        enabled,
        require_approval,
        require_container,
        allow_scheduled,
        guidance,
        sources,
    }
}

/// Advisory rule text injected into prompts: bodies of the applied library rules
/// (resolved by id) then the agent-local custom rules, each a titled block.
/// Missing ids are skipped; empty when there are no rules.
pub fn compose_rule_guidance(
    library: &[GovernanceRule],
    applied_ids: &[String],
    custom: &[GovernanceRule],
) -> String {
    let mut blocks: Vec<String> = Vec::new();
    for id in applied_ids {
        if let Some(rule) = library.iter().find(|r| &r.id == id) {
            blocks.push(format!("## {}\n{}", rule.title, rule.body));
        }
    }
    for rule in custom {
        blocks.push(format!("## {}\n{}", rule.title, rule.body));
    }
    blocks.join("\n\n")
}

impl EffectiveGovernancePolicy {
    pub fn blocked_reasons(
        &self,
        harness: Harness,
        profile: PermissionProfile,
        uses_container: bool,
        scheduled: bool,
    ) -> Vec<GovernanceBlockedReason> {
        let mut reasons = Vec::new();
        if !self.enabled {
            reasons.push(GovernanceBlockedReason::Disabled);
        }
        if self.require_container && (!uses_container || matches!(harness, Harness::Antigravity)) {
            reasons.push(GovernanceBlockedReason::ContainerRequired);
        }
        if self.require_approval
            && (!matches!(harness, Harness::Codex)
                || matches!(profile, PermissionProfile::FullControl))
        {
            reasons.push(GovernanceBlockedReason::HarnessUnsupportedApproval);
        }
        if scheduled && !self.allow_scheduled {
            reasons.push(GovernanceBlockedReason::ScheduledRunsDisabled);
        }
        reasons
    }
}

/// A model a harness offers, for populating a model picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelOption {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppOption {
    pub id: String,
    pub label: String,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harness_defaults_to_codex() {
        assert_eq!(Harness::default(), Harness::Codex);
    }

    #[test]
    fn user_prompt_event_round_trips_with_tagged_serde() {
        let event = AgentEventKind::UserPrompt { text: "hi".into() };
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"type": "user_prompt", "data": {"text": "hi"}})
        );
        assert_eq!(
            serde_json::from_value::<AgentEventKind>(value).unwrap(),
            event
        );
    }

    #[test]
    fn harness_uses_snake_case_json_values() {
        assert_eq!(
            serde_json::to_string(&Harness::Opencode).unwrap(),
            "\"opencode\""
        );
        assert_eq!(
            serde_json::to_string(&Harness::ClaudeCode).unwrap(),
            "\"claude_code\""
        );
        assert_eq!(
            serde_json::to_string(&Harness::Antigravity).unwrap(),
            "\"antigravity\""
        );
        assert_eq!(
            serde_json::from_str::<Harness>("\"codex\"").unwrap(),
            Harness::Codex
        );
        assert_eq!(
            serde_json::from_str::<Harness>("\"opencode\"").unwrap(),
            Harness::Opencode
        );
        assert_eq!(
            serde_json::from_str::<Harness>("\"claude_code\"").unwrap(),
            Harness::ClaudeCode
        );
        assert_eq!(
            serde_json::from_str::<Harness>("\"antigravity\"").unwrap(),
            Harness::Antigravity
        );
    }

    #[test]
    fn workspace_json_without_harness_defaults_to_codex() {
        let workspace = serde_json::from_value::<Workspace>(serde_json::json!({
            "id": Uuid::nil(),
            "name": "Demo",
            "host_path": "/projects/demo",
            "profile": "workspace",
            "resources": {"cpus": 2.0, "memory_bytes": 1, "pids": 1, "soft_disk_bytes": 1},
            "state": "creating"
        }))
        .unwrap();
        assert_eq!(workspace.harness, Harness::Codex);
    }

    #[test]
    fn workspace_view_json_without_harness_defaults_to_codex() {
        let view = serde_json::from_value::<WorkspaceView>(serde_json::json!({
            "id": Uuid::nil(),
            "name": "Demo",
            "host_path": "/projects/demo",
            "profile": "workspace",
            "resources": {"cpus": 2.0, "memory_bytes": 1, "pids": 1, "soft_disk_bytes": 1},
            "state": "creating"
        }))
        .unwrap();
        assert_eq!(view.harness, Harness::Codex);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    Creating,
    Starting,
    Running,
    Stopping,
    Stopped,
    Resetting,
    Deleting,
    Deleted,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub cpus: f64,
    pub memory_bytes: u64,
    pub pids: u32,
    pub soft_disk_bytes: u64,
}

impl ResourceLimits {
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn new(
        cpus: f64,
        memory_bytes: u64,
        pids: u32,
        soft_disk_bytes: u64,
    ) -> Result<Self, WorkspaceError> {
        if !cpus.is_finite()
            || cpus <= 0.0
            || memory_bytes == 0
            || pids == 0
            || soft_disk_bytes == 0
        {
            return Err(WorkspaceError::InvalidResourceLimits);
        }
        Ok(Self {
            cpus,
            memory_bytes,
            pids,
            soft_disk_bytes,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMetadata {
    pub container_id: String,
    pub container_name: String,
    pub volume_name: String,
    pub upstream_port: u16,
    pub image_ref: String,
}

#[derive(Clone, PartialEq)]
pub struct RuntimeSecret {
    pub workspace_id: Uuid,
    pub webtop_password: String,
}

impl std::fmt::Debug for RuntimeSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeSecret")
            .field("workspace_id", &self.workspace_id)
            .field("webtop_secret", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub host_path: PathBuf,
    pub profile: PermissionProfile,
    #[serde(default)]
    pub harness: Harness,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub apps: Vec<String>,
    pub resources: ResourceLimits,
    pub state: LifecycleState,
    #[serde(default)]
    pub runtime: Option<RuntimeMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineRunStatus {
    Ok,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Routine {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub instruction: String,
    pub interval_minutes: u32,
    pub enabled: bool,
    #[serde(default)]
    pub last_run_unix_ms: Option<u64>,
    #[serde(default)]
    pub last_status: Option<RoutineRunStatus>,
    #[serde(default)]
    pub last_skip_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub instruction: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateSettings {
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub notifications: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateRoutine {
    pub name: String,
    pub instruction: String,
    pub interval_minutes: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateSkill {
    pub name: String,
    pub instruction: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Template {
    pub id: Uuid,
    pub name: String,
    pub settings: TemplateSettings,
    #[serde(default)]
    pub routines: Vec<TemplateRoutine>,
    #[serde(default)]
    pub skills: Vec<TemplateSkill>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BotSettings {
    pub workspace_id: Uuid,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub avatar_color: Option<String>,
    pub notifications: bool,
}

/// Public representation of a workspace. Runtime/container details are kept
/// out of IPC responses deliberately.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceView {
    pub id: Uuid,
    pub name: String,
    pub host_path: PathBuf,
    pub profile: PermissionProfile,
    #[serde(default)]
    pub harness: Harness,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub apps: Vec<String>,
    pub resources: ResourceLimits,
    pub state: LifecycleState,
}

impl From<&Workspace> for WorkspaceView {
    fn from(workspace: &Workspace) -> Self {
        Self {
            id: workspace.id,
            name: workspace.name.clone(),
            host_path: workspace.host_path.clone(),
            profile: workspace.profile,
            harness: workspace.harness,
            model: workspace.model.clone(),
            reasoning_effort: workspace.reasoning_effort.clone(),
            apps: workspace.apps.clone(),
            resources: workspace.resources.clone(),
            state: workspace.state,
        }
    }
}

impl From<Workspace> for WorkspaceView {
    fn from(workspace: Workspace) -> Self {
        Self::from(&workspace)
    }
}

impl Workspace {
    pub fn new(
        name: String,
        host_path: PathBuf,
        profile: PermissionProfile,
        resources: ResourceLimits,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            host_path,
            profile,
            harness: Harness::default(),
            model: None,
            reasoning_effort: None,
            apps: Vec::new(),
            resources,
            state: LifecycleState::Creating,
            runtime: None,
        }
    }
    pub fn transition(&mut self, next: LifecycleState) -> Result<(), WorkspaceError> {
        let allowed = matches!(
            (self.state, next),
            (
                LifecycleState::Creating,
                LifecycleState::Starting | LifecycleState::Failed
            ) | (
                LifecycleState::Starting,
                LifecycleState::Running | LifecycleState::Stopping | LifecycleState::Failed
            ) | (
                LifecycleState::Running,
                LifecycleState::Stopping | LifecycleState::Failed
            ) | (
                LifecycleState::Stopping,
                LifecycleState::Stopped | LifecycleState::Failed
            ) | (
                LifecycleState::Stopped,
                LifecycleState::Starting | LifecycleState::Resetting | LifecycleState::Deleting
            ) | (
                LifecycleState::Resetting,
                LifecycleState::Starting | LifecycleState::Failed
            ) | (
                LifecycleState::Deleting,
                LifecycleState::Deleted | LifecycleState::Failed
            ) | (
                LifecycleState::Failed,
                LifecycleState::Starting
                    | LifecycleState::Resetting
                    | LifecycleState::Deleting
                    | LifecycleState::Stopping
            )
        );
        if !allowed {
            return Err(WorkspaceError::InvalidTransition {
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum WorkspaceError {
    #[error("invalid transition from {from:?} to {to:?}")]
    InvalidTransition {
        from: LifecycleState,
        to: LifecycleState,
    },
    #[error("resource limits must all be greater than zero")]
    InvalidResourceLimits,
}
