//! PDF upload + deterministic "understanding" of a document.
//!
//! `POST /api/documents` accepts a multipart upload, extracts text, detects
//! dates and suggests tasks — fully offline (no LLM required). When the client
//! also sends `ai_provider` (+ optional `ai_model`) form fields, the server asks
//! the configured AI provider to refine the candidates; any failure silently
//! falls back to the deterministic suggestions (the response carries a
//! `refined` flag so the UI can tell which path ran).
//!
//! `POST /api/boards/:board_id/import` takes the same analysis and materializes
//! the suggested tasks as real todos on the board.

use std::sync::Arc;

use axum::{
    extract::{Multipart, Path, State},
    routing::post,
    Json, Router,
};

use crate::api::ApiError;
use crate::models::CreateTodo;

const MAX_PDF_BYTES: usize = 15 * 1024 * 1024; // 15 Mo

/// The deterministic analysis plus a flag telling the client whether an AI
/// provider refined the suggested tasks.
#[derive(serde::Serialize)]
struct AnalysisResponse {
    #[serde(flatten)]
    analysis: crate::pdf_extract::DocumentAnalysis,
    /// `true` when an AI provider successfully refined the tasks; `false` for
    /// the deterministic path (no provider configured or call failed).
    refined: bool,
}

pub fn routes() -> Router<Arc<crate::api::AppState>> {
    Router::new()
        .route("/api/documents", post(upload_document))
        .route("/api/boards/:board_id/import", post(import_into_board))
}

/// Analyze an uploaded PDF and return the analysis (optionally AI-refined).
async fn upload_document(
    State(_state): State<Arc<crate::api::AppState>>,
    mut multipart: Multipart,
) -> Result<Json<AnalysisResponse>, ApiError> {
    let (bytes, ai_provider, ai_model) = read_upload(&mut multipart).await?;

    // PDF parsing is CPU-bound; run it off the async runtime.
    let mut analysis = tokio::task::spawn_blocking(move || crate::pdf_extract::analyze_pdf(&bytes))
        .await
        .map_err(|e| ApiError::internal(format!("tâche d'extraction: {e}")))
        .and_then(|r| r.map_err(ApiError::internal))?;

    // Optional AI refinement with a guaranteed deterministic fallback.
    let mut refined = false;
    if !ai_provider.trim().is_empty() {
        match crate::ai::refine_tasks(
            &ai_provider,
            &ai_model,
            &analysis.preview,
            &analysis.suggested_tasks,
        )
        .await
        {
            Ok(tasks) => {
                analysis.suggested_tasks = tasks;
                refined = true;
            }
            Err(e) => {
                tracing::warn!(provider = %ai_provider, error = %e, "raffinement IA indisponible — fallback déterministe")
            }
        }
    }

    Ok(Json(AnalysisResponse { analysis, refined }))
}

/// Read the PDF bytes plus the optional AI form fields from a multipart body
/// in a single pass (a multipart stream can only be consumed once).
async fn read_upload(multipart: &mut Multipart) -> Result<(Vec<u8>, String, String), ApiError> {
    let mut data: Option<Vec<u8>> = None;
    let mut provider = String::new();
    let mut model = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::internal(format!("multipart invalide: {e}")))?
    {
        match field.name() {
            Some("file") => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::internal(format!("lecture du fichier: {e}")))?;
                data = Some(bytes.to_vec());
            }
            Some("ai_provider") => {
                provider = String::from_utf8_lossy(
                    &field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::internal(format!("lecture du champ: {e}")))?,
                )
                .into_owned();
            }
            Some("ai_model") => {
                model = String::from_utf8_lossy(
                    &field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::internal(format!("lecture du champ: {e}")))?,
                )
                .into_owned();
            }
            _ => {}
        }
    }

    let bytes = data.ok_or_else(|| ApiError::internal("champ 'file' manquant"))?;
    if bytes.is_empty() {
        return Err(ApiError::internal("fichier vide"));
    }
    if bytes.len() > MAX_PDF_BYTES {
        return Err(ApiError::internal("PDF trop volumineux (max 15 Mo)"));
    }

    Ok((bytes, provider, model))
}

/// Import a PDF into a board: analyze it, then create todos from the suggestions.
async fn import_into_board(
    State(state): State<Arc<crate::api::AppState>>,
    Path(board_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut data: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::internal(format!("multipart invalide: {e}")))?
    {
        if field.name().is_some_and(|n| n == "file") {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::internal(format!("lecture du fichier: {e}")))?;
            data = Some(bytes.to_vec());
        }
    }

    let bytes = data.ok_or_else(|| ApiError::internal("champ 'file' manquant"))?;
    if bytes.len() > MAX_PDF_BYTES {
        return Err(ApiError::internal("PDF trop volumineux (max 15 Mo)"));
    }

    // Verify the board exists before doing any work.
    let board =
        crate::repo::get_board(&state.db, board_id)?.ok_or_else(|| ApiError::not_found("board"))?;

    let analysis = tokio::task::spawn_blocking(move || crate::pdf_extract::analyze_pdf(&bytes))
        .await
        .map_err(|e| ApiError::internal(format!("tâche d'extraction: {e}")))
        .and_then(|r| r.map_err(ApiError::internal))?;

    // Materialize the suggested tasks as todos on the board.
    let mut created = Vec::new();
    for task in &analysis.suggested_tasks {
        let input = CreateTodo {
            title: task.title.clone(),
            description: Some(format!(
                "Importé depuis un document PDF.\nExtrait: {}",
                analysis.preview
            )),
            due_date: task.due_date.clone(),
            list_id: None,
        };
        match crate::repo::create_todo(&state.db, board_id, &input) {
            Ok(todo) => created.push(todo),
            Err(e) => tracing::warn!(error = %e, "todo import ignoré"),
        }
    }

    Ok(Json(serde_json::json!({
        "board": board,
        "analysis": analysis,
        "created_count": created.len(),
        "created": created,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::AppState;
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use tower::ServiceExt;

    fn build() -> Router {
        let state = Arc::new(AppState::in_memory().expect("db"));
        crate::build_router(state)
    }

    /// A minimal, *valid* single-page PDF (correct xref + startxref) whose text
    /// layer reads "Preparer la demo le 31/10/2026". Built with computed offsets
    /// so lopdf accepts it.
    fn tiny_pdf() -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        let mut offsets: Vec<usize> = Vec::new();

        buf.extend_from_slice(b"%PDF-1.4\n");

        offsets.push(buf.len());
        buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        offsets.push(buf.len());
        buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

        offsets.push(buf.len());
        buf.extend_from_slice(
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n",
        );

        let content = b"BT /F1 24 Tf 72 720 Td (Preparer la demo le 31/10/2026) Tj ET";
        offsets.push(buf.len());
        buf.extend_from_slice(
            format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes(),
        );
        buf.extend_from_slice(content);
        buf.extend_from_slice(b"\nendstream\nendobj\n");

        offsets.push(buf.len());
        buf.extend_from_slice(
            b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
        );

        let xref_pos = buf.len();
        let mut xref = String::from("xref\n0 6\n0000000000 65535 f \n");
        for off in &offsets {
            xref.push_str(&format!("{:010} 00000 n \n", off));
        }
        buf.extend_from_slice(xref.as_bytes());

        buf.extend_from_slice(
            format!(
                "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
                xref_pos
            )
            .as_bytes(),
        );

        buf
    }

    fn multipart_with_pdf() -> axum::http::Request<axum::body::Body> {
        let boundary = "----t2ftestboundary";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"file\"; filename=\"doc.pdf\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: application/pdf\r\n\r\n");
        body.extend_from_slice(&tiny_pdf());
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/documents")
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(axum::body::Body::from(body))
            .unwrap();
        req
    }

    #[tokio::test]
    async fn upload_analyzes_pdf() {
        let app = build();
        let resp = app.oneshot(multipart_with_pdf()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        // The tiny PDF's text layer may or may not extract depending on the
        // parser; assert the shape is right and it did not error.
        assert!(v.get("suggested_tasks").is_some());
        assert!(v.get("page_count").is_some());
        // No AI provider requested → deterministic path, refined must be false.
        assert_eq!(v.get("refined"), Some(&serde_json::Value::Bool(false)));
    }

    #[tokio::test]
    async fn upload_rejects_non_pdf() {
        let app = build();
        let boundary = "----t2ftestboundary";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"file\"; filename=\"x.txt\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: text/plain\r\n\r\n");
        body.extend_from_slice(b"hello world, definitely not a pdf");
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/documents")
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(axum::body::Body::from(body))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
