//! SQLite storage layer. A single `Mutex<Connection>` keeps access simple and
//! safe; all handlers run on the tokio runtime, so we hop to a blocking thread
//! for any DB work via `tokio::task::spawn_blocking` at the call site.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

/// Errors surfaced by the storage layer.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sql(#[from] rusqlite::Error),
}

/// Thread-safe handle to the SQLite database.
#[derive(Clone)]
pub struct Db(Arc<Mutex<Connection>>);

impl Db {
    /// Open (or create) a database at `path`, running migrations.
    pub fn open(path: &Path) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Self::migrate(&conn)?;
        Ok(Self(Arc::new(Mutex::new(conn))))
    }

    /// Open an in-memory database (used by tests).
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Self::migrate(&conn)?;
        Ok(Self(Arc::new(Mutex::new(conn))))
    }

    fn migrate(conn: &Connection) -> Result<(), DbError> {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS boards (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS todos (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                board_id    INTEGER NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
                title       TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                due_date    TEXT,
                done        INTEGER NOT NULL DEFAULT 0,
                position    INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_todos_board ON todos(board_id);

            CREATE TABLE IF NOT EXISTS comments (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                todo_id     INTEGER NOT NULL REFERENCES todos(id) ON DELETE CASCADE,
                author      TEXT NOT NULL,
                body        TEXT NOT NULL,
                created_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_comments_todo ON comments(todo_id);

            CREATE TABLE IF NOT EXISTS reactions (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                comment_id  INTEGER NOT NULL REFERENCES comments(id) ON DELETE CASCADE,
                author      TEXT NOT NULL,
                emoji       TEXT NOT NULL,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE (comment_id, author, emoji)
            );
            CREATE INDEX IF NOT EXISTS idx_reactions_comment ON reactions(comment_id);
            "#,
        )?;
        Ok(())
    }

    /// Run `f` with a guard on the connection. Returns whatever `f` returns.
    pub fn with<F, T>(&self, f: F) -> Result<T, DbError>
    where
        F: FnOnce(&mut Connection) -> Result<T, DbError>,
    {
        let mut guard = self.0.lock().expect("db lock poisoned");
        f(&mut guard)
    }
}
