use orbit_domain::{
    AgentEvent, AgentEventBatch, AgentEventKind, AgentSessionState, AgentSessionView,
    ApprovalDecision, RedactedSecret, RedactedUrl,
};
use orbit_protocol::{Command, ResponseBody};
use uuid::Uuid;

#[test]
fn agent_commands_and_responses_round_trip_with_snake_case_tags() {
    let workspace_id = Uuid::new_v4();
    let command = Command::ResolveAgentApproval {
        workspace_id,
        request_id: "approval-1".into(),
        decision: ApprovalDecision::AcceptForSession,
    };
    let json = serde_json::to_string(&command).unwrap();
    assert!(json.contains("resolve_agent_approval"));
    assert!(json.contains("accept_for_session"));
    assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);

    let body = ResponseBody::AgentEvents(AgentEventBatch {
        workspace_id,
        after: 0,
        next_cursor: 1,
        events: vec![AgentEvent {
            cursor: 1,
            kind: AgentEventKind::AssistantDelta {
                text: "hello".into(),
            },
        }],
        has_more: false,
    });
    assert_eq!(
        serde_json::from_str::<ResponseBody>(&serde_json::to_string(&body).unwrap()).unwrap(),
        body
    );
}

#[test]
fn every_agent_state_serializes_and_auth_urls_are_debug_redacted() {
    for state in [
        AgentSessionState::Starting,
        AgentSessionState::NeedsAuthentication,
        AgentSessionState::Idle,
        AgentSessionState::Running,
        AgentSessionState::WaitingForApproval,
        AgentSessionState::Interrupted,
        AgentSessionState::Completed,
        AgentSessionState::Failed,
    ] {
        let view = AgentSessionView {
            workspace_id: Uuid::new_v4(),
            thread_id: None,
            state,
            next_cursor: 0,
        };
        let json = serde_json::to_string(&view).unwrap();
        assert_eq!(
            serde_json::from_str::<AgentSessionView>(&json).unwrap(),
            view
        );
    }

    let secret = "https://auth.example.test/device?token=super-secret";
    let event = AgentEvent {
        cursor: 1,
        kind: AgentEventKind::AuthenticationRequired {
            url: RedactedUrl(secret.into()),
            user_code: Some(RedactedSecret("ABCD-EFGH".into())),
        },
    };
    assert!(serde_json::to_string(&event).unwrap().contains(secret));
    assert!(!format!("{event:?}").contains("super-secret"));
    assert!(!format!("{event:?}").contains("ABCD-EFGH"));
    assert!(format!("{event:?}").contains("[REDACTED]"));
}
