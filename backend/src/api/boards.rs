//! Board & Todo CRUD endpoints.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};

use crate::api::{ApiError, AppState};
use crate::models::{Board, CreateBoard, CreateTodo, Todo, UpdateTodo};
use crate::repo;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/boards", post(create_board).get(list_boards))
        .route("/api/boards/:board_id", get(get_board).delete(remove_board))
        .route(
            "/api/boards/:board_id/todos",
            post(create_todo).get(list_todos),
        )
        .route(
            "/api/todos/:todo_id",
            get(get_todo).put(patch_todo).delete(remove_todo),
        )
}

// ---------- Boards ----------

async fn create_board(
    State(state): State<Arc<AppState>>,
    Json(input): Json<CreateBoard>,
) -> Result<(StatusCode, Json<Board>), ApiError> {
    let board = repo::create_board(&state.db, &input)?;
    Ok((StatusCode::CREATED, Json(board)))
}

async fn list_boards(State(state): State<Arc<AppState>>) -> Result<Json<Vec<Board>>, ApiError> {
    Ok(Json(repo::list_boards(&state.db)?))
}

async fn get_board(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Board>, ApiError> {
    match repo::get_board(&state.db, id)? {
        Some(b) => Ok(Json(b)),
        None => Err(ApiError::not_found("board")),
    }
}

async fn remove_board(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if repo::delete_board(&state.db, id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("board"))
    }
}

// ---------- Todos ----------

async fn create_todo(
    State(state): State<Arc<AppState>>,
    Path(board_id): Path<i64>,
    Json(input): Json<CreateTodo>,
) -> Result<(StatusCode, Json<Todo>), ApiError> {
    // Ensure the board exists.
    if repo::get_board(&state.db, board_id)?.is_none() {
        return Err(ApiError::not_found("board"));
    }
    let todo = repo::create_todo(&state.db, board_id, &input)?;
    Ok((StatusCode::CREATED, Json(todo)))
}

async fn list_todos(
    State(state): State<Arc<AppState>>,
    Path(board_id): Path<i64>,
) -> Result<Json<Vec<Todo>>, ApiError> {
    Ok(Json(repo::list_todos(&state.db, board_id)?))
}

async fn get_todo(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Json<Todo>, ApiError> {
    match repo::get_todo(&state.db, id)? {
        Some(t) => Ok(Json(t)),
        None => Err(ApiError::not_found("todo")),
    }
}

async fn patch_todo(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    Json(patch): Json<UpdateTodo>,
) -> Result<Json<Todo>, ApiError> {
    match repo::update_todo(&state.db, id, &patch)? {
        Some(t) => Ok(Json(t)),
        None => Err(ApiError::not_found("todo")),
    }
}

async fn remove_todo(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if repo::delete_todo(&state.db, id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("todo"))
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
    async fn full_board_and_todo_crud_flow() {
        let app = app();

        // 1. Create a board.
        let (s, b) = req(
            app.clone(),
            "POST",
            "/api/boards",
            Some(r#"{"name":"Sprint 1"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        let board_id: i64 = b["id"].as_i64().unwrap();
        assert_eq!(b["name"], "Sprint 1");

        // 2. List boards contains it.
        let (s, list) = req(app.clone(), "GET", "/api/boards", None).await;
        assert_eq!(s, StatusCode::OK);
        assert!(list.as_array().unwrap().iter().any(|x| x["id"] == board_id));

        // 3. Get the single board.
        let (s, got) = req(app.clone(), "GET", &format!("/api/boards/{board_id}"), None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(got["name"], "Sprint 1");

        // 4. Create a todo on the board.
        let (s, t) = req(
            app.clone(),
            "POST",
            &format!("/api/boards/{board_id}/todos"),
            Some(r#"{"title":"Ship it","due_date":"2026-11-01"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        let todo_id: i64 = t["id"].as_i64().unwrap();
        assert_eq!(t["title"], "Ship it");
        assert_eq!(t["due_date"], "2026-11-01");

        // 5. List todos on the board.
        let (s, tl) = req(
            app.clone(),
            "GET",
            &format!("/api/boards/{board_id}/todos"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(tl.as_array().unwrap().len(), 1);

        // 6. Get the todo.
        let (s, gt) = req(app.clone(), "GET", &format!("/api/todos/{todo_id}"), None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(gt["title"], "Ship it");

        // 7. Update the todo (mark done + new title).
        let (s, ut) = req(
            app.clone(),
            "PUT",
            &format!("/api/todos/{todo_id}"),
            Some(r#"{"title":"Shipped","done":true}"#),
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(ut["title"], "Shipped");
        assert_eq!(ut["done"], true);

        // 8. Delete the todo.
        let (s, _) = req(
            app.clone(),
            "DELETE",
            &format!("/api/todos/{todo_id}"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);

        // 9. Todo is gone.
        let (s, _) = req(app.clone(), "GET", &format!("/api/todos/{todo_id}"), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);

        // 10. Delete the board.
        let (s, _) = req(
            app.clone(),
            "DELETE",
            &format!("/api/boards/{board_id}"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);

        // 11. Board is gone.
        let (s, _) = req(app, "GET", &format!("/api/boards/{board_id}"), None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn todo_on_missing_board_is_404() {
        let (s, _) = req(
            app(),
            "POST",
            "/api/boards/9999/todos",
            Some(r#"{"title":"nope"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn missing_board_and_todo_are_404() {
        let (s, body) = req(app(), "GET", "/api/boards/424242", None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "board not found");

        let (s, body) = req(app(), "GET", "/api/todos/424242", None).await;
        assert_eq!(s, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "todo not found");
    }
}
