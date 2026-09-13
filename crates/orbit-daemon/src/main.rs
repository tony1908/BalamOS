use anyhow::{Context, Result};
use clap::Parser;
use directories::ProjectDirs;
use fs2::FileExt;
use orbit_daemon::{Daemon, docker_cli::DockerCliRuntime, workspace_manager::WorkspaceManager};
use orbit_store::WorkspaceStore;
use rand::{Rng, distr::Alphanumeric};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Debug, Parser)]
#[command(name = "orbit-daemon", about = "Orbit workspace daemon")]
struct Args {
    #[arg(long, default_value = "orbit-workspace-daemon")]
    socket_name: String,
    #[arg(long)]
    data_dir: Option<PathBuf>,
}

fn default_data_dir() -> Result<PathBuf> {
    ProjectDirs::from("com", "Orbit", "Workspace")
        .map(|d| d.data_local_dir().to_path_buf())
        .context("could not determine local data directory")
}

fn load_or_create_token(data_dir: &Path) -> Result<String> {
    if fs::symlink_metadata(data_dir).is_err() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.recursive(true).mode(0o700).create(data_dir)?;
        }
        #[cfg(not(unix))]
        fs::create_dir_all(data_dir)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = fs::symlink_metadata(data_dir)?;
        // SAFETY: geteuid only reads the effective user ID.
        let uid = unsafe { libc::geteuid() };
        anyhow::ensure!(
            m.file_type().is_dir() && m.uid() == uid && m.mode() & 0o777 == 0o700,
            "unsafe data directory"
        );
    }
    let path = data_dir.join("daemon.token");
    let lock_path = data_dir.join("daemon.token.lock");
    let mut lock_options = OpenOptions::new();
    lock_options.read(true).write(true).create(true);
    configure_token_options(&mut lock_options);
    let lock = lock_options.open(&lock_path)?;
    validate_token_file(&lock)?;
    lock.lock_exclusive()?;
    if path.exists() {
        return read_existing_token(&path);
    }
    let temp = data_dir.join(format!("daemon.token.{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    configure_token_options(&mut options);
    let mut file = options.open(&temp)?;
    let token: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(64)
        .map(char::from)
        .collect();
    file.write_all(token.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    validate_token_file(&file)?;
    fs::rename(&temp, &path)?;
    #[cfg(unix)]
    File::open(data_dir)?.sync_all()?;
    Ok(token)
}

fn read_existing_token(path: &Path) -> Result<String> {
    let mut options = OpenOptions::new();
    options.read(true);
    configure_read_options(&mut options);
    let mut file = options.open(path)?;
    validate_token_file(&file)?;
    let mut token = String::new();
    file.read_to_string(&mut token)?;
    let token = token.strip_suffix('\n').unwrap_or(&token).to_owned();
    anyhow::ensure!(
        token.len() == 64 && token.bytes().all(|b| b.is_ascii_alphanumeric()),
        "invalid daemon token"
    );
    Ok(token)
}
fn validate_token_file(file: &File) -> Result<()> {
    let m = file.metadata()?;
    anyhow::ensure!(m.is_file(), "unsafe token file");
    #[cfg(unix)]
    secure_token_permissions_mode(&m)?;
    Ok(())
}
#[cfg(unix)]
fn secure_token_permissions_mode(m: &std::fs::Metadata) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let uid = unsafe { libc::geteuid() };
    anyhow::ensure!(
        m.uid() == uid && m.mode() & 0o777 == 0o600,
        "unsafe token file"
    );
    Ok(())
}
#[cfg(unix)]
fn configure_read_options(o: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    o.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
}
#[cfg(not(unix))]
fn configure_read_options(_o: &mut OpenOptions) {}
#[cfg(unix)]
fn configure_token_options(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
    options.custom_flags(libc::O_NOFOLLOW);
}
#[cfg(not(unix))]
fn configure_token_options(_options: &mut OpenOptions) {}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let data_dir = args.data_dir.unwrap_or(default_data_dir()?);
    let token = load_or_create_token(&data_dir)?;
    // Dev opt-in: when ORBIT_CREDENTIAL_DIR is set, store secrets in local 0600
    // files instead of the OS keychain (unsigned dev binaries can't use the
    // macOS Keychain). Unset in production -> the keychain-backed store is used.
    let store = {
        let base = WorkspaceStore::open(data_dir.join("orbit.db"))?;
        match std::env::var("ORBIT_CREDENTIAL_DIR") {
            Ok(dir) if !dir.trim().is_empty() => base.with_credentials(Arc::new(
                orbit_store::FileCredentialStore::new(dir.trim())?,
            )),
            _ => base,
        }
    };
    let store = Arc::new(store);
    let runtime = Arc::new(DockerCliRuntime::new("docker"));
    let manager = Arc::new(WorkspaceManager::new(store, runtime));
    Daemon::with_lock_path(
        args.socket_name,
        token,
        manager,
        data_dir.canonicalize()?.join("daemon.lock"),
    )
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_is_stable_across_reopen() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("data");
        let a = load_or_create_token(&p).unwrap();
        assert_eq!(a, load_or_create_token(&p).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn fresh_data_directory_is_user_only() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("fresh-data");
        load_or_create_token(&p).unwrap();
        assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o700);
    }
    #[cfg(unix)]
    #[test]
    fn token_file_is_user_only() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("data");
        load_or_create_token(&p).unwrap();
        assert_eq!(
            fs::metadata(p.join("daemon.token"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
