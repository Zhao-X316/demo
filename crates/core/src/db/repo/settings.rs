//! `app_settings` 仓储：非敏感键值配置（评分门槛/间隔阶梯等）。

use rusqlite::{Connection, OptionalExtension};

use crate::error::CoreResult;

pub fn get(conn: &Connection, key: &str) -> CoreResult<Option<String>> {
    let v = conn
        .query_row("SELECT value FROM app_settings WHERE key = ?1", [key], |r| {
            r.get::<_, String>(0)
        })
        .optional()?;
    Ok(v)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value, updated_at) VALUES (?1, ?2, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = datetime('now')",
        (key, value),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    #[test]
    fn set_then_get_and_overwrite() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        assert_eq!(get(&conn, "accuracy_threshold").unwrap(), None);
        set(&conn, "accuracy_threshold", "95").unwrap();
        assert_eq!(get(&conn, "accuracy_threshold").unwrap().as_deref(), Some("95"));
        set(&conn, "accuracy_threshold", "90").unwrap();
        assert_eq!(get(&conn, "accuracy_threshold").unwrap().as_deref(), Some("90"));
    }
}
