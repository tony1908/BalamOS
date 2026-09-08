use orbit_domain::{Harness, PermissionProfile, ResourceLimits};
use orbit_protocol::{Command, PROTOCOL_VERSION, Request, Response, ResponseBody};
use std::path::PathBuf;
use uuid::Uuid;

#[test]
fn request_round_trips_as_json() {
    let request = Request {
        protocol: PROTOCOL_VERSION,
        id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
        token: "secret".to_owned(),
        command: Command::CreateWorkspace {
            name: "Demo".to_owned(),
            host_path: PathBuf::from("/projects/demo"),
            profile: PermissionProfile::Workspace,
            harness: Harness::Codex,
            model: None,
            reasoning_effort: None,
            resources: ResourceLimits::new(2.0, 1024, 64, 2048).unwrap(),
        },
    };

    let json = serde_json::to_string(&request).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
        "protocol": 5,
            "id": "11111111-1111-4111-8111-111111111111",
            "token": "secret",
            "command": {
                "type": "create_workspace",
                "name": "Demo",
                "host_path": "/projects/demo",
                "profile": "workspace",
                "harness": "codex",
                "model": null,
                "reasoning_effort": null,
                "resources": {
                    "cpus": 2.0,
                    "memory_bytes": 1024,
                    "pids": 64,
                    "soft_disk_bytes": 2048
                }
            }
        })
    );
    let decoded: Request = serde_json::from_value(value).unwrap();
    assert_eq!(decoded, request);
}

#[test]
fn create_workspace_defaults_missing_harness_to_codex() {
    let payload = serde_json::json!({
        "type": "create_workspace",
        "name": "Demo",
        "host_path": "/projects/demo",
        "profile": "workspace",
        "resources": {
            "cpus": 2.0,
            "memory_bytes": 1024,
            "pids": 64,
            "soft_disk_bytes": 2048
        }
    });

    let command: Command = serde_json::from_value(payload).unwrap();
    assert!(matches!(
        command,
        Command::CreateWorkspace {
            harness: Harness::Codex,
            ..
        }
    ));
}

#[test]
fn create_workspace_carries_model_and_reasoning_effort() {
    let command = Command::CreateWorkspace {
        name: "Demo".into(),
        host_path: "/tmp/demo".into(),
        profile: PermissionProfile::Workspace,
        harness: Harness::Codex,
        model: Some("gpt-5-codex".into()),
        reasoning_effort: Some("high".into()),
        resources: ResourceLimits::new(1.0, 1, 1, 1).unwrap(),
    };
    let decoded: Command = serde_json::from_value(serde_json::to_value(command).unwrap()).unwrap();
    assert!(
        matches!(decoded, Command::CreateWorkspace { model: Some(ref model), reasoning_effort: Some(ref effort), .. } if model == "gpt-5-codex" && effort == "high")
    );
}

#[test]
fn create_workspace_deserializes_and_round_trips_opencode_harness() {
    let payload = serde_json::json!({
        "type": "create_workspace",
        "name": "Demo",
        "host_path": "/projects/demo",
        "profile": "workspace",
        "harness": "opencode",
        "resources": {
            "cpus": 2.0,
            "memory_bytes": 1024,
            "pids": 64,
            "soft_disk_bytes": 2048
        }
    });

    let command: Command = serde_json::from_value(payload).unwrap();
    assert!(matches!(
        command,
        Command::CreateWorkspace {
            harness: Harness::Opencode,
            ..
        }
    ));
    let encoded = serde_json::to_string(&command).unwrap();
    assert_eq!(serde_json::from_str::<Command>(&encoded).unwrap(), command);
}

#[test]
fn request_debug_redacts_token() {
    let request = Request {
        protocol: PROTOCOL_VERSION,
        id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
        token: "secret".to_owned(),
        command: Command::Ping,
    };
    let debug = format!("{request:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("secret"));
}

#[test]
fn response_round_trips_as_json() {
    let response = Response {
        protocol: PROTOCOL_VERSION,
        id: Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
        body: ResponseBody::Pong,
    };

    let json = serde_json::to_string(&response).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
        "protocol": 5,
            "id": "22222222-2222-4222-8222-222222222222",
            "body": { "type": "pong" }
        })
    );
    let decoded: Response = serde_json::from_value(value).unwrap();
    assert_eq!(decoded, response);
}

#[test]
fn workspace_view_excludes_runtime_metadata_and_desktop_session_debug_redacts_url() {
    let workspace = orbit_domain::Workspace {
        id: Uuid::new_v4(),
        name: "Demo".into(),
        host_path: "/tmp/demo".into(),
        profile: PermissionProfile::Workspace,
        harness: orbit_domain::Harness::default(),
        model: None,
        reasoning_effort: None,
        apps: Vec::new(),
        resources: ResourceLimits::new(1.0, 1, 1, 1).unwrap(),
        state: orbit_domain::LifecycleState::Running,
        runtime: Some(orbit_domain::RuntimeMetadata {
            container_id: "container-id".into(),
            container_name: "name".into(),
            volume_name: "volume".into(),
            upstream_port: 3000,
            image_ref: "image".into(),
        }),
    };
    let json = serde_json::to_string(&orbit_domain::WorkspaceView::from(workspace)).unwrap();
    for secret in ["container-id", "3000", "volume", "image"] {
        assert!(!json.contains(secret));
    }
    let session = orbit_protocol::DesktopSession {
        url: "http://127.0.0.1:3000/?token=BEARER".into(),
        expires_at_unix_ms: 42,
    };
    assert!(format!("{session:?}").contains("[REDACTED]"));
    assert!(!format!("{session:?}").contains("BEARER"));
    assert_eq!(
        serde_json::from_str::<orbit_protocol::DesktopSession>(
            &serde_json::to_string(&session).unwrap()
        )
        .unwrap(),
        session
    );
}

#[test]
fn app_commands_and_response_round_trip() {
    let id = Uuid::new_v4();
    for command in [
        Command::ListApps,
        Command::SetWorkspaceApps {
            workspace_id: id,
            apps: vec!["metamask".into()],
        },
    ] {
        let decoded: Command =
            serde_json::from_str(&serde_json::to_string(&command).unwrap()).unwrap();
        assert_eq!(decoded, command);
    }
    let response = ResponseBody::Apps(vec![orbit_domain::AppOption {
        id: "obsidian".into(),
        label: "Obsidian".into(),
        description: "Markdown knowledge base (AppImage)".into(),
    }]);
    assert_eq!(
        serde_json::from_str::<ResponseBody>(&serde_json::to_string(&response).unwrap()).unwrap(),
        response
    );
}

#[test]
fn lifecycle_commands_and_runtime_status_round_trip() {
    let id = Uuid::new_v4();
    for command in [
        Command::RuntimeStatus,
        Command::ReconcileRuntime,
        Command::StartWorkspace { id },
        Command::StopWorkspace { id },
        Command::ResetWorkspace { id },
        Command::DeleteWorkspace { id },
        Command::CreateDesktopSession { id },
    ] {
        let encoded = serde_json::to_string(&command).unwrap();
        assert_eq!(serde_json::from_str::<Command>(&encoded).unwrap(), command);
    }
    let status = orbit_protocol::RuntimeStatus {
        available: true,
        mutations_ready: false,
        version: Some("v1".into()),
        message: "ready".into(),
    };
    assert_eq!(
        serde_json::from_str::<orbit_protocol::RuntimeStatus>(
            &serde_json::to_string(&status).unwrap()
        )
        .unwrap(),
        status
    );
}
