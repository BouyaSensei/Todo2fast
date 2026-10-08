pub mod ai;
pub mod api;
pub mod db;
pub mod models;
pub mod pdf_extract;
pub mod repo;

use std::sync::Arc;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

/// Build the application router. `state` is shared, immutable app state.
/// If `web_dir` points to a valid directory, the frontend SPA is served at `/`.
pub fn build_router(state: Arc<api::AppState>, web_dir: Option<std::path::PathBuf>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let mut app = Router::new()
        .merge(api::routes())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    if let Some(dir) = web_dir {
        if dir.join("index.html").exists() {
            tracing::info!("Serving frontend from {}", dir.display());
            // SPA: serve static files, fall back to index.html for client routes.
            let spa = ServeDir::new(&dir).not_found_service(ServeFile::new(dir.join("index.html")));
            app = app.fallback_service(spa);
        } else {
            tracing::warn!("web_dir {} has no index.html — API only", dir.display());
        }
    }

    app
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "todo2fast=info,tower_http=info".into()),
        )
        .init();

    let state = Arc::new(api::AppState::from_env().expect("failed to open database"));

    // Optional: serve the built frontend (SPA) alongside the API.
    // Look for T2F_WEB_DIR, then ../frontend/dist (dev layout), then ./web (installed layout).
    let web_dir = std::env::var("T2F_WEB_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| std::path::Path::new("../frontend/dist").canonicalize().ok())
        .or_else(|| std::path::Path::new("web").canonicalize().ok());

    let app = build_router(state, web_dir);

    let addr: std::net::SocketAddr = std::env::var("T2F_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()
        .expect("invalid T2F_ADDR");

    tracing::info!("Todo2fast listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind failed");
    axum::serve(listener, app).await.expect("server error");
}
