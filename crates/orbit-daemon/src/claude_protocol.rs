use orbit_domain::AgentEventKind;
use serde_json::Value;

pub fn map_claude_line(line: &str) -> Vec<AgentEventKind> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    map_claude_event(&value)
}

pub fn map_claude_event(value: &Value) -> Vec<AgentEventKind> {
    match value.get("type").and_then(Value::as_str) {
        Some("assistant") => map_assistant(value),
        Some("user") => map_user(value),
        Some("result") => map_result(value),
        _ => Vec::new(),
    }
}

fn map_assistant(value: &Value) -> Vec<AgentEventKind> {
    value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| match block.get("type").and_then(Value::as_str) {
            Some("text") => block
                .get("text")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(|text| AgentEventKind::AssistantDelta { text: text.into() }),
            Some("tool_use") => {
                let item_id = block.get("id").and_then(Value::as_str)?;
                let title = block.get("name").and_then(Value::as_str)?;
                let input = block.get("input");
                let detail = input
                    .and_then(|input| input.get("command"))
                    .and_then(Value::as_str)
                    .or_else(|| {
                        input
                            .and_then(|input| input.get("description"))
                            .and_then(Value::as_str)
                    })
                    .map(str::to_owned)
                    .or_else(|| {
                        input
                            .filter(|input| {
                                !input.is_null()
                                    && !input.as_object().is_some_and(|object| object.is_empty())
                            })
                            .and_then(|input| serde_json::to_string(input).ok())
                    });
                Some(AgentEventKind::ToolStarted {
                    item_id: item_id.into(),
                    title: title.into(),
                    detail,
                })
            }
            _ => None,
        })
        .collect()
}

fn map_user(value: &Value) -> Vec<AgentEventKind> {
    value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|block| {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                return None;
            }
            Some(AgentEventKind::ToolCompleted {
                item_id: block.get("tool_use_id").and_then(Value::as_str)?.into(),
                success: !block
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                detail: stringify_content(block.get("content")),
            })
        })
        .collect()
}

fn map_result(value: &Value) -> Vec<AgentEventKind> {
    let mut events = Vec::new();
    if let Some(usage) = value.get("usage") {
        events.push(AgentEventKind::UsageUpdated {
            input_tokens: number(usage, "input_tokens"),
            cached_input_tokens: number(usage, "cache_read_input_tokens"),
            output_tokens: number(usage, "output_tokens"),
        });
    }
    let subtype = value.get("subtype").and_then(Value::as_str);
    if subtype != Some("success") || value.get("is_error").and_then(Value::as_bool) == Some(true) {
        let subtype = subtype.unwrap_or("claude");
        events.push(AgentEventKind::Error {
            code: subtype.into(),
            message: value
                .get("result")
                .and_then(Value::as_str)
                .unwrap_or(subtype)
                .into(),
            retryable: false,
        });
    }
    events
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn stringify_content(value: Option<&Value>) -> Option<String> {
    let value = value?;
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| {
                (block.get("type").and_then(Value::as_str) == Some("text"))
                    .then(|| block.get("text").and_then(Value::as_str))
                    .flatten()
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => serde_json::to_string(value).ok()?,
    };
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use orbit_domain::AgentEventKind;

    use super::map_claude_line;

    #[test]
    fn maps_assistant_text() {
        assert_eq!(
            map_claude_line(
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello"}]},"session_id":"s1"}"#
            ),
            vec![AgentEventKind::AssistantDelta {
                text: "Hello".into()
            }]
        );
    }

    #[test]
    fn maps_assistant_tool_use() {
        assert_eq!(
            map_claude_line(
                r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ls","description":"list"}}]}}"#
            ),
            vec![AgentEventKind::ToolStarted {
                item_id: "toolu_1".into(),
                title: "Bash".into(),
                detail: Some("ls".into())
            }]
        );
    }

    #[test]
    fn maps_successful_tool_result() {
        assert_eq!(
            map_claude_line(
                r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"file1\nfile2","is_error":false}]}}"#
            ),
            vec![AgentEventKind::ToolCompleted {
                item_id: "toolu_1".into(),
                success: true,
                detail: Some("file1\nfile2".into())
            }]
        );
    }

    #[test]
    fn maps_error_tool_result() {
        let events = map_claude_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"failed","is_error":true}]}}"#,
        );
        assert_eq!(
            events,
            vec![AgentEventKind::ToolCompleted {
                item_id: "toolu_1".into(),
                success: false,
                detail: Some("failed".into())
            }]
        );
    }

    #[test]
    fn maps_success_result_usage_without_error_or_delta() {
        assert_eq!(
            map_claude_line(
                r#"{"type":"result","subtype":"success","is_error":false,"result":"done","session_id":"s1","usage":{"input_tokens":100,"output_tokens":20,"cache_read_input_tokens":10}}"#
            ),
            vec![AgentEventKind::UsageUpdated {
                input_tokens: 100,
                cached_input_tokens: 10,
                output_tokens: 20
            }]
        );
    }

    #[test]
    fn maps_failed_result_to_non_retryable_error() {
        assert!(
            matches!(map_claude_line(r#"{"type":"result","subtype":"error_during_execution","is_error":true}"#).as_slice(), [AgentEventKind::Error { code, retryable: false, .. }] if code == "error_during_execution")
        );
    }

    #[test]
    fn ignores_system_unknown_and_malformed_lines() {
        assert!(
            map_claude_line(r#"{"type":"system","subtype":"init","session_id":"s1","tools":[]}"#)
                .is_empty()
        );
        assert!(map_claude_line(r#"{"type":"unknown"}"#).is_empty());
        assert!(map_claude_line("not json").is_empty());
    }
}
