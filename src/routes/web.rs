use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use chrono::{DateTime, Utc};
use chrono_tz::Europe::Paris;
use serde::Deserialize;
use tower_cookies::{Cookie, Cookies};

use crate::auriga::get_default_school_year_range;
use crate::db::get_database_stats;
use crate::session::verify_session;
use crate::state::AppState;
use crate::templates::create_session_value;

#[derive(Deserialize)]
pub struct LoginQuery {
    pub logged_out: Option<String>,
}

#[derive(Deserialize)]
pub struct RoomsQuery {
    pub now: Option<String>,
}

fn get_session_info(cookies: &Cookies, secret: &str) -> (bool, Option<String>) {
    if let Some(cookie) = cookies.get("session") {
        if let Some(sess) = verify_session(cookie.value(), secret) {
            return (sess.logged_in, sess.username);
        } else {
            let mut removal = Cookie::new("session", "");
            removal.set_path("/");
            cookies.remove(removal);
        }
    }
    (false, None)
}

pub async fn dashboard_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> Result<Html<String>, StatusCode> {
    let pool = state.db_pool.clone();
    let stats = tokio::task::spawn_blocking(move || {
        let conn = pool.get().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        get_database_stats(&conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    })
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)??;

    let (logged_in, username) = get_session_info(&cookies, &state.secret_key);
    let session_val = create_session_value(logged_in, username.as_deref());

    let tmpl = state
        .jinja
        .get_template("dashboard.html")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rendered = tmpl
        .render(minijinja::context! {
            stats => stats,
            active_page => "dashboard",
            session => session_val,
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}

pub async fn old_zeus_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> Result<Html<String>, StatusCode> {
    let (start_date, end_date) = get_default_school_year_range();
    let now: DateTime<chrono_tz::Tz> = Utc::now().with_timezone(&Paris);
    let today_str = now.format("%Y-%m-%d").to_string();

    let (logged_in, username) = get_session_info(&cookies, &state.secret_key);
    let session_val = create_session_value(logged_in, username.as_deref());

    let tmpl = state
        .jinja
        .get_template("zeus_classic.html")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rendered = tmpl
        .render(minijinja::context! {
            default_start => start_date,
            default_end => end_date,
            today_date => today_str,
            active_page => "old-zeus",
            session => session_val,
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}

pub async fn login_handler(
    State(state): State<AppState>,
    Query(params): Query<LoginQuery>,
    cookies: Cookies,
) -> Result<Html<String>, StatusCode> {
    let (start_date, end_date) = get_default_school_year_range();
    let is_logged_out = params.logged_out.as_deref() == Some("1");

    let (logged_in, username) = get_session_info(&cookies, &state.secret_key);
    let session_val = create_session_value(logged_in, username.as_deref());

    let tmpl = state
        .jinja
        .get_template("login.html")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rendered = tmpl
        .render(minijinja::context! {
            default_start => start_date,
            default_end => end_date,
            active_page => "login",
            logged_out => is_logged_out,
            session => session_val,
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}

pub async fn logout_handler(cookies: Cookies) -> Redirect {
    let mut cookie = Cookie::new("session", "");
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
    let secure_cookie = std::env::var("SESSION_COOKIE_SECURE")
        .map(|v| v.to_lowercase() == "true" || v == "1")
        .unwrap_or(false);
    cookie.set_secure(secure_cookie);
    cookies.remove(cookie);
    Redirect::to("/login?logged_out=1")
}

pub async fn personal_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> Result<Html<String>, StatusCode> {
    let (logged_in, username) = get_session_info(&cookies, &state.secret_key);
    let session_val = create_session_value(logged_in, username.as_deref());

    let tmpl = state
        .jinja
        .get_template("personal.html")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rendered = tmpl
        .render(minijinja::context! {
            active_page => "personal",
            session => session_val,
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}

pub async fn custom_calendar_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> Result<Html<String>, StatusCode> {
    let (start_date, end_date) = get_default_school_year_range();
    let (logged_in, username) = get_session_info(&cookies, &state.secret_key);
    let session_val = create_session_value(logged_in, username.as_deref());

    let tmpl = state
        .jinja
        .get_template("custom_calendar.html")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rendered = tmpl
        .render(minijinja::context! {
            default_start => start_date,
            default_end => end_date,
            active_page => "custom",
            session => session_val,
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}

pub async fn rooms_handler(
    State(state): State<AppState>,
    Query(query): Query<RoomsQuery>,
    cookies: Cookies,
) -> Result<Html<String>, StatusCode> {
    let now_dt: DateTime<chrono_tz::Tz> = Utc::now().with_timezone(&Paris);
    let today_str = now_dt.format("%Y-%m-%d").to_string();
    let now_hour: u32 = now_dt.format("%H").to_string().parse().unwrap_or(14);
    let now_minute: u32 = now_dt.format("%M").to_string().parse().unwrap_or(0);

    let is_now = matches!(query.now.as_deref(), Some("1") | Some("true") | Some("yes"));

    let (start_h, end_h) = if is_now {
        let start = format!("{:02}:{:02}", now_hour, now_minute);
        let end = format!("{:02}:{:02}", (now_hour + 2) % 24, now_minute);
        (start, end)
    } else if (8..=19).contains(&now_hour) {
        (format!("{:02}:00", now_hour), format!("{:02}:00", now_hour + 2))
    } else {
        ("14:00".to_string(), "16:00".to_string())
    };

    let (logged_in, username) = get_session_info(&cookies, &state.secret_key);
    let session_val = create_session_value(logged_in, username.as_deref());

    let tmpl = state
        .jinja
        .get_template("rooms.html")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rendered = tmpl
        .render(minijinja::context! {
            today_date => today_str,
            default_start_time => start_h,
            default_end_time => end_h,
            is_now => is_now,
            active_page => "rooms",
            session => session_val,
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}

pub async fn manifest_handler() -> impl IntoResponse {
    let content = tokio::fs::read_to_string("static/manifest.json")
        .await
        .unwrap_or_default();
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        content,
    )
}

pub async fn service_worker_handler() -> impl IntoResponse {
    let content = tokio::fs::read_to_string("static/sw.js")
        .await
        .unwrap_or_default();
    (
        [
            ("content-type", "application/javascript"),
            ("service-worker-allowed", "/"),
            ("cache-control", "no-cache, no-store, must-revalidate"),
        ],
        content,
    )
}

pub async fn offline_handler(State(state): State<AppState>) -> Result<Html<String>, StatusCode> {
    let tmpl = state
        .jinja
        .get_template("offline.html")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let rendered = tmpl
        .render(minijinja::context! {})
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}

pub async fn health_handler() -> impl IntoResponse {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    axum::Json(serde_json::json!({
        "status": "ok",
        "time": now
    }))
}
