use orbit_client::DaemonClient;
use orbit_daemon::{Daemon, docker_cli::DockerCliRuntime, workspace_manager::WorkspaceManager};
use orbit_protocol::{ErrorCode, ResponseBody};
use orbit_store::WorkspaceStore;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[tokio::test]
async fn valid_token_can_ping_and_invalid_token_is_rejected() {
    let socket_name = format!("orbit-test-{}", Uuid::new_v4());
    let daemon = Daemon::new(
        socket_name.clone(),
        "valid-token".into(),
        Arc::new(WorkspaceManager::new(
            Arc::new(WorkspaceStore::in_memory().unwrap()),
            // IPC authentication does not depend on a live Docker daemon. A
            // guaranteed-missing executable keeps startup deterministic.
            Arc::new(DockerCliRuntime::new("orbit-test-missing-docker")),
        )),
    );
    let task = tokio::spawn(async move { daemon.run().await.unwrap() });
    let valid = DaemonClient::new(socket_name.clone(), "valid-token".into());
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match valid.ping().await {
            Ok(ResponseBody::Pong) => break,
            Ok(other) => panic!("unexpected daemon readiness response: {other:?}"),
            Err(error)
                if matches!(
                    error,
                    orbit_client::ClientError::SocketName
                        | orbit_client::ClientError::Timeout
                        | orbit_client::ClientError::Io(_)
                ) && Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(10)).await
            }
            Err(error) => panic!("daemon readiness failed: {error:?}"),
        }
    }

    let invalid = DaemonClient::new(socket_name.clone(), "wrong-token".into());
    let error = invalid.ping().await.unwrap_err();
    assert_eq!(error.code(), Some(ErrorCode::Unauthorized));

    #[cfg(unix)]
    {
        use std::os::unix::fs::{FileTypeExt, MetadataExt};
        let dir =
            std::path::PathBuf::from(format!("/tmp/orbit-runtime-{}", unsafe { libc::geteuid() }));
        let socket = dir.join(format!("{socket_name}.sock"));
        let d = std::fs::symlink_metadata(&dir).unwrap();
        let s = std::fs::symlink_metadata(&socket).unwrap();
        assert_eq!(d.uid(), unsafe { libc::geteuid() });
        assert_eq!(d.mode() & 0o777, 0o700);
        assert!(s.file_type().is_socket());
        assert_eq!(s.uid(), unsafe { libc::geteuid() });
        assert_eq!(s.mode() & 0o077, 0);
    }
    task.abort();
    let _ = task.await;
    #[cfg(unix)]
    {
        // SAFETY: geteuid has no preconditions.
        let uid = unsafe { libc::geteuid() };
        let deadline = Instant::now() + Duration::from_millis(500);
        while std::path::Path::new(&format!("/tmp/orbit-runtime-{}/{}.sock", uid, socket_name))
            .exists()
            && Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            !std::path::Path::new(&format!("/tmp/orbit-runtime-{}/{}.sock", uid, socket_name))
                .exists()
        );
    }
}
