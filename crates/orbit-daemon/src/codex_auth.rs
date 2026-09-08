use directories::BaseDirs;
use serde_json::Value;
use std::{
    env,
    ffi::{OsStr, OsString},
    io::Read,
    path::PathBuf,
};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    time::{Duration, timeout},
};

const MAX_AUTH_BYTES: usize = 1024 * 1024;
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(10);

fn source_path() -> Option<PathBuf> {
    env::var_os("CODEX_HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .or_else(|| BaseDirs::new().map(|dirs| dirs.home_dir().join(".codex")))
        .map(|home| home.join("auth.json"))
}

fn valid_cache(bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > MAX_AUTH_BYTES {
        return false;
    }
    let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    let mode = object
        .get("auth_mode")
        .and_then(Value::as_str)
        .map(str::trim);
    let api_key = object
        .get("OPENAI_API_KEY")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty());
    let chatgpt = object
        .get("tokens")
        .and_then(Value::as_object)
        .is_some_and(|tokens| {
            ["access_token", "refresh_token", "id_token", "account_id"]
                .into_iter()
                .all(|key| {
                    tokens
                        .get(key)
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.trim().is_empty())
                })
        });
    match mode {
        Some("chatgpt") => chatgpt,
        Some("api_key" | "apikey" | "openai_api_key") => api_key,
        Some(_) => false,
        None => chatgpt || api_key,
    }
}

fn load_cache(path: &std::path::Path) -> Option<Vec<u8>> {
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    #[cfg(not(unix))]
    if std::fs::symlink_metadata(path)
        .ok()?
        .file_type()
        .is_symlink()
    {
        return None;
    }
    let file = options.open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_AUTH_BYTES as u64 {
        return None;
    }
    let mut bytes = Vec::new();
    file.take((MAX_AUTH_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= MAX_AUTH_BYTES && valid_cache(&bytes)).then_some(bytes)
}

fn valid_container_id(container_id: &str) -> bool {
    !container_id.is_empty()
        && !container_id.starts_with('-')
        && !container_id.chars().any(char::is_control)
}

async fn bootstrap_codex_auth_at(
    binary: &OsStr,
    container_id: &str,
    path: &std::path::Path,
    destination: &str,
    timeout_duration: Duration,
) -> Result<(), &'static str> {
    if !valid_container_id(container_id) {
        return Err("Codex auth bootstrap failed");
    }
    let Some(bytes) = load_cache(path) else {
        return Ok(());
    };

    let script = r#"set -eu
umask 077
base=$1
test -d "$base" && test ! -L "$base"
test -d "$base/.codex" && test ! -L "$base/.codex"
cd -P "$base/.codex"
if test -L auth.json; then exit 1; fi
if test -e auth.json; then test -f auth.json; cat > /dev/null; exit 0; fi
tmp=$(mktemp .auth.json.orbit.XXXXXX)
trap 'rm -f "$tmp"' EXIT
cat > "$tmp"
chown abc:abc "$tmp"
chmod 600 "$tmp"
if ln -T "$tmp" auth.json 2>/dev/null; then exit 0; fi
if test -L auth.json || ! test -f auth.json; then exit 1; fi
"#;
    let mut command = Command::new(binary);
    command.args([
        OsString::from("exec"),
        OsString::from("-i"),
        OsString::from("-u"),
        OsString::from("root"),
        OsString::from(container_id),
        OsString::from("sh"),
        OsString::from("-c"),
        OsString::from(script),
        OsString::from("orbit-codex-auth"),
        OsString::from(destination),
    ]);
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| "Codex auth bootstrap failed")?;
    let result = timeout(timeout_duration, async {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(&bytes).await?;
        }
        child.wait().await
    })
    .await;
    match result {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(_)) => Err("Codex auth bootstrap failed"),
        Ok(Err(_)) => {
            let _ = timeout(Duration::from_secs(1), child.wait()).await;
            Err("Codex auth bootstrap failed")
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = timeout(Duration::from_secs(1), child.wait()).await;
            Err("Codex auth bootstrap timed out")
        }
    }
}

pub async fn bootstrap_codex_auth(binary: &OsStr, container_id: &str) -> Result<(), &'static str> {
    let Some(path) = source_path() else {
        return Ok(());
    };
    bootstrap_codex_auth_at(binary, container_id, &path, "/config", BOOTSTRAP_TIMEOUT).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::ffi::CString;
    #[test]
    fn accepts_chatgpt_and_api_key_shapes() {
        assert!(!valid_cache(br#"{"tokens":{"access_token":"x"}}"#));
        assert!(valid_cache(br#"{"tokens":{"access_token":"x","refresh_token":"x","id_token":"x","account_id":"x"}}"#));
        assert!(valid_cache(br#"{"OPENAI_API_KEY":"sk-test"}"#));
        assert!(!valid_cache(br#"{"tokens":{}}"#));
        assert!(!valid_cache(
            br#"{"auth_mode":"chatgpt","OPENAI_API_KEY":"sk-test"}"#
        ));
        assert!(!valid_cache(br#"{"tokens":{"access_token":"   ","refresh_token":"x","id_token":"x","account_id":"x"}}"#));
    }

    #[test]
    fn load_cache_is_bounded_and_rejects_invalid_paths() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("missing");
        assert!(load_cache(&missing).is_none());
        let huge = root.path().join("huge");
        std::fs::write(&huge, vec![b'x'; MAX_AUTH_BYTES + 1]).unwrap();
        assert!(load_cache(&huge).is_none());
        #[cfg(unix)]
        {
            let link = root.path().join("link");
            std::os::unix::fs::symlink(&huge, &link).unwrap();
            assert!(load_cache(&link).is_none());
            let fifo = root.path().join("fifo");
            assert_eq!(
                unsafe {
                    libc::mkfifo(
                        CString::new(fifo.as_os_str().as_encoded_bytes())
                            .unwrap()
                            .as_ptr(),
                        0o600,
                    )
                },
                0
            );
            assert!(load_cache(&fifo).is_none());
        }
    }

    #[cfg(unix)]
    mod docker_tests {
        use super::*;
        use std::os::unix::fs::PermissionsExt;

        fn fake_docker(source: &str) -> (tempfile::TempDir, PathBuf) {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("docker");
            std::fs::write(&path, source).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            (dir, path)
        }

        fn cache(path: &std::path::Path, value: &[u8]) {
            std::fs::write(path, value).unwrap();
        }

        #[tokio::test]
        async fn fake_docker_receives_correct_shell_argv_and_stdin() {
            let (_dir, docker) = fake_docker(
                "#!/usr/bin/env python3\nimport json, sys\nassert sys.argv[1:7] == ['exec', '-i', '-u', 'root', 'container', 'sh']\nassert sys.argv[7] == '-c'\nassert sys.argv[9] == 'orbit-codex-auth' and sys.argv[10] == '/config'\ndata = sys.stdin.buffer.read()\nassert json.loads(data)['OPENAI_API_KEY'] == 'fixture-secret'\nassert all('fixture-secret' not in arg for arg in sys.argv)\n",
            );
            let fixture = tempfile::tempdir().unwrap();
            let source = fixture.path().join("auth.json");
            cache(&source, br#"{"OPENAI_API_KEY":"fixture-secret"}"#);

            bootstrap_codex_auth_at(
                docker.as_os_str(),
                "container",
                &source,
                "/config",
                Duration::from_secs(2),
            )
            .await
            .unwrap();
        }

        #[tokio::test]
        async fn fake_docker_nonzero_exit_is_reported() {
            let (_dir, docker) = fake_docker("#!/usr/bin/env python3\nimport sys\nsys.exit(7)\n");
            let fixture = tempfile::tempdir().unwrap();
            let source = fixture.path().join("auth.json");
            cache(&source, br#"{"OPENAI_API_KEY":"fixture-secret"}"#);

            assert_eq!(
                bootstrap_codex_auth_at(
                    docker.as_os_str(),
                    "container",
                    &source,
                    "/config",
                    Duration::from_secs(2),
                )
                .await,
                Err("Codex auth bootstrap failed")
            );
        }

        #[tokio::test]
        async fn fake_docker_that_does_not_read_stdin_times_out() {
            let (_dir, docker) =
                fake_docker("#!/usr/bin/env python3\nimport time\ntime.sleep(5)\n");
            let fixture = tempfile::tempdir().unwrap();
            let source = fixture.path().join("auth.json");
            let value = format!(
                "{{\"auth_mode\":\"api_key\",\"OPENAI_API_KEY\":\"{}\"}}",
                "k".repeat(250_000)
            );
            cache(&source, value.as_bytes());

            let start = std::time::Instant::now();
            assert_eq!(
                bootstrap_codex_auth_at(
                    docker.as_os_str(),
                    "container",
                    &source,
                    "/config",
                    Duration::from_millis(100),
                )
                .await,
                Err("Codex auth bootstrap timed out")
            );
            assert!(start.elapsed() < Duration::from_secs(2));
        }
    }
}
