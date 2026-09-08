#[test]
fn csp_allows_only_loopback_desktop_connections() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let csp = value["app"]["security"]["csp"].as_str().unwrap();
    assert!(
        !csp.split_whitespace()
            .any(|token| token == "*" || token == "'none'")
    );
    assert!(!csp.contains("http://localhost"));
    assert!(csp.contains("http://127.0.0.1:"));
    assert!(csp.contains("ws://127.0.0.1:"));
    assert!(csp.contains("frame-src 'self' http://127.0.0.1:"));
}
