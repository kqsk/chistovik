//! Local history of successful runs (HIST-01..03): SQLite in AppData. The texts are stored
//! only here; they never go to the log.

use rusqlite::{params, Connection};
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub const PAGE: usize = 50;
/// Allowed limits; 0 = no limit.
pub const LIMITS: [u32; 4] = [100, 500, 2000, 0];
pub const DEFAULT_LIMIT: u32 = 500;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: i64,
    /// Unix time, ms.
    pub ts: i64,
    pub prompt: String,
    /// "Provider · model".
    pub model: String,
    pub before: String,
    pub after: String,
}

pub struct NewEntry<'a> {
    pub ts: i64,
    pub prompt: &'a str,
    pub model: &'a str,
    pub before: &'a str,
    pub after: &'a str,
}

pub fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS entries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts INTEGER NOT NULL,
            prompt TEXT NOT NULL,
            model TEXT NOT NULL,
            before TEXT NOT NULL,
            after TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS entries_ts ON entries(ts);",
    )
}

/// Inserts and drops the oldest entries beyond `limit` (0 = keep all).
pub fn insert(conn: &Connection, e: &NewEntry, limit: u32) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO entries (ts, prompt, model, before, after) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![e.ts, e.prompt, e.model, e.before, e.after],
    )?;
    let id = conn.last_insert_rowid();
    prune(conn, limit)?;
    Ok(id)
}

pub fn prune(conn: &Connection, limit: u32) -> rusqlite::Result<()> {
    if limit > 0 {
        conn.execute(
            "DELETE FROM entries WHERE id NOT IN
                (SELECT id FROM entries ORDER BY ts DESC, id DESC LIMIT ?1)",
            params![limit],
        )?;
    }
    Ok(())
}

/// `%`, `_` and `\` in the query are literal.
fn like_pattern(q: &str) -> String {
    let mut out = String::with_capacity(q.len() + 2);
    out.push('%');
    for c in q.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// Newest first, `PAGE` at a time; the query matches the command, the "before" or the
/// "after" text (SQLite LIKE: case-insensitive for ASCII only).
pub fn list(conn: &Connection, query: &str, offset: usize) -> rusqlite::Result<Vec<Entry>> {
    let q = query.trim();
    let mut stmt = conn.prepare(
        "SELECT id, ts, prompt, model, before, after FROM entries
         WHERE ?1 = '' OR before LIKE ?2 ESCAPE '\\' OR after LIKE ?2 ESCAPE '\\'
            OR prompt LIKE ?2 ESCAPE '\\'
         ORDER BY ts DESC, id DESC LIMIT ?3 OFFSET ?4",
    )?;
    let rows = stmt.query_map(
        params![q, like_pattern(q), PAGE as i64, offset as i64],
        |r| {
            Ok(Entry {
                id: r.get(0)?,
                ts: r.get(1)?,
                prompt: r.get(2)?,
                model: r.get(3)?,
                before: r.get(4)?,
                after: r.get(5)?,
            })
        },
    )?;
    rows.collect()
}

pub fn delete(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM entries WHERE id = ?1", params![id])
        .map(|_| ())
}

pub fn clear(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM entries", [])?;
    // Give the space (and the deleted texts) back.
    conn.execute_batch("VACUUM")
}

/// Lazily opened database (no file is created until history is used).
#[derive(Default)]
pub struct Db(Mutex<Option<Connection>>);

pub fn with_db<T>(
    app: &AppHandle,
    f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
) -> Result<T, String> {
    let db = app.state::<Db>();
    let mut guard = db.0.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_none() {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let conn = Connection::open(dir.join("history.db")).map_err(|e| e.to_string())?;
        init(&conn).map_err(|e| e.to_string())?;
        *guard = Some(conn);
    }
    let conn = guard.as_ref().ok_or("history unavailable")?;
    // Errors carry no text content: rusqlite messages are about SQL/IO only.
    f(conn).map_err(|e| e.to_string())
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        init(&c).unwrap();
        c
    }

    fn add(c: &Connection, ts: i64, before: &str, limit: u32) -> i64 {
        insert(
            c,
            &NewEntry {
                ts,
                prompt: "Исправить",
                model: "OpenAI · gpt",
                before,
                after: "после",
            },
            limit,
        )
        .unwrap()
    }

    #[test]
    fn newest_first_and_paged() {
        let c = db();
        for i in 0..60 {
            add(&c, i, &format!("текст {i}"), 0);
        }
        let p1 = list(&c, "", 0).unwrap();
        assert_eq!(p1.len(), PAGE);
        assert_eq!(p1[0].ts, 59);
        let p2 = list(&c, "", PAGE).unwrap();
        assert_eq!(p2.len(), 10);
        assert_eq!(p2.last().unwrap().ts, 0);
    }

    #[test]
    fn limit_drops_oldest() {
        let c = db();
        for i in 0..105 {
            add(&c, i, "x", 100);
        }
        let all: Vec<i64> = (0..3)
            .flat_map(|p| list(&c, "", p * PAGE).unwrap())
            .map(|e| e.ts)
            .collect();
        assert_eq!(all.len(), 100);
        assert_eq!(*all.last().unwrap(), 5);
    }

    #[test]
    fn search_in_both_texts_and_literal_wildcards() {
        let c = db();
        add(&c, 1, "кошка", 0);
        add(&c, 2, "100% готово", 0);
        add(&c, 3, "100 готово", 0);
        assert_eq!(list(&c, "кошк", 0).unwrap().len(), 1);
        assert_eq!(list(&c, "после", 0).unwrap().len(), 3);
        let r = list(&c, "100%", 0).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].before, "100% готово");
        assert_eq!(list(&c, "_", 0).unwrap().len(), 0);
    }

    #[test]
    fn delete_and_clear() {
        let c = db();
        let a = add(&c, 1, "a", 0);
        add(&c, 2, "b", 0);
        delete(&c, a).unwrap();
        assert_eq!(list(&c, "", 0).unwrap().len(), 1);
        clear(&c).unwrap();
        assert!(list(&c, "", 0).unwrap().is_empty());
    }

    #[test]
    fn lowering_the_limit_prunes() {
        let c = db();
        for i in 0..10 {
            add(&c, i, "x", 0);
        }
        prune(&c, 3).unwrap();
        assert_eq!(list(&c, "", 0).unwrap().len(), 3);
    }
}
