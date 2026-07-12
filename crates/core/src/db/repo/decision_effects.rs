//! `decision_effects` 仓储：人工终审副作用的可逆账本。

use rusqlite::{Connection, OptionalExtension};

use crate::error::CoreResult;
use crate::models::ModuleKey;

#[derive(Debug, Clone)]
pub struct DecisionEffect {
    pub id: i64,
    pub verdict_id: i64,
    pub revision: i64,
    pub module: ModuleKey,
    pub student_id: i64,
    pub ref_type: String,
    pub ref_id: i64,
    pub result: String,
    pub card_before_json: String,
    pub task_before_json: String,
    pub card_after_json: String,
    pub task_after_json: String,
    pub created_makeup_task_ids_json: String,
}

pub struct NewDecisionEffect<'a> {
    pub verdict_id: i64,
    pub revision: i64,
    pub module: ModuleKey,
    pub student_id: i64,
    pub ref_type: &'a str,
    pub ref_id: i64,
    pub result: &'a str,
    pub card_before_json: &'a str,
    pub task_before_json: &'a str,
    pub card_after_json: &'a str,
    pub task_after_json: &'a str,
    pub created_makeup_task_ids_json: &'a str,
}

const COLS: &str = "id, verdict_id, revision, module, student_id, ref_type, ref_id, result, \
    card_before_json, task_before_json, card_after_json, task_after_json, \
    created_makeup_task_ids_json";
const JOIN_COLS: &str = "de.id AS id, de.verdict_id AS verdict_id, de.revision AS revision, \
    de.module AS module, de.student_id AS student_id, de.ref_type AS ref_type, \
    de.ref_id AS ref_id, de.result AS result, de.card_before_json AS card_before_json, \
    de.task_before_json AS task_before_json, de.card_after_json AS card_after_json, \
    de.task_after_json AS task_after_json, \
    de.created_makeup_task_ids_json AS created_makeup_task_ids_json";

fn row_to_effect(row: &rusqlite::Row<'_>) -> rusqlite::Result<DecisionEffect> {
    Ok(DecisionEffect {
        id: row.get("id")?,
        verdict_id: row.get("verdict_id")?,
        revision: row.get("revision")?,
        module: ModuleKey::from_db(&row.get::<_, String>("module")?),
        student_id: row.get("student_id")?,
        ref_type: row.get("ref_type")?,
        ref_id: row.get("ref_id")?,
        result: row.get("result")?,
        card_before_json: row.get("card_before_json")?,
        task_before_json: row.get("task_before_json")?,
        card_after_json: row.get("card_after_json")?,
        task_after_json: row.get("task_after_json")?,
        created_makeup_task_ids_json: row.get("created_makeup_task_ids_json")?,
    })
}

pub fn next_revision(conn: &Connection, verdict_id: i64) -> CoreResult<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(max(revision), 0) + 1 FROM decision_effects WHERE verdict_id=?1",
        [verdict_id],
        |row| row.get(0),
    )?)
}

pub fn insert(conn: &Connection, effect: &NewDecisionEffect<'_>) -> CoreResult<i64> {
    conn.execute(
        "INSERT INTO decision_effects
            (verdict_id, revision, module, student_id, ref_type, ref_id, result,
             card_before_json, task_before_json, card_after_json, task_after_json,
             created_makeup_task_ids_json)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        (
            effect.verdict_id,
            effect.revision,
            effect.module.as_str(),
            effect.student_id,
            effect.ref_type,
            effect.ref_id,
            effect.result,
            effect.card_before_json,
            effect.task_before_json,
            effect.card_after_json,
            effect.task_after_json,
            effect.created_makeup_task_ids_json,
        ),
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn active_for_verdict(
    conn: &Connection,
    verdict_id: i64,
) -> CoreResult<Option<DecisionEffect>> {
    let sql = format!(
        "SELECT {COLS} FROM decision_effects WHERE verdict_id=?1 AND state='active' LIMIT 1"
    );
    Ok(conn
        .query_row(&sql, [verdict_id], row_to_effect)
        .optional()?)
}

pub fn latest_active_for_scope(
    conn: &Connection,
    module: ModuleKey,
    student_id: i64,
    ref_type: &str,
    ref_id: i64,
) -> CoreResult<Option<DecisionEffect>> {
    let sql = format!(
        "SELECT {COLS} FROM decision_effects
         WHERE module=?1 AND student_id=?2 AND ref_type=?3 AND ref_id=?4 AND state='active'
         ORDER BY id DESC LIMIT 1"
    );
    Ok(conn
        .query_row(
            &sql,
            (module.as_str(), student_id, ref_type, ref_id),
            row_to_effect,
        )
        .optional()?)
}

/// 同一 submission 最近一条仍生效的终审效果（用于重评分后的替换确认）。
pub fn latest_active_for_submission(
    conn: &Connection,
    submission_id: i64,
) -> CoreResult<Option<DecisionEffect>> {
    let sql = format!(
        "SELECT {JOIN_COLS} FROM decision_effects de
         JOIN verdicts v ON v.id=de.verdict_id
         WHERE v.submission_id=?1 AND de.state='active'
         ORDER BY de.id DESC LIMIT 1"
    );
    Ok(conn
        .query_row(&sql, [submission_id], row_to_effect)
        .optional()?)
}

pub fn mark_reverted(conn: &Connection, id: i64) -> CoreResult<()> {
    conn.execute(
        "UPDATE decision_effects
         SET state='reverted', reverted_at=datetime('now')
         WHERE id=?1 AND state='active'",
        [id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::students::{upsert as upsert_student, StudentInput};
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    #[test]
    fn active_effect_is_unique_and_revisions_advance() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let student = upsert_student(
            &conn,
            &StudentInput {
                student_no: "2023001",
                name: "张三",
                class_id: None,
                enabled: true,
            },
        )
        .unwrap();
        conn.execute(
            "INSERT INTO submissions (module, media_type, file_path, file_hash)
             VALUES ('recitation','audio','/x.m4a','effect-hash')",
            [],
        )
        .unwrap();
        let submission_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO verdicts (submission_id, module) VALUES (?1, 'recitation')",
            [submission_id],
        )
        .unwrap();
        let verdict_id = conn.last_insert_rowid();
        let first = insert(
            &conn,
            &NewDecisionEffect {
                verdict_id,
                revision: next_revision(&conn, verdict_id).unwrap(),
                module: ModuleKey::Recitation,
                student_id: student.id,
                ref_type: "content",
                ref_id: 1,
                result: "pass",
                card_before_json: "null",
                task_before_json: "[]",
                card_after_json: "null",
                task_after_json: "[]",
                created_makeup_task_ids_json: "[]",
            },
        )
        .unwrap();
        assert_eq!(
            active_for_verdict(&conn, verdict_id).unwrap().unwrap().id,
            first
        );
        mark_reverted(&conn, first).unwrap();
        assert!(active_for_verdict(&conn, verdict_id).unwrap().is_none());
        assert_eq!(next_revision(&conn, verdict_id).unwrap(), 2);
    }
}
