use orbit_domain::{
    AgentApprovalKind, AgentEventKind, AgentPlanStep, AgentPlanStepState, AgentSessionState,
    ApprovalDecision,
};
use serde_json::{Value, json};

pub const MAX_CODEX_FRAME_BYTES: usize = 1024 * 1024;

pub fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"id": id, "method": method, "params": params})
}

pub fn notification(method: &str, params: Value) -> Value {
    json!({"method": method, "params": params})
}

pub fn approval_result(decision: ApprovalDecision) -> Value {
    let decision = match decision {
        ApprovalDecision::Accept => "accept",
        ApprovalDecision::AcceptForSession => "acceptForSession",
        ApprovalDecision::Decline => "decline",
        ApprovalDecision::Cancel => "cancel",
    };
    json!({"decision": decision})
}

pub fn thread_id(result: &Value) -> Option<String> {
    result.pointer("/thread/id")?.as_str().map(str::to_owned)
}

pub fn turn_id(result: &Value) -> Option<String> {
    result.pointer("/turn/id")?.as_str().map(str::to_owned)
}

pub fn approval_event(method: &str, params: &Value, request_id: String) -> Option<AgentEventKind> {
    let (kind, default_title) = match method {
        "item/commandExecution/requestApproval" => (AgentApprovalKind::Command, "Run command"),
        "item/fileChange/requestApproval" => (AgentApprovalKind::FileChange, "Change files"),
        _ => return None,
    };
    let title = params
        .get("command")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(default_title)
        .chars()
        .take(300)
        .collect();
    let detail = params
        .get("reason")
        .and_then(Value::as_str)
        .map(|value| value.chars().take(4_000).collect());
    Some(AgentEventKind::ApprovalRequested {
        request_id,
        kind,
        title,
        detail,
    })
}

pub fn normalized_notification(method: &str, params: &Value) -> Option<AgentEventKind> {
    match method {
        "turn/started" => Some(AgentEventKind::StateChanged {
            state: AgentSessionState::Running,
        }),
        "turn/completed" => {
            let status = params.pointer("/turn/status");
            let state = if status
                .and_then(Value::as_str)
                .is_some_and(|status| status.eq_ignore_ascii_case("interrupted"))
            {
                AgentSessionState::Interrupted
            } else if status.is_some_and(|status| {
                status
                    .as_object()
                    .is_some_and(|status| status.contains_key("failed"))
                    || status
                        .as_str()
                        .is_some_and(|status| status.eq_ignore_ascii_case("failed"))
            }) {
                AgentSessionState::Failed
            } else {
                AgentSessionState::Completed
            };
            Some(AgentEventKind::StateChanged { state })
        }
        "item/agentMessage/delta" => {
            params
                .get("delta")
                .and_then(Value::as_str)
                .map(|text| AgentEventKind::AssistantDelta {
                    text: text.to_owned(),
                })
        }
        "turn/plan/updated" => {
            let steps = params
                .get("plan")?
                .as_array()?
                .iter()
                .filter_map(|step| {
                    let text = step.get("step")?.as_str()?.to_owned();
                    let state = match step.get("status")?.as_str()? {
                        "inProgress" | "in_progress" => AgentPlanStepState::InProgress,
                        "completed" => AgentPlanStepState::Completed,
                        _ => AgentPlanStepState::Pending,
                    };
                    Some(AgentPlanStep { text, state })
                })
                .collect();
            Some(AgentEventKind::PlanUpdated {
                explanation: params
                    .get("explanation")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                steps,
            })
        }
        "item/started" => {
            let item = params.get("item")?;
            let item_id = item.get("id")?.as_str()?.to_owned();
            let item_type = item.get("type").and_then(Value::as_str).unwrap_or("tool");
            if matches!(item_type, "agentMessage" | "reasoning" | "plan") {
                return None;
            }
            Some(AgentEventKind::ToolStarted {
                item_id,
                title: tool_title(item_type).to_owned(),
                detail: tool_detail(item),
            })
        }
        "item/completed" => {
            let item = params.get("item")?;
            let item_id = item.get("id")?.as_str()?.to_owned();
            let item_type = item.get("type").and_then(Value::as_str).unwrap_or("tool");
            if matches!(item_type, "agentMessage" | "reasoning" | "plan") {
                return None;
            }
            let success = !item
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| matches!(status, "failed" | "declined"));
            Some(AgentEventKind::ToolCompleted {
                item_id,
                success,
                detail: tool_detail(item),
            })
        }
        "error" => Some(AgentEventKind::Error {
            code: params
                .pointer("/error/codexErrorInfo")
                .and_then(Value::as_str)
                .unwrap_or("codex_error")
                .to_owned(),
            message: params
                .pointer("/error/message")
                .or_else(|| params.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("Codex reported an error")
                .chars()
                .take(4_000)
                .collect(),
            retryable: params
                .get("willRetry")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        _ => None,
    }
}

fn tool_title(item_type: &str) -> &str {
    match item_type {
        "commandExecution" => "Command",
        "fileChange" => "File change",
        "mcpToolCall" => "MCP tool",
        "webSearch" => "Web search",
        _ => "Tool",
    }
}

fn tool_detail(item: &Value) -> Option<String> {
    item.get("command")
        .and_then(Value::as_str)
        .or_else(|| item.get("name").and_then(Value::as_str))
        .or_else(|| item.get("path").and_then(Value::as_str))
        .map(|value| value.chars().take(4_000).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_streaming_plan_tool_and_approval_messages() {
        assert!(matches!(
            normalized_notification(
                "item/agentMessage/delta",
                &json!({"delta": "hello"})
            ),
            Some(AgentEventKind::AssistantDelta { text }) if text == "hello"
        ));
        assert!(matches!(
            normalized_notification(
                "turn/plan/updated",
                &json!({"plan": [{"step": "Build", "status": "inProgress"}]})
            ),
            Some(AgentEventKind::PlanUpdated { steps, .. }) if steps[0].state == AgentPlanStepState::InProgress
        ));
        assert!(matches!(
            approval_event(
                "item/commandExecution/requestApproval",
                &json!({"command": "apt install git", "reason": "needs package"}),
                "7".into()
            ),
            Some(AgentEventKind::ApprovalRequested { request_id, .. }) if request_id == "7"
        ));
    }
}
