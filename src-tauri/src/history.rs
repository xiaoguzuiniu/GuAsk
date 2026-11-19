use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub struct Db(pub Mutex<Connection>);

#[derive(Serialize, Deserialize, Clone)]
pub struct Item {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub ts: i64,
    pub mode: String,
    pub source: String,
    /// 对话 JSON：[{role, content}, ...]（不含 system）
    pub messages: String,
    #[serde(default)]
    pub model: String,
}

pub fn open(dir: &Path) -> Result<Db, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let c = Connection::open(dir.join("history.db")).map_err(|e| e.to_string())?;
    c.execute_batch(
        "CREATE TABLE IF NOT EXISTS history(
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts INTEGER NOT NULL,
            mode TEXT NOT NULL,
            source TEXT NOT NULL,
            messages TEXT NOT NULL,
            model TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_history_ts ON history(ts DESC);",
    )
    .map_err(|e| e.to_string())?;
    Ok(Db(Mutex::new(c)))
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// 新增（id=0）或更新同一会话（id>0，追问时覆盖），返回 id
pub fn save(db: &Db, it: Item) -> Result<i64, String> {
    let c = db.0.lock().unwrap();
    if it.id > 0 {
        c.execute("UPDATE history SET messages=?1, model=?2, ts=?3 WHERE id=?4", params![it.messages, it.model, now(), it.id])
            .map_err(|e| e.to_string())?;
        Ok(it.id)
    } else {
        c.execute(
            "INSERT INTO history(ts,mode,source,messages,model) VALUES(?1,?2,?3,?4,?5)",
            params![now(), it.mode, it.source, it.messages, it.model],
        )
        .map_err(|e| e.to_string())?;
        Ok(c.last_insert_rowid())
    }
}

pub fn list(db: &Db, q: &str, mode: &str, offset: i64, limit: i64) -> Result<(Vec<Item>, i64), String> {
    let c = db.0.lock().unwrap();
    let like = format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
    let wh = "(?1 = '%%' OR source LIKE ?1 ESCAPE '\\' OR messages LIKE ?1 ESCAPE '\\') AND (?2 = '' OR mode = ?2)";
    let total: i64 = c
        .query_row(&format!("SELECT COUNT(*) FROM history WHERE {wh}"), params![like, mode], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let mut st = c
        .prepare(&format!("SELECT id,ts,mode,source,messages,model FROM history WHERE {wh} ORDER BY ts DESC LIMIT ?3 OFFSET ?4"))
        .map_err(|e| e.to_string())?;
    let rows = st
        .query_map(params![like, mode, limit, offset], |r| {
            Ok(Item { id: r.get(0)?, ts: r.get(1)?, mode: r.get(2)?, source: r.get(3)?, messages: r.get(4)?, model: r.get(5)? })
        })
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();
    Ok((rows, total))
}

pub fn delete(db: &Db, id: i64) -> Result<(), String> {
    db.0.lock().unwrap().execute("DELETE FROM history WHERE id=?1", params![id]).map(|_| ()).map_err(|e| e.to_string())
}

pub fn clear(db: &Db) -> Result<(), String> {
    db.0.lock().unwrap().execute("DELETE FROM history", []).map(|_| ()).map_err(|e| e.to_string())
}
