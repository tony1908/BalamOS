use orbit_daemon::agent_manager::docker_exec_args;
use orbit_daemon::codex_auth::bootstrap_codex_auth;
use orbit_daemon::codex_harness::CodexHarness;
use orbit_domain::{AgentEventKind, AgentSessionState, ApprovalDecision, PermissionProfile};
use std::ffi::OsStr;
use tokio::process::Command;
use uuid::Uuid;

#[test]
fn desktop_control_instructions_are_named_and_discoverable() {
    assert!(
        orbit_daemon::codex_harness::DESKTOP_CONTROL_INSTRUCTIONS.contains("orbit-desktop-control")
    );
    assert!(orbit_daemon::codex_harness::DESKTOP_CONTROL_INSTRUCTIONS.contains("orbitctl --help"));
}

const FAKE_SERVER: &str = r#"
import json, sys

def send(value):
    print(json.dumps(value), flush=True)

for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    request_id = message.get("id")
    if method == "initialize":
        send({"id": request_id, "result": {"platformOs": "linux", "platformFamily": "unix", "codexHome": "/config/.codex", "userAgent": "fake"}})
    elif method == "account/read":
        send({"id": request_id, "result": {"account": {"type": "chatgpt"}, "requiresOpenaiAuth": True}})
    elif method == "thread/start":
        assert message["params"]["cwd"] == "/workspace"
        assert message["params"]["sandbox"] == "workspace-write"
        assert message["params"]["approvalPolicy"] == "on-request"
        instructions = message["params"]["developerInstructions"]
        assert "orbit-desktop-control" in instructions
        assert "orbitctl --help" in instructions
        send({"id": request_id, "result": {"thread": {"id": "thread-1"}}})
    elif method == "turn/start":
        send({"id": request_id, "result": {"turn": {"id": "turn-1"}}})
        send({"method": "turn/started", "params": {"threadId": "thread-1", "turn": {"id": "turn-1"}}})
        send({"method": "item/agentMessage/delta", "params": {"threadId": "thread-1", "turnId": "turn-1", "itemId": "message-1", "delta": "hello from Codex"}})
        send({"id": "approval-rpc", "method": "item/commandExecution/requestApproval", "params": {"threadId": "thread-1", "turnId": "turn-1", "itemId": "command-1", "startedAtMs": 1, "command": "touch verified"}})
    elif request_id == "approval-rpc" and message.get("result"):
        assert message["result"]["decision"] == "acceptForSession"
        send({"method": "turn/completed", "params": {"threadId": "thread-1", "turn": {"id": "turn-1", "status": "completed"}}})
    elif method == "turn/interrupt":
        send({"id": request_id, "result": {}})
"#;

const LOGIN_SERVER: &str = r#"
import json, sys
def send(value): print(json.dumps(value), flush=True)
for line in sys.stdin:
    message = json.loads(line)
    method = message.get("method")
    if method == "initialize":
        send({"id": message["id"], "result": {"platformOs": "linux"}})
    elif method == "account/read":
        send({"id": message["id"], "result": {"account": None, "requiresOpenaiAuth": True}})
    elif method == "account/login/start":
        assert message["params"]["type"] == "chatgptDeviceCode"
        send({"id": message["id"], "result": {"verificationUrl": "https://example.invalid", "userCode": "ABCD"}})
"#;

#[tokio::test]
async fn missing_account_requests_device_login_without_starting_thread() {
    let mut command = Command::new("python3");
    command.args(["-u", "-c", LOGIN_SERVER]);
    let harness = CodexHarness::spawn(
        command,
        Uuid::new_v4(),
        PermissionProfile::Workspace,
        vec![],
    )
    .await
    .unwrap();
    assert_eq!(
        harness.view().await.state,
        AgentSessionState::NeedsAuthentication
    );
    let page = harness.poll(0).await;
    assert!(
        page.events
            .iter()
            .any(|event| matches!(&event.kind, AgentEventKind::AuthenticationRequired { .. }))
    );
    assert!(harness.send_prompt("must not start".into()).await.is_err());
    harness.stop().await;
}

#[tokio::test]
async fn app_server_transport_initializes_streams_resolves_approval_and_stops() {
    let mut command = Command::new("python3");
    command.args(["-u", "-c", FAKE_SERVER]);
    let harness = CodexHarness::spawn(
        command,
        Uuid::new_v4(),
        PermissionProfile::Workspace,
        vec![],
    )
    .await
    .unwrap();
    assert_eq!(harness.view().await.state, AgentSessionState::Idle);

    harness.send_prompt("say hello".into()).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let first = harness.poll(0).await;
    assert!(first.events.iter().any(|event| matches!(
        &event.kind,
        AgentEventKind::AssistantDelta { text } if text == "hello from Codex"
    )));
    assert!(first.events.iter().any(|event| matches!(
        &event.kind,
        AgentEventKind::ApprovalRequested { request_id, .. } if request_id == "approval-rpc"
    )));
    assert_eq!(
        harness.view().await.state,
        AgentSessionState::WaitingForApproval
    );

    harness
        .resolve_approval("approval-rpc", ApprovalDecision::AcceptForSession)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(harness.view().await.state, AgentSessionState::Completed);
    harness.stop().await;
}

#[tokio::test]
#[ignore]
async fn codex_auth_container_smoke() {
    let id = std::env::var("ORBIT_CODEX_SMOKE_CONTAINER").expect("ORBIT_CODEX_SMOKE_CONTAINER");
    bootstrap_codex_auth(OsStr::new("docker"), &id)
        .await
        .unwrap();
    let mut command = Command::new("docker");
    command.args(docker_exec_args(&id, PermissionProfile::Workspace));
    let harness = CodexHarness::spawn(
        command,
        Uuid::new_v4(),
        PermissionProfile::Workspace,
        vec![],
    )
    .await
    .unwrap();
    assert_eq!(harness.view().await.state, AgentSessionState::Idle);
    let page = harness.poll(0).await;
    assert!(
        !page
            .events
            .iter()
            .any(|event| matches!(event.kind, AgentEventKind::AuthenticationRequired { .. }))
    );
    harness.stop().await;
}
