//! Data-access functions for boards and todos.

use crate::db::{Db, DbError};
use crate::models::{parse_ts, Board, CreateBoard, CreateList, CreateTodo, List, Todo, UpdateTodo};

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
    let (board_id, title, position): (i64, String, i64) = conn.query_row(
        "SELECT board_id, title, position FROM lists WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    Ok(List {
        id,
        board_id,
        title,
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
            "INSERT INTO lists (board_id, title, position) VALUES (?1, ?2, ?3)",
            rusqlite::params![board_id, input.title, position],
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

pub fn rename_list(db: &Db, id: i64, title: &str) -> Result<Option<List>> {
    db.with(|conn| {
        let n = conn.execute(
            "UPDATE lists SET title = ?1 WHERE id = ?2",
            rusqlite::params![title, id],
        )?;
        if n == 0 {
            Ok(None)
        } else {
            Ok(Some(list_row(conn, id)?))
        }
    })
}

pub fn delete_list(db: &Db, id: i64) -> Result<bool> {
    db.with(|conn| Ok(conn.execute("DELETE FROM lists WHERE id = ?1", [id])? > 0))
}

// ---------- Todos ----------

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
    Ok(Todo {
        id,
        board_id,
        list_id,
        title,
        description,
        due_date,
        done: done != 0,
        position,
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
