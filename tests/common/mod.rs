//! Shared helpers: a full app instance on temporary databases, signed session
//! cookies, request shortcuts and course fixtures.
#![allow(dead_code)]

use axum::{
    body::Body,
    http::{header, HeaderMap, Method, Request, StatusCode},
    Router,
};
use chrono::Utc;
use epikairos::{
    build_router, db::models::Course, init_databases, open_pool,
    session::{sign_session, UserSession},
    templates, AppState,
};
use http_body_util::BodyExt;
use std::sync::Once;
use tempfile::TempDir;
use tower::ServiceExt;

pub const SECRET: &str = "integration-test-session-secret-0123456789abcdef";
pub const ADMIN: &str = "fernand.huttois";

static ENV: Once = Once::new();

/// Process-wide settings, applied once before any app is built.
pub fn setup_env() {
    ENV.call_once(|| {
        // Fixed feed-encryption key, so tests never write .server_secret to disk.
        std::env::set_var("SERVER_SECRET_KEY", "integration-test-server-key");
        std::env::remove_var("ADMIN_USERS");
        std::env::remove_var("SERVER_DOMAIN");
        std::env::remove_var("ALLOWED_HOSTS");
        std::env::remove_var("TRUSTED_PROXY");
    });
}

pub struct TestApp {
    pub router: Router,
    pub state: AppState,
    _dir: TempDir,
}

pub struct TestResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: String,
}

impl TestResponse {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|e| panic!("body is not JSON ({e}): {}", self.body))
    }

    pub fn header(&self, name: &str) -> String {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    }

    pub fn location(&self) -> String {
        self.header("location")
    }
}

impl TestApp {
    pub fn new() -> Self {
        setup_env();
        let dir = tempfile::tempdir().expect("temp dir");
        let school = dir.path().join("school.db");
        let feeds = dir.path().join("feeds.db");
        let state = AppState::with_secret(
            open_pool(school.to_str().unwrap()).unwrap(),
            open_pool(feeds.to_str().unwrap()).unwrap(),
            templates::create_template_env(),
            SECRET.to_string(),
        );
        init_databases(&state).expect("init databases");
        Self {
            router: build_router(state.clone()),
            state,
            _dir: dir,
        }
    }

    pub async fn send(&self, req: Request<Body>) -> TestResponse {
        let res = self.router.clone().oneshot(req).await.expect("request");
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.expect("body").to_bytes();
        TestResponse {
            status,
            headers,
            body: String::from_utf8_lossy(&bytes).to_string(),
        }
    }

    pub async fn request(
        &self,
        method: Method,
        path: &str,
        user: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> TestResponse {
        let mut builder = Request::builder().method(method).uri(path).header(header::HOST, "localhost:5000");
        if let Some(u) = user {
            builder = builder.header(header::COOKIE, session_cookie(u));
        }
        let body = match body {
            Some(json) => {
                builder = builder.header(header::CONTENT_TYPE, "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        self.send(builder.body(body).unwrap()).await
    }

    pub async fn get(&self, path: &str, user: Option<&str>) -> TestResponse {
        self.request(Method::GET, path, user, None).await
    }

    pub async fn post(&self, path: &str, user: Option<&str>, body: serde_json::Value) -> TestResponse {
        self.request(Method::POST, path, user, Some(body)).await
    }

    pub async fn put(&self, path: &str, user: Option<&str>, body: serde_json::Value) -> TestResponse {
        self.request(Method::PUT, path, user, Some(body)).await
    }

    pub async fn delete(&self, path: &str, user: Option<&str>) -> TestResponse {
        self.request(Method::DELETE, path, user, None).await
    }

    /// Stores courses in the shared school database, as a login would.
    pub fn ingest(&self, courses: &[Course]) {
        let mut conn = self.state.db_pool.get().unwrap();
        epikairos::db::ingest_courses(&mut conn, courses, true).unwrap();
    }
}

/// `Cookie` header value for a logged-in user, signed with the test secret.
pub fn session_cookie(username: &str) -> String {
    session_cookie_with(username, SECRET, Utc::now().timestamp() + 3600)
}

pub fn session_cookie_with(username: &str, secret: &str, exp: i64) -> String {
    let session = UserSession {
        logged_in: true,
        username: Some(username.to_string()),
        exp,
    };
    format!("session={}", sign_session(&session, secret))
}

/// A course fixture. `start`/`end` are stored as given (use a trailing `Z` for UTC).
pub fn course(id: &str, start: &str, end: &str, name: &str, room: &str, group: &str) -> Course {
    Course {
        id: id.to_string(),
        start: start.to_string(),
        end: end.to_string(),
        duration: 60,
        summary: name.to_string(),
        course_name: name.to_string(),
        course_code: String::new(),
        activity_type: "CM".to_string(),
        activity_name: "Cours magistral".to_string(),
        is_exam: false,
        is_cancelled: false,
        status: "PLANIFIE".to_string(),
        teachers: vec!["Ada Lovelace".to_string()],
        rooms: if room.is_empty() { vec![] } else { vec![room.to_string()] },
        groups: if group.is_empty() { vec![] } else { vec![group.to_string()] },
        description: String::new(),
        searchable_text: name.to_string(),
    }
}

pub fn cancelled(mut c: Course) -> Course {
    c.is_cancelled = true;
    c.status = "ANNULE".to_string();
    c
}
