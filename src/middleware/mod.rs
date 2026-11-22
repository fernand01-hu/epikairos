use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue, Request, Response, StatusCode},
    middleware::Next,
};
use tower_cookies::Cookies;

use crate::session::verify_session;
use crate::state::AppState;

/// True when the site is served over HTTPS (set with SESSION_COOKIE_SECURE=true).
fn https_mode() -> bool {
    static HTTPS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HTTPS.get_or_init(|| {
        std::env::var("SESSION_COOKIE_SECURE")
            .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
            .unwrap_or(false)
    })
}

pub async fn security_headers_middleware(req: Request<Body>, next: Next) -> Response<Body> {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    headers.insert(
        header::HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("SAMEORIGIN"),
    );
    headers.insert(
        header::HeaderName::from_static("x-xss-protection"),
        HeaderValue::from_static("1; mode=block"),
    );
    headers.insert(
        header::HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    // Behind HTTPS (SESSION_COOKIE_SECURE=true), tell browsers to never use plain HTTP
    if https_mode() {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=15552000"),
        );
    }

    // Keep every page and feed out of search engines, even for crawlers that skip robots.txt
    headers.insert(
        header::HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow, noarchive"),
    );
    headers.insert(
        header::HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("geolocation=(), microphone=(), camera=()"),
    );
    headers.insert(
        header::HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(
            "default-src 'self'; \
             script-src 'self' https://cdn.jsdelivr.net; \
             style-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; \
             font-src 'self' https://cdn.jsdelivr.net data:; \
             img-src 'self' data: https:; \
             connect-src 'self'; \
             frame-ancestors 'self';",
        ),
    );

    response
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    cookies: Cookies,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let path = req.uri().path().to_string();

    let is_public = path.starts_with("/static/")
        || path.starts_with("/feed/")
        || path == "/login"
        || path == "/logout"
        || path == "/api/fetch-planning"
        || path == "/manifest.json"
        || path == "/robots.txt"
        || path == "/sw.js"
        || path == "/offline"
        || path == "/health";

    if is_public {
        return next.run(req).await;
    }

    let is_logged_in = cookies
        .get("session")
        .and_then(|c| verify_session(c.value(), &state.secret_key))
        .map(|s| s.logged_in)
        .unwrap_or(false);

    if !is_logged_in {
        if path.starts_with("/api/") {
            let body = serde_json::json!({
                "success": false,
                "error": "Authentification requise. Veuillez vous connecter avec votre compte Auriga.",
                "redirect": "/login"
            })
            .to_string();

            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap();
        } else {
            return Response::builder()
                .status(StatusCode::SEE_OTHER)
                .header(header::LOCATION, "/login")
                .body(Body::empty())
                .unwrap();
        }
    }

    next.run(req).await
}
