use crate::runtime::*;
use async_trait::async_trait;
use std::{
    ffi::OsString,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{process::Command, time::timeout};

pub const INSPECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const MUTATION_TIMEOUT: Duration = Duration::from_secs(30);
pub const PULL_TIMEOUT: Duration = Duration::from_secs(30 * 60);

fn valid_operand(value: &str, kind: &str) -> Result<(), RuntimeError> {
    if value.is_empty() || value.starts_with('-') || value.chars().any(|c| c.is_control()) {
        return Err(RuntimeError::InvalidInput(format!("invalid {kind}")));
    }
    Ok(())
}
fn missing_container(stderr: &str, target: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    let t = target.to_ascii_lowercase();
    s.contains(&format!("no such container: {t}")) || s.contains(&format!("no such object: {t}"))
}
fn missing_volume(stderr: &str, target: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    let t = target.to_ascii_lowercase();
    s.contains(&format!("no such volume: {t}")) || s.contains(&format!("get {t}: no such volume"))
}
fn missing_image(stderr: &str, target: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    let t = target.to_ascii_lowercase();
    s.contains(&format!("no such image: {t}")) || s.contains(&format!("no such object: {t}"))
}
pub fn sanitize_stderr(stderr: &str, sensitive: &[&str]) -> String {
    let mut result = stderr.to_owned();
    for secret in sensitive.iter().filter(|s| !s.is_empty()) {
        result = result.replace(secret, "[REDACTED]");
        result = result.replace(&format!("PASSWORD={secret}"), "PASSWORD=[REDACTED]");
    }
    result
}

pub fn build_create_args(s: &ContainerSpec) -> Vec<OsString> {
    let mut a = vec![
        "create".into(),
        "--name".into(),
        s.container_name.clone().into(),
    ];
    let mut labels = s.labels.clone();
    labels.insert("com.orbit.managed".into(), "true".into());
    labels.insert("com.orbit.workspace-id".into(), s.workspace_id.to_string());
    labels.insert("com.orbit.schema".into(), "1".into());
    labels.insert("com.orbit.image".into(), "webtop-ubuntu-xfce-v2".into());
    for (k, v) in &labels {
        a.extend(["--label".into(), format!("{k}={v}").into()]);
    }
    a.extend([
        "--cpus".into(),
        s.resources.cpus.to_string().into(),
        "--memory".into(),
        s.resources.memory_bytes.to_string().into(),
        "--pids-limit".into(),
        s.resources.pids.to_string().into(),
        "--shm-size=512m".into(),
        "--restart=no".into(),
        "--security-opt=no-new-privileges:true".into(),
        "--network".into(),
        if matches!(s.profile, orbit_domain::PermissionProfile::Observe) {
            "none"
        } else {
            "bridge"
        }
        .into(),
        "--env".into(),
        "PUID=1000".into(),
        "--env".into(),
        "PGID=1000".into(),
        "--env".into(),
        "TZ=America/Mexico_City".into(),
        "--env".into(),
        "CUSTOM_USER=agent".into(),
        "--env".into(),
        format!("PASSWORD={}", s.webtop_password).into(),
        "--mount".into(),
        format!("type=volume,src={},dst=/config", s.volume_name).into(),
        "-v".into(),
        format!(
            "{}:/workspace{}",
            s.host_path.to_str().unwrap_or(""),
            if matches!(s.profile, orbit_domain::PermissionProfile::Observe) {
                ":ro"
            } else {
                ""
            }
        )
        .into(),
        "--publish".into(),
        "127.0.0.1::3000".into(),
        s.image_ref.clone().into(),
    ]);
    a
}

#[derive(Debug)]
pub struct CommandOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[async_trait]
pub(crate) trait CommandExecutor: Send + Sync {
    async fn execute(
        &self,
        binary: &Path,
        args: &[OsString],
        timeout: Duration,
    ) -> Result<CommandOutput, RuntimeError>;
}

struct TokioCommandExecutor;

#[async_trait]
impl CommandExecutor for TokioCommandExecutor {
    async fn execute(
        &self,
        binary: &Path,
        args: &[OsString],
        timeout_duration: Duration,
    ) -> Result<CommandOutput, RuntimeError> {
        let child = Command::new(binary)
            .args(args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .output();
        let output = timeout(timeout_duration, child)
            .await
            .map_err(|_| RuntimeError::Command("command timed out".into()))?
            .map_err(|e| RuntimeError::Unavailable(e.to_string()))?;
        Ok(CommandOutput {
            success: output.status.success(),
            code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

fn required_str(v: &serde_json::Value, path: &str) -> Result<String, RuntimeError> {
    v.pointer(path)
        .and_then(|x| x.as_str())
        .map(str::to_owned)
        .ok_or_else(|| RuntimeError::InvalidOutput(format!("missing {path}")))
}

pub struct DockerCliRuntime {
    binary: OsString,
    executor: Arc<dyn CommandExecutor>,
    image_context: PathBuf,
}
impl DockerCliRuntime {
    pub const ORBIT_WEBTOP_IMAGE: &'static str = "orbit-webtop:0.4.0";
    pub const UPSTREAM_WEBTOP_IMAGE: &'static str = "lscr.io/linuxserver/webtop:ubuntu-xfce@sha256:1bd141d5d7aaf3e98e47b7d9665f50657d1628617b4ef47bc3bbd43d726fd77e";
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            binary: path.as_ref().as_os_str().to_os_string(),
            executor: Arc::new(TokioCommandExecutor),
            image_context: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../images/orbit-webtop"),
        }
    }
    #[cfg(test)]
    pub(crate) fn with_executor(
        path: impl AsRef<Path>,
        executor: Arc<dyn CommandExecutor>,
    ) -> Self {
        Self {
            binary: path.as_ref().as_os_str().to_os_string(),
            executor,
            image_context: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../images/orbit-webtop"),
        }
    }
    async fn run(
        &self,
        args: Vec<OsString>,
        sensitive: &[&str],
        timeout_duration: Duration,
    ) -> Result<(bool, String), RuntimeError> {
        let result = self
            .executor
            .execute(Path::new(&self.binary), &args, timeout_duration)
            .await?;
        let text = String::from_utf8_lossy(&result.stdout).to_string();
        if result.success {
            Ok((true, text))
        } else {
            let err = String::from_utf8_lossy(&result.stderr).to_string();
            Err(RuntimeError::Command(sanitize_stderr(
                err.trim(),
                sensitive,
            )))
        }
    }
}

impl Default for DockerCliRuntime {
    fn default() -> Self {
        Self::new("docker")
    }
}

#[async_trait]
impl ContainerRuntime for DockerCliRuntime {
    async fn prerequisite(&self) -> Result<RuntimePrerequisite, RuntimeError> {
        match self
            .run(
                vec![
                    "version".into(),
                    "--format".into(),
                    "{{.Server.Version}}".into(),
                ],
                &[],
                INSPECT_TIMEOUT,
            )
            .await
        {
            Ok((_, v)) => Ok(RuntimePrerequisite {
                available: true,
                version: Some(v.trim().into()),
                message: "Docker available".into(),
            }),
            Err(RuntimeError::Unavailable(e)) | Err(RuntimeError::Command(e)) => {
                Ok(RuntimePrerequisite {
                    available: false,
                    version: None,
                    message: e,
                })
            }
            Err(e) => Err(e),
        }
    }
    async fn ensure_image(&self, image: &str) -> Result<(), RuntimeError> {
        valid_operand(image, "image reference")?;
        match self
            .run(
                vec!["image".into(), "inspect".into(), image.into()],
                &[],
                INSPECT_TIMEOUT,
            )
            .await
        {
            Ok(_) => {}
            Err(RuntimeError::Command(e)) if missing_image(&e, image) => {
                if image == Self::ORBIT_WEBTOP_IMAGE {
                    self.run(
                        vec![
                            "build".into(),
                            "--pull=false".into(),
                            "--tag".into(),
                            image.into(),
                            self.image_context.as_os_str().to_owned(),
                        ],
                        &[],
                        PULL_TIMEOUT,
                    )
                    .await?;
                } else {
                    self.run(vec!["pull".into(), image.into()], &[], PULL_TIMEOUT)
                        .await?;
                }
            }
            Err(e) => return Err(e),
        }
        Ok(())
    }
    async fn create(&self, s: &ContainerSpec) -> Result<ContainerInfo, RuntimeError> {
        valid_operand(&s.container_name, "container name")?;
        valid_operand(&s.volume_name, "volume name")?;
        valid_operand(&s.image_ref, "image reference")?;
        if s.host_path.to_str().is_none() {
            return Err(RuntimeError::InvalidInput("host path is not UTF-8".into()));
        }
        let a = build_create_args(s);
        let (_, out) = self.run(a, &[&s.webtop_password], MUTATION_TIMEOUT).await?;
        let id = out.trim();
        self.inspect(id).await?.ok_or_else(|| {
            RuntimeError::InvalidOutput("created container missing from inspect".into())
        })
    }
    async fn start(&self, id: &str) -> Result<(), RuntimeError> {
        valid_operand(id, "container id")?;
        self.run(vec!["start".into(), id.into()], &[], MUTATION_TIMEOUT)
            .await
            .map(|_| ())
    }
    async fn exec(&self, id: &str, user: &str, args: &[&str]) -> Result<(), RuntimeError> {
        valid_operand(id, "container id")?;
        let mut command: Vec<OsString> = vec![
            "exec".into(),
            "-u".into(),
            user.into(),
            "-e".into(),
            "HOME=/config".into(),
            id.into(),
        ];
        command.extend(args.iter().map(OsString::from));
        self.run(command, &[], MUTATION_TIMEOUT).await.map(|_| ())
    }
    async fn stop(&self, id: &str) -> Result<(), RuntimeError> {
        valid_operand(id, "container id")?;
        self.run(vec!["stop".into(), id.into()], &[], MUTATION_TIMEOUT)
            .await
            .map(|_| ())
    }
    async fn remove(&self, id: &str) -> Result<(), RuntimeError> {
        valid_operand(id, "container id")?;
        self.run(
            vec!["rm".into(), "--force".into(), id.into()],
            &[],
            MUTATION_TIMEOUT,
        )
        .await
        .or_else(|e| {
            if matches!(&e, RuntimeError::Command(msg) if missing_container(msg, id)) {
                Ok((true, String::new()))
            } else {
                Err(e)
            }
        })
        .map(|_| ())
    }
    async fn remove_volume(&self, n: &str) -> Result<(), RuntimeError> {
        valid_operand(n, "volume name")?;
        self.run(
            vec!["volume".into(), "rm".into(), n.into()],
            &[],
            MUTATION_TIMEOUT,
        )
        .await
        .or_else(|e| {
            if matches!(&e, RuntimeError::Command(msg) if missing_volume(msg, n)) {
                Ok((true, String::new()))
            } else {
                Err(e)
            }
        })
        .map(|_| ())
    }
    async fn inspect(&self, id: &str) -> Result<Option<ContainerInfo>, RuntimeError> {
        valid_operand(id, "container id")?;
        let (_, o) = match self
            .run(vec!["inspect".into(), id.into()], &[], INSPECT_TIMEOUT)
            .await
        {
            Ok(x) => x,
            Err(RuntimeError::Command(e)) if missing_container(&e, id) => return Ok(None),
            Err(e) => return Err(e),
        };
        let v: serde_json::Value =
            serde_json::from_str(&o).map_err(|e| RuntimeError::InvalidOutput(e.to_string()))?;
        let arr = v
            .as_array()
            .ok_or_else(|| RuntimeError::InvalidOutput("inspect is not array".into()))?;
        if arr.is_empty() {
            return Err(RuntimeError::InvalidOutput("empty inspect".into()));
        }
        let x = &v[0];
        let actual_id = required_str(x, "/Id")?;
        let name = required_str(x, "/Name")?.trim_start_matches('/').to_owned();
        let image = required_str(x, "/Config/Image")?;
        let status = required_str(x, "/State/Status")?;
        let running = x
            .pointer("/State/Running")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| RuntimeError::InvalidOutput("missing /State/Running".into()))?;
        let labels = x
            .pointer("/Config/Labels")
            .and_then(|v| v.as_object())
            .ok_or_else(|| RuntimeError::InvalidOutput("missing labels".into()))?
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_owned())))
            .collect();
        let mounts =
            x.pointer("/Mounts")
                .and_then(|v| v.as_array())
                .ok_or_else(|| RuntimeError::InvalidOutput("missing mounts".into()))?
                .iter()
                .map(|m| {
                    Ok(MountInfo {
                        source: required_str(m, "/Source")?,
                        destination: required_str(m, "/Destination")?,
                        read_only: !m.pointer("/RW").and_then(|v| v.as_bool()).ok_or_else(
                            || RuntimeError::InvalidOutput("missing mount RW".into()),
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, RuntimeError>>()?;
        let published_port = x
            .get("NetworkSettings")
            .and_then(|v| v.get("Ports"))
            .and_then(|v| v.get("3000/tcp"));
        let published_port = match published_port {
            None | Some(serde_json::Value::Null) => None,
            Some(ports) => {
                let binding = ports
                    .as_array()
                    .and_then(|ports| ports.first())
                    .ok_or_else(|| RuntimeError::InvalidOutput("invalid published port".into()))?;
                let host_ip = binding
                    .get("HostIp")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| RuntimeError::InvalidOutput("invalid published host".into()))?;
                if !host_ip
                    .parse::<IpAddr>()
                    .map(|ip| ip.is_loopback())
                    .unwrap_or(false)
                {
                    return Err(RuntimeError::InvalidOutput(
                        "published port is not loopback".into(),
                    ));
                }
                Some(
                    binding
                        .get("HostPort")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            RuntimeError::InvalidOutput("invalid published port".into())
                        })?
                        .parse()
                        .map_err(|_| {
                            RuntimeError::InvalidOutput("invalid published port".into())
                        })?,
                )
            }
        };
        Ok(Some(ContainerInfo {
            id: actual_id,
            name,
            status,
            running,
            published_port,
            image,
            labels,
            mounts,
        }))
    }
    async fn managed(&self) -> Result<Vec<ContainerInfo>, RuntimeError> {
        let (_, o) = self
            .run(
                vec![
                    "ps".into(),
                    "-a".into(),
                    "--filter".into(),
                    "label=com.orbit.managed=true".into(),
                    "--format".into(),
                    "{{.ID}}".into(),
                ],
                &[],
                INSPECT_TIMEOUT,
            )
            .await?;
        let mut r = Vec::new();
        for id in o.lines().filter(|x| !x.is_empty()) {
            if let Some(c) = self.inspect(id).await? {
                r.push(c)
            }
        }
        Ok(r)
    }
    async fn healthy(&self, port: u16, user: &str, password: &str) -> Result<bool, RuntimeError> {
        let c = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .map_err(|e| RuntimeError::HealthProbe(e.to_string()))?;
        Ok(c.get(format!("http://127.0.0.1:{port}/"))
            .basic_auth(user, Some(password))
            .send()
            .await
            .map(|r| r.status().is_success() || r.status().is_redirection())
            .unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, ffi::OsString, sync::Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct ScriptedExecutor {
        responses: Mutex<VecDeque<Result<CommandOutput, RuntimeError>>>,
        calls: Mutex<Vec<(OsString, Vec<OsString>, Duration)>>,
    }
    impl ScriptedExecutor {
        fn new(responses: Vec<Result<CommandOutput, RuntimeError>>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses.into()),
                calls: Mutex::new(Vec::new()),
            })
        }
    }
    #[async_trait]
    impl CommandExecutor for ScriptedExecutor {
        async fn execute(
            &self,
            binary: &Path,
            args: &[OsString],
            timeout: Duration,
        ) -> Result<CommandOutput, RuntimeError> {
            self.calls.lock().unwrap().push((
                binary.as_os_str().to_owned(),
                args.to_vec(),
                timeout,
            ));
            self.responses
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted response")
        }
    }
    fn ok(s: &str) -> Result<CommandOutput, RuntimeError> {
        Ok(CommandOutput {
            success: true,
            code: Some(0),
            stdout: s.into(),
            stderr: Vec::new(),
        })
    }
    fn err(s: &str) -> Result<CommandOutput, RuntimeError> {
        Ok(CommandOutput {
            success: false,
            code: Some(1),
            stdout: Vec::new(),
            stderr: s.into(),
        })
    }
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    fn inspect_json(id: &str, port: u16) -> String {
        serde_json::json!([{"Id": id, "Name": "/orbit-test", "Config": {"Image": "webtop:test", "Labels": {"com.orbit.managed": "true", "role": "test"}}, "State": {"Status": "running", "Running": true}, "Mounts": [{"Source": "/host", "Destination": "/workspace", "RW": true}, {"Source": "vol", "Destination": "/config", "RW": false}], "NetworkSettings": {"Ports": {"3000/tcp": [{"HostIp": "127.0.0.1", "HostPort": port.to_string()}]}}}]).to_string()
    }

    fn spec(password: &str) -> ContainerSpec {
        ContainerSpec {
            workspace_id: uuid::Uuid::new_v4(),
            container_name: "orbit-test".into(),
            volume_name: "orbit-volume".into(),
            host_path: "/host".into(),
            profile: orbit_domain::PermissionProfile::Workspace,
            resources: orbit_domain::ResourceLimits::new(1.0, 1024, 64, 1024).unwrap(),
            webtop_password: password.into(),
            image_ref: "webtop:test".into(),
            labels: Default::default(),
        }
    }

    #[test]
    fn build_create_args_preserves_values_as_exact_argv_elements_and_fixed_contract() {
        let mut s = spec("p a'ss;$(whoami)");
        s.container_name = "name ; \"quoted\" $(touch pwned)".into();
        s.host_path = "/tmp/a path/'quote';$(x)".into();
        s.labels.insert("com.orbit.managed".into(), "false".into());
        s.labels.insert("com.orbit.schema".into(), "99".into());
        s.labels.insert("user".into(), "value;$(x)".into());
        let a = build_create_args(&s);
        let strings: Vec<&str> = a.iter().map(|x| x.to_str().unwrap()).collect();
        assert_eq!(strings[0], "create");
        assert_eq!(strings[2], s.container_name);
        assert!(strings.contains(&"--label"));
        assert!(strings.contains(&"com.orbit.managed=true"));
        assert!(strings.contains(&"com.orbit.schema=1"));
        assert!(strings.contains(&"com.orbit.image=webtop-ubuntu-xfce-v2"));
        assert!(strings.contains(&"--cpus") && strings.contains(&"1"));
        assert!(strings.contains(&"--memory") && strings.contains(&"1024"));
        assert!(strings.contains(&"--pids-limit") && strings.contains(&"64"));
        assert!(strings.contains(&"--shm-size=512m"));
        assert!(strings.contains(&"--restart=no"));
        assert!(strings.contains(&"--security-opt=no-new-privileges:true"));
        assert!(strings.contains(&"PUID=1000") && strings.contains(&"PGID=1000"));
        assert!(
            strings.contains(&"TZ=America/Mexico_City") && strings.contains(&"CUSTOM_USER=agent")
        );
        assert!(strings.contains(&"PASSWORD=p a'ss;$(whoami)"));
        assert!(strings.contains(&"type=volume,src=orbit-volume,dst=/config"));
        assert!(strings.contains(&"/tmp/a path/'quote';$(x):/workspace"));
        assert!(strings.contains(&"--network") && strings.contains(&"bridge"));
        assert!(strings.contains(&"--publish") && strings.contains(&"127.0.0.1::3000"));
        assert_eq!(strings.last().copied(), Some(s.image_ref.as_str()));
        assert!(!strings.iter().any(|x| matches!(*x, "sh" | "bash" | "-c")));
        assert!(
            !strings
                .iter()
                .any(|x| x.contains(" && ") || x.contains("; docker"))
        );
    }

    #[test]
    fn observe_create_args_use_none_network_and_readonly_bind() {
        let mut s = spec("secret");
        s.profile = orbit_domain::PermissionProfile::Observe;
        let strings: Vec<String> = build_create_args(&s)
            .into_iter()
            .map(|x| x.to_string_lossy().into())
            .collect();
        assert!(strings.contains(&"none".into()));
        assert!(strings.iter().any(|x| x == "/host:/workspace:ro"));
    }

    #[tokio::test]
    async fn create_returns_inspected_info_and_records_create_then_inspect() {
        let e = ScriptedExecutor::new(vec![
            ok("created-id\n"),
            ok(&inspect_json("actual-id", 4321)),
        ]);
        let r = DockerCliRuntime::with_executor("docker", e.clone());
        let info = r.create(&spec("secret")).await.unwrap();
        assert_eq!(info.id, "actual-id");
        assert_eq!(info.published_port, Some(4321));
        assert_eq!(info.mounts.len(), 2);
        assert_eq!(info.labels.get("role").map(String::as_str), Some("test"));
        let calls = e.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].1.first().unwrap(), "create");
        assert_eq!(calls[1].1, args(&["inspect", "created-id"]));
    }

    #[tokio::test]
    async fn create_redacts_password_in_command_error() {
        let e = ScriptedExecutor::new(vec![err("PASSWORD=secret leaked: secret")]);
        let result = DockerCliRuntime::with_executor("docker", e)
            .create(&spec("secret"))
            .await;
        assert!(
            matches!(result, Err(RuntimeError::Command(message)) if !message.contains("secret") && message.contains("[REDACTED]"))
        );
    }

    #[tokio::test]
    async fn managed_lists_and_inspects_ids_ignoring_blank_lines() {
        let e = ScriptedExecutor::new(vec![
            ok("first\n\nsecond\n"),
            ok(&inspect_json("first", 4001)),
            ok(&inspect_json("second", 4002)),
        ]);
        let r = DockerCliRuntime::with_executor("docker", e.clone());
        let infos = r.managed().await.unwrap();
        assert_eq!(infos.len(), 2);
        assert_eq!(infos[0].id, "first");
        assert_eq!(infos[1].id, "second");
        let calls = e.calls.lock().unwrap();
        assert_eq!(
            calls[0].1,
            args(&[
                "ps",
                "-a",
                "--filter",
                "label=com.orbit.managed=true",
                "--format",
                "{{.ID}}"
            ])
        );
        assert_eq!(calls[1].1, args(&["inspect", "first"]));
        assert_eq!(calls[2].1, args(&["inspect", "second"]));
    }

    #[tokio::test]
    async fn healthy_sends_basic_auth_and_host_and_accepts_success() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 2048];
            let n = stream.read(&mut request).await.unwrap();
            let request = String::from_utf8_lossy(&request[..n]);
            assert!(request.contains("authorization: Basic YWdlbnQ6c2VjcmV0"));
            assert!(
                request
                    .lines()
                    .any(|line| line.to_ascii_lowercase().starts_with("host:"))
            );
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        assert!(
            DockerCliRuntime::default()
                .healthy(port, "agent", "secret")
                .await
                .unwrap()
        );
        task.await.unwrap();
    }

    #[tokio::test]
    async fn healthy_returns_false_for_unauthorized_and_server_error() {
        for status in [401, 500] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let task = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0; 512];
                let _ = stream.read(&mut request).await.unwrap();
                let response = format!("HTTP/1.1 {status} Error\r\nContent-Length: 0\r\n\r\n");
                stream.write_all(response.as_bytes()).await.unwrap();
            });
            assert!(
                !DockerCliRuntime::default()
                    .healthy(port, "agent", "secret")
                    .await
                    .unwrap()
            );
            task.await.unwrap();
        }
    }

    #[tokio::test]
    async fn healthy_returns_false_when_port_is_unreachable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        assert!(
            !DockerCliRuntime::default()
                .healthy(port, "agent", "secret")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn prerequisite_reports_unavailable_and_version() {
        let e = ScriptedExecutor::new(vec![Err(RuntimeError::Unavailable(
            "missing docker".into(),
        ))]);
        let r = DockerCliRuntime::with_executor("docker", e)
            .prerequisite()
            .await
            .unwrap();
        assert!(!r.available);
        assert!(r.message.contains("missing docker"));
        let e = ScriptedExecutor::new(vec![ok("27.1\n")]);
        let r = DockerCliRuntime::with_executor("docker", e)
            .prerequisite()
            .await
            .unwrap();
        assert!(r.available);
        assert_eq!(r.version.as_deref(), Some("27.1"));
    }

    #[tokio::test]
    async fn ensure_image_inspects_then_pulls_only_when_needed() {
        let e = ScriptedExecutor::new(vec![ok("[]")]);
        DockerCliRuntime::with_executor("docker", e.clone())
            .ensure_image("img")
            .await
            .unwrap();
        assert_eq!(e.calls.lock().unwrap().len(), 1);
        let e = ScriptedExecutor::new(vec![
            err("Error response from daemon: No such image: img"),
            ok("pulled"),
        ]);
        DockerCliRuntime::with_executor("docker", e.clone())
            .ensure_image("img")
            .await
            .unwrap();
        let calls = &e.calls.lock().unwrap();
        assert_eq!(calls[0].1, args(&["image", "inspect", "img"]));
        assert_eq!(calls[1].1, args(&["pull", "img"]));
        assert_eq!(calls[0].2, INSPECT_TIMEOUT);
        assert_eq!(calls[1].2, PULL_TIMEOUT);
    }

    #[tokio::test]
    async fn ensure_orbit_image_builds_the_local_context_instead_of_pulling() {
        let e = ScriptedExecutor::new(vec![
            err("Error response from daemon: No such image: orbit-webtop:0.4.0"),
            ok("built"),
        ]);
        DockerCliRuntime::with_executor("docker", e.clone())
            .ensure_image(DockerCliRuntime::ORBIT_WEBTOP_IMAGE)
            .await
            .unwrap();
        let calls = e.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[1].1[..4],
            args(&["build", "--pull=false", "--tag", "orbit-webtop:0.4.0"])
        );
        assert!(
            calls[1].1[4]
                .to_string_lossy()
                .ends_with("images/orbit-webtop")
        );
        assert_eq!(calls[1].2, PULL_TIMEOUT);
    }

    #[tokio::test]
    async fn start_stop_and_missing_remove_are_exact_and_idempotent() {
        let e = ScriptedExecutor::new(vec![
            ok(""),
            ok(""),
            err("No such container: x"),
            err("No such volume: v"),
        ]);
        let r = DockerCliRuntime::with_executor("docker", e.clone());
        r.start("id").await.unwrap();
        r.stop("id").await.unwrap();
        r.remove("x").await.unwrap();
        r.remove_volume("v").await.unwrap();
        let calls = &e.calls.lock().unwrap();
        assert_eq!(calls[0].1, args(&["start", "id"]));
        assert_eq!(calls[1].1, args(&["stop", "id"]));
        assert_eq!(calls[2].1, args(&["rm", "--force", "x"]));
        assert_eq!(calls[3].1, args(&["volume", "rm", "v"]));
        assert_eq!(calls[1].2, MUTATION_TIMEOUT);
        assert!(calls[1].2 > Duration::from_secs(10));
    }

    #[tokio::test]
    async fn inspect_missing_and_malformed_are_handled() {
        let e = ScriptedExecutor::new(vec![err("No such object: x")]);
        assert!(
            DockerCliRuntime::with_executor("docker", e)
                .inspect("x")
                .await
                .unwrap()
                .is_none()
        );
        let e = ScriptedExecutor::new(vec![ok("not json")]);
        assert!(matches!(
            DockerCliRuntime::with_executor("docker", e)
                .inspect("x")
                .await,
            Err(RuntimeError::InvalidOutput(_))
        ));
    }

    #[tokio::test]
    async fn inspect_allows_absent_or_null_published_port() {
        for ports in [serde_json::json!({}), serde_json::json!({"3000/tcp": null})] {
            let mut value: serde_json::Value =
                serde_json::from_str(&inspect_json("id", 4321)).unwrap();
            value[0]["NetworkSettings"]["Ports"] = ports;
            let e = ScriptedExecutor::new(vec![ok(&value.to_string())]);
            let info = DockerCliRuntime::with_executor("docker", e)
                .inspect("id")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(info.published_port, None);
        }
    }

    #[tokio::test]
    async fn inspect_rejects_non_loopback_published_port() {
        let mut value: serde_json::Value = serde_json::from_str(&inspect_json("id", 4321)).unwrap();
        value[0]["NetworkSettings"]["Ports"]["3000/tcp"][0]["HostIp"] = "0.0.0.0".into();
        let e = ScriptedExecutor::new(vec![ok(&value.to_string())]);
        assert!(matches!(
            DockerCliRuntime::with_executor("docker", e)
                .inspect("id")
                .await,
            Err(RuntimeError::InvalidOutput(_))
        ));
    }
}
