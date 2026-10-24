//! Admin panel and community calendars (clubs, associations...).
//!
//! Every handler re-reads the role from the database, so promoting or demoting a
//! moderator takes effect on their next request without a new login.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Json,
};
use chrono::NaiveDate;
use serde::Deserialize;
use tower_cookies::Cookies;

use crate::community::{self, normalize_username, Role, MAX_EVENT_RANGE_DAYS};
use crate::routes::web::session_context;
use crate::session::verify_session;
use crate::state::AppState;

fn json_error(status: StatusCode, message: impl Into<String>) -> Response {
    (
        status,
        Json(serde_json::json!({ "success": false, "error": message.into() })),
    )
        .into_response()
}

fn internal_error() -> Response {
    json_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Erreur interne du serveur. Veuillez réessayer plus tard.",
    )
}

fn forbidden() -> Response {
    json_error(StatusCode::FORBIDDEN, "Action réservée aux modérateurs de ce calendrier.")
}

/// The logged-in user and their current role, or a 401 response.
async fn current_user(state: &AppState, cookies: &Cookies) -> Result<(String, Role), Response> {
    let username = cookies
        .get("session")
        .and_then(|c| verify_session(c.value(), &state.secret_key))
        .filter(|s| s.logged_in)
        .and_then(|s| s.username)
        .map(|u| normalize_username(&u))
        .filter(|u| !u.is_empty())
        .ok_or_else(|| json_error(StatusCode::UNAUTHORIZED, "Authentification requise."))?;
    let role = community::lookup_role(&state.db_pool, &username).await;
    Ok((username, role))
}

/// Runs a blocking closure on a pooled school.db connection.
async fn with_conn<T, F>(state: &AppState, f: F) -> Result<T, Response>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection) -> Result<T, String> + Send + 'static,
{
    let pool = state.db_pool.clone();
    tokio::task::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        f(&conn)
    })
    .await
    .map_err(|_| internal_error())?
    .map_err(|e| json_error(StatusCode::BAD_REQUEST, e))
}

// -----------------------------------------------------------------------------
// GET /admin
// -----------------------------------------------------------------------------
pub async fn admin_page_handler(State(state): State<AppState>, cookies: Cookies) -> Response {
    match current_user(&state, &cookies).await {
        Ok((_, Role::Admin)) => {}
        // Hide the page's existence from everyone else
        _ => return (StatusCode::NOT_FOUND, Html("Not Found")).into_response(),
    }

    let session_val = session_context(&state, &cookies).await;
    let rendered = state.jinja.get_template("admin.html").and_then(|tmpl| {
        tmpl.render(minijinja::context! {
            active_page => "admin",
            session => session_val,
        })
    });
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

// -----------------------------------------------------------------------------
// GET /api/admin/users
// -----------------------------------------------------------------------------
#[derive(Deserialize)]
pub struct UsersQuery {
    pub q: Option<String>,
}

pub async fn admin_list_users_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Query(query): Query<UsersQuery>,
) -> Response {
    match current_user(&state, &cookies).await {
        Ok((_, Role::Admin)) => {}
        Ok(_) => return json_error(StatusCode::FORBIDDEN, "Réservé à l'administrateur."),
        Err(resp) => return resp,
    }

    let q = query.q.map(|s| s.chars().take(64).collect::<String>());
    match with_conn(&state, move |conn| {
        community::list_users(conn, q.as_deref(), 200).map_err(|e| e.to_string())
    })
    .await
    {
        Ok(users) => Json(serde_json::json!({
            "success": true,
            "users": users,
            "admins": community::admin_usernames(),
        }))
        .into_response(),
        Err(resp) => resp,
    }
}

// -----------------------------------------------------------------------------
// POST /api/admin/users/role
// -----------------------------------------------------------------------------
#[derive(Deserialize)]
pub struct SetRolePayload {
    pub username: String,
    pub role: String,
}

pub async fn admin_set_role_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Json(payload): Json<SetRolePayload>,
) -> Response {
    match current_user(&state, &cookies).await {
        Ok((_, Role::Admin)) => {}
        Ok(_) => return json_error(StatusCode::FORBIDDEN, "Réservé à l'administrateur."),
        Err(resp) => return resp,
    }

    let role = match payload.role.as_str() {
        "moderator" => Role::Moderator,
        "user" => Role::User,
        _ => return json_error(StatusCode::BAD_REQUEST, "Rôle inconnu."),
    };
    let username = normalize_username(&payload.username);
    let target = username.clone();
    match with_conn(&state, move |conn| community::set_role(conn, &target, role)).await {
        Ok(()) => Json(serde_json::json!({ "success": true, "username": username, "role": role })).into_response(),
        Err(resp) => resp,
    }
}

// -----------------------------------------------------------------------------
// Calendars: GET/POST /api/community/calendars, PUT/DELETE /api/community/calendars/:id
// -----------------------------------------------------------------------------
#[derive(Deserialize)]
pub struct CalendarPayload {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub color: String,
}

pub async fn list_calendars_handler(State(state): State<AppState>, cookies: Cookies) -> Response {
    let (username, role) = match current_user(&state, &cookies).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    match with_conn(&state, |conn| community::list_calendars(conn).map_err(|e| e.to_string())).await {
        Ok(calendars) => {
            let calendars: Vec<serde_json::Value> = calendars
                .into_iter()
                .map(|c| {
                    let can_edit = role.can_edit_calendar(&username, &c.owner);
                    let mut value = serde_json::to_value(&c).unwrap_or_default();
                    value["can_edit"] = serde_json::Value::Bool(can_edit);
                    value
                })
                .collect();
            Json(serde_json::json!({
                "success": true,
                "can_create": role.can_moderate(),
                "role": role,
                "calendars": calendars,
            }))
            .into_response()
        }
        Err(resp) => resp,
    }
}

pub async fn create_calendar_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Json(payload): Json<CalendarPayload>,
) -> Response {
    let (username, role) = match current_user(&state, &cookies).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };
    if !role.can_moderate() {
        return json_error(StatusCode::FORBIDDEN, "Seuls les modérateurs peuvent créer un calendrier.");
    }
    let input = match community::validate_calendar_input(&payload.name, &payload.description, &payload.color) {
        Ok(input) => input,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, e),
    };

    match with_conn(&state, move |conn| community::create_calendar(conn, &username, &input)).await {
        Ok(id) => Json(serde_json::json!({ "success": true, "id": id })).into_response(),
        Err(resp) => resp,
    }
}

/// Loads a calendar and checks the caller may edit it.
async fn editable_calendar(
    state: &AppState,
    cookies: &Cookies,
    id: i64,
) -> Result<(String, community::CalendarRow), Response> {
    let (username, role) = current_user(state, cookies).await?;
    let calendar = with_conn(state, move |conn| community::get_calendar(conn, id).map_err(|e| e.to_string()))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "Calendrier introuvable."))?;
    if !role.can_edit_calendar(&username, &calendar.owner) {
        return Err(forbidden());
    }
    Ok((username, calendar))
}

pub async fn update_calendar_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Path(id): Path<i64>,
    Json(payload): Json<CalendarPayload>,
) -> Response {
    if let Err(resp) = editable_calendar(&state, &cookies, id).await {
        return resp;
    }
    let input = match community::validate_calendar_input(&payload.name, &payload.description, &payload.color) {
        Ok(input) => input,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, e),
    };
    match with_conn(&state, move |conn| community::update_calendar(conn, id, &input).map_err(|e| e.to_string())).await {
        Ok(()) => Json(serde_json::json!({ "success": true })).into_response(),
        Err(resp) => resp,
    }
}

pub async fn delete_calendar_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Path(id): Path<i64>,
) -> Response {
    if let Err(resp) = editable_calendar(&state, &cookies, id).await {
        return resp;
    }
    match with_conn(&state, move |conn| community::delete_calendar(conn, id).map_err(|e| e.to_string())).await {
        Ok(()) => Json(serde_json::json!({ "success": true })).into_response(),
        Err(resp) => resp,
    }
}

// -----------------------------------------------------------------------------
// Events: GET /api/community/events, POST /api/community/calendars/:id/events,
//         PUT/DELETE /api/community/events/:id
// -----------------------------------------------------------------------------
#[derive(Deserialize)]
pub struct EventsQuery {
    pub start: String,
    pub end: String,
    /// Comma-separated calendar ids
    pub calendars: Option<String>,
}

#[derive(Deserialize)]
pub struct EventPayload {
    pub title: String,
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub description: String,
    /// Optional on update: moves the event to another calendar the user can edit
    pub calendar_id: Option<i64>,
}

pub async fn list_events_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Query(query): Query<EventsQuery>,
) -> Response {
    let (username, role) = match current_user(&state, &cookies).await {
        Ok(user) => user,
        Err(resp) => return resp,
    };

    let start = NaiveDate::parse_from_str(&query.start, "%Y-%m-%d");
    let end = NaiveDate::parse_from_str(&query.end, "%Y-%m-%d");
    let (start, end) = match (start, end) {
        (Ok(s), Ok(e)) if e >= s && (e - s).num_days() <= MAX_EVENT_RANGE_DAYS => (s, e),
        _ => return json_error(StatusCode::BAD_REQUEST, "Période invalide."),
    };
    let calendar_ids: Vec<i64> = query
        .calendars
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .take(100)
        .collect();

    let start_str = start.format("%Y-%m-%d").to_string();
    let end_str = end.format("%Y-%m-%d").to_string();
    match with_conn(&state, move |conn| {
        community::list_events(conn, &calendar_ids, &start_str, &end_str).map_err(|e| e.to_string())
    })
    .await
    {
        Ok(events) => {
            let events: Vec<serde_json::Value> = events
                .into_iter()
                .map(|e| {
                    let can_edit = role.can_edit_calendar(&username, &e.calendar_owner);
                    let mut value = serde_json::to_value(&e).unwrap_or_default();
                    value["can_edit"] = serde_json::Value::Bool(can_edit);
                    value
                })
                .collect();
            Json(serde_json::json!({ "success": true, "events": events })).into_response()
        }
        Err(resp) => resp,
    }
}

pub async fn create_event_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Path(calendar_id): Path<i64>,
    Json(payload): Json<EventPayload>,
) -> Response {
    let username = match editable_calendar(&state, &cookies, calendar_id).await {
        Ok((username, _)) => username,
        Err(resp) => return resp,
    };
    let input = match community::validate_event_input(
        &payload.title,
        &payload.start,
        &payload.end,
        &payload.location,
        &payload.description,
    ) {
        Ok(input) => input,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, e),
    };
    match with_conn(&state, move |conn| community::create_event(conn, calendar_id, &username, &input)).await {
        Ok(id) => Json(serde_json::json!({ "success": true, "id": id })).into_response(),
        Err(resp) => resp,
    }
}

/// Loads an event and checks the caller may edit its calendar.
async fn editable_event(state: &AppState, cookies: &Cookies, id: i64) -> Result<community::EventRow, Response> {
    let (username, role) = current_user(state, cookies).await?;
    let event = with_conn(state, move |conn| community::get_event(conn, id).map_err(|e| e.to_string()))
        .await?
        .ok_or_else(|| json_error(StatusCode::NOT_FOUND, "Événement introuvable."))?;
    if !role.can_edit_calendar(&username, &event.calendar_owner) {
        return Err(forbidden());
    }
    Ok(event)
}

pub async fn update_event_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Path(id): Path<i64>,
    Json(payload): Json<EventPayload>,
) -> Response {
    let event = match editable_event(&state, &cookies, id).await {
        Ok(event) => event,
        Err(resp) => return resp,
    };
    // Moving to another calendar requires edit rights on the destination too
    let target_calendar = payload.calendar_id.unwrap_or(event.calendar_id);
    if target_calendar != event.calendar_id {
        if let Err(resp) = editable_calendar(&state, &cookies, target_calendar).await {
            return resp;
        }
    }
    let input = match community::validate_event_input(
        &payload.title,
        &payload.start,
        &payload.end,
        &payload.location,
        &payload.description,
    ) {
        Ok(input) => input,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, e),
    };
    match with_conn(&state, move |conn| {
        community::update_event(conn, id, target_calendar, &input).map_err(|e| e.to_string())
    })
    .await
    {
        Ok(()) => Json(serde_json::json!({ "success": true })).into_response(),
        Err(resp) => resp,
    }
}

pub async fn delete_event_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Path(id): Path<i64>,
) -> Response {
    if let Err(resp) = editable_event(&state, &cookies, id).await {
        return resp;
    }
    match with_conn(&state, move |conn| community::delete_event(conn, id).map_err(|e| e.to_string())).await {
        Ok(()) => Json(serde_json::json!({ "success": true })).into_response(),
        Err(resp) => resp,
    }
}
