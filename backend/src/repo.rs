//! Data-access functions for boards and todos.

use crate::db::{Db, DbError};
use crate::models::{parse_ts, Board, CreateBoard, CreateTodo, Todo, UpdateTodo};

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

// ---------- Todos ----------

fn todo_row(conn: &mut rusqlite::Connection, id: i64) -> Result<Todo> {
    let (board_id, title, description, due_date, done, position, created_at, updated_at): (
        i64,
        String,
        String,
        Option<String>,
        i64,
        i64,
        String,
        String,
    ) = conn.query_row(
        "SELECT board_id, title, description, due_date, done, position, created_at, updated_at
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
            ))
        },
    )?;
    Ok(Todo {
        id,
        board_id,
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
            "INSERT INTO todos (board_id, title, description, due_date, position)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                board_id,
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
        conn.execute(
            "UPDATE todos SET title=?1, description=?2, due_date=?3, done=?4, position=?5,
             updated_at=datetime('now') WHERE id=?6",
            rusqlite::params![title, description, due_date, done as i64, position, id],
        )?;
        Ok(Some(todo_row(conn, id)?))
    })
}

pub fn delete_todo(db: &Db, id: i64) -> Result<bool> {
    db.with(|conn| Ok(conn.execute("DELETE FROM todos WHERE id = ?1", [id])? > 0))
}
