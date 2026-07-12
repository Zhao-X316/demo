//! `rec_contents` 仓储：背诵内容（答案带版本，改答案 +1）。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::error::CoreResult;
use suite_core::models::ModuleKey;

const MODULE: ModuleKey = ModuleKey::Recitation;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecContent {
    pub id: i64,
    pub content_no: String,
    pub title: String,
    pub answer_text: String,
    pub answer_version: i64,
    pub subject_id: Option<i64>,
    pub enabled: bool,
}

pub struct ContentInput<'a> {
    pub content_no: &'a str,
    pub title: &'a str,
    pub answer_text: &'a str,
    pub subject_id: Option<i64>,
    pub enabled: bool,
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RecContent> {
    Ok(RecContent {
        id: r.get("id")?,
        content_no: r.get("content_no")?,
        title: r.get("title")?,
        answer_text: r.get("answer_text")?,
        answer_version: r.get("answer_version")?,
        subject_id: r.get("subject_id")?,
        enabled: r.get::<_, i64>("enabled")? != 0,
    })
}

const COLS: &str = "id, content_no, title, answer_text, answer_version, subject_id, enabled";

pub fn get_by_no(conn: &Connection, content_no: &str) -> CoreResult<Option<RecContent>> {
    let sql = format!("SELECT {COLS} FROM rec_contents WHERE content_no=?1");
    Ok(conn.query_row(&sql, [content_no], row).optional()?)
}

pub fn get_by_id(conn: &Connection, id: i64) -> CoreResult<Option<RecContent>> {
    let sql = format!("SELECT {COLS} FROM rec_contents WHERE id=?1");
    Ok(conn.query_row(&sql, [id], row).optional()?)
}

/// 按 content_no upsert；答案文本变化时 answer_version + 1（用于重判）。
pub fn upsert(conn: &Connection, input: &ContentInput<'_>) -> CoreResult<RecContent> {
    match get_by_no(conn, input.content_no)? {
        Some(existing) => {
            let bump = i64::from(existing.answer_text != input.answer_text);
            conn.execute(
                "UPDATE rec_contents SET title=?2, answer_text=?3,
                    answer_version = answer_version + ?4, subject_id=?5, enabled=?6,
                    updated_at=datetime('now') WHERE content_no=?1",
                (
                    input.content_no,
                    input.title,
                    input.answer_text,
                    bump,
                    input.subject_id,
                    input.enabled as i64,
                ),
            )?;
        }
        None => {
            conn.execute(
                "INSERT INTO rec_contents (content_no, title, answer_text, subject_id, enabled, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5, datetime('now'), datetime('now'))",
                (
                    input.content_no, input.title, input.answer_text,
                    input.subject_id, input.enabled as i64,
                ),
            )?;
        }
    }
    get_by_no(conn, input.content_no)?
        .ok_or_else(|| suite_core::error::CoreError::Db("upsert 后未取回内容".into()))
}

/// 批量启用/停用（软删，保留历史）。返回受影响行数。
pub fn set_enabled(conn: &Connection, ids: &[i64], enabled: bool) -> CoreResult<usize> {
    let mut n = 0;
    for &id in ids {
        n += conn.execute(
            "UPDATE rec_contents SET enabled=?1, updated_at=datetime('now') WHERE id=?2",
            (enabled as i64, id),
        )?;
    }
    Ok(n)
}

/// 批量硬删。被任务/提交引用的删不掉（计入 blocked），其余删除。
/// 返回 (删除数, 删不掉的 id 列表——只能停用)。
pub fn delete(conn: &Connection, ids: &[i64]) -> CoreResult<(usize, Vec<i64>)> {
    let mut deleted = 0;
    let mut blocked = Vec::new();
    for &id in ids {
        let refs: i64 = conn.query_row(
            "SELECT
                (SELECT count(*) FROM tasks WHERE module=?1 AND ref_type='content' AND ref_id=?2) +
                (SELECT count(*) FROM submissions WHERE module=?1 AND ref_id=?2) +
                (SELECT count(*) FROM memory_cards WHERE module=?1 AND ref_type='content' AND ref_id=?2)",
            (MODULE.as_str(), id),
            |r| r.get(0),
        )?;
        if refs > 0 {
            blocked.push(id);
            continue;
        }
        match conn.execute("DELETE FROM rec_contents WHERE id=?1", [id]) {
            Ok(n) => deleted += n,
            Err(_) => blocked.push(id), // 外键约束：有历史任务/提交
        }
    }
    Ok((deleted, blocked))
}

pub fn list(conn: &Connection, only_enabled: bool) -> CoreResult<Vec<RecContent>> {
    let sql = if only_enabled {
        format!("SELECT {COLS} FROM rec_contents WHERE enabled=1 ORDER BY content_no")
    } else {
        format!("SELECT {COLS} FROM rec_contents ORDER BY content_no")
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn setup() -> Connection {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, crate::recitation_migrations()).unwrap();
        conn
    }

    #[test]
    fn answer_version_bumps_on_change() {
        let conn = setup();
        let c = upsert(
            &conn,
            &ContentInput {
                content_no: "C012",
                title: "静夜思",
                answer_text: "床前明月光",
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap();
        assert_eq!(c.answer_version, 1);

        // 仅改标题，版本不变
        let c2 = upsert(
            &conn,
            &ContentInput {
                content_no: "C012",
                title: "静夜思(唐)",
                answer_text: "床前明月光",
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap();
        assert_eq!(c2.answer_version, 1);

        // 改答案，版本 +1
        let c3 = upsert(
            &conn,
            &ContentInput {
                content_no: "C012",
                title: "静夜思(唐)",
                answer_text: "床前明月光疑是地上霜",
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap();
        assert_eq!(c3.answer_version, 2);
        assert_eq!(list(&conn, true).unwrap().len(), 1);
    }

    #[test]
    fn delete_blocks_contents_with_history() {
        let conn = setup();
        conn.execute(
            "INSERT INTO students (student_no, name, enabled) VALUES ('2023001', '张三', 1)",
            [],
        )
        .unwrap();
        let sid = conn.last_insert_rowid();
        let c = upsert(
            &conn,
            &ContentInput {
                content_no: "C012",
                title: "静夜思",
                answer_text: "床前明月光",
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tasks (module, student_id, ref_type, ref_id, due_date)
             VALUES ('recitation', ?1, 'content', ?2, '2026-06-25')",
            (sid, c.id),
        )
        .unwrap();

        let (deleted, blocked) = delete(&conn, &[c.id]).unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(blocked, vec![c.id]);
        assert!(get_by_id(&conn, c.id).unwrap().is_some());
    }
}
