//! 数据库层：连接、迁移运行器、通用表仓储。
//!
//! 单库（SQLite）。`run_migrations` 按 id 幂等执行，已应用的跳过。
//! 应用外壳用 `open` 打开应用数据目录下的 `data.db` 并用 `Mutex` 共享连接；
//! 模块通过 `ports::Module::migrations()` 提供自己的迁移，外壳汇总后一并运行。

use rusqlite::{Connection, OptionalExtension};

use crate::error::CoreResult;
use crate::ports::Migration;

pub mod repo;

/// Core 通用表迁移（外壳应在各模块迁移之前先运行它）。
pub static CORE_MIGRATIONS: &[Migration] = &[Migration {
    id: "core_0001",
    sql: include_str!("schema/0001_core.sql"),
}];

/// 打开磁盘数据库并开启外键。
pub fn open(path: &std::path::Path) -> CoreResult<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

/// 打开内存数据库（测试用）。
pub fn open_in_memory() -> CoreResult<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

/// 幂等执行迁移：未应用的按顺序执行并登记。
pub fn run_migrations(conn: &Connection, migrations: &[Migration]) -> CoreResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            id TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )?;
    for m in migrations {
        let applied: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE id = ?1",
                [m.id],
                |r| r.get(0),
            )
            .optional()?;
        if applied.is_none() {
            conn.execute_batch(m.sql)?;
            conn.execute("INSERT INTO schema_migrations (id) VALUES (?1)", [m.id])?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        // 再跑一次不应报错
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        // 关键表存在
        let t: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='memory_cards'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(t, 1);
    }
}
