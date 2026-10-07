//! AI provider endpoints: list available providers and their models.
//!
//! No secret is ever exposed — only provider names, availability flags and
//! model identifiers cross the wire.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};

use crate::ai;
use crate::api::{ApiError, AppState};

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/ai/providers", get(list_providers))
        .route("/api/ai/providers/:provider/models", get(list_models))
}

/// List the providers the server knows about and whether each is usable.
async fn list_providers(State(_state): State<Arc<AppState>>) -> Json<Vec<ai::ProviderInfo>> {
    Json(ai::list_providers().await)
}

/// Fetch the model catalog for one provider. A not-configured or unreachable
/// provider yields a 400 so the client can fall back to deterministic mode.
async fn list_models(
    State(_state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<Json<Vec<ai::ModelInfo>>, ApiError> {
    let models = ai::list_models(&provider)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(models))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_router;
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use tower::ServiceExt;

    fn app() -> Router {
        build_router(Arc::new(AppState::default()))
    }

    #[tokio::test]
    async fn providers_endpoint_lists_ollama() {
        let req = axum::http::Request::builder()
            .uri("/api/ai/providers")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // Ollama is always listed, whatever its runtime state.
        assert!(v.as_array().unwrap().iter().any(|p| p["id"] == "ollama"));
    }

    #[tokio::test]
    async fn models_endpoint_unknown_provider_is_400() {
        let req = axum::http::Request::builder()
            .uri("/api/ai/providers/nonexistent/models")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
}
