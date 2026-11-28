//! EpiKairos: EPITA Auriga calendar web app (library crate, used by the binary and the tests).

pub mod auriga;
pub mod accounts;
pub mod db;
pub mod feed;
pub mod changelog;
pub mod i18n;
pub mod logins;
pub mod middleware;
pub mod routes;
pub mod session;
pub mod state;
pub mod templates;
pub mod uptime;

use axum::{
    middleware::{from_fn, from_fn_with_state},
    routing::{get, post, put},
    Router,
};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use tower_cookies::CookieManagerLayer;
use tower_http::services::ServeDir;

pub use crate::state::AppState;

/// Opens a SQLite pool with WAL mode and a busy timeout.
pub fn open_pool(path: &str) -> Result<Pool<SqliteConnectionManager>, r2d2::Error> {
    let manager = SqliteConnectionManager::file(path).with_init(|c| {
        c.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )
    });
    Pool::builder().max_size(16).build(manager)
}

/// Creates every table (idempotent) and migrates legacy feed passwords.
pub fn init_databases(state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
    {
        let conn = state.db_pool.get()?;
        db::init_db(&conn)?;
        accounts::init_db(&conn)?;
        logins::init_db(&conn)?;
    }
    let conn = state.feed_pool.get()?;
    let removed = feed::init_db(&conn)?;
    if removed > 0 {
        println!("[*] Deleted {} old subscription link(s) that stored a password", removed);
    }
    Ok(())
}

/// All routes and middleware of the app.
pub fn build_router(state: AppState) -> Router {
    let _ = routes::web::STARTED_AT.set(std::time::Instant::now());
    Router::new()
        // Web views
        .route("/", get(routes::web::dashboard_handler))
        .route("/dashboard", get(routes::web::dashboard_handler))
        .route("/planning", get(routes::web::old_zeus_handler))
        .route("/old-zeus", get(routes::web::old_zeus_handler))
        .route("/zeus", get(routes::web::old_zeus_handler))
        .route("/login", get(routes::web::login_handler))
        .route(
            "/logout",
            get(routes::web::logout_page_handler).post(routes::web::logout_handler),
        )
        .route("/personal", get(routes::web::personal_handler))
        .route("/my-calendar", get(routes::web::personal_handler))
        .route("/custom-calendar", get(routes::web::custom_calendar_handler))
        .route("/rooms", get(routes::web::rooms_handler))
        .route("/confidentialite", get(routes::web::privacy_handler))
        .route("/mises-a-jour", get(routes::web::updates_handler))
        .route("/updates", get(routes::web::updates_handler))
        .route("/privacy", get(routes::web::privacy_handler))
        .route("/robots.txt", get(routes::web::robots_handler))
        .route("/manifest.json", get(routes::web::manifest_handler))
        .route("/sw.js", get(routes::web::service_worker_handler))
        .route("/offline", get(routes::web::offline_handler))
        .route("/health", get(routes::web::health_handler))
        .route("/admin", get(routes::admin::admin_page_handler))
        // API routes
        .route("/api/fetch-planning", post(routes::api::fetch_planning_handler))
        .route(
            "/api/database/free-rooms",
            post(routes::api::db_free_rooms_handler),
        )
        .route("/api/database/rooms-day", get(routes::api::db_rooms_day_handler))
        .route("/api/groups/tree", get(routes::api::groups_tree_handler))
        .route("/api/teachers", get(routes::api::teachers_handler))
        .route(
            "/api/groups/schedule",
            post(routes::api::groups_schedule_handler),
        )
        .route("/api/export-ics", post(routes::api::export_ics_handler))
        .route("/api/create-feed", post(routes::api::create_feed_handler))
        .route("/api/feeds", get(routes::api::list_feeds_handler))
        .route(
            "/api/feeds/:id",
            put(routes::api::update_feed_handler).delete(routes::api::delete_feed_handler),
        )
        .route("/feed/:feed_id", get(routes::api::serve_feed_handler))
        // Admin page
        .route("/api/admin/users", get(routes::admin::admin_list_users_handler))
        .route("/api/admin/logins", get(routes::admin::admin_user_logins_handler))
        .route("/api/account/delete", post(routes::api::delete_account_handler))
        // Static assets
        .nest_service("/static", ServeDir::new("static"))
        // Login check on matched routes only: unknown addresses get the 404 page,
        // not a redirect to /login
        .route_layer(from_fn_with_state(
            state.clone(),
            middleware::auth_middleware,
        ))
        .fallback(routes::web::fallback_handler)
        // Request body limit layer (2 MB)
        .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(CookieManagerLayer::new())
        .layer(from_fn(middleware::security_headers_middleware))
        .with_state(state)
}
