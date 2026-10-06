pub mod api;
pub mod db;
pub mod models;
pub mod pdf_extract;
pub mod repo;

use std::sync::Arc;

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// Build the application router. `state` is shared, immutable app state.
pub fn build_router(state: Arc<api::AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .merge(api::routes())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
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
    let app = build_router(state);

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
