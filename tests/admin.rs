//! Admin panel, moderator roles and club calendars permissions.

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;

const ALICE: &str = "alice.martin";
const BOB: &str = "bob.durand";

async fn promote(app: &TestApp, user: &str) {
    let res = app.post("/api/admin/users/role", Some(ADMIN), json!({ "username": user, "role": "moderator" })).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
}

async fn new_calendar(app: &TestApp, owner: &str, name: &str) -> i64 {
    let res = app.post("/api/community/calendars", Some(owner), json!({ "name": name, "color": "#10b981" })).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    res.json()["id"].as_i64().unwrap()
}

fn event(title: &str) -> serde_json::Value {
    json!({ "title": title, "start": "2026-10-22T18:00", "end": "2026-10-22T20:00", "location": "Salle 301" })
}

#[tokio::test]
async fn only_the_admin_reaches_admin_apis() {
    let app = TestApp::new();
    assert_eq!(app.get("/api/admin/users", Some(ALICE)).await.status, StatusCode::FORBIDDEN);
    let res = app.post("/api/admin/users/role", Some(ALICE), json!({ "username": ALICE, "role": "moderator" })).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get("/api/admin/users", Some(ADMIN)).await.status, StatusCode::OK);
}

#[tokio::test]
async fn role_changes_are_validated() {
    let app = TestApp::new();
    let set = |user: &'static str, role: &'static str| app.post("/api/admin/users/role", Some(ADMIN), json!({ "username": user, "role": role }));
    assert_eq!(set(ADMIN, "user").await.status, StatusCode::BAD_REQUEST, "admin role is fixed");
    assert_eq!(set("<script>", "moderator").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(set(ALICE, "superuser").await.status, StatusCode::BAD_REQUEST);
    assert_eq!(set("Alice.Martin@epita.fr", "moderator").await.status, StatusCode::OK);

    let users = app.get("/api/admin/users", Some(ADMIN)).await.json();
    let alice = users["users"].as_array().unwrap().iter().find(|u| u["username"] == ALICE).expect("normalized name");
    assert_eq!(alice["role"], "moderator");
}

#[tokio::test]
async fn students_cannot_create_calendars() {
    let app = TestApp::new();
    let res = app.post("/api/community/calendars", Some(ALICE), json!({ "name": "X", "color": "#3b82f6" })).await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn moderators_manage_only_their_own_calendars() {
    let app = TestApp::new();
    promote(&app, ALICE).await;
    promote(&app, BOB).await;
    let alice_cal = new_calendar(&app, ALICE, "Club Robotique").await;
    let bob_cal = new_calendar(&app, BOB, "BDE").await;

    let res = app.post(&format!("/api/community/calendars/{alice_cal}/events"), Some(ALICE), event("Atelier")).await;
    assert_eq!(res.status, StatusCode::OK);
    let ev = res.json()["id"].as_i64().unwrap();

    // Bob cannot touch Alice's calendar or event
    let edit = json!({ "name": "mine", "color": "#3b82f6" });
    assert_eq!(app.put(&format!("/api/community/calendars/{alice_cal}"), Some(BOB), edit).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.post(&format!("/api/community/calendars/{alice_cal}/events"), Some(BOB), event("x")).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.put(&format!("/api/community/events/{ev}"), Some(BOB), event("hack")).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.delete(&format!("/api/community/calendars/{alice_cal}"), Some(BOB)).await.status, StatusCode::FORBIDDEN);

    // Alice cannot move her event into Bob's calendar
    let mut moved = event("x");
    moved["calendar_id"] = json!(bob_cal);
    assert_eq!(app.put(&format!("/api/community/events/{ev}"), Some(ALICE), moved).await.status, StatusCode::FORBIDDEN);

    // The admin can edit anything
    assert_eq!(app.put(&format!("/api/community/events/{ev}"), Some(ADMIN), event("Atelier v2")).await.status, StatusCode::OK);
}

#[tokio::test]
async fn demoted_moderators_lose_edit_rights_immediately() {
    let app = TestApp::new();
    promote(&app, ALICE).await;
    let cal = new_calendar(&app, ALICE, "Club").await;
    let res = app.post("/api/admin/users/role", Some(ADMIN), json!({ "username": ALICE, "role": "user" })).await;
    assert_eq!(res.status, StatusCode::OK);
    let edit = json!({ "name": "x", "color": "#3b82f6" });
    assert_eq!(app.put(&format!("/api/community/calendars/{cal}"), Some(ALICE), edit).await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn everyone_can_read_events_with_edit_flags() {
    let app = TestApp::new();
    promote(&app, ALICE).await;
    let cal = new_calendar(&app, ALICE, "Club").await;
    app.post(&format!("/api/community/calendars/{cal}/events"), Some(ALICE), event("Atelier")).await;

    let url = format!("/api/community/events?start=2026-10-19&end=2026-10-25&calendars={cal}");
    let as_bob = app.get(&url, Some(BOB)).await.json();
    assert_eq!(as_bob["events"][0]["title"], "Atelier");
    assert_eq!(as_bob["events"][0]["can_edit"], false);
    let as_alice = app.get(&url, Some(ALICE)).await.json();
    assert_eq!(as_alice["events"][0]["can_edit"], true);
}

#[tokio::test]
async fn calendar_and_event_input_is_validated() {
    let app = TestApp::new();
    promote(&app, ALICE).await;
    let bad_color = app.post("/api/community/calendars", Some(ALICE), json!({ "name": "X", "color": "red" })).await;
    assert_eq!(bad_color.status, StatusCode::BAD_REQUEST);

    let cal = new_calendar(&app, ALICE, "Club").await;
    let multi_day = json!({ "title": "X", "start": "2026-10-22T18:00", "end": "2026-10-23T02:00" });
    assert_eq!(app.post(&format!("/api/community/calendars/{cal}/events"), Some(ALICE), multi_day).await.status, StatusCode::BAD_REQUEST);
    let reversed = json!({ "title": "X", "start": "2026-10-22T18:00", "end": "2026-10-22T17:00" });
    assert_eq!(app.post(&format!("/api/community/calendars/{cal}/events"), Some(ALICE), reversed).await.status, StatusCode::BAD_REQUEST);

    let too_wide = format!("/api/community/events?start=2026-01-01&end=2026-12-31&calendars={cal}");
    assert_eq!(app.get(&too_wide, Some(BOB)).await.status, StatusCode::BAD_REQUEST);
}
