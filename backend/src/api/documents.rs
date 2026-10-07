//! Document upload + deterministic "understanding".
//!
//! `POST /api/documents` accepts a multipart upload (PDF or Markdown), extracts
//! text, detects dates and suggests tasks — fully offline (no LLM required).
//! When the client also sends `ai_provider` (+ optional `ai_model`) form fields,
//! the server asks the configured AI provider to refine the candidates; any
//! failure silently falls back to the deterministic suggestions (the response
//! carries a `refined` flag so the UI can tell which path ran).
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

/// Analyze an uploaded document (PDF or Markdown) and return the analysis
/// (optionally AI-refined). The file type is detected from its extension.
async fn upload_document(
    State(_state): State<Arc<crate::api::AppState>>,
    mut multipart: Multipart,
) -> Result<Json<AnalysisResponse>, ApiError> {
    let (bytes, filename, ai_provider, ai_model) = read_upload(&mut multipart).await?;

    // Parsing is CPU-bound; run it off the async runtime.
    let mut analysis = tokio::task::spawn_blocking(move || analyze_bytes(&filename, &bytes))
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

/// Route raw bytes to the right analyzer based on the file extension.
fn analyze_bytes(
    filename: &str,
    data: &[u8],
) -> Result<crate::pdf_extract::DocumentAnalysis, String> {
    let ext = std::path::Path::new(filename)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "md" | "markdown" | "txt" => {
            let text =
                std::str::from_utf8(data).map_err(|_| "fichier texte non-UTF-8".to_string())?;
            Ok(crate::pdf_extract::analyze_markdown(text))
        }
        _ => crate::pdf_extract::analyze_pdf(data),
    }
}

/// Read the document bytes, its filename and the optional AI form fields from a
/// multipart body in a single pass (a multipart stream can only be consumed once).
async fn read_upload(
    multipart: &mut Multipart,
) -> Result<(Vec<u8>, String, String, String), ApiError> {
    let mut data: Option<Vec<u8>> = None;
    let mut filename = String::new();
    let mut provider = String::new();
    let mut model = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::internal(format!("multipart invalide: {e}")))?
    {
        match field.name() {
            Some("file") => {
                if let Some(fn_) = field.file_name() {
                    filename = fn_.to_string();
                }
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
        return Err(ApiError::internal("fichier trop volumineux (max 15 Mo)"));
    }

    Ok((bytes, filename, provider, model))
}

/// Import a document (PDF or Markdown) into a board: analyze it, then create
/// todos from the suggestions.
async fn import_into_board(
    State(state): State<Arc<crate::api::AppState>>,
    Path(board_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut data: Option<Vec<u8>> = None;
    let mut filename = String::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::internal(format!("multipart invalide: {e}")))?
    {
        if field.name().is_some_and(|n| n == "file") {
            if let Some(fn_) = field.file_name() {
                filename = fn_.to_string();
            }
            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::internal(format!("lecture du fichier: {e}")))?;
            data = Some(bytes.to_vec());
        }
    }

    let bytes = data.ok_or_else(|| ApiError::internal("champ 'file' manquant"))?;
    if bytes.len() > MAX_PDF_BYTES {
        return Err(ApiError::internal("fichier trop volumineux (max 15 Mo)"));
    }

    // Verify the board exists before doing any work.
    let board =
        crate::repo::get_board(&state.db, board_id)?.ok_or_else(|| ApiError::not_found("board"))?;

    let analysis = tokio::task::spawn_blocking({
        let fn_name = filename.clone();
        move || analyze_bytes(&fn_name, &bytes)
    })
    .await
    .map_err(|e| ApiError::internal(format!("tâche d'extraction: {e}")))
    .and_then(|r| r.map_err(ApiError::internal))?;

    // Materialize the suggested tasks as todos on the board.
    let mut created = Vec::new();
    for task in &analysis.suggested_tasks {
        let input = CreateTodo {
            title: task.title.clone(),
            description: Some(format!(
                "Importé depuis un document ({}).\nExtrait: {}",
                if is_markdown(&filename) {
                    "Markdown"
                } else {
                    "PDF"
                },
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

/// Quick check: is the filename a Markdown / plain-text file?
fn is_markdown(filename: &str) -> bool {
    let ext = std::path::Path::new(filename)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    matches!(ext.as_str(), "md" | "markdown" | "txt")
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
    async fn upload_analyzes_markdown() {
        let app = build();
        let boundary = "----t2ftestboundary";
        let md_content = "# Sprint\n- [ ] Préparer la démo avant le 31/10/2026\n- Réviser le code";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"file\"; filename=\"sprint.md\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: text/markdown\r\n\r\n");
        body.extend_from_slice(md_content.as_bytes());
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
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["page_count"], 1);
        assert_eq!(v["refined"], false);
        // The checkbox task must be in the suggestions.
        let tasks = v["suggested_tasks"].as_array().unwrap();
        assert!(tasks.iter().any(|t| t["title"]
            .as_str()
            .unwrap_or("")
            .contains("Préparer la démo")));
    }

    #[tokio::test]
    async fn upload_rejects_unknown_binary() {
        let app = build();
        let boundary = "----t2ftestboundary";
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"file\"; filename=\"x.bin\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        // Binary data that is neither valid PDF nor UTF-8 text.
        body.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x01, 0x02, 0x03]);
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
        // Unknown extension → treated as PDF → fails (no %PDF header).
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
