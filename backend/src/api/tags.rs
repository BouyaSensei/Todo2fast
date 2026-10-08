//! Personal colored tag endpoints: create, list, delete, and attach/detach to cards.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};

use crate::api::{ApiError, AppState};
use crate::models::{CreateTag, Tag, Todo};
use crate::repo;

#[derive(serde::Deserialize)]
struct TagRef {
    tag_id: i64,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/boards/:board_id/tags",
            post(create_tag).get(list_tags),
        )
        .route("/api/tags/:tag_id", axum::routing::delete(remove_tag))
        .route(
            "/api/todos/:todo_id/tags",
            post(add_tag).delete(remove_tag_from_todo),
        )
}

async fn create_tag(
    State(state): State<Arc<AppState>>,
    Path(board_id): Path<i64>,
    Json(input): Json<CreateTag>,
) -> Result<(StatusCode, Json<Tag>), ApiError> {
    if repo::get_board(&state.db, board_id)?.is_none() {
        return Err(ApiError::not_found("board"));
    }
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("tag name is empty"));
    }
    let tag = repo::create_tag(
        &state.db,
        board_id,
        &CreateTag {
            board_id,
            name,
            color: input.color,
        },
    )?;
    Ok((StatusCode::CREATED, Json(tag)))
}

async fn list_tags(
    State(state): State<Arc<AppState>>,
    Path(board_id): Path<i64>,
) -> Result<Json<Vec<Tag>>, ApiError> {
    Ok(Json(repo::list_tags(&state.db, board_id)?))
}

async fn remove_tag(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if repo::delete_tag(&state.db, id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("tag"))
    }
}

async fn add_tag(
    State(state): State<Arc<AppState>>,
    Path(todo_id): Path<i64>,
    Json(input): Json<TagRef>,
) -> Result<Json<Todo>, ApiError> {
    match repo::add_tag_to_todo(&state.db, todo_id, input.tag_id)? {
        Some(t) => Ok(Json(t)),
        None => Err(ApiError::not_found("todo")),
    }
}

async fn remove_tag_from_todo(
    State(state): State<Arc<AppState>>,
    Path(todo_id): Path<i64>,
    Json(input): Json<TagRef>,
) -> Result<Json<Todo>, ApiError> {
    match repo::remove_tag_from_todo(&state.db, todo_id, input.tag_id)? {
        Some(t) => Ok(Json(t)),
        None => Err(ApiError::not_found("todo")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build_router;
    use axum::body::to_bytes;
    use tower::ServiceExt;

    fn app() -> Router {
        build_router(Arc::new(AppState::default()), None)
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
    async fn tag_crud_and_attach_flow() {
        let app = app();

        // Board + todo.
        let (_, b) = req(app.clone(), "POST", "/api/boards", Some(r#"{"name":"K"}"#)).await;
        let board_id: i64 = b["id"].as_i64().unwrap();
        let (_, t) = req(
            app.clone(),
            "POST",
            &format!("/api/boards/{board_id}/todos"),
            Some(r#"{"title":"Tâche"}"#),
        )
        .await;
        let todo_id: i64 = t["id"].as_i64().unwrap();

        // Create a colored tag.
        let (s, tag) = req(
            app.clone(),
            "POST",
            &format!("/api/boards/{board_id}/tags"),
            Some(r##"{"name":"Urgent","color":"#f06a6a"}"##),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        let tag_id: i64 = tag["id"].as_i64().unwrap();
        assert_eq!(tag["color"], "#f06a6a");

        // List shows it.
        let (s, tags) = req(
            app.clone(),
            "GET",
            &format!("/api/boards/{board_id}/tags"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(tags.as_array().unwrap().len(), 1);

        // Attach to the todo.
        let (s, t) = req(
            app.clone(),
            "POST",
            &format!("/api/todos/{todo_id}/tags"),
            Some(&format!("{{\"tag_id\":{tag_id}}}")),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let attached = t["tags"].as_array().unwrap();
        assert_eq!(attached.len(), 1);
        assert_eq!(attached[0]["name"], "Urgent");

        // Detach.
        let (s, t) = req(
            app.clone(),
            "DELETE",
            &format!("/api/todos/{todo_id}/tags"),
            Some(&format!("{{\"tag_id\":{tag_id}}}")),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(t["tags"].as_array().unwrap().len(), 0);

        // Delete the tag.
        let (s, _) = req(app.clone(), "DELETE", &format!("/api/tags/{tag_id}"), None).await;
        assert_eq!(s, StatusCode::NO_CONTENT);

        let (_, tags) = req(app, "GET", &format!("/api/boards/{board_id}/tags"), None).await;
        assert_eq!(tags.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn tag_on_missing_board_is_404() {
        let (s, _) = req(
            app(),
            "POST",
            "/api/boards/9999/tags",
            Some(r#"{"name":"x"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn empty_tag_name_is_400() {
        let app = app();
        let (_, b) = req(app.clone(), "POST", "/api/boards", Some(r#"{"name":"K"}"#)).await;
        let board_id: i64 = b["id"].as_i64().unwrap();
        let (s, _) = req(
            app,
            "POST",
            &format!("/api/boards/{board_id}/tags"),
            Some(r#"{"name":"   "}"#),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn missing_tag_is_404() {
        let (s, _) = req(app(), "DELETE", "/api/tags/424242", None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }
}
