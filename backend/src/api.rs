//! HTTP API routes and shared application state.

use std::sync::Arc;

use axum::{extract::State, routing::get, Json, Router};

/// Shared, immutable application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    /// Human-readable build/version tag surfaced on the health endpoint.
    pub version: String,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Route table. New feature modules are merged in here.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/health", get(health))
}

async fn health(State(_state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "todo2fast",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_router;
    use axum::body::to_bytes;
    use tower::ServiceExt; // `oneshot`

    async fn get(app: Router, uri: &str) -> serde_json::Value {
        let req = axum::http::Request::builder()
            .uri(uri)
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let app = build_router(Arc::new(AppState::new()));
        let v = get(app, "/api/health").await;
        assert_eq!(v["status"], "ok");
        assert_eq!(v["service"], "todo2fast");
        assert!(v["version"].is_string());
    }
}
