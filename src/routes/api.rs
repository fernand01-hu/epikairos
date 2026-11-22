use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Europe::Paris;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tower_cookies::{Cookie, Cookies};

use crate::auriga::{
    filter_courses, generate_ics_from_courses, get_default_school_year_range, normalize_course,
    AurigaClient,
};
use crate::db::*;
use crate::feed::*;
use crate::session::{sign_session, verify_session, UserSession};
use crate::state::AppState;

const DEFAULT_FEED_CACHE_DURATION: f64 = 43200.0; // 12 hours
/// Minimum delay between Auriga attempts for a feed that has no courses yet.
const FEED_RETRY_INTERVAL: f64 = 600.0;
/// Subscription links one account may own.
pub const MAX_FEEDS_PER_USER: i64 = 20;
const FEED_LIMIT_REACHED: &str = "feed-limit-reached";

/// Feed cache lifetime in seconds, overridable with `FEED_CACHE_DURATION`.
fn feed_cache_duration() -> f64 {
    static DURATION: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
    *DURATION.get_or_init(|| {
        std::env::var("FEED_CACHE_DURATION")
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| *v > 0.0)
            .unwrap_or(DEFAULT_FEED_CACHE_DURATION)
    })
}

/// JSON 500 response so the frontend can always parse `{ success, error }`.
fn internal_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "success": false,
            "error": "Erreur interne du serveur. Veuillez réessayer plus tard."
        })),
    )
        .into_response()
}

/// Truncates a string slice to at most `max_bytes` while respecting UTF-8 char boundaries.
pub fn truncate_str(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &s[..boundary]
}

/// Truncates a mutable `String` to at most `max_bytes` while respecting UTF-8 char boundaries.
pub fn truncate_string(s: &mut String, max_bytes: usize) {
    if s.len() <= max_bytes {
        return;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    s.truncate(boundary);
}

/// Extracts client IP for rate limiting with trusted proxy support and IP validation.
pub fn extract_client_ip(
    headers: &HeaderMap,
    connect_info: Option<&axum::extract::ConnectInfo<std::net::SocketAddr>>,
    trusted_proxy: bool,
) -> String {
    if trusted_proxy {
        let xff_ip = headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.rsplit(',').next().map(|s| s.trim()))
            .filter(|s| s.parse::<std::net::IpAddr>().is_ok())
            .map(|s| s.to_string());

        let x_real_ip = headers
            .get("x-real-ip")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim())
            .filter(|s| s.parse::<std::net::IpAddr>().is_ok())
            .map(|s| s.to_string());

        xff_ip
            .or(x_real_ip)
            .or_else(|| connect_info.map(|ci| ci.0.ip().to_string()))
            .unwrap_or_else(|| "127.0.0.1".to_string())
    } else {
        connect_info
            .map(|ci| ci.0.ip().to_string())
            .unwrap_or_else(|| "127.0.0.1".to_string())
    }
}

/// "https" when the request reached a TLS proxy in front of the app (Cloudflare
/// Tunnel, Caddy...), which reports it in X-Forwarded-Proto; "http" otherwise.
pub fn request_scheme(headers: &HeaderMap) -> &'static str {
    let forwarded_https = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().eq_ignore_ascii_case("https"))
        .unwrap_or(false);
    let cf_https = headers
        .get("cf-visitor")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("\"https\""))
        .unwrap_or(false);
    if forwarded_https || cf_https {
        "https"
    } else {
        "http"
    }
}

/// Resolves a safe host header using ALLOWED_HOSTS and SERVER_DOMAIN whitelists.
pub fn resolve_safe_host(headers: &HeaderMap) -> String {
    let raw_host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim())
        .unwrap_or("");

    let server_domain = std::env::var("SERVER_DOMAIN")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let allowed_hosts = std::env::var("ALLOWED_HOSTS")
        .ok()
        .map(|s| {
            s.split(',')
                .map(|h| h.trim().to_string())
                .filter(|h| !h.is_empty())
                .collect::<Vec<String>>()
        })
        .filter(|v| !v.is_empty());

    let default_fallback = server_domain
        .clone()
        .or_else(|| allowed_hosts.as_ref().and_then(|hosts| hosts.first().cloned()))
        .unwrap_or_else(|| "localhost:5000".to_string());

    let host_valid_chars = Regex::new(r"^[a-zA-Z0-9.:_-]+$").unwrap();

    if !raw_host.is_empty() && host_valid_chars.is_match(raw_host) {
        let mut is_allowed = false;
        if let Some(ref hosts) = allowed_hosts {
            if hosts.iter().any(|h| h.eq_ignore_ascii_case(raw_host)) {
                is_allowed = true;
            }
        }
        if let Some(ref domain) = server_domain {
            if domain.eq_ignore_ascii_case(raw_host) {
                is_allowed = true;
            }
        }
        // Nothing configured: trust the Host the client used to reach us. It only
        // shapes the response to that same client's POST, so it can't be abused to
        // hand someone else a link to another server.
        if allowed_hosts.is_none() && server_domain.is_none() {
            is_allowed = true;
        }

        if is_allowed {
            raw_host.to_string()
        } else {
            default_fallback
        }
    } else {
        default_fallback
    }
}

#[derive(Deserialize)]
pub struct FetchPlanningPayload {
    pub username: Option<String>,
    pub password: Option<String>,
    pub access_token: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    #[serde(default = "default_consent")]
    pub consent: bool,
}

fn default_consent() -> bool {
    true
}

#[derive(Deserialize)]
pub struct FreeRoomsPayload {
    pub date: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
}

#[derive(Deserialize)]
pub struct GroupSchedulePayload {
    #[serde(default)]
    pub patterns: Vec<String>,
    #[serde(default)]
    pub teachers: Vec<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[derive(Deserialize)]
pub struct ExportIcsPayload {
    #[serde(default)]
    pub courses: Vec<Course>,
    pub calendar_name: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateFeedPayload {
    pub username: Option<String>,
    pub password: Option<String>,
    pub include_regex: Option<String>,
    pub exclude_regex: Option<String>,
    pub allowed_types: Option<Vec<String>>,
    pub allowed_courses: Option<Vec<String>>,
    #[serde(default)]
    pub hide_cancelled: bool,
    pub calendar_name: Option<String>,
    #[serde(default)]
    pub courses: Vec<Course>,
}

#[derive(Deserialize)]
pub struct FeedQueryParams {
    pub include: Option<String>,
    pub include_regex: Option<String>,
    pub exclude: Option<String>,
    pub exclude_regex: Option<String>,
    pub hide_cancelled: Option<String>,
    pub course: Option<String>,
    pub courses: Option<String>,
    pub allowed_courses: Option<String>,
}

// -----------------------------------------------------------------------------
// POST /api/fetch-planning
// -----------------------------------------------------------------------------
pub async fn fetch_planning_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    headers: HeaderMap,
    connect_info: Option<axum::extract::ConnectInfo<std::net::SocketAddr>>,
    Json(payload): Json<FetchPlanningPayload>,
) -> Response {
    let trusted_proxy = std::env::var("TRUSTED_PROXY")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let client_ip = extract_client_ip(&headers, connect_info.as_ref(), trusted_proxy);

    if !state.check_login_rate_limit(&client_ip).await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "success": false,
                "error": "Trop de tentatives de connexion. Veuillez patienter une minute avant de réessayer."
            })),
        )
            .into_response();
    }

    let username = payload.username.unwrap_or_default().trim().to_string();
    let password = payload.password.unwrap_or_default().trim().to_string();
    let access_token = payload.access_token.unwrap_or_default().trim().to_string();
    let consent = payload.consent;

    let (def_start, def_end) = get_default_school_year_range();
    let start_date = payload.start_date.unwrap_or(def_start);
    let end_date = payload.end_date.unwrap_or(def_end);

    let (client, detected_user) = if !access_token.is_empty() {
        match AurigaClient::with_access_token(&access_token) {
            Ok(c) => {
                let user_extracted = c.username.clone();
                (c, user_extracted)
            }
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "success": false, "error": e })),
                )
                    .into_response();
            }
        }
    } else if !username.is_empty() && !password.is_empty() {
        let mut c = AurigaClient::new(&username, &password);
        if let Err(e) = c.login().await {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "success": false, "error": e })),
            )
                .into_response();
        }
        let verified = c.username.clone();
        (c, verified)
    } else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Veuillez renseigner vos identifiants ou un jeton d'accès."
            })),
        )
            .into_response();
    };

    let raw_courses = match client.get_planning(&start_date, &end_date).await {
        Ok(courses) => courses,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "success": false, "error": e })),
            )
                .into_response();
        }
    };

    let normalized: Vec<Course> = raw_courses.iter().map(normalize_course).collect();

    // Ingest into database if consented
    if consent && !normalized.is_empty() {
        let pool = state.db_pool.clone();
        let courses_to_ingest = normalized.clone();
        tokio::task::spawn_blocking(move || {
            if let Ok(mut conn) = pool.get() {
                let _ = ingest_courses(&mut conn, &courses_to_ingest, true);
            }
        });
    }

    // Statistics
    let mut types_count: HashMap<String, usize> = HashMap::new();
    let mut cancelled_count = 0;
    for c in &normalized {
        *types_count.entry(c.activity_type.clone()).or_insert(0) += 1;
        if c.is_cancelled {
            cancelled_count += 1;
        }
    }

    let user_display = if detected_user.contains('@') {
        detected_user.split('@').next().unwrap_or(&detected_user).to_string()
    } else if !detected_user.is_empty() {
        detected_user
    } else {
        "Étudiant Auriga".to_string()
    };

    // Keep this account's subscription links current, even those created without a password
    if user_display != "Étudiant Auriga" && !normalized.is_empty() {
        let pool = state.feed_pool.clone();
        let login_name = user_display.clone();
        let fresh = normalized.clone();
        tokio::task::spawn_blocking(move || {
            if let Ok(conn) = pool.get() {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
                if let Ok(n) = update_feeds_for_user(&conn, &login_name, &fresh, now) {
                    if n > 0 {
                        println!("[*] Refreshed {} subscription link(s) from a login", n);
                    }
                }
            }
        });
    }

    // Remember the account so the admin can assign roles from the user list
    if user_display != "Étudiant Auriga" {
        let pool = state.db_pool.clone();
        let login_name = user_display.clone();
        tokio::task::spawn_blocking(move || {
            if let Ok(conn) = pool.get() {
                let _ = crate::community::record_login(&conn, &login_name);
            }
        });
    }

    // Set signed session cookie with 30-day lifetime and secure options
    let session = UserSession {
        logged_in: true,
        username: Some(user_display.clone()),
        exp: Utc::now().timestamp() + crate::session::SESSION_LIFETIME_SECS,
    };
    let signed_val = sign_session(&session, &state.secret_key);
    let mut cookie = Cookie::new("session", signed_val);
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);

    let secure_cookie = std::env::var("SESSION_COOKIE_SECURE")
        .map(|v| v.to_lowercase() == "true" || v == "1")
        .unwrap_or(false);
    cookie.set_secure(secure_cookie);
    cookies.add(cookie);

    Json(serde_json::json!({
        "success": true,
        "courses": normalized,
        "stats": {
            "total": normalized.len(),
            "total_courses": normalized.len(),
            "by_type": types_count,
            "cancelled": cancelled_count,
        },
        "username": user_display,
    }))
    .into_response()
}

// -----------------------------------------------------------------------------
// POST /api/database/free-rooms
// -----------------------------------------------------------------------------
pub async fn db_free_rooms_handler(
    State(state): State<AppState>,
    Json(payload): Json<FreeRoomsPayload>,
) -> Response {
    let now_dt: DateTime<chrono_tz::Tz> = Utc::now().with_timezone(&Paris);
    let target_date = payload
        .date
        .unwrap_or_else(|| now_dt.format("%Y-%m-%d").to_string());
    let start_time = payload.start_time.unwrap_or_else(|| "14:00".to_string());
    let end_time = payload.end_time.unwrap_or_else(|| "16:00".to_string());

    let pool = state.db_pool.clone();
    let res = tokio::task::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        query_free_rooms(&conn, &target_date, &start_time, &end_time).map_err(|e| e.to_string())
    })
    .await;

    match res {
        Ok(Ok(result)) => Json(result).into_response(),
        _ => internal_error(),
    }
}

// -----------------------------------------------------------------------------
// GET /api/database/rooms-day?date=YYYY-MM-DD
// -----------------------------------------------------------------------------
#[derive(Deserialize)]
pub struct RoomsDayQuery {
    pub date: Option<String>,
}

pub async fn db_rooms_day_handler(
    State(state): State<AppState>,
    Query(query): Query<RoomsDayQuery>,
) -> Response {
    let today = Utc::now().with_timezone(&Paris).format("%Y-%m-%d").to_string();
    let date = query
        .date
        .filter(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
        .unwrap_or(today);

    let pool = state.db_pool.clone();
    let res = tokio::task::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        query_rooms_day(&conn, &date).map_err(|e| e.to_string())
    })
    .await;

    match res {
        Ok(Ok(result)) => Json(result).into_response(),
        _ => internal_error(),
    }
}

// -----------------------------------------------------------------------------
// GET /api/groups/tree
// -----------------------------------------------------------------------------
pub async fn groups_tree_handler() -> impl IntoResponse {
    Json(get_epita_group_tree())
}

// -----------------------------------------------------------------------------
// GET /api/teachers
// -----------------------------------------------------------------------------
pub async fn teachers_handler(State(state): State<AppState>) -> Response {
    let pool = state.db_pool.clone();
    let res = tokio::task::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        list_teachers(&conn).map_err(|e| e.to_string())
    })
    .await;

    match res {
        Ok(Ok(rows)) => {
            let teachers: Vec<_> = rows
                .into_iter()
                .map(|(name, courses)| serde_json::json!({ "name": name, "courses": courses }))
                .collect();
            Json(serde_json::json!({ "success": true, "teachers": teachers })).into_response()
        }
        _ => internal_error(),
    }
}

// -----------------------------------------------------------------------------
// POST /api/groups/schedule
// -----------------------------------------------------------------------------
pub async fn groups_schedule_handler(
    State(state): State<AppState>,
    Json(payload): Json<GroupSchedulePayload>,
) -> Response {
    let pool = state.db_pool.clone();
    let res = tokio::task::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        query_courses_by_groups(
            &conn,
            &payload.patterns,
            &payload.teachers,
            payload.start_date.as_deref(),
            payload.end_date.as_deref(),
        )
        .map_err(|e| e.to_string())
    })
    .await;

    match res {
        Ok(Ok(courses)) => {
            let count = courses.len();
            Json(serde_json::json!({
                "success": true,
                "courses": courses,
                "count": count,
            }))
            .into_response()
        }
        _ => internal_error(),
    }
}

// -----------------------------------------------------------------------------
// POST /api/export-ics
// -----------------------------------------------------------------------------
pub async fn export_ics_handler(Json(payload): Json<ExportIcsPayload>) -> Response {
    if payload.courses.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Aucun cours sélectionné pour l'exportation."
            })),
        )
            .into_response();
    }

    let cal_name = payload
        .calendar_name
        .unwrap_or_else(|| "EpiKairos - EPITA Emploi du temps".to_string());
    let safe_cal_name = truncate_str(&cal_name, 120);
    let ics_content = generate_ics_from_courses(&payload.courses, safe_cal_name);

    (
        [
            (header::CONTENT_TYPE, "text/calendar; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"epikairos_planning.ics\"",
            ),
            (
                header::CACHE_CONTROL,
                "no-cache, no-store, must-revalidate",
            ),
        ],
        ics_content,
    )
        .into_response()
}

// -----------------------------------------------------------------------------
// POST /api/create-feed
// -----------------------------------------------------------------------------
pub async fn create_feed_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    headers: HeaderMap,
    Json(payload): Json<CreateFeedPayload>,
) -> Response {
    let session_user = cookies
        .get("session")
        .and_then(|c| verify_session(c.value(), &state.secret_key))
        .filter(|s| s.logged_in)
        .and_then(|s| s.username);

    let mut username = payload.username.clone().unwrap_or_default().trim().to_string();

    if let Some(ref auth_user) = session_user {
        if !username.is_empty() {
            let req_prefix = username.split('@').next().unwrap_or(&username);
            let auth_prefix = auth_user.split('@').next().unwrap_or(auth_user);
            if username != *auth_user && req_prefix != auth_prefix {
                return (
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({
                        "success": false,
                        "error": "Accès refusé : le nom d'utilisateur spécifié ne correspond pas à la session connectée."
                    })),
                )
                    .into_response();
            }
        } else {
            username = auth_user.clone();
        }
    }

    let password = payload.password.unwrap_or_default().trim().to_string();
    let include_regex = payload.include_regex.filter(|s| !s.trim().is_empty());
    let exclude_regex = payload.exclude_regex.filter(|s| !s.trim().is_empty());

    for (label, reg_val) in [
        ("d'inclusion", &include_regex),
        ("d'exclusion", &exclude_regex),
    ] {
        if let Some(r) = reg_val {
            if r.len() > 300 {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "success": false,
                        "error": format!("L'expression Regex {} est trop longue (max 300 caractères).", label)
                    })),
                )
                    .into_response();
            }
            if let Err(e) = Regex::new(r) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "success": false,
                        "error": format!("Expression Regex {} invalide : {}", label, e)
                    })),
                )
                    .into_response();
            }
        }
    }

    if payload.courses.is_empty() && (username.is_empty() || password.is_empty()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "success": false,
                "error": "Veuillez renseigner vos identifiants Auriga ou vous assurer que votre planning contient des cours."
            })),
        )
            .into_response();
    }

    let cal_name = payload
        .calendar_name
        .unwrap_or_else(|| "EpiKairos - EPITA Emploi du temps".to_string());
    let safe_cal_name = truncate_str(&cal_name, 120);
    let initial_ics = if !payload.courses.is_empty() {
        Some(generate_ics_from_courses(&payload.courses, safe_cal_name))
    } else {
        None
    };

    let has_credentials = !username.is_empty() && !password.is_empty();
    let summary_username = if has_credentials { username.clone() } else { String::new() };

    let pool = state.feed_pool.clone();
    let c_name = safe_cal_name.to_string();
    let courses_clone = payload.courses.clone();
    let inc_r = include_regex.clone();
    let exc_r = exclude_regex.clone();
    let types = payload.allowed_types.clone();
    let allowed_c = payload.allowed_courses.clone();
    let hide_canc = payload.hide_cancelled;

    let res = tokio::task::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        if count_feeds_for(&conn, &username).map_err(|e| e.to_string())? >= MAX_FEEDS_PER_USER {
            return Err(FEED_LIMIT_REACHED.to_string());
        }
        create_feed(
            &conn,
            &username,
            &password,
            inc_r.as_deref(),
            exc_r.as_deref(),
            types.as_deref(),
            allowed_c.as_deref(),
            hide_canc,
            &c_name,
            initial_ics.as_deref(),
            Some(&courses_clone),
        )
        .map_err(|e| e.to_string())
    })
    .await;

    let feed_id = match res {
        Ok(Ok(id)) => id,
        Ok(Err(e)) if e == FEED_LIMIT_REACHED => {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(serde_json::json!({
                    "success": false,
                    "error": format!("Vous avez déjà {} liens d'abonnement. Réutilisez un lien existant.", MAX_FEEDS_PER_USER)
                })),
            )
                .into_response();
        }
        _ => return internal_error(),
    };

    let mut query_params = Vec::new();
    if let Some(ref inc) = include_regex {
        query_params.push(format!("include={}", urlencoding::encode(inc)));
    }
    if let Some(ref exc) = exclude_regex {
        query_params.push(format!("exclude={}", urlencoding::encode(exc)));
    }
    if hide_canc {
        query_params.push("hide_cancelled=1".to_string());
    }

    let query_str = if !query_params.is_empty() {
        format!("?{}", query_params.join("&"))
    } else {
        String::new()
    };

    let feed_url = format!("/feed/{}.ics{}", feed_id, query_str);
    let safe_host = resolve_safe_host(&headers);
    let webcal_url = format!("webcal://{}{}", safe_host, feed_url);
    let subscription_url = format!("{}://{}{}", request_scheme(&headers), safe_host, feed_url);

    let filter_summary = serde_json::json!({
        "include_regex": include_regex.unwrap_or_else(|| "Aucun".to_string()),
        "exclude_regex": exclude_regex.unwrap_or_else(|| "Aucun".to_string()),
        "hide_cancelled": hide_canc,
        "allowed_courses_count": payload.allowed_courses.as_ref().map(|v| v.len()),
        "total_courses": payload.courses.len(),
        "sync_frequency": "1 à 2 fois par jour (toutes les 12h)",
        "has_credentials": has_credentials,
        "username": summary_username,
    });

    Json(serde_json::json!({
        "success": true,
        "feed_id": feed_id,
        "feed_url": feed_url,
        "webcal_url": webcal_url,
        "subscription_url": subscription_url,
        "filter_summary": filter_summary,
    }))
    .into_response()
}

// -----------------------------------------------------------------------------
// GET /feed/:feed_id.ics and GET /feed/:feed_id
// -----------------------------------------------------------------------------
pub async fn serve_feed_handler(
    State(state): State<AppState>,
    Path(feed_id_param): Path<String>,
    Query(params): Query<FeedQueryParams>,
) -> Response {
    let feed_id = feed_id_param.strip_suffix(".ics").unwrap_or(&feed_id_param);

    let id_re = Regex::new(r"^[A-Za-z0-9_\-]{1,64}$").unwrap();
    if !id_re.is_match(feed_id) {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain")],
            "Identifiant de flux invalide.",
        )
            .into_response();
    }

    let pool = state.feed_pool.clone();
    let fid = feed_id.to_string();
    let feed_opt = match tokio::task::spawn_blocking(move || {
        let conn = pool.get().ok()?;
        get_feed(&conn, &fid).ok()?
    })
    .await
    {
        Ok(Some(f)) => f,
        _ => {
            return (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/plain")],
                "Flux de calendrier introuvable.",
            )
                .into_response();
        }
    };

    let has_query_overrides = params.include.is_some()
        || params.include_regex.is_some()
        || params.exclude.is_some()
        || params.exclude_regex.is_some()
        || params.hide_cancelled.is_some()
        || params.course.is_some()
        || params.courses.is_some()
        || params.allowed_courses.is_some();

    let include_regex = params
        .include
        .or(params.include_regex)
        .or(feed_opt.include_regex.clone())
        .filter(|s| !s.trim().is_empty())
        .map(|mut s| {
            truncate_string(&mut s, 300);
            s
        });
    let exclude_regex = params
        .exclude
        .or(params.exclude_regex)
        .or(feed_opt.exclude_regex.clone())
        .filter(|s| !s.trim().is_empty())
        .map(|mut s| {
            truncate_string(&mut s, 300);
            s
        });

    if let Some(ref r) = include_regex {
        if let Err(e) = Regex::new(r) {
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                format!("Expression Regex d'inclusion invalide : {}", e),
            )
                .into_response();
        }
    }

    if let Some(ref r) = exclude_regex {
        if let Err(e) = Regex::new(r) {
            return (
                StatusCode::BAD_REQUEST,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                format!("Expression Regex d'exclusion invalide : {}", e),
            )
                .into_response();
        }
    }

    let hide_cancelled = match params.hide_cancelled.as_deref() {
        Some("1") | Some("true") | Some("yes") => true,
        Some("0") | Some("false") | Some("no") => false,
        _ => feed_opt.hide_cancelled,
    };

    let req_courses: Option<Vec<String>> = if let Some(c) = params.course.or(params.courses) {
        Some(vec![c])
    } else if let Some(ac) = params.allowed_courses {
        Some(
            ac.split('\t')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        )
    } else {
        feed_opt.allowed_courses.clone()
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let cache_age = now - feed_opt.last_cached_at;

    // If cache is fresh and no query overrides, return cached directly
    if !has_query_overrides
        && !feed_opt.last_cached_ics.is_empty()
        && cache_age < feed_cache_duration()
    {
        return (
            [
                (header::CONTENT_TYPE, "text/calendar; charset=utf-8"),
                (
                    header::CONTENT_DISPOSITION,
                    "inline; filename=\"planning.ics\"",
                ),
                (
                    header::CACHE_CONTROL,
                    &format!("public, max-age={}, must-revalidate", feed_cache_duration() as u64),
                ),
            ],
            feed_opt.last_cached_ics,
        )
            .into_response();
    }

    let mut raw_courses = feed_opt.cached_courses.clone();

    // Refresh from Auriga when credentials exist and this request wins the refresh slot
    let mut may_refresh = false;
    if feed_opt.password.is_some() && !feed_opt.username.is_empty() {
        let no_courses = raw_courses.is_empty();
        let interval = if no_courses { FEED_RETRY_INTERVAL } else { feed_cache_duration() };
        let pool = state.feed_pool.clone();
        let fid = feed_id.to_string();
        may_refresh = tokio::task::spawn_blocking(move || {
            pool.get()
                .ok()
                .and_then(|conn| claim_feed_refresh(&conn, &fid, interval, now, no_courses).ok())
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false);
    }
    if let (true, Some(pw)) = (may_refresh, feed_opt.password.as_ref()) {
        {
            let mut client = AurigaClient::new(&feed_opt.username, pw);
            if client.login().await.is_ok() {
                let (start_d, end_d) = get_default_school_year_range();
                if let Ok(raw_interventions) = client.get_planning(&start_d, &end_d).await {
                    let normalized: Vec<Course> = raw_interventions.iter().map(normalize_course).collect();
                    if !normalized.is_empty() {
                        raw_courses = normalized.clone();
                        let f_pool = state.feed_pool.clone();
                        let f_id_clone = feed_id.to_string();
                        tokio::task::spawn_blocking(move || {
                            if let Ok(conn) = f_pool.get() {
                                let _ = update_feed_courses(&conn, &f_id_clone, &normalized);
                            }
                        });
                    }
                }
            }
        }
    }

    if !raw_courses.is_empty() {
        let filtered = filter_courses(
            &raw_courses,
            include_regex.as_deref(),
            exclude_regex.as_deref(),
            feed_opt.allowed_types.as_deref(),
            req_courses.as_deref(),
            hide_cancelled,
        )
        .unwrap_or_else(|_| raw_courses.clone());

        let fresh_ics = generate_ics_from_courses(&filtered, &feed_opt.calendar_name);

        if !has_query_overrides {
            let f_pool = state.feed_pool.clone();
            let f_id_clone = feed_id.to_string();
            let ics_clone = fresh_ics.clone();
            tokio::task::spawn_blocking(move || {
                if let Ok(conn) = f_pool.get() {
                    let _ = update_feed_cache(&conn, &f_id_clone, &ics_clone);
                }
            });
        }

        return (
            [
                (header::CONTENT_TYPE, "text/calendar; charset=utf-8"),
                (
                    header::CONTENT_DISPOSITION,
                    "inline; filename=\"planning.ics\"",
                ),
                (
                    header::CACHE_CONTROL,
                    &format!("public, max-age={}, must-revalidate", feed_cache_duration() as u64),
                ),
            ],
            fresh_ics,
        )
            .into_response();
    }

    if !feed_opt.last_cached_ics.is_empty() {
        return (
            [
                (header::CONTENT_TYPE, "text/calendar; charset=utf-8"),
                (
                    header::CONTENT_DISPOSITION,
                    "inline; filename=\"planning.ics\"",
                ),
                (
                    header::CACHE_CONTROL,
                    &format!("public, max-age={}, must-revalidate", feed_cache_duration() as u64),
                ),
            ],
            feed_opt.last_cached_ics,
        )
            .into_response();
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        [(header::CONTENT_TYPE, "text/plain")],
        "Erreur lors de la mise à jour du planning ou aucun cours disponible.",
    )
        .into_response()
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::new();
        for b in s.bytes() {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
                result.push(b as char);
            } else {
                result.push_str(&format!("%{:02X}", b));
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn test_truncate_str_utf8() {
        // String with French accents
        let s = "Mathématiques Appliquées & Électronique";
        let truncated = truncate_str(s, 10);
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
        assert!(truncated.len() <= 10);

        // String with emoji (4 bytes per emoji)
        let s_emoji = "📅 Calendrier EPITA 🚀";
        let trunc_emoji = truncate_str(s_emoji, 3); // 3 bytes is inside '📅' (4 bytes)
        assert_eq!(trunc_emoji, "");

        let trunc_emoji4 = truncate_str(s_emoji, 4);
        assert_eq!(trunc_emoji4, "📅");

        let trunc_full = truncate_str("Short", 100);
        assert_eq!(trunc_full, "Short");
    }

    #[test]
    fn test_truncate_string_utf8() {
        let mut s = "Élève ingénieur".to_string();
        // 'É' is 2 bytes (0..2). Truncating at 1 byte should step back to 0.
        truncate_string(&mut s, 1);
        assert_eq!(s, "");

        let mut s2 = "Élève ingénieur".to_string();
        truncate_string(&mut s2, 2);
        assert_eq!(s2, "É");

        let mut s3 = "Hello world".to_string();
        truncate_string(&mut s3, 5);
        assert_eq!(s3, "Hello");
    }

    #[test]
    fn test_extract_client_ip_trusted_proxy() {
        let mut headers = HeaderMap::new();
        // Malicious client spoofing first IP, trusted proxy appends real IP last
        headers.insert("x-forwarded-for", "1.2.3.4, 203.0.113.195".parse().unwrap());
        let ip = extract_client_ip(&headers, None, true);
        assert_eq!(ip, "203.0.113.195");

        // Invalid IP in x-forwarded-for should fall back to x-real-ip
        headers.insert("x-forwarded-for", "invalid_ip".parse().unwrap());
        headers.insert("x-real-ip", "198.51.100.10".parse().unwrap());
        let ip2 = extract_client_ip(&headers, None, true);
        assert_eq!(ip2, "198.51.100.10");

        // When trusted_proxy is false, ignore forwarded headers
        let ip3 = extract_client_ip(&headers, None, false);
        assert_eq!(ip3, "127.0.0.1");
    }

    #[test]
    fn test_resolve_safe_host_whitelist() {
        let mut headers = HeaderMap::new();
        // Safe localhost
        headers.insert("host", "localhost:5000".parse().unwrap());
        let h = resolve_safe_host(&headers);
        assert_eq!(h, "localhost:5000");

        // Public IP with default env (neither ALLOWED_HOSTS nor SERVER_DOMAIN)
        headers.insert("host", "176.174.155.8:5000".parse().unwrap());
        assert_eq!(resolve_safe_host(&headers), "176.174.155.8:5000");

        // Header injection attempts still fall back
        headers.insert("host", "evil.com/path?x=1".parse().unwrap());
        assert_eq!(resolve_safe_host(&headers), "localhost:5000");
    }

}
