pub mod auriga;
pub mod db;
pub mod feed;
pub mod middleware;
pub mod routes;
pub mod session;
pub mod state;
pub mod templates;

use axum::{
    middleware::from_fn,
    middleware::from_fn_with_state,
    routing::{get, post},
    Router,
};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::net::SocketAddr;
use tower_cookies::CookieManagerLayer;
use tower_http::services::ServeDir;

use crate::state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(5000);
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

    let school_db_path = std::env::var("SCHOOL_DB_PATH").unwrap_or_else(|_| "school.db".to_string());
    let feeds_db_path = std::env::var("FEEDS_DB_PATH").unwrap_or_else(|_| "feeds.db".to_string());

    // Connection pools with WAL mode and busy timeout for concurrent durability
    let school_manager = SqliteConnectionManager::file(&school_db_path).with_init(|c| {
        c.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )
    });
    let db_pool = Pool::builder().max_size(16).build(school_manager)?;

    let feeds_manager = SqliteConnectionManager::file(&feeds_db_path).with_init(|c| {
        c.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )
    });
    let feed_pool = Pool::builder().max_size(16).build(feeds_manager)?;

    // Initialize databases
    {
        let conn = db_pool.get()?;
        db::init_db(&conn)?;
    }
    {
        let conn = feed_pool.get()?;
        feed::init_db(&conn)?;
    }

    // Jinja template environment
    let jinja_env = templates::create_template_env();
    let app_state = AppState::new(db_pool, feed_pool, jinja_env);

    let app = Router::new()
        // Web views
        .route("/", get(routes::web::dashboard_handler))
        .route("/dashboard", get(routes::web::dashboard_handler))
        .route("/old-zeus", get(routes::web::old_zeus_handler))
        .route("/zeus", get(routes::web::old_zeus_handler))
        .route("/login", get(routes::web::login_handler))
        .route("/logout", get(routes::web::logout_handler))
        .route("/personal", get(routes::web::personal_handler))
        .route("/my-calendar", get(routes::web::personal_handler))
        .route("/custom-calendar", get(routes::web::custom_calendar_handler))
        .route("/rooms", get(routes::web::rooms_handler))
        .route("/manifest.json", get(routes::web::manifest_handler))
        .route("/sw.js", get(routes::web::service_worker_handler))
        .route("/offline", get(routes::web::offline_handler))
        .route("/health", get(routes::web::health_handler))
        // API routes
        .route("/api/fetch-planning", post(routes::api::fetch_planning_handler))
        .route("/api/ingest-consent", post(routes::api::ingest_consent_handler))
        .route("/api/database/stats", get(routes::api::db_stats_handler))
        .route("/api/database/options", get(routes::api::db_options_handler))
        .route("/api/database/promos", get(routes::api::db_promos_handler))
        .route(
            "/api/database/recurring-courses",
            get(routes::api::db_recurring_courses_handler),
        )
        .route(
            "/api/database/detect-patterns",
            post(routes::api::db_detect_patterns_handler),
        )
        .route(
            "/api/database/custom-courses",
            post(routes::api::db_custom_courses_handler),
        )
        .route(
            "/api/database/free-rooms",
            post(routes::api::db_free_rooms_handler),
        )
        .route("/api/groups/tree", get(routes::api::groups_tree_handler))
        .route(
            "/api/groups/schedule",
            post(routes::api::groups_schedule_handler),
        )
        .route("/api/export-ics", post(routes::api::export_ics_handler))
        .route("/api/create-feed", post(routes::api::create_feed_handler))
        .route("/feed/:feed_id", get(routes::api::serve_feed_handler))
        // Static assets
        .nest_service("/static", ServeDir::new("static"))
        // Request body limit layer (2 MB)
        .layer(axum::extract::DefaultBodyLimit::max(2 * 1024 * 1024))
        // Auth and Security middleware layers
        .layer(from_fn_with_state(
            app_state.clone(),
            middleware::auth_middleware,
        ))
        .layer(CookieManagerLayer::new())
        .layer(from_fn(middleware::security_headers_middleware))
        .with_state(app_state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    println!("[*] Starting EpiKairos Rust Backend on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
