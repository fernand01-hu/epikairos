//! Salles libres: day schedule per room and the free-rooms search.

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::Value;

const USER: &str = "alice.martin";

fn room<'a>(data: &'a Value, name: &str) -> &'a Value {
    data["rooms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("room {name} missing"))
}

#[tokio::test]
async fn floor_plan_classrooms_are_listed_even_without_courses() {
    let app = TestApp::new();
    let data = app.get("/api/database/rooms-day?date=2026-03-10", Some(USER)).await.json();
    for name in ["Salle 317", "Salle 327", "Salle 329", "Salle 332"] {
        assert_eq!(room(&data, name)["courses"].as_array().unwrap().len(), 0);
    }
}

#[tokio::test]
async fn day_schedule_lists_courses_in_paris_time() {
    let app = TestApp::new();
    app.ingest(&[
        // Winter: UTC+1
        course("w1", "2026-01-15T08:00:00Z", "2026-01-15T10:00:00Z", "Algorithmique", "Salle 301", "SUP L1"),
        // Summer: UTC+2
        course("s1", "2026-06-15T08:00:00Z", "2026-06-15T09:30:00Z", "Physique", "Salle 301", "SUP L1"),
        // No offset: already local time
        course("l1", "2026-01-15T14:00:00", "2026-01-15T15:00:00", "Anglais", "Salle 301", "SUP L1"),
    ]);

    let winter = app.get("/api/database/rooms-day?date=2026-01-15", Some(USER)).await.json();
    let courses = room(&winter, "Salle 301")["courses"].as_array().unwrap().clone();
    assert_eq!(courses.len(), 2);
    assert_eq!((courses[0]["start"].as_str(), courses[0]["end"].as_str()), (Some("09:00"), Some("11:00")));
    assert_eq!(courses[0]["title"], "Algorithmique");
    assert_eq!(courses[0]["teachers"][0], "Ada Lovelace");
    assert_eq!(courses[1]["start"], "14:00");

    let summer = app.get("/api/database/rooms-day?date=2026-06-15", Some(USER)).await.json();
    let courses = room(&summer, "Salle 301")["courses"].as_array().unwrap().clone();
    assert_eq!((courses[0]["start"].as_str(), courses[0]["end"].as_str()), (Some("10:00"), Some("11:30")));
}

#[tokio::test]
async fn late_utc_course_lands_on_the_next_paris_day() {
    let app = TestApp::new();
    app.ingest(&[course("n1", "2026-01-15T23:30:00Z", "2026-01-16T00:30:00Z", "Nuit", "Salle 302", "")]);
    let next_day = app.get("/api/database/rooms-day?date=2026-01-16", Some(USER)).await.json();
    assert_eq!(room(&next_day, "Salle 302")["courses"][0]["start"], "00:30");
    let same_day = app.get("/api/database/rooms-day?date=2026-01-15", Some(USER)).await.json();
    assert!(room(&same_day, "Salle 302")["courses"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn day_schedule_is_sorted_and_skips_cancelled_courses() {
    let app = TestApp::new();
    app.ingest(&[
        course("b", "2026-02-02T15:00:00", "2026-02-02T16:00:00", "Second", "Salle 303", ""),
        course("a", "2026-02-02T09:00:00", "2026-02-02T10:00:00", "First", "Salle 303", ""),
        cancelled(course("x", "2026-02-02T11:00:00", "2026-02-02T12:00:00", "Cancelled", "Salle 303", "")),
    ]);
    let data = app.get("/api/database/rooms-day?date=2026-02-02", Some(USER)).await.json();
    let titles: Vec<&str> = room(&data, "Salle 303")["courses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["First", "Second"]);
}

#[tokio::test]
async fn invalid_dates_fall_back_to_today() {
    let app = TestApp::new();
    let today = chrono::Utc::now().with_timezone(&chrono_tz::Europe::Paris).format("%Y-%m-%d").to_string();
    for query in ["", "?date=yesterday", "?date=2026-13-45", "?date=2026-01-01'%20OR%201=1--"] {
        let res = app.get(&format!("/api/database/rooms-day{query}"), Some(USER)).await;
        assert_eq!(res.status, StatusCode::OK, "{query}");
        assert_eq!(res.json()["date"], today, "{query}");
    }
}

#[tokio::test]
async fn free_rooms_search_splits_free_and_busy_rooms() {
    let app = TestApp::new();
    app.ingest(&[
        course("c1", "2026-03-03T09:00:00Z", "2026-03-03T11:00:00Z", "Busy now", "Salle 305", ""),
        course("c2", "2026-03-03T13:00:00Z", "2026-03-03T14:00:00Z", "Later", "Salle 308", ""),
    ]);
    // 10:30-11:00 Paris = 09:30-10:00 UTC: 305 busy, 308 free until 14:00 Paris
    let data = app
        .post(
            "/api/database/free-rooms",
            Some(USER),
            serde_json::json!({ "date": "2026-03-03", "start_time": "10:30", "end_time": "11:00" }),
        )
        .await
        .json();
    let busy: Vec<&str> = data["occupied_rooms"].as_array().unwrap().iter().map(|r| r["room_name"].as_str().unwrap()).collect();
    assert_eq!(busy, ["Salle 305"]);
    let room_308 = data["free_rooms"].as_array().unwrap().iter().find(|r| r["room_name"] == "Salle 308").unwrap();
    assert_eq!(room_308["next_course"]["start"], "14:00");
    assert_eq!(data["occupied_count"], 1);
}

#[tokio::test]
async fn utc_times_stored_by_older_versions_are_repaired_at_startup() {
    let app = TestApp::new();
    app.ingest(&[course("old", "2026-01-20T08:00:00Z", "2026-01-20T09:00:00Z", "Legacy", "Salle 309", "")]);
    {
        // Simulate the old bug: the UTC hour stored as local time.
        let conn = app.state.db_pool.get().unwrap();
        conn.execute("UPDATE courses SET start_time_str = '08:00', end_time_str = '09:00' WHERE id = 'old'", []).unwrap();
        epikairos::db::init_db(&conn).unwrap();
    }
    let data = app.get("/api/database/rooms-day?date=2026-01-20", Some(USER)).await.json();
    assert_eq!(room(&data, "Salle 309")["courses"][0]["start"], "09:00");
}
