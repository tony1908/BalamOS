use crate::{claude_protocol::map_claude_line, codex_harness::HarnessError};
use async_trait::async_trait;
use orbit_domain::{
    AgentEvent, AgentEventBatch, AgentEventKind, AgentSessionState, AgentSessionView,
    ApprovalDecision, PermissionProfile, SecretEnv,
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

pub enum ClaudeCredential {
    OauthToken(String),
    ApiKey(String),
}

impl ClaudeCredential {
    fn env_name(&self) -> &'static str {
        match self {
            Self::OauthToken(_) => "CLAUDE_CODE_OAUTH_TOKEN",
            Self::ApiKey(_) => "ANTHROPIC_API_KEY",
        }
    }

    fn secret(&self) -> &str {
        match self {
            Self::OauthToken(secret) | Self::ApiKey(secret) => secret,
        }
    }
}

pub struct ClaudeHarness {
    workspace_id: Uuid,
    docker_binary: OsString,
    container_id: String,
    user: &'static str,
    profile: PermissionProfile,
    env_var: &'static str,
    model: Option<String>,
    state: Arc<Mutex<State>>,
    assigned_env: Arc<Vec<SecretEnv>>,
}

impl ClaudeHarness {
    pub async fn spawn(
        docker_binary: OsString,
        container_id: String,
        workspace_id: Uuid,
        profile: PermissionProfile,
        credential: Option<ClaudeCredential>,
        model: Option<String>,
        _reasoning_effort: Option<String>,
        assigned_env: Vec<SecretEnv>,
    ) -> Result<Arc<Self>, HarnessError> {
        let assigned_env = Arc::new(assigned_env);
        let user = if matches!(profile, PermissionProfile::FullControl) {
            "root"
        } else {
            "abc"
        };
        let Some(credential) = credential else {
            return Err(HarnessError::Unavailable(
                "Claude Code credentials not configured (set ORBIT_CLAUDE_CODE_TOKEN from `claude setup-token`, or ORBIT_ANTHROPIC_API_KEY)".into(),
            ));
        };
        let env_var = credential.env_name();
        let mut auth = Command::new(&docker_binary);
        auth.args([
            "exec",
            "-i",
            "-u",
            user,
            "-e",
            "HOME=/config",
            &container_id,
            "sh",
            "-c",
            "mkdir -p /config && cat > /config/.anthropic_key && chmod 600 /config/.anthropic_key",
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
            .write_all(credential.secret().as_bytes())
            .await
            .map_err(|_| HarnessError::Spawn)?;
        if !child
            .wait()
            .await
            .map_err(|_| HarnessError::Spawn)?
            .success()
        {
            return Err(HarnessError::Unavailable(
                "could not configure Claude authentication".into(),
            ));
        }
        Ok(Arc::new(Self {
            workspace_id,
            docker_binary,
            container_id,
            user,
            profile,
            env_var,
            model,
            state: Arc::new(Mutex::new(State {
                events: Vec::new(),
                next_cursor: 0,
                state: AgentSessionState::Idle,
                session_id: None,
                child: None,
            })),
            assigned_env,
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
        let model = self
            .model
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or("sonnet");
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
        command.args([
            &self.container_id,
            "sh",
            "-c",
            &format!(
                "export {}=\"$(cat /config/.anthropic_key)\"; exec \"$@\"",
                self.env_var
            ),
            "sh",
            "claude",
            "-p",
            &prompt,
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            model,
        ]);
        match self.profile {
            PermissionProfile::Observe => command.args(["--permission-mode", "plan"]),
            PermissionProfile::Workspace => command.args(["--permission-mode", "acceptEdits"]),
            PermissionProfile::FullControl => command.arg("--dangerously-skip-permissions"),
        };
        if let Some(session_id) = session_id {
            command.args(["--resume", &session_id]);
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| HarnessError::Spawn)?;
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
            if state.lock().await.state != AgentSessionState::Interrupted {
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

#[async_trait]
impl crate::agent_manager::AgentHarness for ClaudeHarness {
    async fn view(&self) -> AgentSessionView {
        self.view().await
    }
    async fn send_prompt(&self, prompt: String) -> Result<AgentSessionView, HarnessError> {
        self.send_prompt(prompt).await
    }
    async fn poll(&self, after: u64) -> AgentEventBatch {
        self.poll(after).await
    }
    async fn restore(&self, events: Vec<AgentEvent>, next_cursor: u64, session_id: Option<String>) {
        self.restore(events, next_cursor, session_id).await;
    }
    async fn resolve_approval(
        &self,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), HarnessError> {
        self.resolve_approval(request_id, decision).await
    }
    async fn interrupt(&self) -> Result<AgentSessionView, HarnessError> {
        self.interrupt().await
    }
    async fn stop(&self) {
        self.stop().await;
    }
}

async fn set_state(state: &Arc<Mutex<State>>, next: AgentSessionState) {
    apply_kind(state, AgentEventKind::StateChanged { state: next }).await;
}

async fn apply_line(state: &Arc<Mutex<State>>, line: &str) {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(line)
        && let Some(id) = value.get("session_id").and_then(serde_json::Value::as_str)
    {
        let mut current = state.lock().await;
        if current.session_id.is_none() {
            current.session_id = Some(id.to_owned());
        }
    }
    for kind in map_claude_line(line) {
        apply_kind(state, kind).await;
    }
}

async fn apply_kind(state: &Arc<Mutex<State>>, kind: AgentEventKind) {
    let mut state = state.lock().await;
    state.next_cursor += 1;
    let cursor = state.next_cursor;
    if let AgentEventKind::StateChanged { state: next } = kind {
        state.state = next;
        state.events.push(AgentEvent {
            cursor,
            kind: AgentEventKind::StateChanged { state: next },
        });
    } else {
        state.events.push(AgentEvent { cursor, kind });
    }
}

impl ClaudeHarness {
    #[cfg(test)]
    async fn for_test(workspace_id: Uuid) -> Arc<Self> {
        Arc::new(Self {
            workspace_id,
            docker_binary: "docker".into(),
            container_id: "test".into(),
            user: "abc",
            profile: PermissionProfile::Workspace,
            env_var: "ANTHROPIC_API_KEY",
            model: None,
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

    #[tokio::test]
    async fn fresh_harness_is_idle_and_empty() {
        let h = ClaudeHarness::for_test(Uuid::nil()).await;
        let v = h.view().await;
        assert_eq!(v.state, AgentSessionState::Idle);
        assert_eq!(v.next_cursor, 0);
        assert!(h.poll(0).await.events.is_empty());
    }

    #[tokio::test]
    async fn assistant_stream_json_emits_assistant_delta() {
        let h = ClaudeHarness::for_test(Uuid::nil()).await;
        h.apply_line(r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello"}]},"session_id":"s1"}"#).await;
        assert!(
            matches!(h.poll(0).await.events.as_slice(), [AgentEvent { kind: AgentEventKind::AssistantDelta { text }, .. }] if text == "Hello")
        );
        assert_eq!(h.view().await.thread_id.as_deref(), Some("s1"));
    }
}
