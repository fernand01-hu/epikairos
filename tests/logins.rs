//! What each login brings to the shared database, the admin view of it, and the
//! removal of courses that Auriga deleted since the account's previous login.

mod common;

use axum::http::StatusCode;
use common::*;
use epikairos::db::models::Course;
use epikairos::logins::{process_login, LoginInput};

const NOW: f64 = 1_795_600_000.0; // 2026-11-24 ~09:46 UTC

fn at(id: &str, day: &str, name: &str, group: &str) -> Course {
    course(id, &format!("{day}T08:00:00Z"), &format!("{day}T10:00:00Z"), name, "Salle 301", group)
}

fn login(app: &TestApp, user: Option<&str>, consent: bool, courses: &[Course]) -> epikairos::logins::LoginReport {
    let mut school = app.state.db_pool.get().unwrap();
    let feeds = app.state.feed_pool.get().unwrap();
    let input = LoginInput {
        username: user,
        consent,
        range_start: "2026-11-01",
        range_end: "2027-01-31",
        courses,
        now: NOW,
    };
    process_login(&mut school, Some(&feeds), &input).unwrap()
}

fn course_ids(app: &TestApp) -> Vec<String> {
    let conn = app.state.db_pool.get().unwrap();
    let mut ids: Vec<String> = conn.prepare("SELECT id FROM courses ORDER BY id").unwrap()
        .query_map([], |r| r.get(0)).unwrap().filter_map(Result::ok).collect();
    ids.sort();
    ids
}

fn future_week() -> Vec<Course> {
    vec![
        at("a", "2026-11-30", "Algèbre", "L3"),
        at("b", "2026-12-01", "Graphes", "L3"),
        at("c", "2026-12-02", "Transistors", "L3"),
        at("d", "2026-12-03", "Anglais", "L3"),
    ]
}

#[tokio::test]
async fn each_login_counts_new_changed_and_unchanged_courses() {
    let app = TestApp::new();
    let first = login(&app, Some("alice.martin"), true, &future_week());
    assert_eq!((first.added, first.updated, first.unchanged), (4, 0, 0));

    let mut again = future_week();
    again[1].rooms = vec!["Salle 305".into()]; // room changed
    let second = login(&app, Some("Alice.Martin@epita.fr"), true, &again);
    assert_eq!((second.added, second.updated, second.unchanged), (0, 1, 3));

    let conn = app.state.db_pool.get().unwrap();
    let events = epikairos::logins::events_for(&conn, "alice.martin").unwrap();
    assert_eq!(events.len(), 2, "both forms of the login are one account");
    assert_eq!((events[0].received, events[0].updated, events[0].unchanged), (4, 1, 3));
    assert_eq!(events[0].groups, ["L3"]);
    let last = epikairos::logins::snapshot_for(&conn, "alice.martin").unwrap().unwrap();
    assert_eq!(last.courses.iter().find(|c| c.id == "b").unwrap().status, "updated");
}

#[tokio::test]
async fn a_course_moved_to_a_new_id_is_not_kept_twice() {
    let app = TestApp::new();
    login(&app, Some("alice.martin"), true, &future_week());

    // Auriga deleted "c" and re-created it on another day as "c2"
    let mut moved = future_week();
    moved[2] = at("c2", "2026-12-04", "Transistors", "L3");
    let report = login(&app, Some("alice.martin"), true, &moved);
    assert_eq!(report.removed, ["c"]);
    assert_eq!(course_ids(&app), ["a", "b", "c2", "d"]);

    let conn = app.state.db_pool.get().unwrap();
    let rooms: i64 = conn.query_row("SELECT COUNT(*) FROM course_rooms WHERE course_id = 'c'", [], |r| r.get(0)).unwrap();
    assert_eq!(rooms, 0, "links of the removed course go too");
    assert_eq!(epikairos::logins::events_for(&conn, "alice.martin").unwrap()[0].removed, 1);
}

#[tokio::test]
async fn past_courses_and_other_students_courses_are_never_removed() {
    let app = TestApp::new();
    let mut week = future_week();
    week.push(at("old", "2026-11-10", "Cours passé", "L3"));
    login(&app, Some("alice.martin"), true, &week);
    login(&app, Some("bob.durand"), true, &[at("bob1", "2026-12-01", "Option", "L3")]);

    // Alice's next answer misses her past course and doesn't list Bob's
    let report = login(&app, Some("alice.martin"), true, &future_week());
    assert!(report.removed.is_empty(), "{:?}", report.removed);
    assert!(course_ids(&app).contains(&"old".to_string()), "past course kept");
    assert!(course_ids(&app).contains(&"bob1".to_string()), "another student's course kept");
}

#[tokio::test]
async fn courses_outside_the_new_date_range_are_not_treated_as_deleted() {
    let app = TestApp::new();
    let mut week = future_week();
    week.push(at("far", "2027-01-20", "Partiel", "L3"));
    login(&app, Some("alice.martin"), true, &week);

    let mut school = app.state.db_pool.get().unwrap();
    let feeds = app.state.feed_pool.get().unwrap();
    let shorter = LoginInput {
        username: Some("alice.martin"),
        consent: true,
        range_start: "2026-11-01",
        range_end: "2026-12-31", // January not asked this time
        courses: &future_week(),
        now: NOW,
    };
    let report = process_login(&mut school, Some(&feeds), &shorter).unwrap();
    assert!(report.removed.is_empty());
    drop(school);
    assert!(course_ids(&app).contains(&"far".to_string()));
}

#[tokio::test]
async fn an_empty_or_partial_answer_removes_nothing() {
    let app = TestApp::new();
    login(&app, Some("alice.martin"), true, &future_week());

    let empty = login(&app, Some("alice.martin"), true, &[]);
    assert!(empty.removed.is_empty());
    assert_eq!(empty.removal_skipped, Some("empty"));

    // 3 of 4 future courses missing at once: looks like a partial answer
    let partial = login(&app, Some("alice.martin"), true, &future_week()[..1]);
    assert!(partial.removed.is_empty());
    assert_eq!(partial.removal_skipped, Some("partial"));
    assert_eq!(course_ids(&app).len(), 4);
}

#[tokio::test]
async fn removed_courses_leave_every_subscription_link() {
    let app = TestApp::new();
    login(&app, Some("alice.martin"), true, &future_week());
    {
        let feeds = app.state.feed_pool.get().unwrap();
        // A classmate's link still holds the old course in its saved copy
        epikairos::feed::create_feed(&feeds, "bob.durand", None, None, None, None, false, "Cal", &future_week()).unwrap();
    }
    let mut moved = future_week();
    moved[2] = at("c2", "2026-12-04", "Transistors", "L3");
    login(&app, Some("alice.martin"), true, &moved);

    let feeds = app.state.feed_pool.get().unwrap();
    let json: String = feeds.query_row("SELECT cached_courses_json FROM feeds", [], |r| r.get(0)).unwrap();
    let ids: Vec<String> = serde_json::from_str::<Vec<Course>>(&json).unwrap().into_iter().map(|c| c.id).collect();
    assert_eq!(ids, ["a", "b", "d"]);
}

#[tokio::test]
async fn no_contribution_means_nothing_saved_and_no_course_list() {
    let app = TestApp::new();
    login(&app, Some("alice.martin"), true, &future_week());
    let report = login(&app, Some("alice.martin"), false, &future_week()[..1]);
    assert_eq!((report.added, report.updated, report.unchanged), (0, 0, 0));
    assert!(report.removed.is_empty());
    assert_eq!(course_ids(&app).len(), 4, "nothing removed either");

    let conn = app.state.db_pool.get().unwrap();
    assert!(epikairos::logins::snapshot_for(&conn, "alice.martin").unwrap().is_none(), "course list dropped");
    let last = &epikairos::logins::events_for(&conn, "alice.martin").unwrap()[0];
    assert!(!last.consent);
    assert_eq!(last.received, 0);
    assert!(last.groups.is_empty());
}

#[tokio::test]
async fn admin_sees_each_login_and_others_are_refused() {
    let app = TestApp::new();
    login(&app, Some("alice.martin"), true, &future_week());

    assert_eq!(app.get("/api/admin/logins?user=alice.martin", Some("bob.durand")).await.status, StatusCode::FORBIDDEN);
    assert_eq!(app.get("/api/admin/logins?user=alice.martin", None).await.status, StatusCode::UNAUTHORIZED);

    let data = app.get("/api/admin/logins?user=alice.martin", Some(ADMIN)).await.json();
    assert_eq!(data["events"][0]["added"], 4);
    assert_eq!(data["last"]["courses"].as_array().unwrap().len(), 4);
    assert_eq!(data["last"]["courses"][0]["status"], "added");

    let page = app.get("/admin", Some(ADMIN)).await.body;
    assert!(page.contains(r#"id="loginsCard""#) && page.contains(r#"id="lastCourses""#));
}

#[tokio::test]
async fn deleting_my_data_forgets_my_logins() {
    let app = TestApp::new();
    login(&app, Some("alice.martin"), true, &future_week());
    let res = app.post("/api/account/delete", Some("alice.martin"), serde_json::json!({})).await;
    assert_eq!(res.status, StatusCode::OK);
    let conn = app.state.db_pool.get().unwrap();
    assert!(epikairos::logins::events_for(&conn, "alice.martin").unwrap().is_empty());
    assert!(epikairos::logins::snapshot_for(&conn, "alice.martin").unwrap().is_none());
}

#[tokio::test]
async fn history_keeps_the_last_fifty_logins() {
    let app = TestApp::new();
    for _ in 0..55 {
        login(&app, Some("alice.martin"), false, &[]);
    }
    let conn = app.state.db_pool.get().unwrap();
    assert_eq!(epikairos::logins::events_for(&conn, "alice.martin").unwrap().len(), 50);
}
