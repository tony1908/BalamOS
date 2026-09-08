use axum::{
    Router,
    body::Body,
    extract::ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade},
    extract::{FromRequest, State},
    http::{HeaderMap, HeaderName, HeaderValue, Request, Response, StatusCode},
    response::IntoResponse,
    routing::any,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use serde::Serialize;
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{net::TcpListener, sync::RwLock, task::JoinHandle};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, client::IntoClientRequest},
};
use uuid::Uuid;

const MAX_BODY: usize = 16 * 1024 * 1024;
const NOT_FOUND: &str = "Not Found";
const RECONNECT_HTML: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><style>html,body{height:100%;margin:0}body{display:flex;align-items:center;justify-content:center;background:#0b0b0c;color:#a1a2a7;font-family:system-ui,-apple-system,Segoe UI,sans-serif;font-size:13px}</style></head><body><div>Reconnecting to the desktop…</div><script>try{window.parent&&window.parent.postMessage({source:'orbit-desktop',type:'session-invalid'},'*')}catch(e){}</script></body></html>";

#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("gateway unavailable")]
    Io(#[from] std::io::Error),
    #[error("invalid upstream")]
    InvalidUpstream,
    #[error("invalid session parameters")]
    InvalidParameters,
    #[error("upstream request failed")]
    Upstream(#[from] reqwest::Error),
}

#[derive(Clone)]
pub struct DesktopCredential {
    username: String,
    password: String,
}
impl DesktopCredential {
    pub fn basic(
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, GatewayError> {
        let username = username.into();
        let password = password.into();
        if username.contains(':')
            || username.chars().any(|c| c.is_control())
            || password.chars().any(|c| c.is_control())
        {
            return Err(GatewayError::InvalidParameters);
        }
        Ok(Self { username, password })
    }
}
impl std::fmt::Debug for DesktopCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopCredential")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Serialize)]
pub struct DesktopSession {
    pub url: String,
    pub expires_at_unix_ms: u64,
}
impl std::fmt::Debug for DesktopSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopSession")
            .field("url", &redact_url(&self.url))
            .field("expires_at_unix_ms", &self.expires_at_unix_ms)
            .finish()
    }
}

struct Entry {
    workspace: Uuid,
    upstream: SocketAddr,
    credential: DesktopCredential,
    expires: u64,
}
struct Inner {
    sessions: RwLock<HashMap<String, Entry>>,
    client: reqwest::Client,
    addr: SocketAddr,
    shutdown: tokio::sync::watch::Sender<bool>,
    active: tokio::sync::Mutex<HashMap<String, Vec<tokio::sync::watch::Sender<bool>>>>,
}
pub struct DesktopGateway {
    inner: Arc<Inner>,
    cancel: tokio::sync::watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}

impl DesktopGateway {
    pub async fn bind_loopback() -> Result<Self, GatewayError> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let addr = listener.local_addr()?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let (tx, mut rx) = tokio::sync::watch::channel(false);
        let inner = Arc::new(Inner {
            sessions: RwLock::new(HashMap::new()),
            client,
            addr,
            shutdown: tx.clone(),
            active: tokio::sync::Mutex::new(HashMap::new()),
        });
        let state = inner.clone();
        let task = tokio::spawn(async move {
            let app = Router::new()
                .fallback(any(handle_fallback))
                .with_state(state);
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.changed().await;
                })
                .await;
        });
        Ok(Self {
            inner,
            cancel: tx,
            task: Some(task),
        })
    }
    pub async fn create_session(
        &self,
        workspace_id: Uuid,
        upstream: SocketAddr,
        credential: DesktopCredential,
        ttl: Duration,
    ) -> Result<DesktopSession, GatewayError> {
        if !upstream.ip().is_loopback()
            || !(Duration::from_secs(1)..=Duration::from_secs(24 * 3600)).contains(&ttl)
        {
            return Err(GatewayError::InvalidParameters);
        }
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        let expires = now_ms() + ttl.as_millis() as u64;
        self.inner.sessions.write().await.insert(
            token.clone(),
            Entry {
                workspace: workspace_id,
                upstream,
                credential,
                expires,
            },
        );
        Ok(DesktopSession {
            url: format!("http://{}/session/{token}/", self.inner.addr),
            expires_at_unix_ms: expires,
        })
    }
    pub async fn revoke_workspace(&self, workspace: Uuid) {
        // Keep the active-registration lock first, matching the upgrade path. This
        // makes revocation and registration a single coherent operation.
        let mut active = self.inner.active.lock().await;
        let mut sessions = self.inner.sessions.write().await;
        let tokens: Vec<String> = sessions
            .iter()
            .filter(|(_, e)| e.workspace == workspace)
            .map(|(t, _)| t.clone())
            .collect();
        for token in &tokens {
            sessions.remove(token);
        }
        drop(sessions);
        for token in tokens {
            if let Some(channels) = active.remove(&token) {
                for tx in channels {
                    let _ = tx.send(true);
                }
            }
        }
    }
    pub async fn session_count(&self) -> usize {
        self.inner.sessions.read().await.len()
    }
}
impl Drop for DesktopGateway {
    fn drop(&mut self) {
        let _ = self.cancel.send(true);
        if let Some(t) = self.task.take() {
            t.abort();
        }
    }
}

async fn handle_impl(
    State(state): State<Arc<Inner>>,
    token: String,
    raw_path: String,
    req: Request<Body>,
) -> Response<Body> {
    let entry = {
        let sessions = state.sessions.read().await;
        sessions
            .get(&token)
            .filter(|e| e.expires > now_ms())
            .map(|e| (e.upstream, e.credential.clone(), e.expires))
    };
    let Some((upstream, cred, expires)) = entry else {
        return stale_session_response(&req);
    };
    if req.headers().get("upgrade").is_some() {
        let query = req.uri().query().map(str::to_owned);
        let offered = req
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let client_headers = req.headers().clone();
        let client_nominated = nominated_headers(&client_headers);
        let token_for_active = token.clone();
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        // Hold active while revalidating the session: revoke_workspace takes
        // these locks in the same order, so a revoked token cannot register.
        let mut active = state.active.lock().await;
        {
            let sessions = state.sessions.read().await;
            if !sessions.get(&token).is_some_and(|e| e.expires > now_ms()) {
                return (StatusCode::NOT_FOUND, NOT_FOUND).into_response();
            }
        }
        active
            .entry(token_for_active.clone())
            .or_default()
            .push(cancel_tx.clone());
        drop(active);
        let Ok(upgrade) = WebSocketUpgrade::from_request(req, &state).await else {
            remove_active(&state, &token_for_active, &cancel_tx).await;
            return (StatusCode::BAD_REQUEST, "Bad Request").into_response();
        };
        let path = if raw_path.is_empty() { "/" } else { &raw_path };
        let uri = format!(
            "ws://{}{}{}",
            upstream,
            path,
            query.map(|q| format!("?{q}")).unwrap_or_default()
        );
        let mut upstream_req = match uri.into_client_request() {
            Ok(r) => r,
            Err(_) => {
                remove_active(&state, &token_for_active, &cancel_tx).await;
                return (StatusCode::BAD_GATEWAY, "Bad Gateway").into_response();
            }
        };
        let auth = STANDARD.encode(format!("{}:{}", cred.username, cred.password));
        upstream_req.headers_mut().insert(
            "authorization",
            HeaderValue::from_str(&format!("Basic {auth}")).unwrap(),
        );
        if let Some(p) = offered.as_deref() {
            upstream_req
                .headers_mut()
                .insert("sec-websocket-protocol", HeaderValue::from_str(p).unwrap());
        }
        for (name, value) in &client_headers {
            if !blocked(name)
                && !client_nominated.contains(name.as_str())
                && name.as_str() != "sec-websocket-protocol"
            {
                upstream_req.headers_mut().insert(name, value.clone());
            }
        }
        let mut config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
        config.max_message_size = Some(MAX_BODY);
        config.max_frame_size = Some(MAX_BODY);
        let Ok((socket, response)) =
            connect_async_with_config(upstream_req, Some(config), false).await
        else {
            remove_active(&state, &token_for_active, &cancel_tx).await;
            return (StatusCode::BAD_GATEWAY, "Bad Gateway").into_response();
        };
        // Handshake may have taken longer than the session lifetime or included
        // a concurrent revoke. Never let that handshake become a downstream WS.
        if *cancel_rx.borrow() || !session_valid(&state, &token, expires).await {
            remove_active(&state, &token_for_active, &cancel_tx).await;
            return (StatusCode::NOT_FOUND, NOT_FOUND).into_response();
        }
        let selected = response
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let selected = selected.filter(|p| {
            offered
                .as_deref()
                .is_some_and(|o| o.split(',').any(|x| x.trim() == p))
        });
        if *cancel_rx.borrow() {
            remove_active(&state, &token_for_active, &cancel_tx).await;
            return (StatusCode::NOT_FOUND, NOT_FOUND).into_response();
        }
        let global = state.shutdown.subscribe();
        let cleanup_state = state.clone();
        let cleanup_token = token_for_active.clone();
        let cleanup_sender = cancel_tx.clone();
        return upgrade
            .max_message_size(MAX_BODY)
            .max_frame_size(MAX_BODY)
            .protocols(selected.into_iter().collect::<Vec<_>>())
            .on_upgrade(move |downstream| async move {
                bridge(
                    downstream,
                    socket,
                    cancel_rx,
                    global,
                    expires.saturating_sub(now_ms()),
                )
                .await;
                remove_active(&cleanup_state, &cleanup_token, &cleanup_sender).await;
            });
    }
    let path = if raw_path.is_empty() {
        "/".to_string()
    } else {
        raw_path
    };
    let uri = format!(
        "http://{}{}{}",
        upstream,
        path,
        req.uri()
            .query()
            .map(|q| format!("?{q}"))
            .unwrap_or_default()
    );
    let mut builder = state.client.request(req.method().clone(), uri);
    let nominated = nominated_headers(req.headers());
    for (name, val) in req.headers() {
        if !blocked(name) && !nominated.contains(name.as_str()) {
            builder = builder.header(name, val);
        }
    }
    let auth = STANDARD.encode(format!("{}:{}", cred.username, cred.password));
    builder = builder.header("authorization", format!("Basic {auth}"));
    let body = match axum::body::to_bytes(req.into_body(), MAX_BODY + 1).await {
        Ok(b) if b.len() <= MAX_BODY => b,
        _ => return (StatusCode::PAYLOAD_TOO_LARGE, "Payload Too Large").into_response(),
    };
    let Ok(up) = builder.body(body).send().await else {
        return (StatusCode::BAD_GATEWAY, "Bad Gateway").into_response();
    };
    let status = up.status();
    let headers = up.headers().clone();
    let response_nominated = nominated_headers(&headers);
    let bytes = up.bytes().await.unwrap_or_default();
    let mut out = Response::builder().status(status);
    for (n, v) in &headers {
        if !blocked(n) && !response_nominated.contains(n.as_str()) {
            let val = if n == "location" {
                rewrite_location(v, &token, &state.addr, upstream)
            } else {
                v.clone()
            };
            out = out.header(n, val);
        }
    }
    out.body(Body::from(bytes))
        .unwrap_or_else(|_| (StatusCode::BAD_GATEWAY, "Bad Gateway").into_response())
}

fn stale_session_response(req: &Request<Body>) -> Response<Body> {
    let wants_html = req
        .headers()
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("text/html"));
    if wants_html {
        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/html; charset=utf-8")
            .body(Body::from(RECONNECT_HTML))
            .unwrap()
    } else {
        (StatusCode::NOT_FOUND, NOT_FOUND).into_response()
    }
}

async fn session_valid(state: &Arc<Inner>, token: &str, expires: u64) -> bool {
    now_ms() < expires
        && state
            .sessions
            .read()
            .await
            .get(token)
            .is_some_and(|e| e.expires == expires)
}

async fn remove_active(state: &Arc<Inner>, token: &str, sender: &tokio::sync::watch::Sender<bool>) {
    let mut active = state.active.lock().await;
    if let Some(list) = active.get_mut(token) {
        list.retain(|candidate| !candidate.same_channel(sender));
        if list.is_empty() {
            active.remove(token);
        }
    }
}

async fn handle_fallback(State(state): State<Arc<Inner>>, req: Request<Body>) -> Response<Body> {
    let raw = req.uri().path();
    let Some(rest) = raw.strip_prefix("/session/") else {
        return (StatusCode::NOT_FOUND, NOT_FOUND).into_response();
    };
    let (token, suffix) = match rest.split_once('/') {
        Some((token, suffix)) if !token.is_empty() => (token, format!("/{suffix}")),
        _ => return (StatusCode::NOT_FOUND, NOT_FOUND).into_response(),
    };
    handle_impl(State(state), token.to_string(), suffix, req).await
}

async fn bridge<S>(
    mut down: WebSocket,
    mut upstream: tokio_tungstenite::WebSocketStream<S>,
    mut cancel: tokio::sync::watch::Receiver<bool>,
    mut global: tokio::sync::watch::Receiver<bool>,
    ttl_ms: u64,
) where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let expiry = tokio::time::sleep(Duration::from_millis(ttl_ms));
    tokio::pin!(expiry);
    loop {
        tokio::select! {
            _ = &mut expiry => { let _ = down.close().await; let _ = upstream.close(None).await; break; }
            _ = cancel.changed() => { let _ = down.close().await; let _ = upstream.close(None).await; break; }
            _ = global.changed() => { let _ = down.close().await; let _ = upstream.close(None).await; break; }
            Some(Ok(m)) = down.next() => { let x = match m { AxumMessage::Text(v) => Message::Text(v.to_string().into()), AxumMessage::Binary(v) => Message::Binary(v), AxumMessage::Ping(v) => Message::Ping(v), AxumMessage::Pong(v) => Message::Pong(v), AxumMessage::Close(c) => { let cf = c.map(|f| tokio_tungstenite::tungstenite::protocol::CloseFrame { code: f.code.into(), reason: f.reason.to_string().into() }); let _ = upstream.send(Message::Close(cf)).await; if let Some(Ok(Message::Close(Some(f)))) = upstream.next().await { let _ = down.send(AxumMessage::Close(Some(axum::extract::ws::CloseFrame { code: f.code.into(), reason: f.reason.to_string().into() }))).await; } let _ = down.close().await; break; } }; if x.len() > MAX_BODY { let _ = upstream.close(None).await; break } else if upstream.send(x).await.is_err() { let _ = down.close().await; break; } },
            Some(Ok(m)) = upstream.next() => { let x = match m { Message::Text(v) => AxumMessage::Text(v.to_string().into()), Message::Binary(v) => AxumMessage::Binary(v), Message::Ping(v) => AxumMessage::Ping(v), Message::Pong(v) => AxumMessage::Pong(v), Message::Close(c) => { let cf = c.map(|f| axum::extract::ws::CloseFrame { code: f.code.into(), reason: f.reason.to_string().into() }); let _ = down.send(AxumMessage::Close(cf)).await; let _ = down.close().await; break; }, _ => continue }; if down.send(x).await.is_err() { let _ = upstream.close(None).await; break; } }
            else => { let _ = down.close().await; let _ = upstream.close(None).await; break; }
        }
    }
}

fn blocked(n: &HeaderName) -> bool {
    matches!(
        n.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "authorization"
            | "token"
            | "x-token"
            | "referer"
            | "origin"
            | "sec-websocket-key"
            | "sec-websocket-version"
            | "sec-websocket-extensions"
            | "sec-websocket-accept"
            | "sec-websocket-protocol"
    )
}
fn nominated_headers(headers: &HeaderMap) -> std::collections::HashSet<String> {
    headers
        .get_all("connection")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| {
            s.split(',').filter_map(|x| {
                let n = x.trim().to_ascii_lowercase();
                (!n.is_empty()).then_some(n)
            })
        })
        .collect()
}
fn rewrite_location(
    v: &HeaderValue,
    token: &str,
    addr: &SocketAddr,
    upstream: SocketAddr,
) -> HeaderValue {
    let Ok(s) = v.to_str() else { return v.clone() };
    let prefix = format!("http://{addr}/session/{token}");
    let p = if s.starts_with('/') {
        format!("{prefix}{s}")
    } else if let Ok(url) = reqwest::Url::parse(s) {
        let port = url.port_or_known_default();
        if url.scheme() == "http"
            && url.host_str() == Some(&upstream.ip().to_string())
            && port == Some(upstream.port())
        {
            format!(
                "{prefix}{}{}{}",
                url.path(),
                url.query().map(|q| format!("?{q}")).unwrap_or_default(),
                url.fragment().map(|f| format!("#{f}")).unwrap_or_default()
            )
        } else {
            s.to_string()
        }
    } else {
        s.to_string()
    };
    HeaderValue::try_from(p).unwrap_or_else(|_| v.clone())
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn redact_url(url: &str) -> String {
    url.split("/session/")
        .next()
        .map(|x| format!("{x}/session/[REDACTED]/"))
        .unwrap_or_else(|| "[REDACTED]".into())
}
