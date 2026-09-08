use crate::codex_protocol::{
    MAX_CODEX_FRAME_BYTES, approval_event, approval_result, normalized_notification, notification,
    request, thread_id, turn_id,
};
use orbit_protocol::{MAX_AGENT_EVENT_BATCH, MAX_IPC_FRAME_BYTES, ResponseBody};

pub const DESKTOP_CONTROL_INSTRUCTIONS: &str = "You are operating inside the Orbit Ubuntu workspace. For graphical desktop requests, use the orbit-desktop-control skill and its exact orbitctl commands; use orbitctl --help if needed. Never attempt to access the Docker socket or unrelated host paths. Consult AGENTS.md for your available skills and use any that apply. Keep a long-term memory in .orbit/MEMORY.md: record durable facts there and read it before asking about things you may already know. Never store secrets in it.";
use orbit_domain::{
    AgentEvent, AgentEventBatch, AgentEventKind, AgentSessionState, AgentSessionView,
    ApprovalDecision, PermissionProfile, RedactedSecret, RedactedUrl,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    process::Stdio,
    sync::{
        Arc, Weak,
        atomic::{AtomicU64, Ordering},
    },
};
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command},
    sync::{Mutex, oneshot},
    time::{Duration, timeout},
};
use uuid::Uuid;

const RPC_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PENDING_REQUESTS: usize = 64;
const MAX_HISTORY: usize = 2_000;
const IPC_ENVELOPE_RESERVE: usize = 1024;
const EVENT_FIELD_CAP: usize = 1024;
const MAX_PLAN_STEPS: usize = 16;
const TRUNCATION_MARKER: &str = "… [truncated]";

fn bounded(value: String) -> String {
    if value.len() <= EVENT_FIELD_CAP {
        return value;
    }
    let keep = EVENT_FIELD_CAP - TRUNCATION_MARKER.len();
    let mut end = keep.min(value.len());
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &value[..end], TRUNCATION_MARKER)
}

fn sanitize_event(mut event: AgentEvent) -> AgentEvent {
    match &mut event.kind {
        AgentEventKind::AuthenticationRequired { url, user_code } => {
            url.0 = bounded(std::mem::take(&mut url.0));
            *user_code = user_code
                .take()
                .map(|secret| RedactedSecret(bounded(secret.0)));
        }
        AgentEventKind::AssistantDelta { text } => *text = bounded(std::mem::take(text)),
        AgentEventKind::Error { code, message, .. } => {
            *code = bounded(std::mem::take(code));
            *message = bounded(std::mem::take(message));
        }
        AgentEventKind::PlanUpdated { explanation, steps } => {
            *explanation = explanation.take().map(bounded);
            steps.truncate(MAX_PLAN_STEPS);
            for step in steps {
                step.text = bounded(std::mem::take(&mut step.text));
            }
        }
        AgentEventKind::ToolStarted {
            item_id,
            title,
            detail,
        } => {
            *item_id = bounded(std::mem::take(item_id));
            *title = bounded(std::mem::take(title));
            *detail = detail.take().map(bounded);
        }
        AgentEventKind::ToolCompleted {
            item_id, detail, ..
        } => {
            *item_id = bounded(std::mem::take(item_id));
            *detail = detail.take().map(bounded);
        }
        AgentEventKind::ApprovalRequested {
            request_id,
            title,
            detail,
            ..
        } => {
            *request_id = bounded(std::mem::take(request_id));
            *title = bounded(std::mem::take(title));
            *detail = detail.take().map(bounded);
        }
        AgentEventKind::ApprovalResolved { request_id, .. } => {
            *request_id = bounded(std::mem::take(request_id))
        }
        _ => {}
    }
    event
}

pub(crate) fn pack_event_page(history: &[AgentEvent], after: u64) -> AgentEventBatch {
    let mut events = Vec::new();
    for event in history.iter().filter(|event| event.cursor > after) {
        if events.len() == MAX_AGENT_EVENT_BATCH {
            break;
        }
        let mut candidate = events.clone();
        candidate.push(event.clone());
        let batch = AgentEventBatch {
            workspace_id: Uuid::nil(),
            after,
            next_cursor: event.cursor,
            events: candidate.clone(),
            has_more: false,
        };
        let size = serde_json::to_vec(&ResponseBody::AgentEvents(batch))
            .map_or(usize::MAX, |value| value.len());
        if size + IPC_ENVELOPE_RESERVE > MAX_IPC_FRAME_BYTES {
            if events.is_empty() {
                events.push(AgentEvent {
                    cursor: event.cursor,
                    kind: AgentEventKind::Error {
                        code: "event_too_large".into(),
                        message: "event exceeded IPC transport budget".into(),
                        retryable: true,
                    },
                });
            }
            break;
        }
        events = candidate;
    }
    let next_cursor = events.last().map_or(after, |event| event.cursor);
    // has_more iff some event in history still has cursor > next_cursor.
    let has_more = history.iter().any(|event| event.cursor > next_cursor);
    AgentEventBatch {
        workspace_id: Uuid::nil(),
        after,
        next_cursor,
        events,
        has_more,
    }
}

#[cfg(test)]
mod pagination_tests {
    use super::*;

    #[test]
    fn poll_page_cursor_is_last_returned_and_pages_are_contiguous() {
        let events = (1..=300)
            .map(|cursor| AgentEvent {
                cursor,
                kind: AgentEventKind::StateChanged {
                    state: AgentSessionState::Idle,
                },
            })
            .collect::<Vec<_>>();
        let first = pack_event_page(&events, 0);
        assert_eq!(first.events.len(), 256);
        assert_eq!(first.next_cursor, 256);
        assert!(first.has_more);
        let second = pack_event_page(&events, first.next_cursor);
        assert_eq!(second.events.first().map(|event| event.cursor), Some(257));
        assert!(!second.has_more);
    }

    #[test]
    fn oversized_event_field_is_bounded_before_transport() {
        let event = AgentEvent {
            cursor: 1,
            kind: AgentEventKind::AssistantDelta {
                text: "x".repeat(20_000),
            },
        };
        let bounded = sanitize_event(event);
        let AgentEventKind::AssistantDelta { text } = bounded.kind else {
            panic!()
        };
        assert!(text.len() <= EVENT_FIELD_CAP);
        assert!(text.ends_with(TRUNCATION_MARKER));
    }

    #[test]
    fn bounded_crab_string_is_valid_utf8_and_byte_capped() {
        let output = bounded("🦀".repeat(20_000));
        assert!(output.len() <= EVENT_FIELD_CAP);
        assert!(std::str::from_utf8(output.as_bytes()).is_ok());
        assert!(output.ends_with(TRUNCATION_MARKER));
    }

    #[test]
    fn oversized_first_event_is_replaced_and_cursor_progresses() {
        let events = vec![
            AgentEvent {
                cursor: 1,
                kind: AgentEventKind::AssistantDelta {
                    text: "x".repeat(MAX_IPC_FRAME_BYTES),
                },
            },
            AgentEvent {
                cursor: 2,
                kind: AgentEventKind::StateChanged {
                    state: AgentSessionState::Idle,
                },
            },
        ];
        let first = pack_event_page(&events, 0);
        assert_eq!(first.events[0].cursor, 1);
        assert!(matches!(first.events[0].kind, AgentEventKind::Error { .. }));
        assert_eq!(first.next_cursor, 1);
        assert!(first.has_more);
        let second = pack_event_page(&events, first.next_cursor);
        assert_eq!(second.events[0].cursor, 2);
        assert!(!second.has_more);
    }

    #[test]
    fn multibyte_truncation_respects_byte_cap_and_all_event_variants_fit() {
        let text = "🦀".repeat(10_000);
        let variants = vec![
            AgentEventKind::AssistantDelta { text: text.clone() },
            AgentEventKind::Error {
                code: text.clone(),
                message: text.clone(),
                retryable: true,
            },
            AgentEventKind::ToolStarted {
                item_id: text.clone(),
                title: text.clone(),
                detail: Some(text.clone()),
            },
            AgentEventKind::ApprovalRequested {
                request_id: text.clone(),
                kind: orbit_domain::AgentApprovalKind::Command,
                title: text.clone(),
                detail: Some(text.clone()),
            },
            AgentEventKind::PlanUpdated {
                explanation: Some(text.clone()),
                steps: (0..30)
                    .map(|_| orbit_domain::AgentPlanStep {
                        text: text.clone(),
                        state: orbit_domain::AgentPlanStepState::Pending,
                    })
                    .collect(),
            },
        ];
        for kind in variants {
            let event = sanitize_event(AgentEvent { cursor: 1, kind });
            let size = serde_json::to_vec(&event).unwrap().len();
            assert!(size + IPC_ENVELOPE_RESERVE < MAX_IPC_FRAME_BYTES);
        }
    }

    #[test]
    fn ipc_writer_pages_large_logical_stream_without_gaps_or_oversize() {
        let events = (1..=300)
            .map(|cursor| AgentEvent {
                cursor,
                kind: AgentEventKind::ToolStarted {
                    item_id: format!("item-{cursor}"),
                    title: "Command".into(),
                    detail: Some("x".repeat(EVENT_FIELD_CAP)),
                },
            })
            .collect::<Vec<_>>();
        let mut after = 0;
        let mut received = Vec::new();
        let mut pages = 0;
        while after < events.len() as u64 {
            let batch = pack_event_page(&events, after);
            let response = orbit_protocol::Response {
                protocol: orbit_protocol::PROTOCOL_VERSION,
                id: Uuid::new_v4(),
                body: ResponseBody::AgentEvents(batch),
            };
            let payload = crate::encode_response(&response).unwrap();
            assert!(payload.len() <= MAX_IPC_FRAME_BYTES);
            let parsed: orbit_protocol::Response =
                serde_json::from_slice(&payload[..payload.len() - 1]).unwrap();
            let ResponseBody::AgentEvents(batch) = parsed.body else {
                panic!()
            };
            assert!(!batch.events.is_empty());
            received.extend(batch.events.iter().map(|event| event.cursor));
            after = batch.next_cursor;
            pages += 1;
        }
        assert!(pages > 1);
        assert_eq!(received, (1..=300).collect::<Vec<_>>());
    }

    #[test]
    fn pack_event_page_paginates_oversized_history() {
        let events: Vec<AgentEvent> = (0..40)
            .map(|i| AgentEvent {
                cursor: i + 1,
                kind: AgentEventKind::AssistantDelta {
                    text: "x".repeat(4000),
                },
            })
            .collect();
        let batch = pack_event_page(&events, 0);
        let encoded = serde_json::to_vec(&ResponseBody::AgentEvents(batch.clone())).unwrap();
        assert!(encoded.len() + IPC_ENVELOPE_RESERVE <= MAX_IPC_FRAME_BYTES);
        assert!(!batch.events.is_empty());
        assert!(batch.next_cursor < events.last().unwrap().cursor);
        assert!(batch.has_more);
    }
}

#[derive(Debug, Error)]
pub enum HarnessError {
    #[error("harness runtime is not available yet: {0}")]
    Unavailable(String),
    #[error("could not start Codex harness")]
    Spawn,
    #[error("Codex harness closed")]
    Closed,
    #[error("Codex harness request timed out")]
    Timeout,
    #[error("Codex harness protocol error: {0}")]
    Protocol(&'static str),
    #[error("Codex request failed: {0}")]
    Request(String),
    #[error("Codex session is not ready")]
    NotReady,
    #[error("approval request was not found")]
    ApprovalNotFound,
}

struct PendingApproval {
    rpc_id: Value,
}

pub struct CodexHarness {
    workspace_id: Uuid,
    profile: PermissionProfile,
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
    approvals: Mutex<HashMap<String, PendingApproval>>,
    history: Mutex<VecDeque<AgentEvent>>,
    state: Mutex<AgentSessionState>,
    thread_id: Mutex<Option<String>>,
    turn_id: Mutex<Option<String>>,
    next_request_id: AtomicU64,
    next_cursor: AtomicU64,
}

impl CodexHarness {
    pub async fn spawn(
        mut command: Command,
        workspace_id: Uuid,
        profile: PermissionProfile,
        assigned_env: Vec<orbit_domain::SecretEnv>,
    ) -> Result<Arc<Self>, HarnessError> {
        for secret in assigned_env {
            command.env(secret.env_name, secret.value.0);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|_| HarnessError::Spawn)?;
        let stdin = child.stdin.take().ok_or(HarnessError::Spawn)?;
        let stdout = child.stdout.take().ok_or(HarnessError::Spawn)?;
        let stderr = child.stderr.take().ok_or(HarnessError::Spawn)?;
        let harness = Arc::new(Self {
            workspace_id,
            profile,
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            pending: Mutex::new(HashMap::new()),
            approvals: Mutex::new(HashMap::new()),
            history: Mutex::new(VecDeque::new()),
            state: Mutex::new(AgentSessionState::Starting),
            thread_id: Mutex::new(None),
            turn_id: Mutex::new(None),
            next_request_id: AtomicU64::new(1),
            next_cursor: AtomicU64::new(0),
        });
        tokio::spawn(read_stdout(stdout, Arc::downgrade(&harness)));
        tokio::spawn(drain_stderr(stderr));
        if let Err(error) = harness.initialize().await {
            harness.stop().await;
            return Err(error);
        }
        Ok(harness)
    }

    async fn initialize(self: &Arc<Self>) -> Result<(), HarnessError> {
        let result = self
            .call(
                "initialize",
                json!({
                    "clientInfo": {"name": "orbit", "title": "Orbit", "version": "0.1.0"},
                    "capabilities": {"experimentalApi": false}
                }),
            )
            .await?;
        if result.get("platformOs").and_then(Value::as_str) != Some("linux") {
            return Err(HarnessError::Protocol("app-server is not running on Linux"));
        }
        self.notify("initialized", json!({})).await?;
        let account = self.call("account/read", json!({})).await?;
        if account.get("account").is_none_or(Value::is_null)
            && account
                .get("requiresOpenaiAuth")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        {
            let login = self
                .call("account/login/start", json!({"type": "chatgptDeviceCode"}))
                .await?;
            let url = login
                .get("verificationUrl")
                .and_then(Value::as_str)
                .ok_or(HarnessError::Protocol("missing device login URL"))?;
            let code = login
                .get("userCode")
                .and_then(Value::as_str)
                .ok_or(HarnessError::Protocol("missing device login code"))?;
            self.set_state(AgentSessionState::NeedsAuthentication).await;
            self.emit(AgentEventKind::AuthenticationRequired {
                url: RedactedUrl(url.to_owned()),
                user_code: Some(RedactedSecret(code.to_owned())),
            })
            .await;
            return Ok(());
        }
        self.start_thread().await
    }

    async fn start_thread(self: &Arc<Self>) -> Result<(), HarnessError> {
        if self.thread_id.lock().await.is_some() {
            return Ok(());
        }
        self.set_state(AgentSessionState::Starting).await;
        let (sandbox, approval_policy) = match self.profile {
            PermissionProfile::Observe => ("read-only", "untrusted"),
            PermissionProfile::Workspace => ("workspace-write", "on-request"),
            PermissionProfile::FullControl => ("danger-full-access", "never"),
        };
        let result = self
            .call(
                "thread/start",
                json!({
                    "cwd": "/workspace",
                    "sandbox": sandbox,
                    "approvalPolicy": approval_policy,
                    "approvalsReviewer": "user",
                    "developerInstructions": DESKTOP_CONTROL_INSTRUCTIONS
                }),
            )
            .await?;
        *self.thread_id.lock().await =
            Some(thread_id(&result).ok_or(HarnessError::Protocol("missing thread id"))?);
        self.set_state(AgentSessionState::Idle).await;
        Ok(())
    }

    pub async fn send_prompt(&self, prompt: String) -> Result<AgentSessionView, HarnessError> {
        let thread_id = self
            .thread_id
            .lock()
            .await
            .clone()
            .ok_or(HarnessError::NotReady)?;
        if prompt.trim().is_empty() {
            return Err(HarnessError::Protocol("prompt is empty"));
        }
        self.emit(AgentEventKind::UserPrompt {
            text: prompt.clone(),
        })
        .await;
        self.set_state(AgentSessionState::Running).await;
        let result = self
            .call(
                "turn/start",
                json!({
                    "threadId": thread_id,
                    "input": [{"type": "text", "text": prompt}]
                }),
            )
            .await?;
        *self.turn_id.lock().await = turn_id(&result);
        Ok(self.view().await)
    }

    pub async fn resolve_approval(
        &self,
        request_id: &str,
        decision: ApprovalDecision,
    ) -> Result<(), HarnessError> {
        let pending = self
            .approvals
            .lock()
            .await
            .remove(request_id)
            .ok_or(HarnessError::ApprovalNotFound)?;
        self.write_value(&json!({
            "id": pending.rpc_id,
            "result": approval_result(decision)
        }))
        .await?;
        self.emit(AgentEventKind::ApprovalResolved {
            request_id: request_id.to_owned(),
            decision,
        })
        .await;
        self.set_state(AgentSessionState::Running).await;
        Ok(())
    }

    pub async fn interrupt(&self) -> Result<AgentSessionView, HarnessError> {
        let thread_id = self
            .thread_id
            .lock()
            .await
            .clone()
            .ok_or(HarnessError::NotReady)?;
        let turn_id = self
            .turn_id
            .lock()
            .await
            .clone()
            .ok_or(HarnessError::NotReady)?;
        self.call(
            "turn/interrupt",
            json!({"threadId": thread_id, "turnId": turn_id}),
        )
        .await?;
        self.set_state(AgentSessionState::Interrupted).await;
        Ok(self.view().await)
    }

    pub async fn poll(&self, after: u64) -> AgentEventBatch {
        let history = self.history.lock().await;
        let history = history.iter().cloned().collect::<Vec<_>>();
        let mut batch = pack_event_page(&history, after);
        batch.workspace_id = self.workspace_id;
        batch
    }

    pub async fn view(&self) -> AgentSessionView {
        AgentSessionView {
            workspace_id: self.workspace_id,
            thread_id: self.thread_id.lock().await.clone(),
            state: *self.state.lock().await,
            next_cursor: self.next_cursor.load(Ordering::Acquire),
        }
    }

    pub async fn stop(&self) {
        let mut child = self.child.lock().await;
        let _ = child.start_kill();
        let _ = timeout(Duration::from_secs(3), child.wait()).await;
        self.fail_pending("harness stopped").await;
    }

    async fn call(&self, method: &str, params: Value) -> Result<Value, HarnessError> {
        let id = self.next_request_id.fetch_add(1, Ordering::AcqRel);
        let (sender, receiver) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            if pending.len() >= MAX_PENDING_REQUESTS {
                return Err(HarnessError::Protocol("too many pending requests"));
            }
            pending.insert(id, sender);
        }
        if let Err(error) = self.write_value(&request(id, method, params)).await {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        timeout(RPC_TIMEOUT, receiver)
            .await
            .map_err(|_| HarnessError::Timeout)?
            .map_err(|_| HarnessError::Closed)?
            .map_err(HarnessError::Request)
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), HarnessError> {
        self.write_value(&notification(method, params)).await
    }

    async fn write_value(&self, value: &Value) -> Result<(), HarnessError> {
        let mut encoded = serde_json::to_vec(value)
            .map_err(|_| HarnessError::Protocol("could not encode request"))?;
        if encoded.len() > MAX_CODEX_FRAME_BYTES {
            return Err(HarnessError::Protocol("request frame is too large"));
        }
        encoded.push(b'\n');
        self.stdin
            .lock()
            .await
            .write_all(&encoded)
            .await
            .map_err(|_| HarnessError::Closed)
    }

    async fn handle_message(self: &Arc<Self>, message: Value) {
        if let Some(id) = message.get("id").and_then(Value::as_u64)
            && (message.get("result").is_some() || message.get("error").is_some())
        {
            if let Some(sender) = self.pending.lock().await.remove(&id) {
                let result = if let Some(error) = message.get("error") {
                    Err(error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Codex request failed")
                        .chars()
                        .take(1_000)
                        .collect())
                } else {
                    Ok(message.get("result").cloned().unwrap_or(Value::Null))
                };
                let _ = sender.send(result);
            }
            return;
        }
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            return;
        };
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        if let Some(rpc_id) = message.get("id").cloned() {
            let request_id = rpc_id
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| rpc_id.to_string());
            if let Some(event) = approval_event(method, &params, request_id.clone()) {
                self.approvals
                    .lock()
                    .await
                    .insert(request_id, PendingApproval { rpc_id });
                self.emit(event).await;
                self.set_state(AgentSessionState::WaitingForApproval).await;
            } else {
                let _ = self
                    .write_value(&json!({
                        "id": rpc_id,
                        "error": {"code": -32601, "message": "unsupported app-server request"}
                    }))
                    .await;
            }
            return;
        }
        if method == "account/login/completed" {
            if params
                .get("success")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                let weak = Arc::downgrade(self);
                tokio::spawn(async move {
                    if let Some(harness) = weak.upgrade()
                        && let Err(error) = harness.start_thread().await
                    {
                        harness.emit_error(error).await;
                    }
                });
            } else {
                self.emit(AgentEventKind::Error {
                    code: "authentication_failed".into(),
                    message: "Codex sign-in did not complete".into(),
                    retryable: true,
                })
                .await;
                self.set_state(AgentSessionState::Failed).await;
            }
        }
        if let Some(event) = normalized_notification(method, &params) {
            self.emit(event).await;
        }
    }

    async fn emit(&self, kind: AgentEventKind) {
        if let AgentEventKind::StateChanged { state } = &kind {
            *self.state.lock().await = *state;
        }
        let cursor = self.next_cursor.fetch_add(1, Ordering::AcqRel) + 1;
        let mut history = self.history.lock().await;
        history.push_back(sanitize_event(AgentEvent { cursor, kind }));
        while history.len() > MAX_HISTORY {
            history.pop_front();
        }
    }

    async fn set_state(&self, state: AgentSessionState) {
        *self.state.lock().await = state;
        self.emit(AgentEventKind::StateChanged { state }).await;
    }

    async fn emit_error(&self, error: HarnessError) {
        self.emit(AgentEventKind::Error {
            code: "harness_error".into(),
            message: error.to_string(),
            retryable: !matches!(error, HarnessError::Protocol(_)),
        })
        .await;
        self.set_state(AgentSessionState::Failed).await;
    }

    async fn fail_pending(&self, message: &str) {
        let mut pending = self.pending.lock().await;
        for (_, sender) in pending.drain() {
            let _ = sender.send(Err(message.to_owned()));
        }
    }
}

async fn read_stdout(mut stdout: ChildStdout, harness: Weak<CodexHarness>) {
    let mut reader = BufReader::new(&mut stdout);
    loop {
        let mut frame = Vec::new();
        let bytes: usize = reader
            .read_until(b'\n', &mut frame)
            .await
            .unwrap_or_default();
        let Some(harness) = harness.upgrade() else {
            return;
        };
        if bytes == 0 {
            harness.fail_pending("harness closed").await;
            harness.set_state(AgentSessionState::Failed).await;
            return;
        }
        if frame.len() > MAX_CODEX_FRAME_BYTES {
            harness
                .emit_error(HarnessError::Protocol("response frame is too large"))
                .await;
            harness.stop().await;
            return;
        }
        if frame.last() == Some(&b'\n') {
            frame.pop();
        }
        match serde_json::from_slice::<Value>(&frame) {
            Ok(message) => harness.handle_message(message).await,
            Err(_) => {
                harness
                    .emit_error(HarnessError::Protocol("invalid JSON response"))
                    .await;
                harness.stop().await;
                return;
            }
        }
    }
}

async fn drain_stderr(mut stderr: ChildStderr) {
    let mut reader = BufReader::new(&mut stderr);
    let mut total = 0usize;
    loop {
        let mut line = Vec::new();
        let Ok(bytes) = reader.read_until(b'\n', &mut line).await else {
            return;
        };
        if bytes == 0 {
            return;
        }
        total = total.saturating_add(bytes);
        if total > MAX_CODEX_FRAME_BYTES {
            return;
        }
    }
}
