//! Optional AI provider for PDF understanding.
//!
//! The deterministic extractor (`crate::pdf_extract`) remains the default and
//! the guaranteed fallback. When an OpenAI-compatible provider is configured
//! via environment variables, the UI can list its models and ask it to refine
//! the suggested tasks. No API key or credential is ever returned to the
//! client — only provider names and model identifiers.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::pdf_extract::ExtractedTask;

/// A provider exposed to the UI. `api_key` is intentionally absent: secrets
/// live server-side (env) only and must never cross the wire.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    /// Stable id used by the client, e.g. `"openai"`.
    pub id: String,
    /// Human-readable label.
    pub label: String,
    /// `true` when an API key is configured and the provider is usable.
    pub available: bool,
    /// Default model suggested to the user (may be empty).
    pub default_model: String,
}

/// One model returned by a provider's `/models` endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
}

/// A configured provider and its runtime settings.
#[derive(Debug, Clone)]
struct ProviderConfig {
    id: String,
    label: String,
    base_url: String,
    api_key: Option<String>,
    default_model: String,
}

/// Read the AI configuration from the environment. Never panics; a missing
/// key simply marks the provider as unavailable (deterministic fallback).
fn configured_providers() -> Vec<ProviderConfig> {
    let openai_key = std::env::var("T2F_OPENAI_API_KEY")
        .ok()
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .filter(|k| !k.trim().is_empty());

    let base_url = std::env::var("T2F_OPENAI_BASE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    let default_model = std::env::var("T2F_AI_MODEL")
        .ok()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "gpt-4o-mini".to_string());

    vec![ProviderConfig {
        id: "openai".into(),
        label: "OpenAI (compatible)".into(),
        base_url,
        api_key: openai_key,
        default_model,
    }]
}

/// Build an HTTP client with a bounded timeout so a hung provider can never
/// stall the request.
fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("client HTTP: {e}"))
}

/// List the providers known to the server and whether each is usable.
pub fn list_providers() -> Vec<ProviderInfo> {
    configured_providers()
        .into_iter()
        .map(|p| ProviderInfo {
            id: p.id,
            label: p.label,
            available: p.api_key.is_some(),
            default_model: p.default_model,
        })
        .collect()
}

/// Fetch the model catalog for a provider. Returns an error string when the
/// provider is not configured or unreachable (the client then falls back to
/// deterministic mode).
pub async fn list_models(provider_id: &str) -> Result<Vec<ModelInfo>, String> {
    let p = find_provider(provider_id)?;
    let key = p
        .api_key
        .as_ref()
        .ok_or_else(|| "provider non configuré (clé API absente)".to_string())?;

    let client = http_client()?;
    let url = format!("{}/models", p.base_url.trim_end_matches('/'));
    let resp = client
        .get(&url)
        .bearer_auth(key)
        .send()
        .await
        .map_err(|e| format!("requête modèles: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("provider a répondu {}", resp.status()));
    }

    // OpenAI-compatible shape: { "data": [ { "id": ... }, ... ] }.
    #[derive(Deserialize)]
    struct ModelsResp {
        data: Vec<ModelInfo>,
    }
    let body: ModelsResp = resp
        .json()
        .await
        .map_err(|e| format!("JSON modèles: {e}"))?;
    Ok(body.data)
}

fn find_provider(id: &str) -> Result<ProviderConfig, String> {
    configured_providers()
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("provider inconnu: {id}"))
}

/// Ask the provider to refine a list of candidate tasks into clean todos.
/// The prompt is strict about returning JSON only, so parsing is robust.
pub async fn refine_tasks(
    provider_id: &str,
    model: &str,
    text_preview: &str,
    candidates: &[ExtractedTask],
) -> Result<Vec<ExtractedTask>, String> {
    let p = find_provider(provider_id)?;
    let key = p
        .api_key
        .as_ref()
        .ok_or_else(|| "provider non configuré (clé API absente)".to_string())?;
    let model = if model.trim().is_empty() {
        p.default_model.clone()
    } else {
        model.to_string()
    };

    let candidates_json = serde_json::to_string(candidates).unwrap_or_else(|_| "[]".into());
    let prompt = format!(
        "Tu es un assistant qui transforme des extraits de document en tâches \
         claires pour un gestionnaire de todo. Voici le texte source (tronqué) :\n\n\
         {text}\n\nVoici les tâches candidates détectées (JSON) :\n{candidates}\n\n\
         Renvoie UNIQUEMENT un tableau JSON de {{\"title\": string, \"due_date\": \
         string|null}} avec due_date au format YYYY-MM-DD ou null. Nettoie les titres, \
         supprime les doublons et les lignes non actionnables. Pas d'autre texte.",
        text = text_preview.chars().take(4000).collect::<String>(),
        candidates = candidates_json
    );

    let client = http_client()?;
    let url = format!("{}/chat/completions", p.base_url.trim_end_matches('/'));
    let payload = serde_json::json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": "Tu réponds uniquement avec du JSON valide." },
            { "role": "user", "content": prompt }
        ]
    });

    let resp = client
        .post(&url)
        .bearer_auth(key)
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("requête IA: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("provider a répondu {}", resp.status()));
    }

    #[derive(Deserialize)]
    struct ChatResp {
        choices: Vec<Choice>,
    }
    #[derive(Deserialize)]
    struct Choice {
        message: Message,
    }
    #[derive(Deserialize)]
    struct Message {
        content: String,
    }

    let chat: ChatResp = resp
        .json()
        .await
        .map_err(|e| format!("JSON réponse IA: {e}"))?;
    let raw = chat
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| "réponse IA vide".to_string())?;

    parse_tasks_from_llm(&raw)
}

/// Parse the LLM's JSON array, tolerating a surrounding code fence. Returns an
/// error when nothing usable comes back so the caller falls back to the
/// deterministic candidates.
fn parse_tasks_from_llm(raw: &str) -> Result<Vec<ExtractedTask>, String> {
    let trimmed = raw.trim();
    // Strip a ```json ... ``` fence if present.
    let inner = trimmed
        .strip_prefix("```")
        .map(|s| s.strip_prefix("json").unwrap_or(s).trim())
        .and_then(|s| s.strip_suffix("```"))
        .unwrap_or(trimmed)
        .trim();

    let arr: Vec<ExtractedTask> =
        serde_json::from_str(inner).map_err(|e| format!("JSON IA invalide: {e}"))?;
    if arr.is_empty() {
        return Err("aucune tâche dans la réponse IA".to_string());
    }
    // Normalize: trim titles, drop empties, cap the list.
    let cleaned: Vec<ExtractedTask> = arr
        .into_iter()
        .map(|t| ExtractedTask {
            title: t.title.trim().to_string(),
            due_date: t.due_date.filter(|d| !d.trim().is_empty()),
        })
        .filter(|t| !t.title.is_empty())
        .collect();
    if cleaned.is_empty() {
        return Err("aucune tâche valide dans la réponse IA".to_string());
    }
    Ok(cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_openai_provider_as_unavailable_without_key() {
        // In the test environment no key is set, so the provider must report
        // itself as unavailable (deterministic fallback path).
        let providers = list_providers();
        assert!(providers.iter().any(|p| p.id == "openai"));
        let openai = providers.iter().find(|p| p.id == "openai").unwrap();
        assert!(!openai.available);
    }

    #[test]
    fn unknown_provider_errors() {
        assert!(find_provider("does-not-exist").is_err());
    }

    #[test]
    fn parses_plain_json_array() {
        let tasks = parse_tasks_from_llm(
            r#"[{"title":"Préparer la démo","due_date":"2026-10-31"},{"title":"Réviser le code","due_date":null}]"#,
        )
        .unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].due_date.as_deref(), Some("2026-10-31"));
        assert_eq!(tasks[1].due_date, None);
    }

    #[test]
    fn parses_json_in_code_fence() {
        let tasks = parse_tasks_from_llm(
            "```json\n[{\"title\":\"Envoyer le rapport\",\"due_date\":null}]\n```",
        )
        .unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Envoyer le rapport");
    }

    #[test]
    fn rejects_empty_array() {
        assert!(parse_tasks_from_llm("[]").is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_tasks_from_llm("ceci n'est pas du json").is_err());
    }
}
