//! REST routes for comments and reactions.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::api::AppState;
use crate::models::{Comment, CreateComment, CreateReaction};
use crate::repo;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/api/todos/:todo_id/comments",
            post(create_comment).get(list_comments),
        )
        .route(
            "/api/comments/:comment_id",
            get(get_comment).delete(delete_comment),
        )
        .route(
            "/api/comments/:comment_id/reactions",
            post(add_reaction).delete(remove_reaction),
        )
}

// ─── Handlers ──────────────────────────────────────────────────────────────────

async fn create_comment(
    State(state): State<Arc<AppState>>,
    Path(todo_id): Path<i64>,
    Json(mut input): Json<CreateComment>,
) -> Result<(StatusCode, Json<Comment>), crate::api::ApiError> {
    input.todo_id = todo_id;
    let db = state.db.clone();
    let comment = tokio::task::spawn_blocking(move || repo::create_comment(&db, &input))
        .await
        .map_err(|e| crate::api::ApiError::internal(e.to_string()))?
        .map_err(crate::api::ApiError::from)?;
    Ok((StatusCode::CREATED, Json(comment)))
}

async fn list_comments(
    State(state): State<Arc<AppState>>,
    Path(todo_id): Path<i64>,
) -> Result<Json<Vec<Comment>>, crate::api::ApiError> {
    let db = state.db.clone();
    let comments = tokio::task::spawn_blocking(move || repo::list_comments(&db, todo_id))
        .await
        .map_err(|e| crate::api::ApiError::internal(e.to_string()))?
        .map_err(crate::api::ApiError::from)?;
    Ok(Json(comments))
}

async fn get_comment(
    State(state): State<Arc<AppState>>,
    Path(comment_id): Path<i64>,
) -> Result<Json<Comment>, crate::api::ApiError> {
    let db = state.db.clone();
    let comment = tokio::task::spawn_blocking(move || repo::get_comment(&db, comment_id))
        .await
        .map_err(|e| crate::api::ApiError::internal(e.to_string()))?
        .map_err(crate::api::ApiError::from)?;
    match comment {
        Some(c) => Ok(Json(c)),
        None => Err(crate::api::ApiError::not_found("comment")),
    }
}

async fn delete_comment(
    State(state): State<Arc<AppState>>,
    Path(comment_id): Path<i64>,
) -> Result<StatusCode, crate::api::ApiError> {
    let db = state.db.clone();
    let deleted = tokio::task::spawn_blocking(move || repo::delete_comment(&db, comment_id))
        .await
        .map_err(|e| crate::api::ApiError::internal(e.to_string()))?
        .map_err(crate::api::ApiError::from)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(crate::api::ApiError::not_found("comment"))
    }
}

async fn add_reaction(
    State(state): State<Arc<AppState>>,
    Path(comment_id): Path<i64>,
    Json(mut input): Json<CreateReaction>,
) -> Result<(StatusCode, Json<Comment>), crate::api::ApiError> {
    input.comment_id = comment_id;
    let db = state.db.clone();
    let comment = tokio::task::spawn_blocking(move || repo::add_reaction(&db, &input))
        .await
        .map_err(|e| crate::api::ApiError::internal(e.to_string()))?
        .map_err(crate::api::ApiError::from)?;
    Ok((StatusCode::CREATED, Json(comment)))
}

#[derive(Debug, Deserialize)]
struct RemoveReactionQuery {
    author: String,
    emoji: String,
}

async fn remove_reaction(
    State(state): State<Arc<AppState>>,
    Path(comment_id): Path<i64>,
    Query(q): Query<RemoveReactionQuery>,
) -> Result<StatusCode, crate::api::ApiError> {
    let db = state.db.clone();
    let author = q.author;
    let emoji = q.emoji;
    let removed = tokio::task::spawn_blocking(move || {
        repo::remove_reaction(&db, comment_id, &author, &emoji)
    })
    .await
    .map_err(|e| crate::api::ApiError::internal(e.to_string()))?
    .map_err(crate::api::ApiError::from)?;
    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(crate::api::ApiError::not_found("reaction"))
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode as S};
    use tower::ServiceExt;

    fn app() -> Router {
        let state = Arc::new(AppState::in_memory().expect("in-memory db"));
        Router::new()
            .merge(crate::api::boards::routes())
            .merge(routes())
            .with_state(state)
    }

    async fn req(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<&str>,
    ) -> (u16, serde_json::Value) {
        let builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        let body_bytes = body.unwrap_or("").as_bytes();
        let http_req = builder.body(Body::from(body_bytes.to_vec())).unwrap();
        let resp = app.clone().oneshot(http_req).await.unwrap();
        let status = resp.status().as_u16();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = if bytes.is_empty() {
            serde_json::json!(null)
        } else {
            serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                panic!(
                    "JSON parse failed for status {}: {} (body: {:?})",
                    status,
                    e,
                    String::from_utf8_lossy(&bytes)
                )
            })
        };
        (status, json)
    }

    #[tokio::test]
    async fn full_comment_and_reaction_flow() {
        let app = app();

        // Create a board + todo first
        let (s, board) = req(&app, "POST", "/api/boards", Some(r#"{"name":"Test"}"#)).await;
        assert_eq!(s, S::CREATED.as_u16());
        let board_id = board["id"].as_i64().unwrap();

        let (s, todo) = req(
            &app,
            "POST",
            &format!("/api/boards/{board_id}/todos"),
            Some(r#"{"title":"Task"}"#),
        )
        .await;
        assert_eq!(s, S::CREATED.as_u16());
        let todo_id = todo["id"].as_i64().unwrap();

        // Create a comment
        let (s, c) = req(
            &app,
            "POST",
            &format!("/api/todos/{todo_id}/comments"),
            Some(r#"{"author":"alice","body":"Looks good!"}"#),
        )
        .await;
        assert_eq!(s, S::CREATED.as_u16());
        let comment_id = c["id"].as_i64().unwrap();
        assert_eq!(c["author"], "alice");
        assert_eq!(c["body"], "Looks good!");
        assert_eq!(c["reactions"].as_array().unwrap().len(), 0);

        // List comments for the todo
        let (s, list) = req(&app, "GET", &format!("/api/todos/{todo_id}/comments"), None).await;
        assert_eq!(s, S::OK.as_u16());
        assert_eq!(list.as_array().unwrap().len(), 1);

        // Add a reaction
        let (s, c2) = req(
            &app,
            "POST",
            &format!("/api/comments/{comment_id}/reactions"),
            Some(r#"{"author":"bob","emoji":"👍"}"#),
        )
        .await;
        assert_eq!(s, S::CREATED.as_u16());
        let reactions = c2["reactions"].as_array().unwrap();
        assert_eq!(reactions.len(), 1);
        assert_eq!(reactions[0]["author"], "bob");

        // Idempotent: same reaction again → still 1
        let (s, c3) = req(
            &app,
            "POST",
            &format!("/api/comments/{comment_id}/reactions"),
            Some(r#"{"author":"bob","emoji":"👍"}"#),
        )
        .await;
        assert_eq!(s, S::CREATED.as_u16());
        assert_eq!(c3["reactions"].as_array().unwrap().len(), 1);

        // Remove the reaction
        let (s, _) = req(
            &app,
            "DELETE",
            &format!("/api/comments/{comment_id}/reactions?author=bob&emoji=%F0%9F%91%8D"),
            None,
        )
        .await;
        assert_eq!(s, S::NO_CONTENT.as_u16());

        // Verify removed
        let (s, c4) = req(&app, "GET", &format!("/api/comments/{comment_id}"), None).await;
        assert_eq!(s, S::OK.as_u16());
        assert_eq!(c4["reactions"].as_array().unwrap().len(), 0);

        // Delete the comment
        let (s, _) = req(&app, "DELETE", &format!("/api/comments/{comment_id}"), None).await;
        assert_eq!(s, S::NO_CONTENT.as_u16());

        // 404 on deleted comment
        let (s, _) = req(&app, "GET", &format!("/api/comments/{comment_id}"), None).await;
        assert_eq!(s, S::NOT_FOUND.as_u16());
    }
}
