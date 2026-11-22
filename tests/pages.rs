//! HTML pages: rendering, redirects, navigation and removed features.

mod common;

use axum::http::StatusCode;
use common::*;

const USER: &str = "alice.martin";

#[tokio::test]
async fn every_page_renders_for_a_logged_in_user() {
    let app = TestApp::new();
    for path in ["/personal", "/my-calendar", "/planning", "/old-zeus", "/zeus", "/rooms", "/login"] {
        let res = app.get(path, Some(USER)).await;
        assert_eq!(res.status, StatusCode::OK, "{path}");
        assert!(res.header("content-type").starts_with("text/html"), "{path}");
        assert!(res.body.contains("EpiKairos"), "{path}");
    }
}

#[tokio::test]
async fn removed_pages_redirect_to_their_replacement() {
    let app = TestApp::new();
    for (from, to) in [("/", "/personal"), ("/dashboard", "/personal"), ("/custom-calendar", "/planning")] {
        let res = app.get(from, Some(USER)).await;
        assert!(res.status.is_redirection(), "{from} -> {}", res.status);
        assert_eq!(res.location(), to, "{from}");
    }
}

#[tokio::test]
async fn navigation_shows_the_current_sections_only() {
    let app = TestApp::new();
    let body = app.get("/personal", Some(USER)).await.body;
    assert!(body.contains(r#"href="/personal">Mon planning</a>"#));
    assert!(body.contains(r#"href="/planning">Planning</a>"#));
    assert!(body.contains(r#"href="/rooms">Salles libres</a>"#));
    assert!(!body.contains("Old Zeus"));
    assert!(!body.contains("Agendas sur-mesure"));
    assert!(!body.contains("Mes calendriers"));
}

#[tokio::test]
async fn admin_link_and_page_are_for_the_admin_only() {
    let app = TestApp::new();

    let admin_nav = app.get("/personal", Some(ADMIN)).await.body;
    assert!(admin_nav.contains(r#"href="/admin">Admin</a>"#));
    assert_eq!(app.get("/admin", Some(ADMIN)).await.status, StatusCode::OK);

    let user_nav = app.get("/personal", Some(USER)).await.body;
    assert!(!user_nav.contains(r#"href="/admin""#));
    assert_eq!(app.get("/admin", Some(USER)).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_name_matching_ignores_case_and_domain() {
    let app = TestApp::new();
    for name in ["Fernand.Huttois", "fernand.huttois@epita.fr"] {
        assert_eq!(app.get("/admin", Some(name)).await.status, StatusCode::OK, "{name}");
    }
    assert_eq!(app.get("/admin", Some("fernand.huttois2")).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn usernames_are_html_escaped_in_pages() {
    let app = TestApp::new();
    let body = app.get("/personal", Some("<script>alert(1)</script>")).await.body;
    assert!(!body.contains("<script>alert(1)</script>"));
    assert!(body.contains("&lt;script&gt;alert(1)&lt;"));
}

#[tokio::test]
async fn personal_page_has_the_time_grid_and_no_removed_controls() {
    let app = TestApp::new();
    let body = app.get("/personal", Some(USER)).await.body;
    assert!(body.contains(r#"id="weekDaysGrid""#));
    assert!(body.contains(r#"id="downloadIcsBtn""#));
    for gone in ["includeRegexInput", "excludeRegexInput", "viewModeListBtn", "coursesTable", "contributeBtn", "page-header"] {
        assert!(!body.contains(gone), "{gone} should be gone");
    }
}

#[tokio::test]
async fn planning_page_has_no_events_tab_or_collapse_arrow() {
    let app = TestApp::new();
    let body = app.get("/planning", Some(USER)).await.body;
    assert!(body.contains(r#"id="groupTreeContainer""#));
    assert!(body.contains(r#"id="btnToggleSidebar""#));
    assert!(body.contains(r#"id="teacherListContainer""#), "Professeurs section");
    for gone in ["tabEventsBtn", "eventsPanel", "btnCloseSidebar", "toggleSidebarLabel"] {
        assert!(!body.contains(gone), "{gone} should be gone");
    }
}

#[tokio::test]
async fn rooms_page_is_the_floor_plan_with_a_day_picker() {
    let app = TestApp::new();
    let body = app.get("/rooms", Some(USER)).await.body;
    assert!(body.contains(r#"id="floorPlanContainer""#));
    assert!(body.contains(r#"id="dayBtn""#));
    assert!(body.contains("Pris dans moins d"), "legend of the soon state");
    for gone in ["roomSearchForm", "roomFilterInput", "viewListBtn", "page-header"] {
        assert!(!body.contains(gone), "{gone} should be gone");
    }
}

#[tokio::test]
async fn templates_reference_existing_static_files() {
    let app = TestApp::new();
    let mut checked = 0;
    for page in ["/personal", "/planning", "/rooms", "/admin", "/login"] {
        // Attribute values are HTML-escaped ("/" becomes "&#x2f;"), browsers decode them.
        let body = app.get(page, Some(ADMIN)).await.body.replace("&#x2f;", "/");
        for chunk in body.split("/static/").skip(1) {
            let path: String = chunk.chars().take_while(|c| !matches!(c, '"' | '?' | '\'' | ')')).collect();
            let res = app.get(&format!("/static/{path}"), None).await;
            assert_eq!(res.status, StatusCode::OK, "{page} links to missing /static/{path}");
            checked += 1;
        }
    }
    assert!(checked >= 8, "expected the pages to load their CSS and JS");
}

#[tokio::test]
async fn removed_api_endpoints_are_gone() {
    let app = TestApp::new();
    for path in [
        "/api/database/stats",
        "/api/database/options",
        "/api/database/promos",
        "/api/database/recurring-courses",
    ] {
        assert_eq!(app.get(path, Some(USER)).await.status, StatusCode::NOT_FOUND, "{path}");
    }
    for path in ["/api/ingest-consent", "/api/database/detect-patterns", "/api/database/custom-courses"] {
        let res = app.post(path, Some(USER), serde_json::json!({ "courses": [] })).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{path}");
    }
}

#[tokio::test]
async fn service_worker_leaves_other_origins_alone() {
    // Over HTTPS the worker controls the page; if it intercepted the Bootstrap CDN
    // its connect-src 'self' CSP would block the CSS and JS (broken page on iPhone).
    let app = TestApp::new();
    let res = app.get("/sw.js", None).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.header("cache-control").contains("no-cache"));
    assert!(res.body.contains("url.origin !== self.location.origin"));
    assert!(!res.body.contains("cdn.jsdelivr.net"), "CDN files must not be precached by the worker");
}

#[tokio::test]
async fn password_fields_can_be_revealed_and_theme_has_an_auto_mode() {
    let app = TestApp::new();
    let login = app.get("/login", None).await.body;
    assert!(login.contains(r#"data-target="passwordInput""#), "eye button on the login page");
    assert!(login.contains("bi-circle-half"), "theme button starts in automatic mode");
    assert!(login.contains("prefers-color-scheme: dark"));
    // No other page asks for a password anymore (links never store one)
    for page in ["/personal", "/planning"] {
        let body = app.get(page, Some("alice.martin")).await.body;
        assert!(!body.contains(r#"type="password""#), "{page}");
    }
}

#[tokio::test]
async fn login_page_offers_the_token_with_a_guide() {
    let app = TestApp::new();
    let body = app.get("/login", None).await.body;
    assert!(body.contains("trust-notice") && body.contains("bi-exclamation-triangle-fill"));
    assert!(body.contains(r#"id="useTokenLink""#));
    assert!(body.contains("Comment obtenir le jeton ?"));
    assert!(body.contains("myplanning"));
    assert!(!body.contains("5 minutes"));
    let img = app.get("/static/img/token-guide.png", None).await;
    assert_eq!(img.status, StatusCode::OK);
    assert_eq!(img.header("content-type"), "image/png");
}
