use orbit_domain::{
    GovernanceBlockedReason, GovernancePolicy, GovernanceSource, Harness, PermissionProfile,
    WorkspaceGovernancePolicy, resolve_governance,
};

#[test]
fn governance_global_and_local_boolean_resolution_is_restrictive_or_permissive_as_defined() {
    for global in [false, true] {
        for local in [None, Some(false), Some(true)] {
            let policy = GovernancePolicy {
                enabled: global,
                require_approval: global,
                require_container: global,
                allow_scheduled: global,
                ..Default::default()
            };
            let local_policy = WorkspaceGovernancePolicy {
                enabled: local,
                require_approval: local,
                require_container: local,
                allow_scheduled: local,
                ..Default::default()
            };
            let effective = resolve_governance(&policy, &local_policy);
            assert_eq!(effective.enabled, global && local.unwrap_or(true));
            assert_eq!(effective.require_approval, global || local.unwrap_or(false));
            assert_eq!(
                effective.require_container,
                global || local.unwrap_or(false)
            );
            assert_eq!(effective.allow_scheduled, global && local.unwrap_or(true));
            assert!(effective.sources.contains_key("enabled"));
        }
    }
}

#[test]
fn governance_guidance_is_global_then_local_and_sources_are_attributed() {
    let effective = resolve_governance(
        &GovernancePolicy {
            guidance: "global".into(),
            ..Default::default()
        },
        &WorkspaceGovernancePolicy {
            guidance: Some("local".into()),
            ..Default::default()
        },
    );
    assert_eq!(effective.guidance, "[global]\nglobal\n[local]\nlocal");
    assert_eq!(
        effective.sources["guidance_global"],
        GovernanceSource::Global
    );
    assert_eq!(effective.sources["guidance_local"], GovernanceSource::Local);
}

#[test]
fn governance_approval_and_container_matrix_is_explicit() {
    let policy = resolve_governance(
        &GovernancePolicy {
            require_approval: true,
            require_container: true,
            ..Default::default()
        },
        &Default::default(),
    );
    for harness in [
        Harness::Codex,
        Harness::Opencode,
        Harness::ClaudeCode,
        Harness::Antigravity,
    ] {
        for profile in [
            PermissionProfile::Observe,
            PermissionProfile::Workspace,
            PermissionProfile::FullControl,
        ] {
            let reasons = policy.blocked_reasons(harness, profile, true, false);
            assert_eq!(
                reasons.contains(&GovernanceBlockedReason::ContainerRequired),
                harness == Harness::Antigravity
            );
            assert_eq!(
                reasons.contains(&GovernanceBlockedReason::HarnessUnsupportedApproval),
                harness != Harness::Codex || profile == PermissionProfile::FullControl
            );
        }
    }
}

#[test]
fn compose_rule_guidance_picks_applied_and_custom_and_skips_missing() {
    use orbit_domain::{GovernanceRule, compose_rule_guidance};
    let library = vec![
        GovernanceRule {
            id: "a".into(),
            title: "No prod".into(),
            body: "Never touch prod.".into(),
        },
        GovernanceRule {
            id: "b".into(),
            title: "Ask pay".into(),
            body: "Ask before paying.".into(),
        },
    ];
    let custom = vec![GovernanceRule {
        id: "c".into(),
        title: "Repo".into(),
        body: "Stay in /repo.".into(),
    }];
    let out = compose_rule_guidance(&library, &["a".to_string(), "missing".to_string()], &custom);
    assert_eq!(
        out,
        "## No prod\nNever touch prod.\n\n## Repo\nStay in /repo."
    );
    assert_eq!(compose_rule_guidance(&library, &[], &[]), "");
}
