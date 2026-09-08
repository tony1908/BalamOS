use orbit_domain::AgentEventKind;
use serde_json::Value;

pub fn map_antigravity_line(line: &str) -> Vec<AgentEventKind> {
    // Antigravity emits one complete JSON value per stream line.
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    map_antigravity_event(&value)
}

pub fn map_antigravity_event(value: &Value) -> Vec<AgentEventKind> {
    match value.get("event").and_then(Value::as_str) {
        Some("init") => Vec::new(),
        Some("step_update") => map_step_update(value),
        Some("result") => map_result(value),
        _ => Vec::new(),
    }
}

fn map_step_update(value: &Value) -> Vec<AgentEventKind> {
    let Some(step) = value.get("step_update") else {
        return Vec::new();
    };
    if step.get("step_type").and_then(Value::as_str) != Some("agent_response") {
        return Vec::new();
    }
    step.get("text_delta")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .map(|text| vec![AgentEventKind::AssistantDelta { text: text.into() }])
        .unwrap_or_default()
}

fn map_result(value: &Value) -> Vec<AgentEventKind> {
    let Some(result) = value.get("result") else {
        return Vec::new();
    };
    let mut events = Vec::new();
    if let Some(usage) = result.get("usage").filter(|value| value.is_object()) {
        events.push(AgentEventKind::UsageUpdated {
            input_tokens: number(usage, "input_tokens"),
            cached_input_tokens: number(usage, "cache_read_tokens"),
            output_tokens: number(usage, "output_tokens"),
        });
    }
    if let Some(status) = result.get("status").and_then(Value::as_str)
        && status != "SUCCESS"
    {
        events.push(AgentEventKind::Error {
            code: status.into(),
            message: result
                .get("response")
                .and_then(Value::as_str)
                .unwrap_or(status)
                .into(),
            retryable: false,
        });
    }
    events
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use orbit_domain::AgentEventKind;

    use super::{map_antigravity_event, map_antigravity_line};

    #[test]
    fn ignores_init_line() {
        assert!(map_antigravity_line(
            r#"{"event":"init","conversation_id":"c1","init":{"cwd":"/tmp","tools":[],"permission_mode":"default"}}"#
        )
        .is_empty());
    }

    #[test]
    fn ignores_user_input_step_update() {
        assert!(map_antigravity_line(
            r#"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":0,"state":"DONE","step_type":"user_input"}}"#
        )
        .is_empty());
    }

    #[test]
    fn maps_agent_response_delta() {
        assert_eq!(
            map_antigravity_line(
                r#"{"event":"step_update","step_update":{"conversation_id":"c1","step_index":1,"state":"DONE","step_type":"agent_response","text_delta":"PONG\n","duration_seconds":1.8,"usage":{"input_tokens":15964,"output_tokens":448,"thinking_tokens":446,"cache_read_tokens":0,"total_tokens":16412}}}"#
            ),
            vec![AgentEventKind::AssistantDelta {
                text: "PONG\n".into()
            }]
        );
    }

    #[test]
    fn maps_success_result_usage_without_duplicate_delta() {
        assert_eq!(
            map_antigravity_line(
                r#"{"event":"result","result":{"conversation_id":"c1","status":"SUCCESS","response":"PONG\n","duration_seconds":1.9,"num_turns":1,"usage":{"input_tokens":15964,"output_tokens":448,"thinking_tokens":446,"cache_read_tokens":0,"total_tokens":16412}}}"#
            ),
            vec![AgentEventKind::UsageUpdated {
                input_tokens: 15964,
                cached_input_tokens: 0,
                output_tokens: 448
            }]
        );
    }

    #[test]
    fn maps_failed_result_to_non_retryable_error() {
        assert_eq!(
            map_antigravity_line(
                r#"{"event":"result","result":{"status":"ERROR","response":"boom"}}"#
            ),
            vec![AgentEventKind::Error {
                code: "ERROR".into(),
                message: "boom".into(),
                retryable: false
            }]
        );
    }

    #[test]
    fn ignores_unknown_and_malformed_lines() {
        assert!(map_antigravity_line(r#"{"event":"whatever"}"#).is_empty());
        assert!(map_antigravity_line("not json").is_empty());
        assert!(map_antigravity_event(&serde_json::json!({})).is_empty());
    }
}
