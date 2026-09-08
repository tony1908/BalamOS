use anyhow::{Context, Result, bail};
use orbit_client::DaemonClient;
use orbit_protocol::ResponseBody;

#[tokio::main]
async fn main() -> Result<()> {
    let socket = std::env::var("ORBIT_SOCKET_NAME").context("ORBIT_SOCKET_NAME is required")?;
    let token = std::env::var("ORBIT_DAEMON_TOKEN").context("ORBIT_DAEMON_TOKEN is required")?;
    match DaemonClient::new(socket, token).ping().await? {
        ResponseBody::Pong => {
            println!("pong");
            Ok(())
        }
        other => bail!("unexpected daemon response: {other:?}"),
    }
}
