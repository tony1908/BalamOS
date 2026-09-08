pub use orbit_domain::Harness;
use orbit_domain::{
    AgentEventBatch, AgentSessionView, AppOption, ApprovalDecision, BotSettings, GovernancePolicy,
    GovernancePreset, GovernanceRule, GovernanceView, ModelOption, PermissionProfile,
    ResourceLimits, Routine, SecretAssignment, SecretInput, SecretMetadata, Skill, Template,
    TemplateRoutine, TemplateSettings, TemplateSkill, WorkspaceView,
};
use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 5;
pub const MAX_AGENT_PROMPT_BYTES: usize = 64 * 1024;
pub const MAX_AGENT_APPROVAL_ID_BYTES: usize = 256;
pub const MAX_AGENT_EVENT_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_AGENT_EVENT_BATCH: usize = 256;
pub const MAX_ROUTINE_NAME_BYTES: usize = 200;
pub const MAX_ROUTINE_INSTRUCTION_BYTES: usize = 8000;
pub const MAX_SKILL_NAME_BYTES: usize = 200;
pub const MAX_SKILL_INSTRUCTION_BYTES: usize = 8000;
pub const MAX_BOT_NAME_BYTES: usize = 200;
pub const MAX_BOT_LABEL_BYTES: usize = 200;
pub const MAX_BOT_DESCRIPTION_BYTES: usize = 8000;
pub const MAX_TEMPLATE_NAME_BYTES: usize = 200;
pub const MAX_IPC_FRAME_BYTES: usize = 64 * 1024;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub protocol: u16,
    pub id: Uuid,
    pub token: String,
    pub command: Command,
}

impl fmt::Debug for Request {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Request")
            .field("protocol", &self.protocol)
            .field("id", &self.id)
            .field("token", &"[REDACTED]")
            .field("command", &self.command)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Ping,
    RuntimeStatus,
    ReconcileRuntime,
    ListWorkspaces,
    GetGovernancePolicy,
    SaveGovernancePolicy {
        policy: GovernancePolicy,
    },
    GetWorkspaceGovernance {
        workspace_id: Uuid,
    },
    SaveWorkspaceGovernance {
        workspace_id: Uuid,
        policy: orbit_domain::WorkspaceGovernancePolicy,
    },
    ListGovernancePresets,
    CreateGovernancePreset {
        name: String,
        policy: GovernancePolicy,
    },
    DeleteGovernancePreset {
        id: String,
    },
    ListGovernanceRules,
    CreateGovernanceRule {
        title: String,
        body: String,
    },
    UpdateGovernanceRule {
        id: String,
        title: String,
        body: String,
    },
    DeleteGovernanceRule {
        id: String,
    },
    ListSecrets,
    CreateSecret {
        name: String,
        env_name: String,
        value: SecretInput,
    },
    ReplaceSecret {
        id: Uuid,
        value: SecretInput,
    },
    DeleteSecret {
        id: Uuid,
    },
    AssignSecret {
        secret_id: Uuid,
        workspace_id: Uuid,
    },
    ListSecretAssignments {
        workspace_id: Uuid,
    },
    UnassignSecret {
        secret_id: Uuid,
        workspace_id: Uuid,
    },
    CreateWorkspace {
        name: String,
        host_path: PathBuf,
        profile: PermissionProfile,
        #[serde(default)]
        harness: Harness,
        #[serde(default)]
        model: Option<String>,
        #[serde(default)]
        reasoning_effort: Option<String>,
        resources: ResourceLimits,
    },
    ListModels {
        harness: Harness,
    },
    SetWorkspaceModel {
        workspace_id: Uuid,
        model: Option<String>,
        reasoning_effort: Option<String>,
    },
    ListApps,
    SetWorkspaceApps {
        workspace_id: Uuid,
        apps: Vec<String>,
    },
    StartWorkspace {
        id: Uuid,
    },
    StopWorkspace {
        id: Uuid,
    },
    ResetWorkspace {
        id: Uuid,
    },
    DeleteWorkspace {
        id: Uuid,
    },
    CreateRoutine {
        workspace_id: Uuid,
        name: String,
        instruction: String,
        interval_minutes: u32,
        enabled: bool,
    },
    ListRoutines {
        workspace_id: Uuid,
    },
    SetBotSettings {
        workspace_id: Uuid,
        display_name: Option<String>,
        label: Option<String>,
        description: Option<String>,
        avatar_color: Option<String>,
        notifications: bool,
    },
    ListBotSettings,
    UpdateRoutine {
        id: Uuid,
        name: String,
        instruction: String,
        interval_minutes: u32,
        enabled: bool,
    },
    DeleteRoutine {
        id: Uuid,
    },
    CreateSkill {
        workspace_id: Uuid,
        name: String,
        instruction: String,
        enabled: bool,
    },
    ListSkills {
        workspace_id: Uuid,
    },
    UpdateSkill {
        id: Uuid,
        name: String,
        instruction: String,
        enabled: bool,
    },
    DeleteSkill {
        id: Uuid,
    },
    SaveTemplate {
        name: String,
        settings: TemplateSettings,
        routines: Vec<TemplateRoutine>,
        skills: Vec<TemplateSkill>,
    },
    ListTemplates,
    DeleteTemplate {
        id: Uuid,
    },
    CreateDesktopSession {
        id: Uuid,
    },
    StartAgentSession {
        workspace_id: Uuid,
    },
    SendAgentPrompt {
        workspace_id: Uuid,
        prompt: String,
    },
    PollAgentEvents {
        workspace_id: Uuid,
        after: u64,
    },
    ResolveAgentApproval {
        workspace_id: Uuid,
        request_id: String,
        decision: ApprovalDecision,
    },
    InterruptAgent {
        workspace_id: Uuid,
    },
    StopAgentSession {
        workspace_id: Uuid,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub protocol: u16,
    pub id: Uuid,
    pub body: ResponseBody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ResponseBody {
    Pong,
    RuntimeStatus(RuntimeStatus),
    Workspace(WorkspaceView),
    Workspaces(Vec<WorkspaceView>),
    Models(Vec<ModelOption>),
    Apps(Vec<AppOption>),
    Routine(Routine),
    Routines(Vec<Routine>),
    BotSettings(BotSettings),
    BotSettingsList(Vec<BotSettings>),
    RoutineDeleted { id: Uuid },
    Skill(Skill),
    Skills(Vec<Skill>),
    SkillDeleted { id: Uuid },
    Template(Template),
    Templates(Vec<Template>),
    TemplateDeleted { id: Uuid },
    DesktopSession(DesktopSession),
    AgentSession(AgentSessionView),
    AgentEvents(AgentEventBatch),
    AgentStopped { workspace_id: Uuid },
    Governance(GovernanceView),
    GovernancePresets(Vec<GovernancePreset>),
    GovernancePreset(GovernancePreset),
    GovernancePresetDeleted { id: String },
    GovernanceRules(Vec<GovernanceRule>),
    GovernanceRule(GovernanceRule),
    GovernanceRuleDeleted { id: String },
    Secrets(Vec<SecretMetadata>),
    Secret(SecretMetadata),
    SecretDeleted { id: Uuid },
    SecretAssignment(SecretAssignment),
    SecretAssignments(Vec<SecretAssignment>),
    Deleted { id: Uuid },
    Error(ApiError),
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopSession {
    pub url: String,
    pub expires_at_unix_ms: u64,
}

impl fmt::Debug for DesktopSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DesktopSession")
            .field("url", &"[REDACTED]")
            .field("expires_at_unix_ms", &self.expires_at_unix_ms)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub available: bool,
    pub mutations_ready: bool,
    pub version: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    ProtocolMismatch,
    InvalidRequest,
    InvalidTransition,
    NotFound,
    Internal,
    RuntimeUnavailable,
    GovernanceBlocked,
}
