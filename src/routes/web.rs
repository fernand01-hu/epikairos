use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode, Uri},
    response::{Html, IntoResponse, Redirect, Response},
};
use chrono::{DateTime, Utc};
use chrono_tz::Europe::Paris;
use serde::Deserialize;
use tower_cookies::{Cookie, Cookies};

use crate::auriga::get_default_school_year_range;
use crate::accounts::is_admin;
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

/// Template `session` object, including the user's role for role-dependent navigation.
pub async fn session_context(state: &AppState, cookies: &Cookies) -> minijinja::Value {
    let (logged_in, username) = get_session_info(cookies, &state.secret_key);
    let role = match (&username, logged_in) {
        (Some(user), true) => Some(if is_admin(user) { "admin" } else { "user" }),
        _ => None,
    };
    create_session_value(logged_in, username.as_deref(), role, crate::i18n::current_lang(cookies))
}

/// The old landing page was removed: `/` and `/dashboard` go straight to the planning.
pub async fn dashboard_handler() -> Redirect {
    Redirect::to("/personal")
}

pub async fn old_zeus_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> Result<Html<String>, StatusCode> {
    let (start_date, end_date) = get_default_school_year_range();
    let now: DateTime<chrono_tz::Tz> = Utc::now().with_timezone(&Paris);
    let today_str = now.format("%Y-%m-%d").to_string();

    let session_val = session_context(&state, &cookies).await;

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

    let session_val = session_context(&state, &cookies).await;

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

/// Logging out needs a POST from the site's own button: the session cookie is
/// SameSite=Lax, so other sites cannot send that POST with it, and a plain link
/// or image pointing at /logout no longer logs anyone out.
pub async fn logout_page_handler() -> Redirect {
    Redirect::to("/personal")
}

pub async fn logout_handler(cookies: Cookies) -> Redirect {
    clear_session_cookie(&cookies);
    Redirect::to("/login?logged_out=1")
}

/// Removes the session cookie (same attributes as when it was set).
pub fn clear_session_cookie(cookies: &Cookies) {
    let mut cookie = Cookie::new("session", "");
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
    let secure_cookie = std::env::var("SESSION_COOKIE_SECURE")
        .map(|v| v.to_lowercase() == "true" || v == "1")
        .unwrap_or(false);
    cookie.set_secure(secure_cookie);
    cookies.remove(cookie);
}

#[derive(Deserialize)]
pub struct PrivacyQuery {
    deleted: Option<String>,
}

/// What the site stores, and the button to delete your data. Public.
pub async fn privacy_handler(
    State(state): State<AppState>,
    Query(query): Query<PrivacyQuery>,
    cookies: Cookies,
) -> Result<Html<String>, StatusCode> {
    let session_val = session_context(&state, &cookies).await;
    let tmpl = state
        .jinja
        .get_template("privacy.html")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let rendered = tmpl
        .render(minijinja::context! {
            active_page => "privacy",
            deleted => query.deleted.is_some(),
            session => session_val,
        })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Html(rendered))
}

pub async fn personal_handler(
    State(state): State<AppState>,
    cookies: Cookies,
) -> Result<Html<String>, StatusCode> {
    let session_val = session_context(&state, &cookies).await;

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

/// The custom calendar builder was removed; old links land on the planning.
pub async fn custom_calendar_handler() -> Redirect {
    Redirect::to("/planning")
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

    let session_val = session_context(&state, &cookies).await;

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

/// Robots told to stay out, except the bots that build link previews in chat apps.
pub const ROBOTS_TXT: &str = "\
User-agent: Twitterbot
User-agent: facebookexternalhit
User-agent: Discordbot
User-agent: Slackbot-LinkExpanding
User-agent: LinkedInBot
User-agent: WhatsApp
User-agent: TelegramBot
Allow: /

User-agent: *
Disallow: /
";

pub async fn robots_handler() -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], ROBOTS_TXT)
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

pub async fn offline_handler(State(state): State<AppState>, cookies: Cookies) -> Result<Html<String>, StatusCode> {
    let tmpl = state
        .jinja
        .get_template("offline.html")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Cached by the service worker, so it is rendered logged out for everyone.
    let rendered = tmpl
        .render(minijinja::context! { session => create_session_value(false, None, None, crate::i18n::current_lang(&cookies)) })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(rendered))
}

/// When the server started, for the uptime shown on /health.
pub static STARTED_AT: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn db_ok(pool: &crate::db::DbPool) -> bool {
    pool.get()
        .ok()
        .and_then(|c| c.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)).ok())
        == Some(1)
}

/// Health check. Monitors and scripts get JSON; a browser gets a small status page.
/// 200 when both databases answer, 503 otherwise.
pub async fn health_handler(State(state): State<AppState>, cookies: Cookies, headers: HeaderMap) -> Response {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let (school_pool, feed_pool) = (state.db_pool.clone(), state.feed_pool.clone());
    let (school, feeds) = tokio::task::spawn_blocking(move || (db_ok(&school_pool), db_ok(&feed_pool)))
        .await
        .unwrap_or((false, false));
    let ok = school && feeds;
    let status = if ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    let uptime = STARTED_AT.get().map(|t| t.elapsed().as_secs()).unwrap_or(0);

    let wants_html = headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("text/html"));
    if wants_html {
        let session_val = session_context(&state, &cookies).await;
        let strip = crate::uptime::status().await;
        let paris: DateTime<chrono_tz::Tz> = Utc::now().with_timezone(&Paris);
        let rendered = state.jinja.get_template("health.html").and_then(|t| {
            t.render(minijinja::context! {
                ok => ok,
                school_ok => school,
                feeds_ok => feeds,
                uptime_days => uptime / 86400,
                uptime_hours => (uptime % 86400) / 3600,
                uptime_minutes => (uptime % 3600) / 60,
                checked_at => paris.format("%d/%m/%Y %H:%M:%S").to_string(),
                uptime => strip,
                session => session_val,
            })
        });
        return match rendered {
            Ok(html) => (status, [(header::CACHE_CONTROL, "no-store")], Html(html)).into_response(),
            Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
    }

    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(serde_json::json!({
            "status": if ok { "ok" } else { "degraded" },
            "school_db": if school { "ok" } else { "error" },
            "feeds_db": if feeds { "ok" } else { "error" },
            "uptime_secs": uptime,
            "time": now
        })),
    )
        .into_response()
}

/// The 404 page (HTML), with the usual navigation.
pub async fn not_found_page(state: &AppState, cookies: &Cookies) -> Response {
    let session_val = session_context(state, cookies).await;
    match state
        .jinja
        .get_template("404.html")
        .and_then(|t| t.render(minijinja::context! { session => session_val }))
    {
        Ok(html) => (StatusCode::NOT_FOUND, Html(html)).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

/// Any address no route matches: JSON for the API, the 404 page otherwise.
pub async fn fallback_handler(State(state): State<AppState>, cookies: Cookies, uri: Uri) -> Response {
    if uri.path().starts_with("/api/") {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({ "success": false, "error": "Introuvable." })),
        )
            .into_response();
    }
    not_found_page(&state, &cookies).await
}
