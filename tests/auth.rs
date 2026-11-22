//! Authentication middleware, sessions, login endpoint and security headers.

mod common;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use chrono::Utc;
use common::*;

#[tokio::test]
async fn public_paths_need_no_session() {
    let app = TestApp::new();
    for path in ["/health", "/login", "/manifest.json", "/sw.js", "/offline", "/static/css/style.css"] {
        let res = app.get(path, None).await;
        assert_eq!(res.status, StatusCode::OK, "{path} should be public");
    }
    assert_eq!(app.get("/health", None).await.json()["status"], "ok");
}

#[tokio::test]
async fn pages_redirect_anonymous_users_to_login() {
    let app = TestApp::new();
    for path in ["/", "/personal", "/planning", "/rooms", "/admin", "/old-zeus"] {
        let res = app.get(path, None).await;
        assert_eq!(res.status, StatusCode::SEE_OTHER, "{path}");
        assert_eq!(res.location(), "/login", "{path}");
    }
}

#[tokio::test]
async fn api_returns_401_json_to_anonymous_users() {
    let app = TestApp::new();
    for path in ["/api/database/rooms-day", "/api/groups/tree", "/api/admin/users", "/api/community/calendars"] {
        let res = app.get(path, None).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "{path}");
        assert_eq!(res.json()["success"], false);
        assert_eq!(res.json()["redirect"], "/login");
    }
    let res = app.post("/api/create-feed", None, serde_json::json!({})).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn valid_session_grants_access() {
    let app = TestApp::new();
    assert_eq!(app.get("/personal", Some("alice.martin")).await.status, StatusCode::OK);
    assert_eq!(app.get("/api/groups/tree", Some("alice.martin")).await.status, StatusCode::OK);
}

async fn get_with_cookie(app: &TestApp, path: &str, cookie: &str) -> TestResponse {
    app.send(
        Request::builder()
            .uri(path)
            .header(header::COOKIE, cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await
}

#[tokio::test]
async fn forged_expired_and_garbage_cookies_are_rejected() {
    let app = TestApp::new();
    let now = Utc::now().timestamp();

    let wrong_key = session_cookie_with("fernand.huttois", "attacker-chosen-secret", now + 3600);
    let expired = session_cookie_with("alice.martin", SECRET, now - 10);
    let mut tampered = session_cookie("alice.martin");
    tampered.push('x');

    for cookie in [wrong_key, expired, tampered, "session=not-a-session".to_string(), "session=".to_string()] {
        let res = get_with_cookie(&app, "/api/groups/tree", &cookie).await;
        assert_eq!(res.status, StatusCode::UNAUTHORIZED, "cookie {cookie} must be rejected");
    }
}

#[tokio::test]
async fn swapping_the_payload_breaks_the_signature() {
    let app = TestApp::new();
    let alice = session_cookie("alice.martin");
    let admin = session_cookie(ADMIN);
    // Alice's signature with the admin's payload
    let admin_payload = admin.trim_start_matches("session=").split('.').next().unwrap();
    let alice_sig = alice.split('.').nth(1).unwrap();
    let forged = format!("session={admin_payload}.{alice_sig}");
    assert_eq!(get_with_cookie(&app, "/admin", &forged).await.status, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn logout_clears_the_session_cookie() {
    let app = TestApp::new();
    let res = app.request(axum::http::Method::POST, "/logout", Some("alice.martin"), None).await;
    assert_eq!(res.status, StatusCode::SEE_OTHER);
    assert_eq!(res.location(), "/login?logged_out=1");
    let cookie = res.header("set-cookie");
    assert!(cookie.starts_with("session=;"), "cookie should be emptied: {cookie}");
    assert!(cookie.contains("Max-Age=0") || cookie.to_lowercase().contains("expires="));
}

#[tokio::test]
async fn a_plain_link_to_logout_does_not_log_out() {
    // <img src="https://epikairos.org/logout"> on another site sends a GET
    let app = TestApp::new();
    let res = app.get("/logout", Some("alice.martin")).await;
    assert!(res.status.is_redirection());
    assert_eq!(res.location(), "/personal");
    assert!(res.header("set-cookie").is_empty(), "session untouched");
}

#[tokio::test]
async fn login_requires_credentials_or_token() {
    let app = TestApp::new();
    let res = app.post("/api/fetch-planning", None, serde_json::json!({})).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert_eq!(res.json()["success"], false);
    assert!(res.header("set-cookie").is_empty(), "no session without a successful login");
}

#[tokio::test]
async fn login_rejects_truncated_and_expired_tokens_without_calling_auriga() {
    let app = TestApp::new();

    let res = app.post("/api/fetch-planning", None, serde_json::json!({ "access_token": "eyJhbGciOi…" })).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert!(res.json()["error"].as_str().unwrap().contains("tronqué"));

    let expired = jwt(serde_json::json!({ "preferred_username": ADMIN, "exp": Utc::now().timestamp() - 60 }));
    let res = app.post("/api/fetch-planning", None, serde_json::json!({ "access_token": expired })).await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
    assert!(res.json()["error"].as_str().unwrap().contains("expiré"));
    assert!(res.header("set-cookie").is_empty());
}

#[tokio::test]
async fn login_is_rate_limited_per_client() {
    let app = TestApp::new();
    for i in 0..10 {
        let res = app.post("/api/fetch-planning", None, serde_json::json!({})).await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST, "attempt {i}");
    }
    let res = app.post("/api/fetch-planning", None, serde_json::json!({})).await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res.json()["success"], false);
}

#[tokio::test]
async fn security_headers_are_set_on_every_response() {
    let app = TestApp::new();
    for res in [app.get("/login", None).await, app.get("/personal", None).await, app.get("/health", None).await] {
        assert_eq!(res.header("x-content-type-options"), "nosniff");
        assert_eq!(res.header("x-frame-options"), "SAMEORIGIN");
        assert_eq!(res.header("referrer-policy"), "strict-origin-when-cross-origin");
        let csp = res.header("content-security-policy");
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("frame-ancestors 'self'"));
        let scripts = csp.split(';').find(|d| d.trim().starts_with("script-src")).unwrap();
        assert!(!scripts.contains("unsafe-inline"), "inline scripts are blocked: {scripts}");
    }
}

#[tokio::test]
async fn oversized_request_bodies_are_refused() {
    let app = TestApp::new();
    let big = "x".repeat(3 * 1024 * 1024);
    let res = app
        .send(
            Request::builder()
                .method("POST")
                .uri("/api/export-ics")
                .header(header::COOKIE, session_cookie("alice.martin"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!("{{\"calendar_name\":\"{big}\",\"courses\":[]}}")))
                .unwrap(),
        )
        .await;
    assert_eq!(res.status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn static_files_cannot_escape_the_static_directory() {
    let app = TestApp::new();
    for path in ["/static/../Cargo.toml", "/static/%2e%2e/Cargo.toml", "/static/..%2fsrc%2fmain.rs"] {
        let res = app.get(path, None).await;
        assert_ne!(res.status, StatusCode::OK, "{path}");
        assert!(!res.body.contains("[package]") && !res.body.contains("fn main"), "{path} leaked a file");
    }
}

/// Unsigned JWT with the given claims (enough for the client-side checks).
fn jwt(claims: serde_json::Value) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    format!(
        "{}.{}.signature",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    )
}

#[tokio::test]
async fn crawlers_are_kept_out_of_the_whole_site() {
    let app = TestApp::new();
    let robots = app.get("/robots.txt", None).await;
    assert_eq!(robots.status, StatusCode::OK, "public, no login needed");
    assert!(robots.header("content-type").starts_with("text/plain"));
    assert_eq!(robots.body, "User-agent: *\nDisallow: /\n");

    for res in [app.get("/login", None).await, app.get("/personal", Some("alice.martin")).await, app.get("/health", None).await] {
        assert_eq!(res.header("x-robots-tag"), "noindex, nofollow, noarchive");
    }
}

#[tokio::test]
async fn pages_have_no_inline_scripts_or_handlers() {
    // Required by the CSP without 'unsafe-inline' for scripts
    let app = TestApp::new();
    for (page, user) in [("/login", None), ("/offline", None), ("/personal", Some("alice.martin")), ("/planning", Some("alice.martin")), ("/rooms", Some("alice.martin")), ("/admin", Some(ADMIN))] {
        let body = app.get(page, user).await.body;
        for chunk in body.split("<script").skip(1) {
            let tag_end = chunk.find('>').unwrap();
            assert!(chunk[..tag_end].contains("src="), "{page} has an inline <script>");
        }
        for handler in [" onclick=", " onload=", " onsubmit=", " onchange=", "javascript:"] {
            assert!(!body.contains(handler), "{page} uses {handler}");
        }
    }
}

#[tokio::test]
async fn https_mode_sends_hsts_only_when_enabled() {
    // The test environment runs without SESSION_COOKIE_SECURE
    let app = TestApp::new();
    assert!(app.get("/login", None).await.header("strict-transport-security").is_empty());
}
