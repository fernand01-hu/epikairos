//! /health (JSON for monitors, a page for browsers) and the 404 page.

mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;

async fn get_html(app: &TestApp, path: &str, cookie: Option<String>) -> TestResponse {
    let mut req = Request::builder().uri(path).header(header::HOST, "localhost:5000").header(header::ACCEPT, "text/html,application/xhtml+xml");
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    app.send(req.body(Body::empty()).unwrap()).await
}

#[tokio::test]
async fn health_is_json_for_monitors_and_checks_both_databases() {
    let app = TestApp::new();
    let res = app.get("/health", None).await;
    assert_eq!(res.status, StatusCode::OK);
    let j = res.json();
    assert_eq!(j["status"], "ok");
    assert_eq!(j["school_db"], "ok");
    assert_eq!(j["feeds_db"], "ok");
    assert!(j["uptime_secs"].is_u64());
    assert_eq!(res.header("cache-control"), "no-store");
}

#[tokio::test]
async fn health_is_a_page_for_browsers() {
    let app = TestApp::new();
    let res = get_html(&app, "/health", None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.header("content-type").starts_with("text/html"));
    assert!(res.body.contains("Tout fonctionne") && res.body.contains("Base commune"));
    let en = get_html(&app, "/health", Some("lang=en".into())).await;
    assert!(en.body.contains("All systems working"));
}

#[tokio::test]
async fn unknown_pages_get_the_404_page_without_login() {
    let app = TestApp::new();
    for (path, cookie) in [("/nimporte-quoi", None), ("/a/b/c", Some(session_cookie("alice.martin")))] {
        let res = get_html(&app, path, cookie).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{path}");
        assert!(res.body.contains("Page introuvable") && res.body.contains(r#"href="/planning""#), "{path}");
    }
    // Unknown API addresses answer in JSON
    let api = app.get("/api/nope", None).await;
    assert_eq!(api.status, StatusCode::NOT_FOUND);
    assert_eq!(api.json()["success"], false);
    // Real protected pages still ask for a login
    assert_eq!(app.get("/personal", None).await.status, StatusCode::SEE_OTHER);
    // The admin page stays hidden behind the same 404
    let admin = get_html(&app, "/admin", Some(session_cookie("alice.martin"))).await;
    assert_eq!(admin.status, StatusCode::NOT_FOUND);
    assert!(admin.body.contains("Page introuvable"));
}
