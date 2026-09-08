use orbit_client::{ClientError, DaemonClient};
use orbit_domain::{
    AgentEventBatch, AgentSessionView, AppOption, ApprovalDecision, BotSettings, GovernancePolicy,
    GovernancePreset, GovernanceRule, GovernanceView, Harness, LifecycleState, ModelOption,
    PermissionProfile, ResourceLimits, Routine, SecretAssignment, SecretMetadata, Skill, Template,
    TemplateRoutine, TemplateSettings, TemplateSkill, WorkspaceGovernancePolicy, WorkspaceView,
};
use orbit_protocol::{ApiError, Command, DesktopSession, ErrorCode, ResponseBody, RuntimeStatus};
use std::path::PathBuf;
use tauri::State;

pub struct ClientState(pub DaemonClient);

#[tauri::command]
pub async fn list_secrets(
    state: State<'_, ClientState>,
) -> Result<Vec<SecretMetadata>, CommandError> {
    state.0.list_secrets().await.map_err(CommandError::from)
}
#[tauri::command]
pub async fn create_secret(
    state: State<'_, ClientState>,
    name: String,
    env_name: String,
    value: String,
) -> Result<SecretMetadata, CommandError> {
    state
        .0
        .create_secret(name, env_name, value)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn replace_secret(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
    value: String,
) -> Result<(), CommandError> {
    state
        .0
        .replace_secret(id, value)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn delete_secret(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
) -> Result<uuid::Uuid, CommandError> {
    state.0.delete_secret(id).await.map_err(CommandError::from)
}
#[tauri::command]
pub async fn list_secret_assignments(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
) -> Result<Vec<SecretAssignment>, CommandError> {
    state
        .0
        .list_secret_assignments(workspace_id)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn assign_secret(
    state: State<'_, ClientState>,
    secret_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
) -> Result<SecretAssignment, CommandError> {
    state
        .0
        .assign_secret(secret_id, workspace_id)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn unassign_secret(
    state: State<'_, ClientState>,
    secret_id: uuid::Uuid,
    workspace_id: uuid::Uuid,
) -> Result<(), CommandError> {
    state
        .0
        .unassign_secret(secret_id, workspace_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn get_governance_policy(
    state: State<'_, ClientState>,
) -> Result<GovernanceView, CommandError> {
    state
        .0
        .governance_policy()
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn save_governance_policy(
    state: State<'_, ClientState>,
    policy: GovernancePolicy,
) -> Result<GovernanceView, CommandError> {
    state
        .0
        .save_governance_policy(policy)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn get_workspace_governance(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
) -> Result<GovernanceView, CommandError> {
    state
        .0
        .workspace_governance(workspace_id)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn save_workspace_governance(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
    policy: WorkspaceGovernancePolicy,
) -> Result<GovernanceView, CommandError> {
    state
        .0
        .save_workspace_governance(workspace_id, policy)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn list_governance_presets(
    state: State<'_, ClientState>,
) -> Result<Vec<GovernancePreset>, CommandError> {
    state
        .0
        .list_governance_presets()
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn create_governance_preset(
    state: State<'_, ClientState>,
    name: String,
    policy: GovernancePolicy,
) -> Result<GovernancePreset, CommandError> {
    state
        .0
        .create_governance_preset(name, policy)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn delete_governance_preset(
    state: State<'_, ClientState>,
    id: String,
) -> Result<String, CommandError> {
    state
        .0
        .delete_governance_preset(id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn list_governance_rules(
    state: State<'_, ClientState>,
) -> Result<Vec<GovernanceRule>, CommandError> {
    state
        .0
        .list_governance_rules()
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn create_governance_rule(
    state: State<'_, ClientState>,
    title: String,
    body: String,
) -> Result<GovernanceRule, CommandError> {
    state
        .0
        .create_governance_rule(title, body)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn update_governance_rule(
    state: State<'_, ClientState>,
    id: String,
    title: String,
    body: String,
) -> Result<GovernanceRule, CommandError> {
    state
        .0
        .update_governance_rule(id, title, body)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn delete_governance_rule(
    state: State<'_, ClientState>,
    id: String,
) -> Result<String, CommandError> {
    state
        .0
        .delete_governance_rule(id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn save_template(
    state: State<'_, ClientState>,
    name: String,
    settings: TemplateSettings,
    routines: Vec<TemplateRoutine>,
    skills: Vec<TemplateSkill>,
) -> Result<Template, CommandError> {
    state
        .0
        .save_template(name, settings, routines, skills)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn list_templates(state: State<'_, ClientState>) -> Result<Vec<Template>, CommandError> {
    state.0.list_templates().await.map_err(CommandError::from)
}
#[tauri::command]
pub async fn delete_template(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
) -> Result<uuid::Uuid, CommandError> {
    state
        .0
        .delete_template(id)
        .await
        .map_err(CommandError::from)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl From<ClientError> for CommandError {
    fn from(error: ClientError) -> Self {
        match error {
            ClientError::Api(ApiError {
                code,
                message,
                retryable,
            }) => Self {
                code: serde_json::to_value(code)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string(),
                message: if code == ErrorCode::Internal {
                    "daemon encountered an internal error".into()
                } else {
                    message
                },
                retryable,
            },
            ClientError::SocketName | ClientError::Io(_) | ClientError::Timeout => Self {
                code: "daemon_unavailable".into(),
                message: "local daemon is unavailable".into(),
                retryable: true,
            },
            ClientError::Json(_)
            | ClientError::EmptyResponse
            | ClientError::FrameTooLarge
            | ClientError::CorrelationMismatch { .. } => Self {
                code: "invalid_response".into(),
                message: "daemon returned an invalid response".into(),
                retryable: true,
            },
            ClientError::ProtocolMismatch { .. } => Self {
                code: "protocol_mismatch".into(),
                message: "daemon protocol version is incompatible".into(),
                retryable: false,
            },
        }
    }
}

fn response_workspaces(body: ResponseBody) -> Result<Vec<WorkspaceView>, CommandError> {
    match body {
        ResponseBody::Workspaces(workspaces) => Ok(workspaces),
        _ => Err(CommandError {
            code: "invalid_response".into(),
            message: "daemon returned an invalid response".into(),
            retryable: true,
        }),
    }
}

fn response_workspace(body: ResponseBody) -> Result<WorkspaceView, CommandError> {
    match body {
        ResponseBody::Workspace(workspace) => Ok(workspace),
        _ => Err(CommandError {
            code: "invalid_response".into(),
            message: "daemon returned an invalid response".into(),
            retryable: true,
        }),
    }
}

#[tauri::command]
pub async fn list_workspaces(
    state: State<'_, ClientState>,
) -> Result<Vec<WorkspaceView>, CommandError> {
    state
        .0
        .send(Command::ListWorkspaces)
        .await
        .map_err(CommandError::from)
        .and_then(response_workspaces)
}

#[tauri::command]
pub async fn create_workspace(
    state: State<'_, ClientState>,
    name: String,
    host_path: PathBuf,
    profile: PermissionProfile,
    harness: Harness,
    model: Option<String>,
    reasoning_effort: Option<String>,
) -> Result<WorkspaceView, CommandError> {
    let resources =
        ResourceLimits::new(2.0, 3_221_225_472, 1024, 20_000_000_000).map_err(|error| {
            CommandError {
                code: "invalid_request".into(),
                message: error.to_string(),
                retryable: false,
            }
        })?;
    state
        .0
        .send(Command::CreateWorkspace {
            name,
            host_path,
            profile,
            harness,
            model,
            reasoning_effort,
            resources,
        })
        .await
        .map_err(CommandError::from)
        .and_then(response_workspace)
}

#[tauri::command]
pub async fn list_models(
    state: State<'_, ClientState>,
    harness: Harness,
) -> Result<Vec<ModelOption>, CommandError> {
    state
        .0
        .list_models(harness)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn set_workspace_model(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
    model: Option<String>,
    reasoning_effort: Option<String>,
) -> Result<WorkspaceView, CommandError> {
    state
        .0
        .set_workspace_model(workspace_id, model, reasoning_effort)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn list_apps(state: State<'_, ClientState>) -> Result<Vec<AppOption>, CommandError> {
    state.0.list_apps().await.map_err(CommandError::from)
}

#[tauri::command]
pub async fn set_workspace_apps(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
    apps: Vec<String>,
) -> Result<WorkspaceView, CommandError> {
    state
        .0
        .set_workspace_apps(workspace_id, apps)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn create_routine(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
    name: String,
    instruction: String,
    interval_minutes: u32,
    enabled: bool,
) -> Result<Routine, CommandError> {
    state
        .0
        .create_routine(workspace_id, name, instruction, interval_minutes, enabled)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn list_routines(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
) -> Result<Vec<Routine>, CommandError> {
    state
        .0
        .list_routines(workspace_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn create_skill(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
    name: String,
    instruction: String,
    enabled: bool,
) -> Result<Skill, CommandError> {
    state
        .0
        .create_skill(workspace_id, name, instruction, enabled)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn list_skills(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
) -> Result<Vec<Skill>, CommandError> {
    state
        .0
        .list_skills(workspace_id)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn update_skill(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
    name: String,
    instruction: String,
    enabled: bool,
) -> Result<Skill, CommandError> {
    state
        .0
        .update_skill(id, name, instruction, enabled)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn delete_skill(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
) -> Result<uuid::Uuid, CommandError> {
    state.0.delete_skill(id).await.map_err(CommandError::from)
}

#[tauri::command]
pub async fn set_bot_settings(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
    display_name: Option<String>,
    label: Option<String>,
    description: Option<String>,
    avatar_color: Option<String>,
    notifications: bool,
) -> Result<BotSettings, CommandError> {
    state
        .0
        .set_bot_settings(
            workspace_id,
            display_name,
            label,
            description,
            avatar_color,
            notifications,
        )
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn list_bot_settings(
    state: State<'_, ClientState>,
) -> Result<Vec<BotSettings>, CommandError> {
    state
        .0
        .list_bot_settings()
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn update_routine(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
    name: String,
    instruction: String,
    interval_minutes: u32,
    enabled: bool,
) -> Result<Routine, CommandError> {
    state
        .0
        .update_routine(id, name, instruction, interval_minutes, enabled)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn delete_routine(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
) -> Result<uuid::Uuid, CommandError> {
    state.0.delete_routine(id).await.map_err(CommandError::from)
}

#[tauri::command]
pub async fn transition_workspace(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
    next: LifecycleState,
) -> Result<WorkspaceView, CommandError> {
    match next {
        LifecycleState::Starting | LifecycleState::Running => state
            .0
            .start_workspace(id)
            .await
            .map_err(CommandError::from),
        LifecycleState::Stopping | LifecycleState::Stopped => {
            state.0.stop_workspace(id).await.map_err(CommandError::from)
        }
        _ => Err(CommandError {
            code: "invalid_transition".into(),
            message: "unsupported lifecycle transition".into(),
            retryable: false,
        }),
    }
}

#[tauri::command]
pub async fn runtime_status(state: State<'_, ClientState>) -> Result<RuntimeStatus, CommandError> {
    state.0.runtime_status().await.map_err(CommandError::from)
}
#[tauri::command]
pub async fn reconcile_runtime(
    state: State<'_, ClientState>,
) -> Result<RuntimeStatus, CommandError> {
    state
        .0
        .reconcile_runtime()
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn start_workspace(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
) -> Result<WorkspaceView, CommandError> {
    state
        .0
        .start_workspace(id)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn stop_workspace(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
) -> Result<WorkspaceView, CommandError> {
    state.0.stop_workspace(id).await.map_err(CommandError::from)
}
#[tauri::command]
pub async fn reset_workspace(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
) -> Result<WorkspaceView, CommandError> {
    state
        .0
        .reset_workspace(id)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn delete_workspace(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
) -> Result<uuid::Uuid, CommandError> {
    state
        .0
        .delete_workspace(id)
        .await
        .map_err(CommandError::from)
}
#[tauri::command]
pub async fn create_desktop_session(
    state: State<'_, ClientState>,
    id: uuid::Uuid,
) -> Result<DesktopSession, CommandError> {
    state
        .0
        .create_desktop_session(id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn start_agent_session(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
) -> Result<AgentSessionView, CommandError> {
    state
        .0
        .start_agent_session(workspace_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn send_agent_prompt(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
    prompt: String,
) -> Result<AgentSessionView, CommandError> {
    state
        .0
        .send_agent_prompt(workspace_id, prompt)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn poll_agent_events(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
    after: u64,
) -> Result<AgentEventBatch, CommandError> {
    state
        .0
        .poll_agent_events(workspace_id, after)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn resolve_agent_approval(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
    request_id: String,
    decision: ApprovalDecision,
) -> Result<AgentSessionView, CommandError> {
    state
        .0
        .resolve_agent_approval(workspace_id, request_id, decision)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn interrupt_agent(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
) -> Result<AgentSessionView, CommandError> {
    state
        .0
        .interrupt_agent(workspace_id)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn stop_agent_session(
    state: State<'_, ClientState>,
    workspace_id: uuid::Uuid,
) -> Result<uuid::Uuid, CommandError> {
    state
        .0
        .stop_agent_session(workspace_id)
        .await
        .map_err(CommandError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbit_client::ClientError;
    use orbit_protocol::{ApiError, ErrorCode};
    use std::io;

    #[test]
    fn rejects_non_workspace_response() {
        assert!(response_workspaces(ResponseBody::Pong).is_err());
    }

    #[test]
    fn maps_api_errors_and_sanitizes_internal_detail() {
        let error = CommandError::from(ClientError::Api(ApiError {
            code: ErrorCode::NotFound,
            message: "workspace missing".into(),
            retryable: false,
        }));
        assert_eq!(
            error,
            CommandError {
                code: "not_found".into(),
                message: "workspace missing".into(),
                retryable: false
            }
        );
        let error = CommandError::from(ClientError::Api(ApiError {
            code: ErrorCode::Internal,
            message: "injected DB path /secret".into(),
            retryable: true,
        }));
        assert_eq!(error.message, "daemon encountered an internal error");
        assert!(!error.message.contains("/secret"));
    }

    #[test]
    fn maps_io_errors_without_detail() {
        let error = CommandError::from(ClientError::Io(io::Error::other("/secret")));
        assert_eq!(error.code, "daemon_unavailable");
        assert_eq!(error.message, "local daemon is unavailable");
        assert!(!format!("{error:?}").contains("/secret"));
    }
}
