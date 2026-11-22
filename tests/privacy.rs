//! Privacy page and "delete my data".

mod common;

use axum::http::StatusCode;
use common::*;

#[tokio::test]
async fn privacy_page_is_public_and_linked_everywhere() {
    let app = TestApp::new();
    let anon = app.get("/confidentialite", None).await;
    assert_eq!(anon.status, StatusCode::OK, "no login needed");
    assert!(anon.body.contains("jamais enregistré") && anon.body.contains(r#"href="/login""#));
    assert!(!anon.body.contains("deleteAccountBtn"), "button only for logged-in users");
    assert_eq!(app.get("/privacy", None).await.status, StatusCode::OK);

    let logged = app.get("/confidentialite", Some("alice.martin")).await.body;
    assert!(logged.contains(r#"id="deleteAccountBtn""#));
    assert!(app.get("/rooms", None).await.body.contains(r#"href="/confidentialite""#), "footer link");
}

#[tokio::test]
async fn deleting_my_data_removes_my_login_and_links_only() {
    let app = TestApp::new();
    {
        let school = app.state.db_pool.get().unwrap();
        epikairos::accounts::record_login(&school, "alice.martin").unwrap();
        epikairos::accounts::record_login(&school, "bob.durand").unwrap();
        let feeds = app.state.feed_pool.get().unwrap();
        for owner in ["alice.martin", "Alice.Martin@epita.fr", "bob.durand"] {
            epikairos::feed::create_feed(&feeds, owner, None, None, None, None, false, "Cal", &[]).unwrap();
        }
    }
    app.ingest(&[course("1", "2026-11-23T08:00:00Z", "2026-11-23T10:00:00Z", "Algo", "Salle 301", "SUP L1")]);

    let res = app.post("/api/account/delete", Some("alice.martin"), serde_json::json!({})).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.json()["deleted_links"], 2, "both login forms");
    assert!(res.header("set-cookie").starts_with("session=;"), "logged out: {}", res.header("set-cookie"));

    let school = app.state.db_pool.get().unwrap();
    let users: Vec<String> = school.prepare("SELECT username FROM users").unwrap()
        .query_map([], |r| r.get(0)).unwrap().filter_map(Result::ok).collect();
    assert_eq!(users, ["bob.durand"]);
    let feeds = app.state.feed_pool.get().unwrap();
    assert_eq!(epikairos::feed::count_feeds_for(&feeds, "alice.martin").unwrap(), 0);
    assert_eq!(epikairos::feed::count_feeds_for(&feeds, "bob.durand").unwrap(), 1, "others untouched");
    let courses: i64 = school.query_row("SELECT COUNT(*) FROM courses", [], |r| r.get(0)).unwrap();
    assert_eq!(courses, 1, "shared courses are not tied to anyone and stay");
}

#[tokio::test]
async fn deleting_needs_a_session_and_a_post() {
    let app = TestApp::new();
    let res = app.post("/api/account/delete", None, serde_json::json!({})).await;
    assert_eq!(res.status, StatusCode::UNAUTHORIZED);
    let get = app.get("/api/account/delete", Some("alice.martin")).await;
    assert_eq!(get.status, StatusCode::METHOD_NOT_ALLOWED, "a link or image can't trigger it");
}
