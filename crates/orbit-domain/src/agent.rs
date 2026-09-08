use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionState {
    Starting,
    NeedsAuthentication,
    Idle,
    Running,
    WaitingForApproval,
    Interrupted,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSessionView {
    pub workspace_id: Uuid,
    pub thread_id: Option<String>,
    pub state: AgentSessionState,
    pub next_cursor: u64,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RedactedUrl(pub String);

impl fmt::Debug for RedactedUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RedactedSecret(pub String);

impl fmt::Debug for RedactedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecision {
    Accept,
    AcceptForSession,
    Decline,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentApprovalKind {
    Command,
    FileChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPlanStepState {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPlanStep {
    pub text: String,
    pub state: AgentPlanStepState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum AgentEventKind {
    UserPrompt {
        text: String,
    },
    StateChanged {
        state: AgentSessionState,
    },
    AuthenticationRequired {
        url: RedactedUrl,
        user_code: Option<RedactedSecret>,
    },
    AssistantDelta {
        text: String,
    },
    PlanUpdated {
        explanation: Option<String>,
        steps: Vec<AgentPlanStep>,
    },
    ToolStarted {
        item_id: String,
        title: String,
        detail: Option<String>,
    },
    ToolCompleted {
        item_id: String,
        success: bool,
        detail: Option<String>,
    },
    ApprovalRequested {
        request_id: String,
        kind: AgentApprovalKind,
        title: String,
        detail: Option<String>,
    },
    ApprovalResolved {
        request_id: String,
        decision: ApprovalDecision,
    },
    UsageUpdated {
        input_tokens: u64,
        cached_input_tokens: u64,
        output_tokens: u64,
    },
    Error {
        code: String,
        message: String,
        retryable: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEvent {
    pub cursor: u64,
    pub kind: AgentEventKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEventBatch {
    pub workspace_id: Uuid,
    pub after: u64,
    pub next_cursor: u64,
    pub events: Vec<AgentEvent>,
    #[serde(default)]
    pub has_more: bool,
}
