//! `file_ledger` 仓储：文件 hash 幂等账本，导入去重用。

use rusqlite::{Connection, OptionalExtension};

use crate::error::CoreResult;

/// 账本命中结果。
pub struct LedgerHit {
    pub file_hash: String,
    pub seen_count: i64,
    pub submission_id: Option<i64>,
}

/// 查 hash 是否已见过。
pub fn get(conn: &Connection, file_hash: &str) -> CoreResult<Option<LedgerHit>> {
    let hit = conn
        .query_row(
            "SELECT file_hash, seen_count, submission_id FROM file_ledger WHERE file_hash = ?1",
            [file_hash],
            |r| {
                Ok(LedgerHit {
                    file_hash: r.get(0)?,
                    seen_count: r.get(1)?,
                    submission_id: r.get(2)?,
                })
            },
        )
        .optional()?;
    Ok(hit)
}

/// 首次登记一个 hash（导入成功后调用，关联 submission）。
pub fn record(conn: &Connection, file_hash: &str, first_path: &str, submission_id: i64) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO file_ledger (file_hash, first_path, submission_id)
         VALUES (?1, ?2, ?3)",
        (file_hash, first_path, submission_id),
    )?;
    Ok(())
}

/// 再次见到同一 hash：seen_count + 1。
pub fn bump(conn: &Connection, file_hash: &str) -> CoreResult<()> {
    conn.execute(
        "UPDATE file_ledger SET seen_count = seen_count + 1, updated_at = datetime('now')
         WHERE file_hash = ?1",
        [file_hash],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    #[test]
    fn record_then_bump() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        assert!(get(&conn, "abc").unwrap().is_none());
        // 需要一个 submission 外键？file_ledger.submission_id 可空且无强制，此处用占位 id 1 但无 FK 约束行
        // 为避免外键失败，submission_id 列允许任意（无行也可，因 SQLite 默认 FK 对 NULL 跳过；这里传具体值需存在）
        // 改为先不关联（传一个已存在的 submission）。简化：插入一条 submission。
        conn.execute(
            "INSERT INTO submissions (module, media_type, file_path, file_hash)
             VALUES ('recitation','audio','/x.m4a','abc')",
            [],
        )
        .unwrap();
        let sub_id = conn.last_insert_rowid();
        record(&conn, "abc", "/x.m4a", sub_id).unwrap();
        let hit = get(&conn, "abc").unwrap().unwrap();
        assert_eq!(hit.seen_count, 1);
        bump(&conn, "abc").unwrap();
        assert_eq!(get(&conn, "abc").unwrap().unwrap().seen_count, 2);
    }
}
