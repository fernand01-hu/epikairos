//! Calendar export (.ics) and subscription links.

mod common;

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use common::*;
use epikairos::auriga::{filter_courses, generate_ics_from_courses};
use epikairos::feed;

const USER: &str = "alice.martin";

fn sample_courses() -> serde_json::Value {
    serde_json::to_value(vec![
        course("1", "2026-01-15T08:00:00Z", "2026-01-15T10:00:00Z", "Algorithmique", "Salle 301", "SUP L1"),
        cancelled(course("2", "2026-01-16T08:00:00Z", "2026-01-16T10:00:00Z", "Annulé", "Salle 302", "SUP L1")),
        course("3", "2026-01-17T08:00:00Z", "2026-01-17T10:00:00Z", "Anglais", "Salle 303", "SUP L1"),
    ])
    .unwrap()
}

async fn create_feed(app: &TestApp, body: serde_json::Value, host: &str) -> TestResponse {
    create_feed_via(app, body, host, None).await
}

async fn create_feed_via(app: &TestApp, body: serde_json::Value, host: &str, forwarded_proto: Option<&str>) -> TestResponse {
    let mut req = Request::builder().method("POST").uri("/api/create-feed");
    if let Some(proto) = forwarded_proto {
        req = req.header("x-forwarded-proto", proto);
    }
    app.send(
        req
            .header(header::HOST, host)
            .header(header::COOKIE, session_cookie(USER))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await
}

// --- .ics generation ----------------------------------------------------------

#[test]
fn ics_is_rfc5545_shaped() {
    let courses: Vec<_> = serde_json::from_value(sample_courses()).unwrap();
    let ics = generate_ics_from_courses(&courses, "Test");
    assert!(ics.starts_with("BEGIN:VCALENDAR\r\n"));
    assert!(ics.trim_end().ends_with("END:VCALENDAR"));
    assert_eq!(ics.matches("BEGIN:VEVENT").count(), 3);
    assert!(ics.contains("X-WR-CALNAME:Test"));
    assert!(ics.contains("\r\nREFRESH-INTERVAL;VALUE=DURATION:PT12H\r\n"));
    assert!(ics.contains("\r\nX-PUBLISHED-TTL:PT12H\r\n"));
    for line in ics.split("\r\n") {
        assert!(line.len() <= 75, "line longer than 75 octets: {line}");
        assert!(!line.contains('\n') && !line.contains('\r'));
    }
}

#[test]
fn ics_times_are_converted_to_paris() {
    let courses = vec![course("1", "2026-01-15T08:00:00Z", "2026-01-15T10:00:00Z", "Algo", "", "")];
    let ics = generate_ics_from_courses(&courses, "Test");
    assert!(ics.contains("DTSTART;TZID=Europe/Paris:20260115T090000"), "{ics}");
    assert!(ics.contains("DTEND;TZID=Europe/Paris:20260115T110000"));
    assert!(ics.contains("UID:auriga-1@epita.fr"));
}

#[test]
fn ics_neutralizes_line_break_injection() {
    let mut evil = course("1", "2026-01-15T08:00:00Z", "2026-01-15T10:00:00Z", "Cours\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Pirate", "", "");
    evil.rooms = vec!["Salle\nLOCATION:ailleurs".into()];
    let ics = generate_ics_from_courses(&[evil], "Cal\r\nX-EVIL:1");
    let event_lines = ics.split("\r\n").filter(|l| *l == "BEGIN:VEVENT").count();
    assert_eq!(event_lines, 1, "injected text must stay inside one escaped property");
    assert!(ics.contains("\\nEND:VEVENT\\nBEGIN:VEVENT"), "line breaks are escaped as \\n");
    assert!(!ics.contains("\r\nSUMMARY:Pirate"));
    assert!(!ics.contains("\r\nX-EVIL"));
    assert!(!ics.contains("\r\nLOCATION:ailleurs"));
}

#[test]
fn course_filters() {
    let courses: Vec<_> = serde_json::from_value(sample_courses()).unwrap();
    let names = |v: Vec<epikairos::db::models::Course>| v.into_iter().map(|c| c.course_name).collect::<Vec<_>>();

    assert_eq!(names(filter_courses(&courses, None, None, None, None, true).unwrap()), ["Algorithmique", "Anglais"]);
    assert_eq!(names(filter_courses(&courses, Some("algo"), None, None, None, false).unwrap()), ["Algorithmique"]);
    assert_eq!(names(filter_courses(&courses, None, Some("Anglais"), None, None, true).unwrap()), ["Algorithmique"]);
    let allowed = vec!["Anglais".to_string()];
    assert_eq!(names(filter_courses(&courses, None, None, None, Some(&allowed), false).unwrap()), ["Anglais"]);
    assert!(filter_courses(&courses, Some("(unclosed"), None, None, None, false).is_err());
}

#[tokio::test]
async fn export_endpoint_returns_an_ics_attachment() {
    let app = TestApp::new();
    let res = app.post("/api/export-ics", Some(USER), serde_json::json!({ "courses": sample_courses() })).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.header("content-type").starts_with("text/calendar"));
    assert!(res.header("content-disposition").starts_with("attachment"));
    assert_eq!(res.body.matches("BEGIN:VEVENT").count(), 3);

    let empty = app.post("/api/export-ics", Some(USER), serde_json::json!({ "courses": [] })).await;
    assert_eq!(empty.status, StatusCode::BAD_REQUEST);
}

// --- subscription feeds ---------------------------------------------------------

#[tokio::test]
async fn feed_link_uses_the_host_the_browser_used() {
    let app = TestApp::new();
    let res = create_feed(&app, serde_json::json!({ "courses": sample_courses(), "hide_cancelled": true }), "176.174.155.8:5000").await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    let data = res.json();
    let webcal = data["webcal_url"].as_str().unwrap();
    assert!(webcal.starts_with("webcal://176.174.155.8:5000/feed/"), "{webcal}");
    assert!(webcal.ends_with(".ics?hide_cancelled=1"));
    assert!(!webcal.contains("localhost"));
}

#[tokio::test]
async fn subscription_link_is_https_behind_the_tunnel() {
    let app = TestApp::new();
    let body = serde_json::json!({ "courses": sample_courses() });

    // Through Cloudflare Tunnel: https on the public domain
    let data = create_feed_via(&app, body.clone(), "epikairos.org", Some("https")).await.json();
    let link = data["subscription_url"].as_str().unwrap();
    assert!(link.starts_with("https://epikairos.org/feed/") && link.ends_with(".ics"), "{link}");
    assert!(data["webcal_url"].as_str().unwrap().starts_with("webcal://epikairos.org/feed/"));

    // Direct access on the IP: plain http, still an absolute link
    let data = create_feed_via(&app, body, "176.174.155.8:5000", None).await.json();
    let link = data["subscription_url"].as_str().unwrap();
    assert!(link.starts_with("http://176.174.155.8:5000/feed/"), "{link}");

    // The served path is the same feed on both addresses
    let path = link.trim_start_matches("http://176.174.155.8:5000");
    assert_eq!(app.get(path, None).await.status, StatusCode::OK);
}

#[tokio::test]
async fn malformed_host_headers_fall_back_safely() {
    let app = TestApp::new();
    let res = create_feed(&app, serde_json::json!({ "courses": sample_courses() }), "evil.com/phish?x=").await;
    let webcal = res.json()["webcal_url"].as_str().unwrap().to_string();
    assert!(webcal.starts_with("webcal://localhost:5000/feed/"), "{webcal}");
}

#[tokio::test]
async fn feeds_cannot_be_created_for_another_account() {
    let app = TestApp::new();
    let res = create_feed(
        &app,
        serde_json::json!({ "username": "bob.durand", "password": "x", "courses": sample_courses() }),
        "localhost:5000",
    )
    .await;
    assert_eq!(res.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn feed_needs_courses_or_credentials() {
    let app = TestApp::new();
    let res = create_feed(&app, serde_json::json!({ "courses": [] }), "localhost:5000").await;
    assert_eq!(res.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn feed_is_served_publicly_and_respects_filters() {
    let app = TestApp::new();
    let created = create_feed(&app, serde_json::json!({ "courses": sample_courses() }), "localhost:5000").await.json();
    let feed_url = created["feed_url"].as_str().unwrap().to_string();

    let res = app.get(&feed_url, None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.header("content-type").starts_with("text/calendar"));
    assert!(res.body.contains("SUMMARY:Algorithmique"));

    // Query overrides are applied on the fly
    let path = feed_url.split('?').next().unwrap();
    let only_algo = app.get(&format!("{path}?include=Algo"), None).await;
    assert_eq!(only_algo.status, StatusCode::OK);
    assert_eq!(only_algo.body.matches("BEGIN:VEVENT").count(), 1);

    let bad_regex = app.get(&format!("{path}?include=(unclosed"), None).await;
    assert_eq!(bad_regex.status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_and_malformed_feed_ids() {
    let app = TestApp::new();
    assert_eq!(app.get("/feed/doesNotExist123.ics", None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(app.get("/feed/bad%20id%3B.ics", None).await.status, StatusCode::BAD_REQUEST);
    let long = "a".repeat(65);
    assert_eq!(app.get(&format!("/feed/{long}.ics"), None).await.status, StatusCode::BAD_REQUEST);
}

#[test]
fn feed_ids_are_long_and_unique() {
    setup_env();
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    feed::init_db(&conn).unwrap();
    let mut ids = std::collections::HashSet::new();
    for i in 0..50 {
        let id = feed::create_feed(&conn, &format!("user{i}"), None, None, None, None, false, "Cal", &[]).unwrap();
        assert!(id.len() >= 20 && id.chars().all(|c| c.is_ascii_alphanumeric()), "{id}");
        assert!(ids.insert(id));
    }
}

// --- stored passwords ----------------------------------------------------------

// --- abuse protections ------------------------------------------------------------

#[tokio::test]
async fn an_account_cannot_create_unlimited_feeds() {
    let app = TestApp::new();
    for i in 0..epikairos::feed::MAX_FEEDS_PER_USER {
        let res = create_feed(&app, serde_json::json!({ "courses": sample_courses() }), "localhost:5000").await;
        assert_eq!(res.status, StatusCode::OK, "feed {i}");
    }
    let res = create_feed(&app, serde_json::json!({ "courses": sample_courses() }), "localhost:5000").await;
    assert_eq!(res.status, StatusCode::TOO_MANY_REQUESTS);
    assert!(res.json()["error"].as_str().unwrap().contains("Supprimez-en un"));
}

#[tokio::test]
async fn a_login_refreshes_the_users_links_without_a_password() {
    let app = TestApp::new();
    // Link created without credentials: only the courses loaded at that time
    let created = create_feed(&app, serde_json::json!({ "courses": sample_courses() }), "localhost:5000").await.json();
    let feed_url = created["feed_url"].as_str().unwrap().split('?').next().unwrap().to_string();
    assert!(!app.get(&feed_url, None).await.body.contains("Nouveau cours"));

    // Someone else's link must not change
    let other_id = {
        let conn = app.state.feed_pool.get().unwrap();
        feed::create_feed(&conn, "bob.durand", None, None, None, None, false, "Cal", &[]).unwrap()
    };

    // Next login of the same student (session name, Auriga may send the @domain form)
    let fresh = vec![course("9", "2026-02-01T08:00:00Z", "2026-02-01T09:00:00Z", "Nouveau cours", "Salle 305", "")];
    let updated = {
        let conn = app.state.feed_pool.get().unwrap();
        feed::update_feeds_for_user(&conn, "Alice.Martin@epita.fr", &fresh).unwrap()
    };
    assert_eq!(updated, 1);

    let body = app.get(&feed_url, None).await.body;
    assert!(body.contains("SUMMARY:Nouveau cours") && body.contains("LOCATION:Salle 305"), "{body}");
    assert!(!body.contains("Algorithmique"), "old courses replaced");

    // Bob's link still has his own (empty) course list
    let conn = app.state.feed_pool.get().unwrap();
    let other: String = conn.query_row("SELECT cached_courses_json FROM feeds WHERE feed_id = ?1", [&other_id], |r| r.get(0)).unwrap();
    assert_eq!(other, "[]");
}

// --- links kept current by classmates' logins -------------------------------------

#[tokio::test]
async fn classmates_logins_keep_a_password_less_link_current() {
    let app = TestApp::new();
    let mut option = course("p2", "2027-03-02T08:00:00Z", "2027-03-02T09:00:00Z", "Japonais", "Salle 330", "Option Japonais");
    option.teachers = vec!["Yuki Sato".into()];
    let mine = vec![
        course("p1", "2027-03-01T08:00:00Z", "2027-03-01T10:00:00Z", "Algorithmique", "Salle 301", "2627 - S3 Lyon L3"),
        option,
    ];
    let created = create_feed(&app, serde_json::json!({ "courses": mine, "hide_cancelled": true }), "localhost:5000").await.json();
    let feed_url = created["feed_url"].as_str().unwrap().to_string();

    // A classmate of the same group logs in later: room change, a new course, a cancellation...
    let moved = course("p1", "2027-03-01T08:00:00Z", "2027-03-01T10:00:00Z", "Algorithmique", "Salle 309", "2627 - S3 Lyon L3");
    let added = course("c9", "2027-03-03T13:00:00Z", "2027-03-03T15:00:00Z", "Graphes", "Salle 305", "2627 - S3 Lyon L3");
    let gone = cancelled(course("c8", "2027-03-04T08:00:00Z", "2027-03-04T09:00:00Z", "Physique", "Salle 302", "2627 - S3 Lyon L3"));
    // ...and a student of another class
    let other = course("o1", "2027-03-01T08:00:00Z", "2027-03-01T09:00:00Z", "Autre classe", "Salle 303", "2627 - S3 Lyon L1");
    app.ingest(&[moved, added, gone, other]);

    let body = app.get(&feed_url, None).await.body;
    assert!(body.contains("LOCATION:Salle 309") && !body.contains("LOCATION:Salle 301"), "room change picked up");
    assert!(body.contains("Graphes"), "new class course picked up");
    assert!(!body.contains("Physique"), "cancelled course hidden (hide_cancelled)");
    assert!(!body.contains("Autre classe"), "other classes never leak in");
    assert!(body.contains("Japonais"), "the student's own option is kept");
}

#[tokio::test]
async fn links_stored_without_groups_find_them_through_the_shared_database() {
    let app = TestApp::new();
    // Old link: its courses were saved before groups were read from Auriga
    let old = course("p1", "2027-03-01T08:00:00Z", "2027-03-01T10:00:00Z", "Algorithmique", "Salle 301", "");
    let created = create_feed(&app, serde_json::json!({ "courses": [old] }), "localhost:5000").await.json();
    let feed_url = created["feed_url"].as_str().unwrap().to_string();

    app.ingest(&[
        course("p1", "2027-03-01T08:00:00Z", "2027-03-01T10:00:00Z", "Algorithmique", "Salle 301", "2627 - S3 Lyon L3"),
        course("c9", "2027-03-03T13:00:00Z", "2027-03-03T15:00:00Z", "Graphes", "Salle 305", "2627 - S3 Lyon L3"),
    ]);
    assert!(app.get(&feed_url, None).await.body.contains("Graphes"));
}

#[test]
fn merge_prefers_the_shared_version_and_keeps_personal_only_courses() {
    let personal = vec![
        course("a", "2027-03-01T08:00:00Z", "2027-03-01T09:00:00Z", "Old title", "Salle 301", "G"),
        course("b", "2027-03-02T08:00:00Z", "2027-03-02T09:00:00Z", "Option", "", "Opt"),
    ];
    let shared = vec![
        course("a", "2027-03-01T08:00:00Z", "2027-03-01T09:00:00Z", "New title", "Salle 309", "G"),
        course("c", "2027-02-28T08:00:00Z", "2027-02-28T09:00:00Z", "Earlier", "", "G"),
    ];
    let merged = feed::merge_shared_courses(personal, shared);
    let names: Vec<&str> = merged.iter().map(|c| c.course_name.as_str()).collect();
    assert_eq!(names, ["Earlier", "New title", "Option"], "sorted by start");
}

// --- subscription links panel ------------------------------------------------------

async fn as_user(app: &TestApp, method: &str, path: &str, user: &str, body: Option<serde_json::Value>) -> TestResponse {
    let m = axum::http::Method::from_bytes(method.as_bytes()).unwrap();
    app.request(m, path, Some(user), body).await
}

#[tokio::test]
async fn links_never_hold_a_password() {
    let app = TestApp::new();
    let res = create_feed(&app, serde_json::json!({ "username": USER, "password": "hunter2", "courses": sample_courses() }), "localhost:5000").await;
    assert_eq!(res.status, StatusCode::OK);
    let conn = app.state.feed_pool.get().unwrap();
    let cols: Vec<String> = conn.prepare("PRAGMA table_info(feeds)").unwrap()
        .query_map([], |r| r.get::<_, String>(1)).unwrap().filter_map(Result::ok).collect();
    assert!(!cols.iter().any(|c| c.contains("password")), "{cols:?}");
    let dump: String = conn.query_row("SELECT group_concat(feed_id || username || COALESCE(cached_courses_json, ''), '') FROM feeds", [], |r| r.get(0)).unwrap();
    assert!(!dump.contains("hunter2"));
}

#[test]
fn old_password_links_are_deleted_and_the_table_rebuilt() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE feeds (feed_id TEXT PRIMARY KEY, username TEXT NOT NULL, encrypted_password TEXT NOT NULL,
             include_regex TEXT, exclude_regex TEXT, allowed_types TEXT, hide_cancelled INTEGER DEFAULT 0,
             calendar_name TEXT, last_cached_ics TEXT, last_cached_at REAL DEFAULT 0, created_at REAL NOT NULL,
             allowed_courses TEXT, cached_courses_json TEXT, refresh_attempted_at REAL DEFAULT 0);
         INSERT INTO feeds (feed_id, username, encrypted_password, created_at, last_cached_at, cached_courses_json, hide_cancelled)
             VALUES ('withpw', 'alice', 'gAAAAAsecret', 1, 5, '[]', 0), ('nopw', 'alice', '', 2, 7, '[]', 1);",
    )
    .unwrap();
    assert_eq!(feed::init_db(&conn).unwrap(), 1, "one password link deleted");
    assert_eq!(feed::init_db(&conn).unwrap(), 0, "idempotent");
    let kept = feed::get_feed(&conn, "nopw").unwrap().expect("password-less link kept");
    assert!(kept.hide_cancelled);
    assert!(feed::get_feed(&conn, "withpw").unwrap().is_none());
    let used: f64 = conn.query_row("SELECT last_used_at FROM feeds WHERE feed_id = 'nopw'", [], |r| r.get(0)).unwrap();
    assert_eq!(used, 7.0, "last fetch time carried over");
}

#[tokio::test]
async fn panel_lists_only_my_links_with_their_addresses() {
    let app = TestApp::new();
    create_feed(&app, serde_json::json!({ "courses": sample_courses(), "hide_cancelled": true, "allowed_courses": ["Algorithmique"] }), "localhost:5000").await;
    {
        let conn = app.state.feed_pool.get().unwrap();
        feed::create_feed(&conn, "bob.durand", None, None, None, None, false, "Cal", &[]).unwrap();
        // An older link of mine saved with the "@epita.fr" form
        feed::create_feed(&conn, "Alice.Martin@epita.fr", None, None, None, None, false, "Cal", &[]).unwrap();
    }
    let data = app.get("/api/feeds", Some(USER)).await.json();
    let feeds = data["feeds"].as_array().unwrap();
    assert_eq!(feeds.len(), 2, "bob's link is not listed");
    assert_eq!(data["max"], 3);
    let newest = feeds.iter().find(|f| f["subjects"] == 1).unwrap();
    assert_eq!(newest["subjects"], 1);
    assert_eq!(newest["hide_cancelled"], true);
    assert!(newest["subscription_url"].as_str().unwrap().contains("/feed/"));
    assert!(newest["webcal_url"].as_str().unwrap().starts_with("webcal://"));
    assert_eq!(app.get("/api/feeds", None).await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn links_can_be_updated_in_place_and_deleted_by_their_owner_only() {
    let app = TestApp::new();
    let created = create_feed(&app, serde_json::json!({ "courses": sample_courses() }), "localhost:5000").await.json();
    let id = created["feed_id"].as_str().unwrap().to_string();
    let feed_path = format!("/feed/{id}.ics");

    // Someone else can neither change nor delete it (answers like a missing link)
    let update = serde_json::json!({ "courses": [course("z", "2027-03-01T08:00:00Z", "2027-03-01T09:00:00Z", "Hack", "", "")] });
    assert_eq!(as_user(&app, "PUT", &format!("/api/feeds/{id}"), "bob.durand", Some(update.clone())).await.status, StatusCode::NOT_FOUND);
    assert_eq!(as_user(&app, "DELETE", &format!("/api/feeds/{id}"), "bob.durand", None).await.status, StatusCode::NOT_FOUND);

    // The owner updates it: same address, new courses and filters
    let mine = serde_json::json!({
        "courses": [course("n", "2027-03-01T08:00:00Z", "2027-03-01T09:00:00Z", "Nouveau", "Salle 301", "")],
        "hide_cancelled": true
    });
    let res = as_user(&app, "PUT", &format!("/api/feeds/{id}"), USER, Some(mine)).await;
    assert_eq!(res.status, StatusCode::OK, "{}", res.body);
    assert!(res.json()["subscription_url"].as_str().unwrap().contains(&id), "same link");
    let body = app.get(&feed_path, None).await.body;
    assert!(body.contains("Nouveau") && !body.contains("Algorithmique"));

    // Then deletes it
    assert_eq!(as_user(&app, "DELETE", &format!("/api/feeds/{id}"), USER, None).await.status, StatusCode::OK);
    assert_eq!(app.get(&feed_path, None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(as_user(&app, "DELETE", &format!("/api/feeds/{id}"), USER, None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(as_user(&app, "DELETE", "/api/feeds/bad%20id", USER, None).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn deleting_a_link_frees_a_slot() {
    let app = TestApp::new();
    let mut ids = Vec::new();
    for _ in 0..3 {
        ids.push(create_feed(&app, serde_json::json!({ "courses": sample_courses() }), "localhost:5000").await.json()["feed_id"].as_str().unwrap().to_string());
    }
    let full = create_feed(&app, serde_json::json!({ "courses": sample_courses() }), "localhost:5000").await;
    assert_eq!(full.status, StatusCode::TOO_MANY_REQUESTS);
    as_user(&app, "DELETE", &format!("/api/feeds/{}", ids[0]), USER, None).await;
    let again = create_feed(&app, serde_json::json!({ "courses": sample_courses() }), "localhost:5000").await;
    assert_eq!(again.status, StatusCode::OK);
}
