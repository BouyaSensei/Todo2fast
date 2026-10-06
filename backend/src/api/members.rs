//! Board access endpoints: list, add, and remove members of a board.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};

use crate::api::{ApiError, AppState};
use crate::models::{CreateMember, Member};
use crate::repo;

#[derive(serde::Deserialize)]
struct MemberRef {
    name: String,
}

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/boards/:board_id/members", post(add_member))
        .route("/api/boards/:board_id/members", get(list_members))
        .route("/api/boards/:board_id/members", delete(remove_member))
}

async fn add_member(
    State(state): State<Arc<AppState>>,
    Path(board_id): Path<i64>,
    Json(input): Json<CreateMember>,
) -> Result<(StatusCode, Json<Member>), ApiError> {
    if repo::get_board(&state.db, board_id)?.is_none() {
        return Err(ApiError::not_found("board"));
    }
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("member name is empty"));
    }
    let member = repo::add_member(
        &state.db,
        &CreateMember {
            board_id,
            name,
            role: input.role,
        },
    )?;
    Ok((StatusCode::CREATED, Json(member)))
}

async fn list_members(
    State(state): State<Arc<AppState>>,
    Path(board_id): Path<i64>,
) -> Result<Json<Vec<Member>>, ApiError> {
    if repo::get_board(&state.db, board_id)?.is_none() {
        return Err(ApiError::not_found("board"));
    }
    Ok(Json(repo::list_members(&state.db, board_id)?))
}

/// `DELETE /api/boards/:board_id/members` with a JSON body `{ "name": ... }`.
async fn remove_member(
    State(state): State<Arc<AppState>>,
    Path(board_id): Path<i64>,
    Json(input): Json<MemberRef>,
) -> Result<StatusCode, ApiError> {
    if repo::get_board(&state.db, board_id)?.is_none() {
        return Err(ApiError::not_found("board"));
    }
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("member name is empty"));
    }
    if repo::remove_member(&state.db, board_id, &name)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("member"))
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
    async fn member_flow() {
        let app = app();

        // Board with an owner.
        let (_, b) = req(
            app.clone(),
            "POST",
            "/api/boards",
            Some(r#"{"name":"K","owner":"Axel"}"#),
        )
        .await;
        let board_id: i64 = b["id"].as_i64().unwrap();

        // Owner is listed automatically.
        let (s, members) = req(
            app.clone(),
            "GET",
            &format!("/api/boards/{board_id}/members"),
            None,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        let arr = members.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "Axel");
        assert_eq!(arr[0]["role"], "owner");

        // Add a plain member.
        let (s, m) = req(
            app.clone(),
            "POST",
            &format!("/api/boards/{board_id}/members"),
            Some(r#"{"name":"Laurine"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        assert_eq!(m["role"], "member");

        // Two members now.
        let (_, members) = req(
            app.clone(),
            "GET",
            &format!("/api/boards/{board_id}/members"),
            None,
        )
        .await;
        assert_eq!(members.as_array().unwrap().len(), 2);

        // Remove the member.
        let (s, _) = req(
            app.clone(),
            "DELETE",
            &format!("/api/boards/{board_id}/members"),
            Some(r#"{"name":"Laurine"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::NO_CONTENT);

        let (_, members) = req(app, "GET", &format!("/api/boards/{board_id}/members"), None).await;
        assert_eq!(members.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn member_on_missing_board_is_404() {
        let (s, _) = req(
            app(),
            "POST",
            "/api/boards/9999/members",
            Some(r#"{"name":"x"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn empty_member_name_is_400() {
        let app = app();
        let (_, b) = req(app.clone(), "POST", "/api/boards", Some(r#"{"name":"K"}"#)).await;
        let board_id: i64 = b["id"].as_i64().unwrap();
        let (s, _) = req(
            app,
            "POST",
            &format!("/api/boards/{board_id}/members"),
            Some(r#"{"name":"   "}"#),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn removing_missing_member_is_404() {
        let app = app();
        let (_, b) = req(app.clone(), "POST", "/api/boards", Some(r#"{"name":"K"}"#)).await;
        let board_id: i64 = b["id"].as_i64().unwrap();
        let (s, _) = req(
            app,
            "DELETE",
            &format!("/api/boards/{board_id}/members"),
            Some(r#"{"name":"Nobody"}"#),
        )
        .await;
        assert_eq!(s, StatusCode::NOT_FOUND);
    }
}
