use std::net::SocketAddr;

use epikairos::{build_router, init_databases, open_pool, templates, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(5000);
    let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let school_db_path = std::env::var("SCHOOL_DB_PATH").unwrap_or_else(|_| "school.db".to_string());
    let feeds_db_path = std::env::var("FEEDS_DB_PATH").unwrap_or_else(|_| "feeds.db".to_string());

    let state = AppState::new(
        open_pool(&school_db_path)?,
        open_pool(&feeds_db_path)?,
        templates::create_template_env(),
    );
    init_databases(&state)?;

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    println!("[*] Starting EpiKairos Rust Backend on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        build_router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
