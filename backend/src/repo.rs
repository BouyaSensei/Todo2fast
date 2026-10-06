//! Data-access functions for boards and todos.

use crate::db::{Db, DbError};
use crate::models::{
    parse_ts, Board, CreateBoard, CreateList, CreateTag, CreateTodo, List, Tag, Todo, UpdateList,
    UpdateTodo,
};

pub type Result<T> = std::result::Result<T, DbError>;

// ---------- Boards ----------

pub fn create_board(db: &Db, input: &CreateBoard) -> Result<Board> {
    db.with(|conn| {
        conn.execute(
            "INSERT INTO boards (name) VALUES (?1)",
            rusqlite::params![input.name],
        )?;
        let id = conn.last_insert_rowid();
        board_row(conn, id)
    })
}

pub fn list_boards(db: &Db) -> Result<Vec<Board>> {
    db.with(|conn| {
        let mut stmt = conn.prepare("SELECT id, name, created_at FROM boards ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(Board {
                id: r.get(0)?,
                name: r.get(1)?,
                created_at: parse_ts(&r.get::<_, String>(2)?),
            })
        })?;
        rows.collect::<std::result::Result<_, _>>()
            .map_err(Into::into)
    })
}

pub fn get_board(db: &Db, id: i64) -> Result<Option<Board>> {
    db.with(|conn| {
        let row = conn.query_row(
            "SELECT id, name, created_at FROM boards WHERE id = ?1",
            [id],
            |r| {
                Ok(Board {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: parse_ts(&r.get::<_, String>(2)?),
                })
            },
        );
        match row {
            Ok(b) => Ok(Some(b)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
}

pub fn delete_board(db: &Db, id: i64) -> Result<bool> {
    db.with(|conn| Ok(conn.execute("DELETE FROM boards WHERE id = ?1", [id])? > 0))
}

fn board_row(conn: &mut rusqlite::Connection, id: i64) -> Result<Board> {
    let (name, created_at): (String, String) = conn.query_row(
        "SELECT name, created_at FROM boards WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(Board {
        id,
        name,
        created_at: parse_ts(&created_at),
    })
}

// ---------- Lists (kanban columns) ----------

fn list_row(conn: &mut rusqlite::Connection, id: i64) -> Result<List> {
    let (board_id, title, color, position): (i64, String, Option<String>, i64) = conn.query_row(
        "SELECT board_id, title, color, position FROM lists WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    Ok(List {
        id,
        board_id,
        title,
        color,
        position,
    })
}

pub fn create_list(db: &Db, board_id: i64, input: &CreateList) -> Result<List> {
    db.with(|conn| {
        let position: i64 = conn.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM lists WHERE board_id = ?1",
            [board_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO lists (board_id, title, color, position) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![board_id, input.title, input.color, position],
        )?;
        list_row(conn, conn.last_insert_rowid())
    })
}

pub fn list_lists(db: &Db, board_id: i64) -> Result<Vec<List>> {
    db.with(|conn| {
        let ids: Vec<i64> = {
            let mut stmt =
                conn.prepare("SELECT id FROM lists WHERE board_id = ?1 ORDER BY position, id")?;
            let collected = stmt
                .query_map([board_id], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            collected
        };
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(list_row(conn, id)?);
        }
        Ok(out)
    })
}

/// Partial update of a list (title and/or color). `color` is tri-state:
/// absent = unchanged, `Some("")` = reset to default, `Some(hex)` = set it.
pub fn update_list(db: &Db, id: i64, patch: &UpdateList) -> Result<Option<List>> {
    db.with(|conn| {
        let current = match list_row(conn, id) {
            Ok(l) => l,
            Err(_) => return Ok(None),
        };
        let title = patch.title.clone().unwrap_or(current.title);
        let color: Option<String> = match &patch.color {
            Some(v) if v.is_empty() => None,
            Some(v) => Some(v.clone()),
            None => current.color,
        };
        conn.execute(
            "UPDATE lists SET title = ?1, color = ?2 WHERE id = ?3",
            rusqlite::params![title, color, id],
        )?;
        Ok(Some(list_row(conn, id)?))
    })
}

pub fn delete_list(db: &Db, id: i64) -> Result<bool> {
    db.with(|conn| Ok(conn.execute("DELETE FROM lists WHERE id = ?1", [id])? > 0))
}

// ---------- Todos ----------

/// Tags attached to a todo (empty vec when none).
fn tags_for(conn: &mut rusqlite::Connection, todo_id: i64) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.board_id, t.name, t.color FROM tags t
         JOIN todo_tags tt ON tt.tag_id = t.id
         WHERE tt.todo_id = ?1 ORDER BY t.name",
    )?;
    let rows = stmt.query_map([todo_id], |r| {
        Ok(Tag {
            id: r.get(0)?,
            board_id: r.get(1)?,
            name: r.get(2)?,
            color: r.get(3)?,
        })
    })?;
    rows.collect::<std::result::Result<_, _>>()
        .map_err(Into::into)
}

fn todo_row(conn: &mut rusqlite::Connection, id: i64) -> Result<Todo> {
    let (board_id, list_id, title, description, due_date, done, position, created_at, updated_at): (
        i64,
        Option<i64>,
        String,
        String,
        Option<String>,
        i64,
        i64,
        String,
        String,
    ) = conn.query_row(
        "SELECT board_id, list_id, title, description, due_date, done, position, created_at, updated_at
         FROM todos WHERE id = ?1",
        [id],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
                r.get(7)?,
                r.get(8)?,
            ))
        },
    )?;
    let tags = tags_for(conn, id)?;
    Ok(Todo {
        id,
        board_id,
        list_id,
        title,
        description,
        due_date,
        done: done != 0,
        position,
        tags,
        created_at: parse_ts(&created_at),
        updated_at: parse_ts(&updated_at),
    })
}

pub fn create_todo(db: &Db, board_id: i64, input: &CreateTodo) -> Result<Todo> {
    db.with(|conn| {
        let position: i64 = conn.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM todos WHERE board_id = ?1",
            [board_id],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO todos (board_id, list_id, title, description, due_date, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                board_id,
                input.list_id,
                input.title,
                input.description.clone().unwrap_or_default(),
                input.due_date.clone(),
                position
            ],
        )?;
        todo_row(conn, conn.last_insert_rowid())
    })
}

pub fn list_todos(db: &Db, board_id: i64) -> Result<Vec<Todo>> {
    db.with(|conn| {
        let ids: Vec<i64> = {
            let mut stmt =
                conn.prepare("SELECT id FROM todos WHERE board_id = ?1 ORDER BY position, id")?;
            let collected = stmt
                .query_map([board_id], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?;
            collected
        };
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(todo_row(conn, id)?);
        }
        Ok(out)
    })
}

pub fn get_todo(db: &Db, id: i64) -> Result<Option<Todo>> {
    db.with(|conn| {
        let exists = conn.query_row("SELECT 1 FROM todos WHERE id = ?1", [id], |r| {
            r.get::<_, i64>(0)
        });
        match exists {
            Ok(_) => Ok(Some(todo_row(conn, id)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
}

pub fn update_todo(db: &Db, id: i64, patch: &UpdateTodo) -> Result<Option<Todo>> {
    db.with(|conn| {
        let current = match todo_row(conn, id) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        let title = patch.title.clone().unwrap_or(current.title);
        let description = patch.description.clone().unwrap_or(current.description);
        let due_date = match &patch.due_date {
            Some(v) => v.clone(),
            None => current.due_date,
        };
        let done = patch.done.unwrap_or(current.done);
        let position = patch.position.unwrap_or(current.position);
        let list_id = match &patch.list_id {
            Some(v) => *v,
            None => current.list_id,
        };
        conn.execute(
            "UPDATE todos SET title=?1, description=?2, due_date=?3, done=?4, position=?5,
             list_id=?6, updated_at=datetime('now') WHERE id=?7",
            rusqlite::params![
                title,
                description,
                due_date,
                done as i64,
                position,
                list_id,
                id
            ],
        )?;
        Ok(Some(todo_row(conn, id)?))
    })
}

pub fn delete_todo(db: &Db, id: i64) -> Result<bool> {
    db.with(|conn| Ok(conn.execute("DELETE FROM todos WHERE id = ?1", [id])? > 0))
}

// ---------- Comments ----------

fn reaction_row(conn: &mut rusqlite::Connection, id: i64) -> Result<crate::models::Reaction> {
    let (comment_id, author, emoji, created_at): (i64, String, String, String) = conn.query_row(
        "SELECT comment_id, author, emoji, created_at FROM reactions WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    Ok(crate::models::Reaction {
        id,
        comment_id,
        author,
        emoji,
        created_at: parse_ts(&created_at),
    })
}

fn reactions_for(
    conn: &mut rusqlite::Connection,
    comment_id: i64,
) -> Result<Vec<crate::models::Reaction>> {
    let ids: Vec<i64> = {
        let mut stmt =
            conn.prepare("SELECT id FROM reactions WHERE comment_id = ?1 ORDER BY id")?;
        let collected: std::result::Result<Vec<i64>, _> =
            stmt.query_map([comment_id], |r| r.get(0))?.collect();
        collected?
    };
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        out.push(reaction_row(conn, id)?);
    }
    Ok(out)
}

fn comment_row(conn: &mut rusqlite::Connection, id: i64) -> Result<crate::models::Comment> {
    let (todo_id, author, body, created_at): (i64, String, String, String) = conn.query_row(
        "SELECT todo_id, author, body, created_at FROM comments WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let reactions = reactions_for(conn, id)?;
    Ok(crate::models::Comment {
        id,
        todo_id,
        author,
        body,
        created_at: parse_ts(&created_at),
        reactions,
    })
}

pub fn create_comment(
    db: &Db,
    input: &crate::models::CreateComment,
) -> Result<crate::models::Comment> {
    db.with(|conn| {
        conn.execute(
            "INSERT INTO comments (todo_id, author, body) VALUES (?1, ?2, ?3)",
            rusqlite::params![input.todo_id, input.author, input.body],
        )?;
        comment_row(conn, conn.last_insert_rowid())
    })
}

pub fn list_comments(db: &Db, todo_id: i64) -> Result<Vec<crate::models::Comment>> {
    db.with(|conn| {
        let ids: Vec<i64> = {
            let mut stmt =
                conn.prepare("SELECT id FROM comments WHERE todo_id = ?1 ORDER BY id")?;
            let collected: std::result::Result<Vec<i64>, _> =
                stmt.query_map([todo_id], |r| r.get(0))?.collect();
            collected?
        };
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(comment_row(conn, id)?);
        }
        Ok(out)
    })
}

pub fn get_comment(db: &Db, id: i64) -> Result<Option<crate::models::Comment>> {
    db.with(|conn| {
        let exists = conn.query_row("SELECT 1 FROM comments WHERE id = ?1", [id], |r| {
            r.get::<_, i64>(0)
        });
        match exists {
            Ok(_) => Ok(Some(comment_row(conn, id)?)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
}

pub fn delete_comment(db: &Db, id: i64) -> Result<bool> {
    db.with(|conn| Ok(conn.execute("DELETE FROM comments WHERE id = ?1", [id])? > 0))
}

// ---------- Reactions ----------

pub fn add_reaction(
    db: &Db,
    input: &crate::models::CreateReaction,
) -> Result<crate::models::Comment> {
    db.with(|conn| {
        // Idempotent: same (comment, author, emoji) does not duplicate.
        conn.execute(
            "INSERT OR IGNORE INTO reactions (comment_id, author, emoji) VALUES (?1, ?2, ?3)",
            rusqlite::params![input.comment_id, input.author, input.emoji],
        )?;
        comment_row(conn, input.comment_id)
    })
}

pub fn remove_reaction(db: &Db, comment_id: i64, author: &str, emoji: &str) -> Result<bool> {
    db.with(|conn| {
        Ok(conn.execute(
            "DELETE FROM reactions WHERE comment_id = ?1 AND author = ?2 AND emoji = ?3",
            rusqlite::params![comment_id, author, emoji],
        )? > 0)
    })
}

// ---------- Tags ----------

fn tag_row(conn: &mut rusqlite::Connection, id: i64) -> Result<Tag> {
    let (board_id, name, color): (i64, String, String) = conn.query_row(
        "SELECT board_id, name, color FROM tags WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    Ok(Tag {
        id,
        board_id,
        name,
        color,
    })
}

/// Create a tag on a board. A missing/empty color falls back to the accent.
pub fn create_tag(db: &Db, board_id: i64, input: &CreateTag) -> Result<Tag> {
    db.with(|conn| {
        let color = input
            .color
            .clone()
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| "#35c9dd".into());
        conn.execute(
            "INSERT INTO tags (board_id, name, color) VALUES (?1, ?2, ?3)",
            rusqlite::params![board_id, input.name, color],
        )?;
        tag_row(conn, conn.last_insert_rowid())
    })
}

pub fn list_tags(db: &Db, board_id: i64) -> Result<Vec<Tag>> {
    db.with(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, board_id, name, color FROM tags WHERE board_id = ?1 ORDER BY name",
        )?;
        let rows = stmt.query_map([board_id], |r| {
            Ok(Tag {
                id: r.get(0)?,
                board_id: r.get(1)?,
                name: r.get(2)?,
                color: r.get(3)?,
            })
        })?;
        rows.collect::<std::result::Result<_, _>>()
            .map_err(Into::into)
    })
}

pub fn delete_tag(db: &Db, id: i64) -> Result<bool> {
    db.with(|conn| Ok(conn.execute("DELETE FROM tags WHERE id = ?1", [id])? > 0))
}

/// Attach a tag to a todo (idempotent). Returns the refreshed todo.
pub fn add_tag_to_todo(db: &Db, todo_id: i64, tag_id: i64) -> Result<Option<Todo>> {
    db.with(|conn| {
        conn.execute(
            "INSERT OR IGNORE INTO todo_tags (todo_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![todo_id, tag_id],
        )?;
        Ok(())
    })?;
    get_todo(db, todo_id)
}

/// Detach a tag from a todo. Returns the refreshed todo.
pub fn remove_tag_from_todo(db: &Db, todo_id: i64, tag_id: i64) -> Result<Option<Todo>> {
    db.with(|conn| {
        conn.execute(
            "DELETE FROM todo_tags WHERE todo_id = ?1 AND tag_id = ?2",
            rusqlite::params![todo_id, tag_id],
        )?;
        Ok(())
    })?;
    get_todo(db, todo_id)
}
