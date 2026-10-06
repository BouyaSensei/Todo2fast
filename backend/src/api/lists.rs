//! Kanban column (list) endpoints: create, list, rename, delete.
//! Cards move between columns via the todo's `list_id` field (see boards.rs).

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{post, put},
    Json, Router,
};

use crate::api::{ApiError, AppState};
use crate::models::{CreateList, List, UpdateList};
use crate::repo;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/boards/:board_id/lists",
            post(create_list).get(list_lists),
        )
        .route("/api/lists/:list_id", put(update_list).delete(remove_list))
}

async fn create_list(
    State(state): State<Arc<AppState>>,
    Path(board_id): Path<i64>,
    Json(input): Json<CreateList>,
) -> Result<(StatusCode, Json<List>), ApiError> {
    // Ensure the board exists before attaching a column to it.
    if repo::get_board(&state.db, board_id)?.is_none() {
        return Err(ApiError::not_found("board"));
    }
    let list = repo::create_list(&state.db, board_id, &input)?;
    Ok((StatusCode::CREATED, Json(list)))
}

async fn list_lists(
    State(state): State<Arc<AppState>>,
    Path(board_id): Path<i64>,
) -> Result<Json<Vec<List>>, ApiError> {
    Ok(Json(repo::list_lists(&state.db, board_id)?))
}

async fn update_list(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(input): Json<UpdateList>,
) -> Result<Json<List>, ApiError> {
    match repo::update_list(&state.db, id, &input)? {
        Some(l) => Ok(Json(l)),
        None => Err(ApiError::not_found("list")),
    }
}

async fn remove_list(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if repo::delete_list(&state.db, id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("list"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_router;
    use axum::body::to_bytes;
    use tower::ServiceExt;

    fn app() -> Router {
        build_router(Arc::new(AppState::default()))
    }

    async fn req(
        app: Router,
        method: &str,
        uri: &str,
        body: Option<&str>,
    ) -> (StatusCode, serde_json::Value) {
        let builder = axum::http::Request::builder().uri(uri).method(method);
        let body_bytes = body.unwrap_or("").as_bytes().to_vec();
        let req = builder
            .header("content-type", "application/json")
            .body(axum::body::Body::from(body_bytes))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json = if bytes.is_empty() {
            serde_json::json!({})
        } else {
            serde_json::from_slice(&bytes).unwrap_or(serde_json::json!({}))
        };
        (status, json)
    }

    #[tokio::test]
    async fn full_list_crud_flow() {
        let app = app(); // single in-memory DB, cloned per request

        // Create a board first.
        let (_, b) = req(app.clone(), "POST", "/api/boards", Some(r#"{"name":"K"}"#)).await;
        let board_id: i64 = b["id"].as_i64().unwrap();

        // Create two columns.
        let (s, l1) = req(
            app.clone(),
            "POST",
            &format!("/api/boards/{board_id}/lists"),
            Some(r#"{"title":"À faire"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        let list_id: i64 = l1["id"].as_i64().unwrap();

        let (s, _) = req(
            app.clone(),
            "POST",
            &format!("/api/boards/{board_id}/lists"),
            Some(r#"{"title":"En cours"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);

        // List shows both, ordered by position.
        let (s, lists) = req(
            app.clone(),
            "GET",
            &format!("/api/boards/{board_id}/lists"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(lists.as_array().unwrap().len(), 2);

        // Rename the first column.
        let (s, r) = req(
            app.clone(),
            "PUT",
            &format!("/api/lists/{list_id}"),
            Some(r#"{"title":"Backlog"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(r["title"], "Backlog");

        // Delete it.
        let (s, _) = req(
            app.clone(),
            "DELETE",
            &format!("/api/lists/{list_id}"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);

        // Gone from the list.
        let (_, lists) = req(app, "GET", &format!("/api/boards/{board_id}/lists"), None).await;
        assert_eq!(lists.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn list_color_roundtrip() {
        let app = app();
        let (_, b) = req(app.clone(), "POST", "/api/boards", Some(r#"{"name":"K"}"#)).await;
        let board_id: i64 = b["id"].as_i64().unwrap();

        // Create with a color.
        let (s, l1) = req(
            app.clone(),
            "POST",
            &format!("/api/boards/{board_id}/lists"),
            Some(r##"{"title":"Focus","color":"#e8b04a"}"##),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        let list_id: i64 = l1["id"].as_i64().unwrap();
        assert_eq!(l1["color"], "#e8b04a");

        // Update only the color (title untouched).
        let (s, r) = req(
            app.clone(),
            "PUT",
            &format!("/api/lists/{list_id}"),
            Some(r##"{"color":"#5aa9e6"}"##),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(r["title"], "Focus");
        assert_eq!(r["color"], "#5aa9e6");

        // Clear the color back to default (empty string = reset).
        let (s, r) = req(
            app.clone(),
            "PUT",
            &format!("/api/lists/{list_id}"),
            Some(r#"{"color":""}"#),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert!(r["color"].is_null());

        // Reflected in the board's list.
        let (_, lists) = req(app, "GET", &format!("/api/boards/{board_id}/lists"), None).await;
        assert!(lists.as_array().unwrap()[0]["color"].is_null());
    }

    #[tokio::test]
    async fn list_on_missing_board_is_404() {
        let (s, _) = req(
            app(),
            "POST",
            "/api/boards/9999/lists",
            Some(r#"{"title":"x"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn missing_list_is_404() {
        let (s, _) = req(app(), "DELETE", "/api/lists/424242", None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }
}
