//! HTTP API routes and shared application state.

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};

pub mod ai;
pub mod boards;
pub mod comments;
pub mod documents;
pub mod lists;
pub mod members;
pub mod tags;

/// Shared, immutable application state handed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub db: crate::db::Db,
    /// Human-readable build/version tag surfaced on the health endpoint.
    pub version: String,
}

impl AppState {
    /// Build state from environment (used by `main`).
    pub fn from_env() -> std::result::Result<Self, crate::db::DbError> {
        let path = std::env::var("T2F_DB_PATH").unwrap_or_else(|_| "todo2fast.sqlite".into());
        Ok(Self {
            db: crate::db::Db::open(std::path::Path::new(&path))?,
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    /// In-memory state for tests.
    pub fn in_memory() -> std::result::Result<Self, crate::db::DbError> {
        Ok(Self {
            db: crate::db::Db::open_in_memory()?,
            version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::in_memory().expect("in-memory db")
    }
}

/// Route table. New feature modules are merged in here.
pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/health", get(health))
        .merge(boards::routes())
        .merge(comments::routes())
        .merge(documents::routes())
        .merge(lists::routes())
        .merge(tags::routes())
        .merge(members::routes())
        .merge(ai::routes())
}

async fn health(State(_state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "todo2fast",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Uniform API error → HTTP response mapping.
/// Small struct (StatusCode + String) to avoid `clippy::result_large_err`.
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    pub fn not_found(what: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: format!("{what} not found"),
        }
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: msg.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: msg.into(),
        }
    }
}

impl From<crate::db::DbError> for ApiError {
    fn from(e: crate::db::DbError) -> Self {
        tracing::error!(error = %e, "database error");
        Self::internal(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({ "error": self.message }));
        (self.status, body).into_response()
    }
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
        let app = build_router(Arc::new(AppState::default()), None);
        let v = get(app, "/api/health").await;
        assert_eq!(v["status"], "ok");
        assert_eq!(v["service"], "todo2fast");
        assert!(v["version"].is_string());
    }
}
