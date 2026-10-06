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

            CREATE TABLE IF NOT EXISTS lists (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                board_id    INTEGER NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
                title       TEXT NOT NULL,
                color       TEXT,
                position    INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_lists_board ON lists(board_id);

            CREATE TABLE IF NOT EXISTS todos (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                board_id    INTEGER NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
                list_id     INTEGER REFERENCES lists(id) ON DELETE SET NULL,
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

            CREATE TABLE IF NOT EXISTS tags (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                board_id    INTEGER NOT NULL REFERENCES boards(id) ON DELETE CASCADE,
                name        TEXT NOT NULL,
                color       TEXT NOT NULL DEFAULT '#35c9dd',
                UNIQUE (board_id, name)
            );
            CREATE INDEX IF NOT EXISTS idx_tags_board ON tags(board_id);

            CREATE TABLE IF NOT EXISTS todo_tags (
                todo_id     INTEGER NOT NULL REFERENCES todos(id) ON DELETE CASCADE,
                tag_id      INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                UNIQUE (todo_id, tag_id)
            );
            CREATE INDEX IF NOT EXISTS idx_todo_tags_todo ON todo_tags(todo_id);
            "#,
        )?;
        // Idempotent upgrade for databases created before the kanban lists existed.
        let has_list_id: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('todos') WHERE name = 'list_id'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if !has_list_id {
            conn.execute("ALTER TABLE todos ADD COLUMN list_id INTEGER REFERENCES lists(id) ON DELETE SET NULL", [])?;
        }
        // Idempotent upgrade for databases created before lists had a color.
        let has_color: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('lists') WHERE name = 'color'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if !has_color {
            conn.execute("ALTER TABLE lists ADD COLUMN color TEXT", [])?;
        }
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
