//! Admin page: the list of accounts that logged in. Only `ADMIN_USERS` see it.

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use tower_cookies::Cookies;

use crate::accounts;
use crate::routes::web::session_context;
use crate::session::verify_session;
use crate::state::AppState;

fn logged_in_admin(state: &AppState, cookies: &Cookies) -> Option<String> {
    cookies
        .get("session")
        .and_then(|c| verify_session(c.value(), &state.secret_key))
        .filter(|s| s.logged_in)
        .and_then(|s| s.username)
        .filter(|u| accounts::is_admin(u))
}

pub async fn admin_page_handler(State(state): State<AppState>, cookies: Cookies) -> Response {
    if logged_in_admin(&state, &cookies).is_none() {
        // Hide the page's existence from everyone else
        return (StatusCode::NOT_FOUND, Html("Not Found")).into_response();
    }
    let session_val = session_context(&state, &cookies).await;
    let rendered = state.jinja.get_template("admin.html").and_then(|tmpl| {
        tmpl.render(minijinja::context! { active_page => "admin", session => session_val })
    });
    match rendered {
        Ok(html) => Html(html).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Deserialize)]
pub struct UsersQuery {
    pub q: Option<String>,
}

pub async fn admin_list_users_handler(
    State(state): State<AppState>,
    cookies: Cookies,
    Query(query): Query<UsersQuery>,
) -> Response {
    if logged_in_admin(&state, &cookies).is_none() {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "success": false, "error": "Réservé à l'administrateur." })),
        )
            .into_response();
    }
    let q = query.q.map(|s| s.chars().take(64).collect::<String>());
    let pool = state.db_pool.clone();
    let res = tokio::task::spawn_blocking(move || {
        let conn = pool.get().map_err(|e| e.to_string())?;
        accounts::list_users(&conn, q.as_deref(), 500).map_err(|e| e.to_string())
    })
    .await;
    match res {
        Ok(Ok(users)) => Json(serde_json::json!({ "success": true, "users": users })).into_response(),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "success": false, "error": "Erreur interne du serveur." })),
        )
            .into_response(),
    }
}
