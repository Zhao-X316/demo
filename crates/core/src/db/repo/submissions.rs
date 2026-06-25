//! `submissions` 仓储：通用提交（音频/图片/文本）。

use rusqlite::{Connection, OptionalExtension};

use crate::error::CoreResult;
use crate::models::{MediaType, ModuleKey, Submission};

pub struct NewSubmission<'a> {
    pub module: ModuleKey,
    pub task_id: Option<i64>,
    pub student_id: Option<i64>,
    pub ref_id: Option<i64>,
    pub media_type: MediaType,
    pub file_path: &'a str,
    pub file_hash: &'a str,
    pub duration_ms: Option<i64>,
    pub parsed_meta: Option<&'a str>,
    pub anomaly_type: Option<&'a str>,
    pub status: &'a str,
}

const COLS: &str = "id, module, task_id, student_id, ref_id, media_type, file_path, file_hash, \
    duration_ms, parsed_meta, recognized_text, recognize_status, anomaly_type, status";

fn row_to_submission(r: &rusqlite::Row<'_>) -> rusqlite::Result<Submission> {
    Ok(Submission {
        id: r.get("id")?,
        module: ModuleKey::from_db(&r.get::<_, String>("module")?),
        task_id: r.get("task_id")?,
        student_id: r.get("student_id")?,
        ref_id: r.get("ref_id")?,
        media_type: MediaType::from_db(&r.get::<_, String>("media_type")?),
        file_path: r.get("file_path")?,
        file_hash: r.get("file_hash")?,
        duration_ms: r.get("duration_ms")?,
        parsed_meta: r.get("parsed_meta")?,
        recognized_text: r.get("recognized_text")?,
        recognize_status: r.get("recognize_status")?,
        anomaly_type: r.get("anomaly_type")?,
        status: r.get("status")?,
    })
}

pub fn insert(conn: &Connection, s: &NewSubmission<'_>) -> CoreResult<i64> {
    conn.execute(
        "INSERT INTO submissions
            (module, task_id, student_id, ref_id, media_type, file_path, file_hash,
             duration_ms, parsed_meta, anomaly_type, status)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        (
            s.module.as_str(),
            s.task_id,
            s.student_id,
            s.ref_id,
            s.media_type.as_str(),
            s.file_path,
            s.file_hash,
            s.duration_ms,
            s.parsed_meta,
            s.anomaly_type,
            s.status,
        ),
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get(conn: &Connection, id: i64) -> CoreResult<Option<Submission>> {
    let sql = format!("SELECT {COLS} FROM submissions WHERE id = ?1");
    Ok(conn.query_row(&sql, [id], row_to_submission).optional()?)
}

/// 某任务的最新提交（看板用）。
pub fn find_by_task(conn: &Connection, task_id: i64) -> CoreResult<Option<Submission>> {
    let sql = format!("SELECT {COLS} FROM submissions WHERE task_id = ?1 ORDER BY id DESC LIMIT 1");
    Ok(conn.query_row(&sql, [task_id], row_to_submission).optional()?)
}

/// 写入识别结果。
pub fn set_recognition(
    conn: &Connection,
    id: i64,
    text: Option<&str>,
    status: &str,
    meta: Option<&str>,
    duration_ms: Option<i64>,
) -> CoreResult<()> {
    conn.execute(
        "UPDATE submissions SET recognized_text=?2, recognize_status=?3, recognize_meta=?4,
            duration_ms=COALESCE(?5, duration_ms), updated_at=datetime('now') WHERE id=?1",
        (id, text, status, meta, duration_ms),
    )?;
    Ok(())
}

pub fn set_status(conn: &Connection, id: i64, status: &str) -> CoreResult<()> {
    conn.execute(
        "UPDATE submissions SET status=?2, updated_at=datetime('now') WHERE id=?1",
        (id, status),
    )?;
    Ok(())
}

/// 改派：重设关联并清除异常，回到 pending。
pub fn reassign(
    conn: &Connection,
    id: i64,
    student_id: i64,
    ref_id: i64,
    task_id: Option<i64>,
) -> CoreResult<()> {
    conn.execute(
        "UPDATE submissions SET student_id=?2, ref_id=?3, task_id=?4,
            anomaly_type=NULL, status='pending', updated_at=datetime('now') WHERE id=?1",
        (id, student_id, ref_id, task_id),
    )?;
    Ok(())
}

pub fn list_anomalies(conn: &Connection, module: ModuleKey) -> CoreResult<Vec<Submission>> {
    let sql = format!(
        "SELECT {COLS} FROM submissions WHERE module=?1 AND status='anomaly' ORDER BY id DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([module.as_str()], row_to_submission)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    #[test]
    fn insert_get_update() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let id = insert(
            &conn,
            &NewSubmission {
                module: ModuleKey::Recitation,
                task_id: None,
                student_id: None,
                ref_id: None,
                media_type: MediaType::Audio,
                file_path: "/a.m4a",
                file_hash: "h1",
                duration_ms: Some(3000),
                parsed_meta: None,
                anomaly_type: Some("task_not_found"),
                status: "anomaly",
            },
        )
        .unwrap();

        let got = get(&conn, id).unwrap().unwrap();
        assert_eq!(got.status, "anomaly");
        assert_eq!(got.anomaly_type.as_deref(), Some("task_not_found"));

        assert_eq!(list_anomalies(&conn, ModuleKey::Recitation).unwrap().len(), 1);

        set_recognition(&conn, id, Some("床前明月光"), "ok", None, Some(3200)).unwrap();
        let got = get(&conn, id).unwrap().unwrap();
        assert_eq!(got.recognized_text.as_deref(), Some("床前明月光"));
        assert_eq!(got.duration_ms, Some(3200));
    }
}
