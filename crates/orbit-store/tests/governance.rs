use orbit_domain::{GovernancePolicy, GovernanceRule, WorkspaceGovernancePolicy};
use orbit_domain::{PermissionProfile, ResourceLimits, Workspace};
use orbit_store::{StoreError, WorkspaceStore};
use std::path::PathBuf;
use uuid::Uuid;

#[test]
fn governance_default_and_global_roundtrip_reopen() {
    let path = std::env::temp_dir().join(format!("orbit-governance-{}.db", Uuid::new_v4()));
    let store = WorkspaceStore::open(&path).unwrap();
    assert_eq!(
        store.get_governance_policy().unwrap(),
        GovernancePolicy::default()
    );
    let policy = GovernancePolicy {
        guidance: "héllo".into(),
        ..Default::default()
    };
    store.save_governance_policy(&policy).unwrap();
    drop(store);
    assert_eq!(
        WorkspaceStore::open(&path)
            .unwrap()
            .get_governance_policy()
            .unwrap(),
        policy
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn governance_guidance_limit_is_utf8_bytes() {
    let store = WorkspaceStore::in_memory().unwrap();
    let policy = GovernancePolicy {
        guidance: "é".repeat(4001),
        ..Default::default()
    };
    assert!(matches!(
        store.save_governance_policy(&policy),
        Err(StoreError::GuidanceTooLong)
    ));
}

#[test]
fn governance_missing_local_policy_is_default() {
    let store = WorkspaceStore::in_memory().unwrap();
    assert_eq!(
        store.get_workspace_governance(Uuid::new_v4()).unwrap(),
        WorkspaceGovernancePolicy::default()
    );
}

#[test]
fn governance_local_policy_reopen_and_delete_cleanup() {
    let path = std::env::temp_dir().join(format!("orbit-governance-local-{}.db", Uuid::new_v4()));
    let store = WorkspaceStore::open(&path).unwrap();
    let workspace = Workspace::new(
        "governance".into(),
        PathBuf::from("/tmp/governance"),
        PermissionProfile::Workspace,
        ResourceLimits {
            cpus: 1.0,
            memory_bytes: 1,
            pids: 1,
            soft_disk_bytes: 1,
        },
    );
    let id = workspace.id;
    store.upsert(&workspace).unwrap();
    let policy = WorkspaceGovernancePolicy {
        guidance: Some("local".into()),
        ..Default::default()
    };
    store.save_workspace_governance(id, &policy).unwrap();
    drop(store);
    let store = WorkspaceStore::open(&path).unwrap();
    assert_eq!(store.get_workspace_governance(id).unwrap(), policy);
    store.delete_workspace(id).unwrap();
    assert_eq!(
        store.get_workspace_governance(id).unwrap(),
        WorkspaceGovernancePolicy::default()
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn governance_presets_include_builtins_and_reopen_custom() {
    let path = std::env::temp_dir().join(format!("orbit-presets-{}.db", Uuid::new_v4()));
    let store = WorkspaceStore::open(&path).unwrap();
    let preset = store
        .create_governance_preset(
            "  Strict  ".into(),
            GovernancePolicy {
                require_container: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(preset.name, "Strict");
    assert!(
        store
            .list_governance_presets()
            .unwrap()
            .iter()
            .any(|item| item.id == preset.id)
    );
    assert!(matches!(
        store.delete_governance_preset("container"),
        Err(StoreError::BuiltinPreset)
    ));
    drop(store);
    assert!(
        WorkspaceStore::open(&path)
            .unwrap()
            .list_governance_presets()
            .unwrap()
            .iter()
            .any(|item| item.name == "Strict")
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn governance_preset_validates_name_and_guidance() {
    let store = WorkspaceStore::in_memory().unwrap();
    assert!(matches!(
        store.create_governance_preset(" ".into(), GovernancePolicy::default()),
        Err(StoreError::InvalidPresetName)
    ));
    assert!(matches!(
        store.create_governance_preset(
            "x".into(),
            GovernancePolicy {
                guidance: "é".repeat(4001),
                ..Default::default()
            }
        ),
        Err(StoreError::GuidanceTooLong)
    ));
}

#[test]
fn governance_rules_roundtrip_with_workspace_policy() {
    let store = WorkspaceStore::in_memory().unwrap();
    let workspace = Workspace::new(
        "governance rules".into(),
        PathBuf::from("/tmp/governance-rules"),
        PermissionProfile::Workspace,
        ResourceLimits {
            cpus: 1.0,
            memory_bytes: 1,
            pids: 1,
            soft_disk_bytes: 1,
        },
    );
    let workspace_id = workspace.id;
    store.upsert(&workspace).unwrap();

    let rule = store
        .create_governance_rule("  Original title  ".into(), "rule body".into())
        .unwrap();
    assert_eq!(store.list_governance_rules().unwrap().len(), 1);
    let rule = store
        .update_governance_rule(&rule.id, "Updated title".into(), rule.body.clone())
        .unwrap();
    let policy = WorkspaceGovernancePolicy {
        applied_rule_ids: vec![rule.id.clone()],
        custom_rules: vec![GovernanceRule {
            id: "custom-rule".into(),
            title: "Custom rule".into(),
            body: "custom body".into(),
        }],
        ..Default::default()
    };
    store
        .save_workspace_governance(workspace_id, &policy)
        .unwrap();
    assert_eq!(
        store.get_workspace_governance(workspace_id).unwrap(),
        policy
    );

    store.delete_governance_rule(&rule.id).unwrap();
    assert!(store.list_governance_rules().unwrap().is_empty());
}
