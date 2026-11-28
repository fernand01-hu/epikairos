//! "Mises à jour" page, fed by src/changelog.json.

mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use common::*;

#[test]
fn every_entry_is_complete_in_both_languages_and_newest_first() {
    let entries = epikairos::changelog::entries();
    assert!(!entries.is_empty());
    let mut previous = "9999-12-31".to_string();
    for e in entries {
        assert!(chrono::NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").is_ok(), "bad date {}", e.date);
        assert!(e.date <= previous, "{} should come after {}", e.date, previous);
        previous = e.date.clone();
        assert!(!e.title.fr.trim().is_empty() && !e.title.en.trim().is_empty(), "{}", e.date);
        assert!(!e.new.is_empty() || !e.fixes.is_empty(), "{} has no lines", e.date);
        for line in e.new.iter().chain(&e.fixes) {
            assert!(!line.fr.trim().is_empty() && !line.en.trim().is_empty(), "{}: a line misses a language", e.date);
        }
    }
}

#[tokio::test]
async fn updates_page_is_public_in_both_languages() {
    let app = TestApp::new();
    let first = &epikairos::changelog::entries()[0];

    let fr = app.get("/mises-a-jour", None).await;
    assert_eq!(fr.status, StatusCode::OK, "no login needed");
    assert!(fr.body.contains("Nouveautés") || fr.body.contains("Corrections"));
    assert!(fr.body.contains(&format!(r#"datetime="{}""#, first.date)));

    let req = Request::builder().uri("/updates").header(header::HOST, "localhost:5000").header(header::COOKIE, "lang=en");
    let en = app.send(req.body(Body::empty()).unwrap()).await;
    assert_eq!(en.status, StatusCode::OK);
    assert!(en.body.contains("Updates"));

    // In the menu of every page
    assert!(app.get("/rooms", None).await.body.contains(r#"href="/mises-a-jour""#));
}
