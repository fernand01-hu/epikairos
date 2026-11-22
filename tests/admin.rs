//! Admin page: the list of accounts that logged in, for ADMIN_USERS only.

mod common;

use axum::http::StatusCode;
use common::*;

fn record(app: &TestApp, user: &str) {
    let conn = app.state.db_pool.get().unwrap();
    epikairos::accounts::record_login(&conn, user).unwrap();
}

#[tokio::test]
async fn only_the_admin_sees_the_user_list() {
    let app = TestApp::new();
    record(&app, "alice.martin");
    assert_eq!(app.get("/api/admin/users", Some("alice.martin")).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get("/api/admin/users", None).await.status, StatusCode::UNAUTHORIZED);

    let data = app.get("/api/admin/users", Some(ADMIN)).await.json();
    let users = data["users"].as_array().unwrap();
    assert_eq!(users.len(), 1);
    assert_eq!(users[0]["username"], "alice.martin");
    assert_eq!(users[0]["admin"], false);
    assert!(users[0]["last_seen"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn user_search_and_admin_flag() {
    let app = TestApp::new();
    for u in ["alice.martin", "Bob.Durand@epita.fr", "fernand.huttois"] {
        record(&app, u);
    }
    let found = app.get("/api/admin/users?q=bob", Some(ADMIN)).await.json();
    let names: Vec<&str> = found["users"].as_array().unwrap().iter().map(|u| u["username"].as_str().unwrap()).collect();
    assert_eq!(names, ["bob.durand"], "names are normalized");

    let me = app.get("/api/admin/users?q=fernand", Some(ADMIN)).await.json();
    assert_eq!(me["users"][0]["admin"], true);
}

#[tokio::test]
async fn invalid_names_are_not_recorded() {
    let app = TestApp::new();
    record(&app, "<script>alert(1)</script>");
    let data = app.get("/api/admin/users", Some(ADMIN)).await.json();
    assert!(data["users"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn club_calendars_and_moderators_are_gone() {
    let app = TestApp::new();
    for path in ["/api/community/calendars", "/api/community/events"] {
        assert_eq!(app.get(path, Some(ADMIN)).await.status, StatusCode::NOT_FOUND, "{path}");
    }
    let res = app.post("/api/admin/users/role", Some(ADMIN), serde_json::json!({ "username": "x", "role": "moderator" })).await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    let page = app.get("/admin", Some(ADMIN)).await.body;
    assert!(page.contains(r#"id="userTableBody""#));
    for gone in ["moderator", "Modérateur", "calendarTableBody"] {
        assert!(!page.contains(gone), "{gone}");
    }
}
