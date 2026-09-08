use orbit_daemon::docker_cli::DockerCliRuntime;
use orbit_daemon::runtime::NetworkMode;
use std::process::Command;

fn orbitctl(args: &[&str]) -> std::process::Output {
    Command::new("bash")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../images/orbit-webtop/orbitctl"
        ))
        .args(args)
        .output()
        .expect("orbitctl should execute")
}

#[test]
fn orbitctl_help_describes_allowlisted_commands() {
    let output = orbitctl(&["--help"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("orbitctl screen capture"));
    assert!(stdout.contains("orbitctl browser open <url>"));
}

#[test]
fn orbitctl_group_help_describes_browser_commands() {
    let output = orbitctl(&["browser", "--help"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("orbitctl browser open <url>"));
}

#[test]
fn orbitctl_describe_emits_json_command_metadata() {
    let output = orbitctl(&["describe"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(value["name"], "orbitctl");
    assert!(
        value["commands"]
            .as_array()
            .unwrap()
            .iter()
            .any(|command| { command["group"] == "browser" && command["action"] == "open" })
    );
}

#[test]
fn orbitctl_unsupported_invocation_prints_help_and_concise_error() {
    let output = orbitctl(&["browser", "nope"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("orbitctl: unsupported command"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("orbitctl browser open <url>"));
}

#[test]
fn runtime_has_pinned_image_and_network_modes() {
    assert_eq!(DockerCliRuntime::ORBIT_WEBTOP_IMAGE, "orbit-webtop:0.4.0");
    assert_eq!(
        DockerCliRuntime::UPSTREAM_WEBTOP_IMAGE,
        "lscr.io/linuxserver/webtop:ubuntu-xfce@sha256:1bd141d5d7aaf3e98e47b7d9665f50657d1628617b4ef47bc3bbd43d726fd77e"
    );
    assert_eq!(NetworkMode::None, NetworkMode::None);
}

#[test]
fn orbit_image_is_derived_from_the_pinned_upstream_and_installs_the_pinned_codex() {
    let dockerfile = include_str!("../../../images/orbit-webtop/Dockerfile");
    assert!(dockerfile.starts_with(&format!(
        "FROM {}\n",
        DockerCliRuntime::UPSTREAM_WEBTOP_IMAGE
    )));
    assert!(dockerfile.contains("ARG CODEX_VERSION=0.149.1"));
    assert!(dockerfile.contains("@openai/codex@${CODEX_VERSION}"));
    assert!(!dockerfile.contains(".codex/auth.json"));
    let init = include_str!("../../../images/orbit-webtop/20-orbit-codex");
    assert!(init.contains("ensure_private_dir /config/.codex"));

    let helper = include_str!("../../../images/orbit-webtop/orbitctl");
    for command in [
        "screen:capture",
        "mouse:click",
        "mouse:move",
        "keyboard:type",
        "keyboard:key",
        "browser:open",
        "process:list",
        "system:status",
    ] {
        assert!(helper.contains(command));
    }
    assert!(!helper.contains("eval "));
}

#[test]
fn orbit_image_bundles_desktop_control_skill_and_init_installs_private_copy() {
    let dockerfile = include_str!("../../../images/orbit-webtop/Dockerfile");
    assert!(dockerfile.contains("COPY skills/orbit-desktop-control/SKILL.md /opt/orbit/skills/orbit-desktop-control/SKILL.md"));
    let init = include_str!("../../../images/orbit-webtop/20-orbit-codex");
    assert!(init.contains("/opt/orbit/skills/orbit-desktop-control/SKILL.md"));
    assert!(init.contains("skill_path=\"$skill_dir/SKILL.md\""));
    assert!(init.contains("ensure_private_dir /config/.codex/skills"));
    assert!(init.contains("-m 0600 -o abc -g abc"));
    let runtime = include_str!("../../../images/orbit-webtop/scripts/test-orbit-image-init.sh");
    assert!(runtime.contains("orbit-webtop:0.4.0"));
    assert!(runtime.contains("symlink"));
}
