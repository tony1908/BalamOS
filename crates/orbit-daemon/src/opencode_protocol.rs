use orbit_domain::AgentEventKind;
use serde_json::Value;

pub fn map_opencode_event(value: &Value) -> Vec<AgentEventKind> {
    let Some(event_type) = value.get("type").and_then(Value::as_str) else {
        return error_event(value).into_iter().collect();
    };

    if event_type == "error" {
        return error_event(value).into_iter().collect();
    }

    match event_type {
        "text" => value
            .get("part")
            .and_then(|part| part.get("text"))
            .and_then(Value::as_str)
            .map(|text| vec![AgentEventKind::AssistantDelta { text: text.into() }])
            .unwrap_or_default(),
        "tool_use" => map_tool_use(value),
        "step_finish" => map_step_finish(value),
        "step_start" => Vec::new(),
        _ => error_event(value).into_iter().collect(),
    }
}

pub fn map_opencode_line(line: &str) -> Vec<AgentEventKind> {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    map_opencode_event(&value)
}

fn map_tool_use(value: &Value) -> Vec<AgentEventKind> {
    let Some(part) = value.get("part") else {
        return Vec::new();
    };
    let Some(call_id) = part.get("callID").and_then(Value::as_str) else {
        return Vec::new();
    };
    let tool = part.get("tool").and_then(Value::as_str).unwrap_or_default();
    let state = part.get("state");
    let title = state
        .and_then(|state| state.get("title"))
        .and_then(Value::as_str)
        .unwrap_or(tool);
    let detail = state
        .and_then(|state| state.get("input"))
        .and_then(|input| input.get("command"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut events = vec![AgentEventKind::ToolStarted {
        item_id: call_id.into(),
        title: title.into(),
        detail,
    }];
    if let Some(status) = state
        .and_then(|state| state.get("status"))
        .and_then(Value::as_str)
        && (status == "completed" || status == "error")
    {
        events.push(AgentEventKind::ToolCompleted {
            item_id: call_id.into(),
            success: status == "completed",
            detail: state
                .and_then(|state| state.get("output"))
                .and_then(Value::as_str)
                .map(str::to_owned),
        });
    }
    events
}

fn map_step_finish(value: &Value) -> Vec<AgentEventKind> {
    let Some(tokens) = value.get("part").and_then(|part| part.get("tokens")) else {
        return Vec::new();
    };
    vec![AgentEventKind::UsageUpdated {
        input_tokens: number(tokens, "input"),
        cached_input_tokens: tokens
            .get("cache")
            .map(|cache| number(cache, "read"))
            .unwrap_or(0),
        output_tokens: number(tokens, "output"),
    }]
}

fn number(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn error_event(value: &Value) -> Option<AgentEventKind> {
    let message = value
        .get("error")
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))?;
    Some(AgentEventKind::Error {
        code: "opencode".into(),
        message: message.into(),
        retryable: false,
    })
}

#[cfg(test)]
mod tests {
    use orbit_domain::AgentEventKind;

    use super::map_opencode_line;

    #[test]
    fn maps_text_line() {
        let line = r#"{"type":"text","timestamp":1,"sessionID":"ses_x","part":{"id":"p","messageID":"m","sessionID":"ses_x","type":"text","text":"PONG"}}"#;
        assert_eq!(
            map_opencode_line(line),
            vec![AgentEventKind::AssistantDelta {
                text: "PONG".into()
            }]
        );
    }

    #[test]
    fn maps_completed_tool_line() {
        let line = r#"{"type":"tool_use","part":{"type":"tool","tool":"bash","callID":"call_1","state":{"status":"completed","input":{"command":"echo hi"},"output":"hi\n","title":"echo hi","metadata":{"exit":0}}}}"#;
        assert_eq!(
            map_opencode_line(line),
            vec![
                AgentEventKind::ToolStarted {
                    item_id: "call_1".into(),
                    title: "echo hi".into(),
                    detail: Some("echo hi".into()),
                },
                AgentEventKind::ToolCompleted {
                    item_id: "call_1".into(),
                    success: true,
                    detail: Some("hi\n".into()),
                },
            ]
        );
    }

    #[test]
    fn maps_step_finish_line() {
        let line = r#"{"type":"step_finish","part":{"type":"step-finish","reason":"stop","tokens":{"total":9666,"input":3,"output":9,"reasoning":0,"cache":{"write":136,"read":9518}}}}"#;
        assert_eq!(
            map_opencode_line(line),
            vec![AgentEventKind::UsageUpdated {
                input_tokens: 3,
                cached_input_tokens: 9518,
                output_tokens: 9,
            }]
        );
    }

    #[test]
    fn ignores_step_start() {
        assert!(map_opencode_line(r#"{"type":"step_start"}"#).is_empty());
    }

    #[test]
    fn ignores_malformed_line() {
        assert!(map_opencode_line("not json").is_empty());
    }
}
