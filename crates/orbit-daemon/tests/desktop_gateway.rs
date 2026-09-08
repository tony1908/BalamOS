use axum::{
    Router,
    body::Body,
    extract::ws::{Message as WsMessage, WebSocketUpgrade},
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, Request, StatusCode, header::CONNECTION},
    response::{IntoResponse, Response},
    routing::any,
};
use futures_util::{SinkExt, StreamExt};
use orbit_daemon::desktop_gateway::{DesktopCredential, DesktopGateway, GatewayError};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::TcpListener;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use uuid::Uuid;

#[derive(Clone, Default)]
struct CapturedRequest {
    uri: String,
    headers: HeaderMap,
    body: Vec<u8>,
}
#[derive(Clone, Default)]
struct Seen(Arc<Mutex<Vec<CapturedRequest>>>);
async fn upstream(
    State(seen): State<Seen>,
    req: axum::http::Request<axum::body::Body>,
) -> impl IntoResponse {
    let uri = req.uri().to_string();
    let headers = req.headers().clone();
    let body = axum::body::to_bytes(req.into_body(), 32 * 1024 * 1024)
        .await
        .unwrap()
        .to_vec();
    seen.0.lock().unwrap().push(CapturedRequest {
        uri: uri.clone(),
        headers: headers.clone(),
        body,
    });
    if uri.starts_with("/status") {
        let mut response = (
            StatusCode::IM_A_TEAPOT,
            [("x-safe", "yes"), ("x-hop-one", "a"), ("x-hop-two", "b")],
            "status",
        )
            .into_response();
        response
            .headers_mut()
            .append(CONNECTION, HeaderValue::from_static("x-hop-one"));
        response
            .headers_mut()
            .append(CONNECTION, HeaderValue::from_static("x-hop-two"));
        return response;
    }
    if uri.starts_with("/relative") {
        return (StatusCode::FOUND, [("location", "/next")], "").into_response();
    }
    if uri.starts_with("/echo-location") {
        return (
            StatusCode::FOUND,
            [(
                "location",
                headers.get("x-test-location").unwrap().to_str().unwrap(),
            )],
            "",
        )
            .into_response();
    }
    (
        StatusCode::OK,
        [
            ("x-safe", "yes"),
            ("x-hop", "secret"),
            ("connection", "x-hop"),
        ],
        "ok",
    )
        .into_response()
}
async fn ws_upstream(State(seen): State<Seen>, req: Request<Body>) -> Response {
    let uri = req.uri().to_string();
    let headers = req.headers().clone();
    let delayed = req.uri().path() == "/delayed";
    let close = req.uri().path() == "/close";
    let (mut parts, _) = req.into_parts();
    seen.0.lock().unwrap().push(CapturedRequest {
        uri,
        headers,
        body: Vec::new(),
    });
    if delayed {
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let Ok(upgrade) = WebSocketUpgrade::from_request_parts(&mut parts, &seen).await else {
        return (StatusCode::BAD_REQUEST, "Bad Request").into_response();
    };
    upgrade
        .protocols(["known"])
        .on_upgrade(move |mut socket| async move {
            if close {
                let _ = socket
                    .send(WsMessage::Close(Some(axum::extract::ws::CloseFrame {
                        code: 4001,
                        reason: "upstream reason".into(),
                    })))
                    .await;
                return;
            }
            let _ = socket.send(WsMessage::Text("server".into())).await;
            while let Some(Ok(msg)) = socket.next().await {
                if socket.send(msg).await.is_err() {
                    break;
                }
            }
        })
        .into_response()
}
async fn server() -> (SocketAddr, Seen) {
    let seen = Seen::default();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/socket", axum::routing::get(ws_upstream))
        .route("/delayed", axum::routing::get(ws_upstream))
        .route("/close", axum::routing::get(ws_upstream))
        .fallback(any(upstream))
        .with_state(seen.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (addr, seen)
}

fn ws_url(url: &str, path: &str) -> String {
    format!("ws://{}{}", url.trim_start_matches("http://"), path)
}

#[tokio::test]
async fn websocket_forwards_raw_path_query_and_basic_auth_and_bidirectional_frames() {
    let (g, a, seen) = gateway().await;
    let s = session(&g, a).await;
    let mut req = ws_url(&s.url, "socket?x=a%2Fb")
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("x-safe", "forwarded".parse().unwrap());
    req.headers_mut().insert("x-hop", "secret".parse().unwrap());
    req.headers_mut()
        .insert("x-token", "token-leak".parse().unwrap());
    req.headers_mut()
        .insert("authorization", "spoof".parse().unwrap());
    req.headers_mut()
        .insert("connection", "Upgrade, x-hop".parse().unwrap());
    let (mut client, _) = connect_async(req).await.unwrap();
    assert_eq!(
        client.next().await.unwrap().unwrap(),
        Message::Text("server".into())
    );
    client.send(Message::Text("hello".into())).await.unwrap();
    assert_eq!(
        client.next().await.unwrap().unwrap(),
        Message::Text("hello".into())
    );
    client
        .send(Message::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    assert_eq!(
        client.next().await.unwrap().unwrap(),
        Message::Binary(vec![1, 2, 3].into())
    );
    client.send(Message::Ping(vec![9].into())).await.unwrap();
    let mut got_pong = false;
    for _ in 0..3 {
        if matches!(client.next().await.unwrap().unwrap(), Message::Pong(v) if v == vec![9]) {
            got_pong = true;
            break;
        }
    }
    assert!(got_pong);
    client.close(None).await.unwrap();
    let captured = seen.0.lock().unwrap().last().unwrap().clone();
    assert_eq!(captured.uri, "/socket?x=a%2Fb");
    assert_eq!(captured.headers.get("authorization").unwrap(), "Basic dTpw");
    assert_eq!(captured.headers.get("x-safe").unwrap(), "forwarded");
    assert!(!captured.headers.contains_key("x-hop"));
    assert!(!captured.headers.contains_key("x-token"));
}

#[tokio::test]
async fn websocket_invalid_token_and_upstream_failure_are_static_404_and_502_without_leaks() {
    let (g, a, _) = gateway().await;
    let s = session(&g, a).await;
    let bad = format!("ws://{}/session/nope/socket", gaddr(&s));
    match connect_async(bad).await.unwrap_err() {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(response.body(), &Some(b"Not Found".to_vec()));
        }
        other => panic!("expected static 404 response, got {other:?}"),
    }
    let dead = g
        .create_session(
            Uuid::new_v4(),
            SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 65534),
            DesktopCredential::basic("u", "p").unwrap(),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    match connect_async(ws_url(&dead.url, "socket"))
        .await
        .unwrap_err()
    {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
            assert_eq!(response.body(), &Some(b"Bad Gateway".to_vec()));
        }
        other => panic!("expected static 502 response, got {other:?}"),
    }
}

#[tokio::test]
async fn websocket_revoke_workspace_closes_only_target_active_connection() {
    let (g, a, _) = gateway().await;
    let w = Uuid::new_v4();
    let x = g
        .create_session(
            w,
            a,
            DesktopCredential::basic("u", "p").unwrap(),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    let y = session(&g, a).await;
    let (mut cx, _) = connect_async(ws_url(&x.url, "socket")).await.unwrap();
    let (mut cy, _) = connect_async(ws_url(&y.url, "socket")).await.unwrap();
    let _ = cx.next().await;
    let _ = cy.next().await;
    g.revoke_workspace(w).await;
    let closed = tokio::time::timeout(Duration::from_secs(2), cx.next())
        .await
        .unwrap();
    assert_terminal(closed);
    cy.send(Message::Text("ok".into())).await.unwrap();
    assert_eq!(
        cy.next().await.unwrap().unwrap(),
        Message::Text("ok".into())
    );
}

#[tokio::test]
async fn websocket_expiry_closes_active_connection() {
    let (g, a, _) = gateway().await;
    let s = g
        .create_session(
            Uuid::new_v4(),
            a,
            DesktopCredential::basic("u", "p").unwrap(),
            Duration::from_secs(1),
        )
        .await
        .unwrap();
    let (mut c, _) = connect_async(ws_url(&s.url, "socket")).await.unwrap();
    let _ = c.next().await;
    let got = tokio::time::timeout(Duration::from_secs(3), c.next())
        .await
        .unwrap();
    assert!(
        got.as_ref()
            .is_none_or(|r| r.is_err() || matches!(r, Ok(Message::Close(_))))
    );
}

#[tokio::test]
async fn websocket_continuous_traffic_cannot_extend_absolute_expiry() {
    let (g, a, _) = gateway().await;
    let s = g
        .create_session(
            Uuid::new_v4(),
            a,
            DesktopCredential::basic("u", "p").unwrap(),
            Duration::from_secs(1),
        )
        .await
        .unwrap();
    let (mut c, _) = connect_async(ws_url(&s.url, "socket")).await.unwrap();
    let _ = c.next().await;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(1300);
    loop {
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        if c.send(Message::Text("traffic".into())).await.is_err() {
            break;
        }
        let _ = tokio::time::timeout(Duration::from_millis(100), c.next()).await;
    }
    let ended = tokio::time::timeout(Duration::from_secs(2), c.next())
        .await
        .unwrap();
    assert_terminal(ended);
}

#[tokio::test]
async fn websocket_revoke_during_upstream_handshake_returns_static_404() {
    let (g, a, _) = gateway().await;
    let workspace = Uuid::new_v4();
    let s = g
        .create_session(
            workspace,
            a,
            DesktopCredential::basic("u", "p").unwrap(),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    let url = ws_url(&s.url, "delayed");
    let revoke = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        g.revoke_workspace(workspace).await;
    };
    let (result, _) = tokio::join!(connect_async(url), revoke);
    match result.unwrap_err() {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(response.body(), &Some(b"Not Found".to_vec()));
        }
        other => panic!("expected static 404 response, got {other:?}"),
    }
}

#[tokio::test]
async fn websocket_oversize_message_is_rejected_at_downstream_decode() {
    let (g, a, _) = gateway().await;
    let s = session(&g, a).await;
    let connected = tokio::time::timeout(
        Duration::from_secs(5),
        connect_async(ws_url(&s.url, "socket")),
    )
    .await
    .expect("WebSocket handshake timed out")
    .unwrap();
    let (mut c, _) = connected;
    let initial = tokio::time::timeout(Duration::from_secs(5), c.next())
        .await
        .expect("initial WebSocket message timed out");
    assert!(matches!(initial, Some(Ok(Message::Text(_)))));
    let sent = tokio::time::timeout(
        Duration::from_secs(5),
        c.send(Message::Binary(vec![0; 16 * 1024 * 1024 + 1].into())),
    )
    .await;
    match sent {
        Ok(Err(_)) | Err(_) => return,
        Ok(Ok(())) => {}
    }
    let ended = tokio::time::timeout(Duration::from_secs(5), c.next())
        .await
        .expect("oversize WebSocket message was not rejected promptly");
    assert_terminal(ended);
}

#[tokio::test]
async fn websocket_close_code_reason_and_eof_are_preserved_both_directions() {
    let (g, a, _) = gateway().await;
    let s = session(&g, a).await;
    let (mut upstream_close, _) = connect_async(ws_url(&s.url, "close")).await.unwrap();
    let close = upstream_close.next().await.unwrap().unwrap();
    assert_eq!(
        close,
        Message::Close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
            code: 4001.into(),
            reason: "upstream reason".into(),
        }))
    );
    assert!(upstream_close.next().await.is_none());

    let (mut c, _) = connect_async(ws_url(&s.url, "socket")).await.unwrap();
    let _ = c.next().await;
    c.send(Message::Close(Some(
        tokio_tungstenite::tungstenite::protocol::CloseFrame {
            code: 4002.into(),
            reason: "client reason".into(),
        },
    )))
    .await
    .unwrap();
    assert_eq!(
        c.next().await.unwrap().unwrap(),
        Message::Close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
            code: 4002.into(),
            reason: "client reason".into(),
        }))
    );
    assert!(c.next().await.is_none());
}

#[tokio::test]
async fn websocket_gateway_drop_closes_active_socket() {
    let (a, _) = server().await;
    let g = DesktopGateway::bind_loopback().await.unwrap();
    let s = g
        .create_session(
            Uuid::new_v4(),
            a,
            DesktopCredential::basic("u", "p").unwrap(),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    let (mut c, _) = connect_async(ws_url(&s.url, "socket")).await.unwrap();
    let _ = c.next().await;
    drop(g);
    let ended = tokio::time::timeout(Duration::from_secs(2), c.next())
        .await
        .unwrap();
    assert_terminal(ended);
}

#[tokio::test]
async fn websocket_subprotocol_is_negotiated_only_when_offered() {
    let (g, a, _) = gateway().await;
    let s = session(&g, a).await;
    let mut req = ws_url(&s.url, "socket").into_client_request().unwrap();
    req.headers_mut()
        .insert("Sec-WebSocket-Protocol", "known".parse().unwrap());
    let (mut c, resp) = connect_async(req).await.unwrap();
    assert_eq!(
        resp.headers().get("sec-websocket-protocol").unwrap(),
        "known"
    );
    let _ = c.next().await;

    let req = ws_url(&s.url, "socket").into_client_request().unwrap();
    let (_, resp) = connect_async(req).await.unwrap();
    assert!(resp.headers().get("sec-websocket-protocol").is_none());
}
async fn gateway() -> (DesktopGateway, SocketAddr, Seen) {
    let (a, s) = server().await;
    (DesktopGateway::bind_loopback().await.unwrap(), a, s)
}
async fn session(
    g: &DesktopGateway,
    a: SocketAddr,
) -> orbit_daemon::desktop_gateway::DesktopSession {
    g.create_session(
        Uuid::new_v4(),
        a,
        DesktopCredential::basic("u", "p").unwrap(),
        Duration::from_secs(60),
    )
    .await
    .unwrap()
}

fn assert_terminal(result: Option<Result<Message, tokio_tungstenite::tungstenite::Error>>) {
    assert!(
        result.is_none()
            || result.as_ref().is_some_and(Result::is_err)
            || matches!(result, Some(Ok(Message::Close(_))))
    );
}

#[tokio::test]
async fn binds_only_loopback_and_session_url_is_tokenized() {
    let (g, a, _) = gateway().await;
    let s = session(&g, a).await;
    let u = reqwest::get(&s.url).await.unwrap();
    assert_eq!(u.status(), 200);
    let p = reqwest::Url::parse(&s.url).unwrap();
    assert_eq!(p.host_str(), Some("127.0.0.1"));
    assert!(!s.url.contains("password"));
}

#[tokio::test]
async fn valid_session_forwards_method_path_query_body_and_injects_basic_auth() {
    let (g, a, seen) = gateway().await;
    let s = session(&g, a).await;
    let c = reqwest::Client::new();
    let r = c
        .post(format!("{}a%2Fb?q=x%20y", s.url))
        .header("authorization", "spoof")
        .header("host", "spoof")
        .header("referer", "spoof")
        .body("body")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let x = seen.0.lock().unwrap().pop().unwrap();
    assert_eq!(x.uri, "/a%2Fb?q=x%20y");
    assert_eq!(x.body, b"body");
    assert_eq!(x.headers.get("authorization").unwrap(), "Basic dTpw");
    assert_ne!(x.headers.get("host").unwrap(), "spoof");
    assert!(!x.headers.contains_key("referer"));
}

#[tokio::test]
async fn invalid_expired_and_revoked_sessions_are_indistinguishable_404() {
    let (g, a, _) = gateway().await;
    let s = session(&g, a).await;
    let c = reqwest::Client::new();
    let random = c
        .get(format!("http://{}/session/random/", gaddr(&s)))
        .send()
        .await
        .unwrap();
    assert_eq!(random.status(), 404);
    let random_body = random.text().await.unwrap();
    let e = g
        .create_session(
            Uuid::new_v4(),
            a,
            DesktopCredential::basic("u", "p").unwrap(),
            Duration::from_secs(1),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let expired = c.get(&e.url).send().await.unwrap();
    assert_eq!(expired.status(), 404);
    let expired_body = expired.text().await.unwrap();
    let w = Uuid::new_v4();
    let r = g
        .create_session(
            w,
            a,
            DesktopCredential::basic("u", "p").unwrap(),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
    g.revoke_workspace(w).await;
    let revoked = c.get(&r.url).send().await.unwrap();
    assert_eq!(revoked.status(), 404);
    assert_eq!(random_body, expired_body);
    assert_eq!(expired_body, revoked.text().await.unwrap());
}
fn gaddr(s: &orbit_daemon::desktop_gateway::DesktopSession) -> String {
    reqwest::Url::parse(&s.url).unwrap().authority().to_string()
}

#[tokio::test]
async fn forwards_status_and_safe_headers_strips_hop_headers() {
    let (g, a, seen) = gateway().await;
    let s = session(&g, a).await;
    let mut h = HeaderMap::new();
    h.append("connection", "x-hop-one".parse().unwrap());
    h.append("connection", "x-hop-two".parse().unwrap());
    h.insert("x-hop-one", "v".parse().unwrap());
    h.insert("x-hop-two", "v".parse().unwrap());
    let r = reqwest::Client::new()
        .get(format!("{}status", s.url))
        .headers(h)
        .send()
        .await
        .unwrap();
    let x = seen.0.lock().unwrap().pop().unwrap();
    assert!(!x.headers.contains_key("x-hop-one"));
    assert!(!x.headers.contains_key("x-hop-two"));
    assert_eq!(r.status(), 418);
    assert_eq!(r.headers().get("x-safe").unwrap(), "yes");
    assert!(r.headers().get("x-hop-one").is_none());
    assert!(r.headers().get("x-hop-two").is_none());
    assert!(r.headers().get("connection").is_none());
}

#[tokio::test]
async fn enforces_16mib_body_limit() {
    let (g, a, seen) = gateway().await;
    let s = session(&g, a).await;
    let c = reqwest::Client::new();
    let r = c
        .post(&s.url)
        .body(vec![b'x'; 16 * 1024 * 1024])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        seen.0.lock().unwrap().pop().unwrap().body.len(),
        16 * 1024 * 1024
    );
    let r = c
        .post(&s.url)
        .body(vec![b'x'; 16 * 1024 * 1024 + 1])
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 413);
    assert!(seen.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn rejects_non_loopback_upstream_and_invalid_credentials_ttl() {
    let g = DesktopGateway::bind_loopback().await.unwrap();
    let c = DesktopCredential::basic("a:b", "p");
    assert!(c.is_err());
    assert!(DesktopCredential::basic("a", "p\n").is_err());
    let e = DesktopCredential::basic("a", "p").unwrap();
    assert!(matches!(
        g.create_session(
            Uuid::new_v4(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 1),
            e.clone(),
            Duration::ZERO
        )
        .await,
        Err(GatewayError::InvalidParameters)
    ));
    assert!(
        g.create_session(
            Uuid::new_v4(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1),
            e,
            Duration::from_secs(24 * 3600 + 1)
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn debug_and_errors_redact_token_and_password() {
    let (g, a, _) = gateway().await;
    let s = session(&g, a).await;
    let e = DesktopCredential::basic("u", "gateway-secret-sentinel\n").unwrap_err();
    assert!(!format!("{e:?} {e}").contains("gateway-secret-sentinel"));
    let token = s
        .url
        .split("/session/")
        .nth(1)
        .unwrap()
        .trim_end_matches('/');
    assert!(!format!("{s:?}").contains(token));
    let j = serde_json::to_string(&s).unwrap();
    assert!(j.contains("\"url\""));
    assert!(j.contains(&s.url));
}

#[tokio::test]
async fn concurrent_sessions_are_workspace_scoped_and_revoke_only_target() {
    let (g, a, _) = gateway().await;
    let w = Uuid::new_v4();
    let (x, y) = tokio::join!(
        g.create_session(
            w,
            a,
            DesktopCredential::basic("u", "p").unwrap(),
            Duration::from_secs(60),
        ),
        g.create_session(
            Uuid::new_v4(),
            a,
            DesktopCredential::basic("u", "p").unwrap(),
            Duration::from_secs(60),
        )
    );
    let x = x.unwrap();
    let y = y.unwrap();
    assert_eq!(g.session_count().await, 2);
    g.revoke_workspace(w).await;
    assert_eq!(g.session_count().await, 1);
    assert_eq!(reqwest::get(x.url).await.unwrap().status(), 404);
    assert_eq!(reqwest::get(y.url).await.unwrap().status(), 200);
}

#[tokio::test]
async fn redirects_are_rewritten_under_session_prefix() {
    let (g, a, _) = gateway().await;
    let s = session(&g, a).await;
    let c = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let r = c.get(format!("{}relative", s.url)).send().await.unwrap();
    assert_eq!(
        r.headers().get("location").unwrap().to_str().unwrap(),
        format!("{}/next", s.url.trim_end_matches('/'))
    );
    let target = format!("http://{a}/next?x=1#frag");
    let r = c
        .get(format!("{}echo-location", s.url))
        .header("x-test-location", &target)
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.headers().get("location").unwrap().to_str().unwrap(),
        format!("{}{}/next?x=1#frag", s.url.trim_end_matches('/'), "")
    );
    let foreign = format!("http://127.0.0.1:{}/foreign#f", a.port() + 1);
    let r = c
        .get(format!("{}echo-location", s.url))
        .header("x-test-location", &foreign)
        .send()
        .await
        .unwrap();
    assert_eq!(
        r.headers().get("location").unwrap().to_str().unwrap(),
        foreign
    );
}
