//! Domain types shared across the API and storage layers.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// A kanban column on a board (Trello/Asana-style).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct List {
    pub id: i64,
    pub board_id: i64,
    pub title: String,
    /// Custom accent color for the column (`None` = default).
    #[serde(default)]
    pub color: Option<String>,
    pub position: i64,
}

/// Payload for creating a list on a board. `board_id` is injected from the URL path.
#[derive(Debug, Deserialize)]
pub struct CreateList {
    #[serde(default)]
    pub board_id: i64,
    pub title: String,
    #[serde(default)]
    pub color: Option<String>,
}

/// Partial update payload for a list (all fields optional).
#[derive(Debug, Default, Deserialize)]
pub struct UpdateList {
    pub title: Option<String>,
    /// Tri-state: field absent = unchanged, `Some("")` = reset to default,
    /// `Some("#hex")` = set the color. (A bare `null` is treated as "absent".)
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: i64,
    pub board_id: i64,
    /// Kanban column the card belongs to (`None` = unassigned).
    #[serde(default)]
    pub list_id: Option<i64>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// ISO-8601 date (e.g. `2026-10-31`) or `None`.
    pub due_date: Option<String>,
    pub done: bool,
    pub position: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Payload for creating a board.
#[derive(Debug, Deserialize)]
pub struct CreateBoard {
    pub name: String,
}

/// Payload for creating a todo. `due_date` is an ISO date string.
#[derive(Debug, Deserialize)]
pub struct CreateTodo {
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    /// Kanban column to place the card in (defaults to unassigned).
    #[serde(default)]
    pub list_id: Option<i64>,
}

/// Partial update payload for a todo (all fields optional).
#[derive(Debug, Default, Deserialize)]
pub struct UpdateTodo {
    pub title: Option<String>,
    pub description: Option<String>,
    pub due_date: Option<Option<String>>,
    pub done: Option<bool>,
    pub position: Option<i64>,
    /// Move the card to a different kanban column (`Some(None)` clears it).
    pub list_id: Option<Option<i64>>,
}

/// Parse a stored `datetime('now')` UTC string into a `DateTime<Utc>`.
pub fn parse_ts(s: &str) -> DateTime<Utc> {
    s.parse()
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .map(|naive| naive.and_utc())
        })
        .unwrap_or_else(|_| Utc::now())
}

// ─── Comments & Reactions ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Comment {
    pub id: i64,
    pub todo_id: i64,
    pub author: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Reaction {
    pub id: i64,
    pub comment_id: i64,
    pub author: String,
    pub emoji: String,
    pub created_at: DateTime<Utc>,
}

/// Payload for creating a comment. `todo_id` is injected from the URL path.
#[derive(Debug, Deserialize)]
pub struct CreateComment {
    #[serde(default)]
    pub todo_id: i64,
    pub author: String,
    pub body: String,
}

/// Payload for adding a reaction to a comment. `comment_id` is injected from the URL path.
#[derive(Debug, Deserialize)]
pub struct CreateReaction {
    #[serde(default)]
    pub comment_id: i64,
    pub author: String,
    pub emoji: String,
}
