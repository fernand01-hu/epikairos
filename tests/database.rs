//! Shared school database: ingestion, lookups and group schedules.

mod common;

use axum::http::StatusCode;
use common::*;
use epikairos::db::{self, paris_date_time};
use rusqlite::Connection;

fn memory_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    db::init_db(&conn).unwrap();
    conn
}

#[test]
fn init_db_is_idempotent() {
    let conn = memory_db();
    db::init_db(&conn).unwrap();
    db::init_db(&conn).unwrap();
    let rooms: i64 = conn.query_row("SELECT COUNT(*) FROM rooms", [], |r| r.get(0)).unwrap();
    assert_eq!(rooms, 4, "only the four seeded classrooms, never duplicated");
}

#[test]
fn ingest_inserts_then_updates_by_id() {
    let mut conn = memory_db();
    let c = course("42", "2026-01-15T08:00:00Z", "2026-01-15T09:00:00Z", "Algo", "Salle 301", "SUP L1");
    let first = db::ingest_courses(&mut conn, &[c.clone()], true).unwrap();
    assert_eq!((first["inserted"].as_i64(), first["updated"].as_i64()), (Some(1), Some(0)));

    let mut renamed = c;
    renamed.course_name = "Algorithmique".into();
    let second = db::ingest_courses(&mut conn, &[renamed], true).unwrap();
    assert_eq!((second["inserted"].as_i64(), second["updated"].as_i64()), (Some(0), Some(1)));

    let name: String = conn.query_row("SELECT course_name FROM courses WHERE id = '42'", [], |r| r.get(0)).unwrap();
    assert_eq!(name, "Algorithmique");
}

#[test]
fn reimport_without_rooms_keeps_the_stored_rooms_and_groups() {
    let mut conn = memory_db();
    let full = course("7", "2026-01-15T08:00:00Z", "2026-01-15T09:00:00Z", "Algo", "Salle 301", "SUP L1");
    db::ingest_courses(&mut conn, &[full.clone()], true).unwrap();

    let mut bare = full.clone();
    bare.rooms.clear();
    bare.groups.clear();
    bare.teachers.clear();
    db::ingest_courses(&mut conn, &[bare], true).unwrap();

    let ids = vec!["7".to_string()];
    assert_eq!(db::fetch_course_rooms_batched(&conn, &ids).unwrap()["7"], ["Salle 301"]);
    assert_eq!(db::fetch_course_groups_batched(&conn, &ids).unwrap()["7"], ["SUP L1"]);
    assert_eq!(db::fetch_course_teachers_batched(&conn, &ids).unwrap()["7"], ["Ada Lovelace"]);

    // A new non-empty list still replaces the old one
    let mut moved = full;
    moved.rooms = vec!["Salle 302".into()];
    db::ingest_courses(&mut conn, &[moved], true).unwrap();
    assert_eq!(db::fetch_course_rooms_batched(&conn, &ids).unwrap()["7"], ["Salle 302"]);
}

#[test]
fn ingest_without_consent_stores_nothing() {
    let mut conn = memory_db();
    let c = course("1", "2026-01-15T08:00:00Z", "2026-01-15T09:00:00Z", "Algo", "Salle 301", "");
    let res = db::ingest_courses(&mut conn, &[c], false).unwrap();
    assert_eq!(res["inserted"], 0);
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM courses", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn ingest_skips_courses_without_id_or_dates() {
    let mut conn = memory_db();
    let courses = [
        course("", "2026-01-15T08:00:00Z", "2026-01-15T09:00:00Z", "No id", "", ""),
        course("2", "", "2026-01-15T09:00:00Z", "No start", "", ""),
        course("3", "2026-01-15T08:00:00Z", "2026-01-15T09:00:00Z", "Valid", "", ""),
    ];
    db::ingest_courses(&mut conn, &courses, true).unwrap();
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM courses", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1);
}

#[test]
fn same_slot_courses_of_different_cohorts_are_kept_apart() {
    let mut conn = memory_db();
    let a = course("g1", "2026-10-14T09:00:00", "2026-10-14T10:30:00", "Algo", "Salle 301", "SUP-1");
    let b = course("g2", "2026-10-14T09:00:00", "2026-10-14T10:30:00", "Algo", "Salle 302", "SPE-1");
    db::ingest_courses(&mut conn, &[a], true).unwrap();
    db::ingest_courses(&mut conn, &[b], true).unwrap();

    let ids = vec!["g1".to_string(), "g2".to_string()];
    let rooms = db::fetch_course_rooms_batched(&conn, &ids).unwrap();
    let groups = db::fetch_course_groups_batched(&conn, &ids).unwrap();
    assert_eq!(rooms["g1"], ["Salle 301"]);
    assert_eq!(rooms["g2"], ["Salle 302"]);
    assert_eq!(groups["g1"], ["SUP-1"]);
    assert_eq!(groups["g2"], ["SPE-1"]);
}

#[test]
fn batched_lookups_cover_more_ids_than_one_sql_chunk() {
    let mut conn = memory_db();
    let courses: Vec<_> = (0..850)
        .map(|i| {
            let mut c = course(&format!("c{i:04}"), "2026-10-14T09:00:00", "2026-10-14T10:00:00", "X", &format!("Room {}", i % 5), &format!("G{}", i % 20));
            c.teachers = vec![format!("Prof {}", i % 10)];
            c
        })
        .collect();
    db::ingest_courses(&mut conn, &courses, true).unwrap();

    let ids: Vec<String> = (0..850).map(|i| format!("c{i:04}")).collect();
    let teachers = db::fetch_course_teachers_batched(&conn, &ids).unwrap();
    let rooms = db::fetch_course_rooms_batched(&conn, &ids).unwrap();
    let groups = db::fetch_course_groups_batched(&conn, &ids).unwrap();
    assert_eq!((teachers.len(), rooms.len(), groups.len()), (850, 850, 850));
    assert_eq!(teachers["c0849"], [format!("Prof {}", 849 % 10)]);
    assert_eq!(rooms["c0849"], [format!("Room {}", 849 % 5)]);
    assert_eq!(groups["c0849"], [format!("G{}", 849 % 20)]);
}

#[test]
fn group_schedule_matches_patterns_and_dates_and_skips_cancelled() {
    let mut conn = memory_db();
    db::ingest_courses(
        &mut conn,
        &[
            course("1", "2026-11-02T09:00:00", "2026-11-02T10:00:00", "L3 course", "", "2627 - S3 Lyon L3"),
            course("2", "2026-11-03T09:00:00", "2026-11-03T10:00:00", "L1 course", "", "2627 - S3 Lyon L1"),
            course("3", "2026-12-20T09:00:00", "2026-12-20T10:00:00", "Out of range", "", "2627 - S3 Lyon L3"),
            cancelled(course("4", "2026-11-04T09:00:00", "2026-11-04T10:00:00", "Cancelled", "", "2627 - S3 Lyon L3")),
        ],
        true,
    )
    .unwrap();

    let found = db::query_courses_by_groups(&conn, &["S3 Lyon L3".to_string()], Some("2026-11-01"), Some("2026-11-30")).unwrap();
    let names: Vec<&str> = found.iter().map(|c| c.course_name.as_str()).collect();
    assert_eq!(names, ["L3 course"]);
}

#[test]
fn group_patterns_are_bound_parameters_not_sql() {
    let mut conn = memory_db();
    db::ingest_courses(&mut conn, &[course("1", "2026-11-02T09:00:00", "2026-11-02T10:00:00", "A", "", "G1")], true).unwrap();
    let found = db::query_courses_by_groups(&conn, &["' OR 1=1 --".to_string()], None, None).unwrap();
    assert!(found.is_empty());
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM courses", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1, "table intact");
}

#[test]
fn paris_date_time_handles_utc_offsets_and_local_times() {
    assert_eq!(paris_date_time("2026-01-15T08:00:00Z", "00:00"), ("2026-01-15".into(), "09:00".into()));
    assert_eq!(paris_date_time("2026-07-01T08:00:00Z", "00:00"), ("2026-07-01".into(), "10:00".into()));
    assert_eq!(paris_date_time("2026-07-01T08:00:00+02:00", "00:00"), ("2026-07-01".into(), "08:00".into()));
    assert_eq!(paris_date_time("2026-07-01T08:00:00", "00:00"), ("2026-07-01".into(), "08:00".into()));
    assert_eq!(paris_date_time("2026-07-01", "23:59"), ("2026-07-01".into(), "23:59".into()));
    // Daylight saving switch (29 March 2026, 01:00 UTC)
    assert_eq!(paris_date_time("2026-03-29T00:30:00Z", "00:00").1, "01:30");
    assert_eq!(paris_date_time("2026-03-29T01:30:00Z", "00:00").1, "03:30");
}

#[tokio::test]
async fn group_tree_and_schedule_endpoints() {
    let app = TestApp::new();
    let tree = app.get("/api/groups/tree", Some("alice.martin")).await;
    assert_eq!(tree.status, StatusCode::OK);
    assert!(!tree.json()["children"].as_array().unwrap().is_empty());

    app.ingest(&[course("1", "2026-11-02T09:00:00Z", "2026-11-02T10:00:00Z", "Graphes", "Salle 309", "2627 - S3 Lyon L3")]);
    let res = app
        .post("/api/groups/schedule", Some("alice.martin"), serde_json::json!({ "patterns": ["S3 Lyon L3"] }))
        .await;
    assert_eq!(res.status, StatusCode::OK);
    let data = res.json();
    assert_eq!(data["count"], 1);
    assert_eq!(data["courses"][0]["course_name"], "Graphes");
    assert_eq!(data["courses"][0]["rooms"][0], "Salle 309");
}
