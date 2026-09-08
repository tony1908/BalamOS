use crate::{antigravity_protocol::map_antigravity_line, codex_harness::HarnessError};
use async_trait::async_trait;
use orbit_domain::{
    AgentEvent, AgentEventBatch, AgentEventKind, AgentSessionState, AgentSessionView,
    ApprovalDecision, PermissionProfile, SecretEnv,
};
use std::{ffi::OsString, path::PathBuf, process::Stdio, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::Mutex,
};
use uuid::Uuid;

struct State {
    events: Vec<AgentEvent>,
    next_cursor: u64,
    state: AgentSessionState,
    conversation_id: Option<String>,
    child: Option<Child>,
}

pub struct AntigravityHarness {
    workspace_id: Uuid,
    binary: OsString,
    host_path: PathBuf,
    profile: PermissionProfile,
    model: Option<String>,
    reasoning_effort: Option<String>,
    state: Arc<Mutex<State>>,
    assigned_env: Arc<Vec<SecretEnv>>,
}

impl AntigravityHarness {
    pub async fn spawn(
        binary: OsString,
        host_path: PathBuf,
        workspace_id: Uuid,
        profile: PermissionProfile,
        model: Option<String>,
        reasoning_effort: Option<String>,
        assigned_env: Vec<SecretEnv>,
    ) -> Result<Arc<Self>, HarnessError> {
        Ok(Arc::new(Self {
            workspace_id,
            binary,
            host_path,
            profile,
            model,
            reasoning_effort,
            state: Arc::new(Mutex::new(State {
                events: Vec::new(),
                next_cursor: 0,
                state: AgentSessionState::Idle,
                conversation_id: None,
                child: None,
            })),
            assigned_env: Arc::new(assigned_env),
        }))
    }
    pub async fn view(&self) -> AgentSessionView {
        let s = self.state.lock().await;
        AgentSessionView {
            workspace_id: self.workspace_id,
            thread_id: s.conversation_id.clone(),
            state: s.state,
            next_cursor: s.next_cursor,
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
        let conversation_id = self.state.lock().await.conversation_id.clone();
        let mut command = Command::new(&self.binary);
        for secret in self.assigned_env.iter() {
            command.env(&secret.env_name, &secret.value.0);
        }
        command.current_dir(&self.host_path).args([
            "--prompt",
            &prompt,
            "--output-format",
            "stream-json",
            "--input-format",
            "text",
        ]);
        if let Some(model) = self.model.as_deref().filter(|value| !value.is_empty()) {
            command.args(["--model", model]);
        }
        if let Some(effort) = self
            .reasoning_effort
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            command.args(["--effort", effort]);
        }
        match self.profile {
            PermissionProfile::Observe => {
                command.args(["--mode", "plan"]);
            }
            PermissionProfile::Workspace => {
                // ponytail: antigravity runs host-side and unattended (no stdin/TTY), so accept-edits's request-review silently no-ops any non-edit tool; it is unsandboxed either way, so Workspace also auto-proceeds.
                command.arg("--dangerously-skip-permissions");
            }
            PermissionProfile::FullControl => {
                command.arg("--dangerously-skip-permissions");
            }
        }
        if let Some(id) = conversation_id {
            command.args(["--conversation", &id]);
        }
        let mut child = command
            .stdin(Stdio::null())
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
        let s = self.state.lock().await;
        let events = s
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
        conversation_id: Option<String>,
    ) {
        let mut s = self.state.lock().await;
        s.events = events;
        s.next_cursor = next_cursor;
        if conversation_id.is_some() {
            s.conversation_id = conversation_id;
        }
        s.state = AgentSessionState::Idle;
    }
    pub async fn resolve_approval(&self, _: &str, _: ApprovalDecision) -> Result<(), HarnessError> {
        Ok(())
    }
    pub async fn interrupt(&self) -> Result<AgentSessionView, HarnessError> {
        if let Some(c) = self.state.lock().await.child.as_mut() {
            let _ = c.start_kill();
        }
        set_state(&self.state, AgentSessionState::Interrupted).await;
        Ok(self.view().await)
    }
    pub async fn stop(&self) {
        if let Some(c) = self.state.lock().await.child.as_mut() {
            let _ = c.start_kill();
        }
    }
    #[cfg(test)]
    async fn apply_line(&self, line: &str) {
        apply_line(&self.state, line).await;
    }
    #[cfg(test)]
    async fn for_test(workspace_id: Uuid) -> Arc<Self> {
        Self::spawn(
            "antigravity".into(),
            "/tmp".into(),
            workspace_id,
            PermissionProfile::Workspace,
            None,
            None,
            vec![],
        )
        .await
        .unwrap()
    }
}

#[async_trait]
impl crate::agent_manager::AgentHarness for AntigravityHarness {
    async fn view(&self) -> AgentSessionView {
        self.view().await
    }
    async fn send_prompt(&self, p: String) -> Result<AgentSessionView, HarnessError> {
        self.send_prompt(p).await
    }
    async fn poll(&self, a: u64) -> AgentEventBatch {
        self.poll(a).await
    }
    async fn restore(&self, e: Vec<AgentEvent>, n: u64, c: Option<String>) {
        self.restore(e, n, c).await
    }
    async fn resolve_approval(&self, r: &str, d: ApprovalDecision) -> Result<(), HarnessError> {
        self.resolve_approval(r, d).await
    }
    async fn interrupt(&self) -> Result<AgentSessionView, HarnessError> {
        self.interrupt().await
    }
    async fn stop(&self) {
        self.stop().await
    }
}

async fn set_state(state: &Arc<Mutex<State>>, next: AgentSessionState) {
    apply_kind(state, AgentEventKind::StateChanged { state: next }).await
}
async fn apply_line(state: &Arc<Mutex<State>>, line: &str) {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(line)
        && value.get("event").and_then(serde_json::Value::as_str) == Some("init")
        && let Some(id) = value
            .get("conversation_id")
            .and_then(serde_json::Value::as_str)
    {
        let mut s = state.lock().await;
        if s.conversation_id.is_none() {
            s.conversation_id = Some(id.into());
        }
    }
    for kind in map_antigravity_line(line) {
        apply_kind(state, kind).await;
    }
}
async fn apply_kind(state: &Arc<Mutex<State>>, kind: AgentEventKind) {
    let mut s = state.lock().await;
    s.next_cursor += 1;
    let cursor = s.next_cursor;
    if let AgentEventKind::StateChanged { state: next } = kind {
        s.state = next;
        s.events.push(AgentEvent {
            cursor,
            kind: AgentEventKind::StateChanged { state: next },
        });
    } else {
        s.events.push(AgentEvent { cursor, kind });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fresh_harness_is_idle_and_empty() {
        let h = AntigravityHarness::for_test(Uuid::nil()).await;
        let v = h.view().await;
        assert_eq!(v.state, AgentSessionState::Idle);
        assert_eq!(v.next_cursor, 0);
        assert!(h.poll(0).await.events.is_empty());
    }

    #[tokio::test]
    async fn agent_response_emits_assistant_delta() {
        let h = AntigravityHarness::for_test(Uuid::nil()).await;
        h.apply_line(r#"{"event":"step_update","step_update":{"step_type":"agent_response","text_delta":"PONG\n"}}"#).await;
        assert!(
            matches!(h.poll(0).await.events.as_slice(), [AgentEvent { kind: AgentEventKind::AssistantDelta { text }, .. }] if text == "PONG\n")
        );
    }

    #[tokio::test]
    async fn init_captures_conversation_id() {
        let h = AntigravityHarness::for_test(Uuid::nil()).await;
        h.apply_line(r#"{"event":"init","conversation_id":"c1"}"#)
            .await;
        assert_eq!(h.view().await.thread_id.as_deref(), Some("c1"));
    }
}
