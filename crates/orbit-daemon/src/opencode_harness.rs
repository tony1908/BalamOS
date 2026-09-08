use crate::{codex_harness::HarnessError, opencode_protocol::map_opencode_line};
use orbit_domain::{
    AgentEvent, AgentEventBatch, AgentEventKind, AgentSessionState, AgentSessionView,
    ApprovalDecision, PermissionProfile,
};
use std::{ffi::OsString, process::Stdio, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
};
use uuid::Uuid;

struct State {
    events: Vec<AgentEvent>,
    next_cursor: u64,
    state: AgentSessionState,
    session_id: Option<String>,
    child: Option<Child>,
}

pub struct OpencodeHarness {
    workspace_id: Uuid,
    docker_binary: OsString,
    container_id: String,
    user: &'static str,
    _api_key: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
    assigned_env: Arc<Vec<orbit_domain::SecretEnv>>,
    state: Arc<Mutex<State>>,
}

impl OpencodeHarness {
    pub async fn spawn(
        docker_binary: OsString,
        container_id: String,
        workspace_id: Uuid,
        profile: PermissionProfile,
        api_key: Option<String>,
        model: Option<String>,
        reasoning_effort: Option<String>,
        assigned_env: Vec<orbit_domain::SecretEnv>,
    ) -> Result<Arc<Self>, HarnessError> {
        let assigned_env = Arc::new(assigned_env);
        let user = if matches!(profile, PermissionProfile::FullControl) {
            "root"
        } else {
            "abc"
        };
        let Some(api_key) = api_key else {
            return Err(HarnessError::Unavailable(
                "opencode API key not configured (set ORBIT_OPENCODE_API_KEY)".into(),
            ));
        };
        let mut auth = Command::new(&docker_binary);
        auth.args([
            "exec", "-i", "-u", user, "-e", "HOME=/config", &container_id,
            "sh", "-c",
            "mkdir -p /config/.local/share/opencode && cat > /config/.local/share/opencode/auth.json",
        ])
        .stdin(Stdio::piped());
        for secret in assigned_env.iter() {
            auth.env(&secret.env_name, &secret.value.0);
        }
        let mut child = auth.spawn().map_err(|_| HarnessError::Spawn)?;
        child
            .stdin
            .take()
            .ok_or(HarnessError::Spawn)?
            .write_all(
                serde_json::json!({"opencode-go": {"type": "api", "key": api_key}})
                    .to_string()
                    .as_bytes(),
            )
            .await
            .map_err(|_| HarnessError::Spawn)?;
        if !child
            .wait()
            .await
            .map_err(|_| HarnessError::Spawn)?
            .success()
        {
            return Err(HarnessError::Unavailable(
                "could not configure opencode authentication".into(),
            ));
        }
        Ok(Arc::new(Self {
            workspace_id,
            docker_binary,
            container_id,
            user,
            _api_key: Some(api_key),
            model,
            reasoning_effort,
            assigned_env,
            state: Arc::new(Mutex::new(State {
                events: Vec::new(),
                next_cursor: 0,
                state: AgentSessionState::Idle,
                session_id: None,
                child: None,
            })),
        }))
    }

    pub async fn view(&self) -> AgentSessionView {
        let state = self.state.lock().await;
        AgentSessionView {
            workspace_id: self.workspace_id,
            thread_id: state.session_id.clone(),
            state: state.state,
            next_cursor: state.next_cursor,
        }
    }

    pub async fn send_prompt(&self, prompt: String) -> Result<AgentSessionView, HarnessError> {
        if prompt.trim().is_empty() {
            return Err(HarnessError::Protocol("prompt is empty"));
        }
        apply_kind(
            &self.state,
            AgentEventKind::UserPrompt {
                text: prompt.clone(),
            },
        )
        .await;
        set_state(&self.state, AgentSessionState::Running).await;
        let session_id = self.state.lock().await.session_id.clone();
        let mut command = Command::new(&self.docker_binary);
        command.args([
            "exec",
            "-i",
            "-u",
            self.user,
            "-w",
            "/workspace",
            "-e",
            "HOME=/config",
        ]);
        for secret in self.assigned_env.iter() {
            command.args(["-e", &secret.env_name]);
            command.env(&secret.env_name, &secret.value.0);
        }
        command.args([&self.container_id, "opencode", "run"]);
        if let Some(session_id) = session_id {
            command.args(["--session", &session_id]);
        }
        let model = self
            .model
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or("opencode-go/gpt-5.6-luna");
        command.args(["-m", model, "--format", "json", "--auto"]);
        if let Some(effort) = self
            .reasoning_effort
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            // opencode variants are low/high; map medium -> high.
            let variant = if effort == "medium" { "high" } else { effort };
            command.args(["--variant", variant]);
        }
        command
            .arg(&prompt)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|_| HarnessError::Spawn)?;
        let stdout = child.stdout.take().ok_or(HarnessError::Spawn)?;
        self.state.lock().await.child = Some(child);
        let state = Arc::clone(&self.state);
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                apply_line(&state, &line).await;
            }
            let success = if let Some(mut child) = state.lock().await.child.take() {
                child.wait().await.map(|s| s.success()).unwrap_or(false)
            } else {
                false
            };
            let current_state = state.lock().await.state;
            if current_state != AgentSessionState::Interrupted {
                let has_events = !state.lock().await.events.is_empty();
                set_state(
                    &state,
                    if success || has_events {
                        AgentSessionState::Completed
                    } else {
                        AgentSessionState::Failed
                    },
                )
                .await;
            }
        });
        Ok(self.view().await)
    }

    pub async fn poll(&self, after: u64) -> AgentEventBatch {
        let state = self.state.lock().await;
        let events = state
            .events
            .iter()
            .filter(|e| e.cursor > after)
            .cloned()
            .collect::<Vec<_>>();
        AgentEventBatch {
            workspace_id: self.workspace_id,
            after,
            next_cursor: events.last().map_or(after, |e| e.cursor),
            events,
            has_more: false,
        }
    }

    pub async fn restore(
        &self,
        events: Vec<AgentEvent>,
        next_cursor: u64,
        session_id: Option<String>,
    ) {
        let mut state = self.state.lock().await;
        state.events = events;
        state.next_cursor = next_cursor;
        if session_id.is_some() {
            state.session_id = session_id;
        }
        state.state = AgentSessionState::Idle;
    }

    pub async fn resolve_approval(
        &self,
        _request_id: &str,
        _decision: ApprovalDecision,
    ) -> Result<(), HarnessError> {
        Ok(())
    }

    pub async fn interrupt(&self) -> Result<AgentSessionView, HarnessError> {
        if let Some(child) = self.state.lock().await.child.as_mut() {
            let _ = child.start_kill();
        }
        set_state(&self.state, AgentSessionState::Interrupted).await;
        Ok(self.view().await)
    }

    pub async fn stop(&self) {
        if let Some(child) = self.state.lock().await.child.as_mut() {
            let _ = child.start_kill();
        }
    }

    #[cfg(test)]
    async fn apply_line(&self, line: &str) {
        apply_line(&self.state, line).await;
    }
}

async fn set_state(state: &Arc<Mutex<State>>, next: AgentSessionState) {
    apply_kind(state, AgentEventKind::StateChanged { state: next }).await;
}

async fn apply_line(state: &Arc<Mutex<State>>, line: &str) {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(line)
        && let Some(id) = value.get("sessionID").and_then(serde_json::Value::as_str)
    {
        let mut current = state.lock().await;
        if current.session_id.is_none() {
            current.session_id = Some(id.to_owned());
        }
    }
    for kind in map_opencode_line(line) {
        apply_kind(state, kind).await;
    }
}

async fn apply_kind(state: &Arc<Mutex<State>>, kind: AgentEventKind) {
    let mut state = state.lock().await;
    if let AgentEventKind::StateChanged { state: next } = kind {
        state.state = next;
        state.next_cursor += 1;
        let cursor = state.next_cursor;
        state.events.push(AgentEvent {
            cursor,
            kind: AgentEventKind::StateChanged { state: next },
        });
    } else {
        state.next_cursor += 1;
        let cursor = state.next_cursor;
        state.events.push(AgentEvent { cursor, kind });
    }
}

impl OpencodeHarness {
    #[cfg(test)]
    async fn for_test(workspace_id: Uuid) -> Arc<Self> {
        Arc::new(Self {
            workspace_id,
            docker_binary: "docker".into(),
            container_id: "test".into(),
            user: "abc",
            _api_key: None,
            model: None,
            reasoning_effort: None,
            state: Arc::new(Mutex::new(State {
                events: Vec::new(),
                next_cursor: 0,
                state: AgentSessionState::Idle,
                session_id: None,
                child: None,
            })),
            assigned_env: Arc::new(Vec::new()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_manager::AgentHarness;
    #[tokio::test]
    async fn fresh_harness_is_idle_and_empty() {
        let h = OpencodeHarness::for_test(Uuid::nil()).await;
        let v = h.view().await;
        assert_eq!(v.state, AgentSessionState::Idle);
        assert_eq!(v.next_cursor, 0);
        assert!(h.poll(0).await.events.is_empty());
    }
    #[tokio::test]
    async fn restores_events_and_session_for_polling() {
        let h = OpencodeHarness::for_test(Uuid::nil()).await;
        let events = vec![AgentEvent {
            cursor: 1,
            kind: AgentEventKind::AssistantDelta {
                text: "restored".into(),
            },
        }];
        h.restore(events.clone(), 2, Some("ses_restored".into()))
            .await;
        assert_eq!(h.poll(0).await.events, events);
        assert_eq!(h.view().await.thread_id.as_deref(), Some("ses_restored"));
        assert_eq!(h.view().await.state, AgentSessionState::Idle);
    }
    #[tokio::test]
    async fn ingests_events_and_captures_session() {
        let h = OpencodeHarness::for_test(Uuid::nil()).await;
        h.apply_line(r#"{"type":"text","sessionID":"ses_x","part":{"text":"hi"}}"#)
            .await;
        h.apply_line(r#"{"type":"step_finish","part":{"tokens":{"input":1,"output":2}}}"#)
            .await;
        let b = h.poll(1).await;
        assert_eq!(b.events.len(), 1);
        assert_eq!(h.view().await.thread_id.as_deref(), Some("ses_x"));
    }
    #[tokio::test]
    async fn interrupt_sets_interrupted() {
        let h = OpencodeHarness::for_test(Uuid::nil()).await;
        assert_eq!(
            h.interrupt().await.unwrap().state,
            AgentSessionState::Interrupted
        );
    }

    #[tokio::test]
    async fn trait_send_prompt_rejects_empty_prompt_without_recursing() {
        let h: Arc<dyn AgentHarness> = OpencodeHarness::for_test(Uuid::nil()).await;
        let result = h.send_prompt(String::new()).await;
        assert!(matches!(
            result,
            Err(HarnessError::Protocol("prompt is empty"))
        ));
    }
}
