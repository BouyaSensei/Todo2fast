//! Deterministic PDF understanding: extract text, detect due dates, and turn a
//! document into task candidates. No LLM required — fully offline and testable.

use chrono::NaiveDate;
use regex::Regex;

/// A task candidate extracted from a document.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractedTask {
    pub title: String,
    /// ISO-8601 date (`YYYY-MM-DD`) if one was detected on the line, else `None`.
    pub due_date: Option<String>,
}

/// The full result of "understanding" a document.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentAnalysis {
    /// Number of pages that yielded text.
    pub page_count: usize,
    /// Total characters of extracted text.
    pub char_count: usize,
    /// Detected due dates (ISO), in order of appearance.
    pub detected_dates: Vec<String>,
    /// Suggested tasks derived from the document's bullet/numbered lines.
    pub suggested_tasks: Vec<ExtractedTask>,
    /// First ~280 chars of the text, for a quick preview in the UI.
    pub preview: String,
}

/// French month names (accentuated + plain) used to parse natural-language dates.
const MONTHS_FR: [(&str, u32); 15] = [
    ("janvier", 1),
    ("février", 2),
    ("fevrier", 2),
    ("mars", 3),
    ("avril", 4),
    ("mai", 5),
    ("juin", 6),
    ("juillet", 7),
    ("août", 8),
    ("aout", 8),
    ("septembre", 9),
    ("octobre", 10),
    ("novembre", 11),
    ("décembre", 12),
    ("decembre", 12),
];

/// Extract plain text from the raw bytes of a PDF.
pub fn extract_text(data: &[u8]) -> Result<String, String> {
    pdf_extract::extract_text_from_mem(data).map_err(|e| format!("PDF invalide: {e}"))
}

/// Number of pages in a PDF (best-effort; 0 if unreadable).
fn page_count(data: &[u8]) -> usize {
    pdf_extract::extract_text_from_mem_by_pages(data)
        .map(|pages| pages.len())
        .unwrap_or(0)
}

/// Parse a natural-language date token (French or English) into an ISO date.
/// Handles: `31/10/2026`, `31-10-2026`, `31.10.2026`, `31 octobre 2026`,
/// `octobre 31, 2026`, `Oct 31 2026`, `2026-10-31`.
fn parse_date_token(s: &str) -> Option<NaiveDate> {
    let t = s.trim();

    // ISO: YYYY-MM-DD
    if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        return Some(d);
    }

    // Numeric: DD/MM/YYYY or DD-MM-YYYY or DD.MM.YYYY (also accept YY for year).
    {
        let parts: Vec<&str> = t.split(['/', '-', '.']).filter(|p| !p.is_empty()).collect();
        if parts.len() == 3 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
            let (a, b, c) = (parts[0], parts[1], parts[2]);
            let year = |s: &str| -> Option<i32> {
                if s.len() == 4 {
                    s.parse().ok()
                } else if s.len() == 2 {
                    Some(2000 + s.parse::<i32>().ok()?)
                } else {
                    None
                }
            };
            // Assume DD/MM/YYYY (French convention) when first part <= 31.
            if let (Some(d), Some(m), Some(y)) =
                (a.parse::<u32>().ok(), b.parse::<u32>().ok(), year(c))
            {
                if (1..=31).contains(&d) && (1..=12).contains(&m) {
                    return NaiveDate::from_ymd_opt(y, m, d);
                }
            }
        }
    }

    // Natural language: "31 octobre 2026", "octobre 31, 2026", "Oct 31 2026".
    let lower = t.to_lowercase();
    let mut day: Option<u32> = None;
    let mut month: Option<u32> = None;
    let mut year: Option<i32> = None;

    for (name, num) in MONTHS_FR.iter() {
        if lower.contains(name) {
            month = Some(*num);
            break;
        }
    }
    // English short months.
    if month.is_none() {
        let en = [
            ("jan", 1u32),
            ("feb", 2),
            ("mar", 3),
            ("apr", 4),
            ("may", 5),
            ("jun", 6),
            ("jul", 7),
            ("aug", 8),
            ("sep", 9),
            ("oct", 10),
            ("nov", 11),
            ("dec", 12),
        ];
        for (name, num) in en {
            if lower.contains(name) {
                month = Some(num);
                break;
            }
        }
    }

    // Collect all numbers in order.
    let nums: Vec<u32> = Regex::new(r"\d{1,4}")
        .ok()
        .map(|re| {
            re.find_iter(t)
                .filter_map(|m| m.as_str().parse::<u32>().ok())
                .collect()
        })
        .unwrap_or_default();
    for n in &nums {
        if year.is_none() && (1000..=2999).contains(n) {
            year = Some(*n as i32);
        } else if day.is_none() && *n <= 31 {
            day = Some(*n);
        }
    }

    if let (Some(d), Some(m), Some(y)) = (day, month, year) {
        NaiveDate::from_ymd_opt(y, m, d)
    } else {
        None
    }
}

/// Find all dates in a chunk of text, returning them as ISO strings.
fn find_dates(text: &str) -> Vec<String> {
    let re_numeric = Regex::new(r"\b\d{1,2}[/\-.]\d{1,2}[/\-.]\d{2,4}\b").unwrap();
    let re_iso = Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").unwrap();
    let re_nl = Regex::new(
        r"\b\d{1,2}\s+(?:janvier|février|fevrier|mars|avril|mai|juin|juillet|août|aout|septembre|octobre|novembre|décembre|decembre)\s+\d{4}\b|\b(?:janvier|février|fevrier|mars|avril|mai|juin|juillet|août|aout|septembre|octobre|novembre|décembre|decembre)\s+\d{1,2},?\s+\d{4}\b",
    )
    .unwrap();

    let mut spans: Vec<(usize, String)> = Vec::new();
    for re in [&re_iso, &re_numeric, &re_nl] {
        for cap in re.find_iter(text) {
            if let Some(d) = parse_date_token(cap.as_str()) {
                spans.push((cap.start(), d.format("%Y-%m-%d").to_string()));
            }
        }
    }

    // Sort by position, then deduplicate while preserving first-seen order.
    spans.sort_by_key(|(pos, _)| *pos);
    let mut out: Vec<String> = Vec::new();
    for (_, v) in spans {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

/// Turn extracted text into suggested tasks. Heuristic: lines that look like
/// bullets, numbered items, or imperative action sentences become tasks. Each
/// task inherits the first date found on its line (if any).
fn suggest_tasks(text: &str) -> Vec<ExtractedTask> {
    let mut tasks = Vec::new();
    let num_re = Regex::new(r"^\d+[.)]\s*").unwrap();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.len() < 4 || line.len() > 120 {
            continue;
        }
        // Skip Markdown headings — they are section titles, not tasks.
        if line.starts_with('#') {
            continue;
        }
        // Strip leading bullet / number markers.
        let cleaned = line.trim_start_matches(['-', '*', '•', '·']).trim_start();
        let cleaned = num_re.replace(cleaned, "").to_string();
        // Strip a Markdown task-list checkbox: "[ ]" or "[x]".
        let cleaned = cleaned
            .trim_start()
            .strip_prefix("[ ]")
            .or_else(|| cleaned.trim_start().strip_prefix("[x]"))
            .or_else(|| cleaned.trim_start().strip_prefix("[X]"))
            .unwrap_or(cleaned.trim_start());
        let cleaned = cleaned.trim().to_string();
        if cleaned.chars().count() < 4 {
            continue;
        }
        let due = find_dates(&cleaned).into_iter().next();
        tasks.push(ExtractedTask {
            title: cleaned,
            due_date: due,
        });
    }
    // De-duplicate while preserving order.
    let mut seen = std::collections::HashSet::new();
    tasks.retain(|t| seen.insert(t.title.clone()));
    tasks.truncate(50);
    tasks
}

/// Analyze a PDF document: extract text, dates, and suggested tasks.
pub fn analyze_pdf(data: &[u8]) -> Result<DocumentAnalysis, String> {
    let text = extract_text(data)?;
    let pages = page_count(data);
    let char_count = text.chars().count();
    let detected_dates = find_dates(&text);
    let suggested_tasks = suggest_tasks(&text);
    let preview: String = text.chars().take(280).collect();
    Ok(DocumentAnalysis {
        page_count: pages,
        char_count,
        detected_dates,
        suggested_tasks,
        preview,
    })
}

/// Analyze a Markdown (or plain-text) document. The file content is used as-is;
/// bullets, numbered items and `- [ ]` checkboxes become task candidates.
pub fn analyze_markdown(text: &str) -> DocumentAnalysis {
    let char_count = text.chars().count();
    let detected_dates = find_dates(text);
    let suggested_tasks = suggest_tasks(text);
    let preview: String = text.chars().take(280).collect();
    DocumentAnalysis {
        page_count: 1, // text document — no pagination
        char_count,
        detected_dates,
        suggested_tasks,
        preview,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_numeric_french_date() {
        assert_eq!(
            parse_date_token("31/10/2026")
                .unwrap()
                .format("%Y-%m-%d")
                .to_string(),
            "2026-10-31"
        );
        assert_eq!(
            parse_date_token("5-12-2027")
                .unwrap()
                .format("%Y-%m-%d")
                .to_string(),
            "2027-12-05"
        );
    }

    #[test]
    fn parses_natural_language_date() {
        assert_eq!(
            parse_date_token("31 octobre 2026")
                .unwrap()
                .format("%Y-%m-%d")
                .to_string(),
            "2026-10-31"
        );
        assert_eq!(
            parse_date_token("octobre 31, 2026")
                .unwrap()
                .format("%Y-%m-%d")
                .to_string(),
            "2026-10-31"
        );
    }

    #[test]
    fn finds_dates_in_text() {
        let dates = find_dates("Livraison prévue le 31/10/2026 et rapport le 5 décembre 2026.");
        assert!(dates.contains(&"2026-10-31".to_string()));
        assert!(dates.contains(&"2026-12-05".to_string()));
    }

    #[test]
    fn suggests_tasks_from_bullets() {
        let text = "Objectifs:\n- Préparer la démo avant le 31/10/2026\n* Réviser le code\n1. Envoyer le rapport";
        let tasks = suggest_tasks(text);
        assert!(tasks.iter().any(|t| t.title.contains("Préparer la démo")));
        assert!(tasks
            .iter()
            .any(|t| t.due_date.as_deref() == Some("2026-10-31")));
    }

    #[test]
    fn rejects_non_pdf() {
        assert!(analyze_pdf(b"this is not a pdf").is_err());
    }

    #[test]
    fn analyzes_markdown_bullets_and_checkboxes() {
        let md = "# Sprint\n- [ ] Préparer la démo avant le 31/10/2026\n- [x] Réviser le code\n* Envoyer le rapport le 5 décembre 2026";
        let a = analyze_markdown(md);
        assert_eq!(a.page_count, 1);
        assert!(a
            .suggested_tasks
            .iter()
            .any(|t| t.title.contains("Préparer la démo")));
        assert!(a
            .suggested_tasks
            .iter()
            .any(|t| t.due_date.as_deref() == Some("2026-10-31")));
        // Checkbox must be stripped from the title.
        assert!(!a.suggested_tasks.iter().any(|t| t.title.contains("[ ]")));
    }
}
