//! `verdicts` 仓储：判定（机器评分 + 人工结论）。

use rusqlite::{Connection, OptionalExtension};

use crate::error::CoreResult;
use crate::models::{ModuleKey, Verdict};

pub struct NewVerdict<'a> {
    pub submission_id: i64,
    pub module: ModuleKey,
    pub primary_score: Option<f64>,
    pub pass: Option<bool>,
    pub secondary_score: Option<f64>,
    pub quality: Option<&'a str>,
    pub confidence: Option<f64>,
    pub answer_version: i64,
    pub metrics_json: Option<&'a str>,
    pub machine_note: Option<&'a str>,
}

const COLS: &str = "id, submission_id, module, primary_score, pass, secondary_score, quality, \
    confidence, answer_version, metrics_json, machine_note, human_result, human_note";

fn row_to_verdict(r: &rusqlite::Row<'_>) -> rusqlite::Result<Verdict> {
    Ok(Verdict {
        id: r.get("id")?,
        submission_id: r.get("submission_id")?,
        module: ModuleKey::from_db(&r.get::<_, String>("module")?),
        primary_score: r.get("primary_score")?,
        pass: r.get::<_, Option<i64>>("pass")?.map(|v| v != 0),
        secondary_score: r.get("secondary_score")?,
        quality: r.get("quality")?,
        confidence: r.get("confidence")?,
        answer_version: r.get("answer_version")?,
        metrics_json: r.get("metrics_json")?,
        machine_note: r.get("machine_note")?,
        human_result: r.get("human_result")?,
        human_note: r.get("human_note")?,
    })
}

pub fn insert(conn: &Connection, v: &NewVerdict<'_>) -> CoreResult<i64> {
    conn.execute(
        "INSERT INTO verdicts
            (submission_id, module, primary_score, pass, secondary_score, quality,
             confidence, answer_version, metrics_json, machine_note)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        (
            v.submission_id, v.module.as_str(), v.primary_score, v.pass.map(|b| b as i64),
            v.secondary_score, v.quality, v.confidence, v.answer_version, v.metrics_json, v.machine_note,
        ),
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_by_submission(conn: &Connection, submission_id: i64) -> CoreResult<Option<Verdict>> {
    let sql = format!("SELECT {COLS} FROM verdicts WHERE submission_id=?1 ORDER BY id DESC LIMIT 1");
    Ok(conn.query_row(&sql, [submission_id], row_to_verdict).optional()?)
}

/// 人工最终结论。
pub fn set_human_result(
    conn: &Connection,
    verdict_id: i64,
    result: &str,
    note: Option<&str>,
    decided_by: Option<&str>,
) -> CoreResult<()> {
    conn.execute(
        "UPDATE verdicts SET human_result=?2, human_note=?3, decided_by=?4, updated_at=datetime('now')
         WHERE id=?1",
        (verdict_id, result, note, decided_by),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn seed_submission(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO submissions (module, media_type, file_path, file_hash)
             VALUES ('recitation','audio','/x.m4a','h1')",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn insert_get_and_human_decide() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let sub = seed_submission(&conn);

        let vid = insert(
            &conn,
            &NewVerdict {
                submission_id: sub,
                module: ModuleKey::Recitation,
                primary_score: Some(96.0),
                pass: Some(true),
                secondary_score: Some(78.0),
                quality: Some("B"),
                confidence: Some(0.9),
                answer_version: 1,
                metrics_json: Some("{}"),
                machine_note: Some("正确率96%"),
            },
        )
        .unwrap();

        let v = get_by_submission(&conn, sub).unwrap().unwrap();
        assert_eq!(v.pass, Some(true));
        assert_eq!(v.quality.as_deref(), Some("B"));

        set_human_result(&conn, vid, "pass", Some("确认通过"), Some("老师")).unwrap();
        let v = get_by_submission(&conn, sub).unwrap().unwrap();
        assert_eq!(v.human_result.as_deref(), Some("pass"));
    }
}
