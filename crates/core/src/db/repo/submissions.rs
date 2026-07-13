//! `submissions` 仓储：通用提交（音频/图片/文本）。

use rusqlite::{Connection, OptionalExtension};

use crate::error::{CoreError, CoreResult};
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

const COLS: &str = "id, module, task_id, student_id, ref_id, media_type, file_path, archived_path, file_hash, artifact_id, \
    duration_ms, parsed_meta, recognized_text, recognize_meta, recognize_status, anomaly_type, status";

fn row_to_submission(r: &rusqlite::Row<'_>) -> rusqlite::Result<Submission> {
    Ok(Submission {
        id: r.get("id")?,
        module: ModuleKey::from_db(&r.get::<_, String>("module")?),
        task_id: r.get("task_id")?,
        student_id: r.get("student_id")?,
        ref_id: r.get("ref_id")?,
        media_type: MediaType::from_db(&r.get::<_, String>("media_type")?),
        file_path: r.get("file_path")?,
        archived_path: r.get("archived_path")?,
        file_hash: r.get("file_hash")?,
        artifact_id: r.get("artifact_id")?,
        duration_ms: r.get("duration_ms")?,
        parsed_meta: r.get("parsed_meta")?,
        recognized_text: r.get("recognized_text")?,
        recognize_meta: r.get("recognize_meta")?,
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

pub fn get_by_hash(conn: &Connection, file_hash: &str) -> CoreResult<Option<Submission>> {
    let sql = format!("SELECT {COLS} FROM submissions WHERE file_hash = ?1");
    Ok(conn
        .query_row(&sql, [file_hash], row_to_submission)
        .optional()?)
}

pub fn set_file_path(conn: &Connection, id: i64, file_path: &str) -> CoreResult<()> {
    conn.execute(
        "UPDATE submissions SET file_path=?2, updated_at=datetime('now') WHERE id=?1",
        (id, file_path),
    )?;
    Ok(())
}

pub fn set_archived_path(conn: &Connection, id: i64, archived_path: &str) -> CoreResult<()> {
    conn.execute(
        "UPDATE submissions SET archived_path=?2, updated_at=datetime('now') WHERE id=?1",
        (id, archived_path),
    )?;
    Ok(())
}

/// 原子抢占识别权；同一 submission 只允许一个 processing。
pub fn claim_recognition(conn: &Connection, id: i64) -> CoreResult<()> {
    let changed = conn.execute(
        "UPDATE submissions SET recognize_status='processing', updated_at=datetime('now')
         WHERE id=?1 AND recognize_status<>'processing' AND status NOT IN ('voided','confirmed')",
        [id],
    )?;
    if changed == 1 {
        return Ok(());
    }
    let submission = get(conn, id)?
        .ok_or_else(|| CoreError::NotFound(format!("submission {id}")))?;
    if submission.recognize_status == "processing" {
        Err(CoreError::Invalid("该提交正在识别，请勿重复操作".into()))
    } else {
        Err(CoreError::Invalid(format!(
            "提交状态 {} 不允许重新识别",
            submission.status
        )))
    }
}

/// 识别失败只更新失败状态和脱敏元数据，保留上一次文本/时长供审计。
pub fn set_recognition_failure(conn: &Connection, id: i64, meta: &str) -> CoreResult<()> {
    conn.execute(
        "UPDATE submissions SET recognize_status='failed', recognize_meta=?2,
            updated_at=datetime('now') WHERE id=?1",
        (id, meta),
    )?;
    Ok(())
}

/// 某任务的最新提交（看板用）。
pub fn find_by_task(conn: &Connection, task_id: i64) -> CoreResult<Option<Submission>> {
    let sql = format!("SELECT {COLS} FROM submissions WHERE task_id = ?1 ORDER BY id DESC LIMIT 1");
    Ok(conn
        .query_row(&sql, [task_id], row_to_submission)
        .optional()?)
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

pub fn mark_anomaly(
    conn: &Connection,
    id: i64,
    anomaly_type: &str,
    parsed_meta: Option<&str>,
) -> CoreResult<()> {
    conn.execute(
        "UPDATE submissions SET anomaly_type=?2, parsed_meta=COALESCE(?3, parsed_meta),
            status='anomaly', updated_at=datetime('now') WHERE id=?1",
        (id, anomaly_type, parsed_meta),
    )?;
    Ok(())
}

pub fn clear_anomaly(conn: &Connection, id: i64) -> CoreResult<()> {
    conn.execute(
        "UPDATE submissions SET anomaly_type=NULL, updated_at=datetime('now') WHERE id=?1",
        [id],
    )?;
    Ok(())
}

/// 改派：只重设关联。异常状态保留到识别和评分全部成功，防止失败后从待处理区消失。
pub fn reassign(
    conn: &Connection,
    id: i64,
    student_id: i64,
    ref_id: i64,
    task_id: Option<i64>,
) -> CoreResult<()> {
    conn.execute(
        "UPDATE submissions SET student_id=?2, ref_id=?3, task_id=?4,
            updated_at=datetime('now') WHERE id=?1",
        (id, student_id, ref_id, task_id),
    )?;
    Ok(())
}

pub fn set_resolution_preserve_task(
    conn: &Connection,
    id: i64,
    student_id: i64,
    ref_id: i64,
) -> CoreResult<()> {
    conn.execute(
        "UPDATE submissions SET student_id=?2, ref_id=?3,
            anomaly_type=NULL, status='pending', updated_at=datetime('now') WHERE id=?1",
        (id, student_id, ref_id),
    )?;
    Ok(())
}

pub fn list_anomalies(conn: &Connection, module: ModuleKey) -> CoreResult<Vec<Submission>> {
    let sql = format!(
        "SELECT {COLS} FROM submissions
         WHERE module=?1 AND status='anomaly' AND recognize_status<>'failed' ORDER BY id DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([module.as_str()], row_to_submission)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn list_recognition_failures(
    conn: &Connection,
    module: ModuleKey,
) -> CoreResult<Vec<Submission>> {
    let sql = format!(
        "SELECT {COLS} FROM submissions
         WHERE module=?1 AND recognize_status='failed' AND status<>'voided' ORDER BY id DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([module.as_str()], row_to_submission)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn list_processing(conn: &Connection, module: ModuleKey) -> CoreResult<Vec<Submission>> {
    let sql = format!(
        "SELECT {COLS} FROM submissions
         WHERE module=?1 AND recognize_status='processing' ORDER BY id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([module.as_str()], row_to_submission)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn has_other_active_for_task(
    conn: &Connection,
    task_id: i64,
    excluding_submission_id: i64,
) -> CoreResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM submissions
         WHERE task_id=?1 AND id<>?2 AND status<>'voided'",
        (task_id, excluding_submission_id),
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

/// 导入历史：某模块最近的提交（含已评分/异常/暂存等全部状态），最新在前。
pub fn list_recent(
    conn: &Connection,
    module: ModuleKey,
    limit: i64,
) -> CoreResult<Vec<Submission>> {
    let sql = format!("SELECT {COLS} FROM submissions WHERE module=?1 ORDER BY id DESC LIMIT ?2");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map((module.as_str(), limit), row_to_submission)?;
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
        assert!(got.recognize_meta.is_none());

        assert_eq!(
            list_anomalies(&conn, ModuleKey::Recitation).unwrap().len(),
            1
        );

        set_recognition(&conn, id, Some("床前明月光"), "ok", None, Some(3200)).unwrap();
        let got = get(&conn, id).unwrap().unwrap();
        assert_eq!(got.recognized_text.as_deref(), Some("床前明月光"));
        assert_eq!(got.duration_ms, Some(3200));

        claim_recognition(&conn, id).unwrap();
        let duplicate = claim_recognition(&conn, id).unwrap_err().to_string();
        assert!(duplicate.contains("正在识别"));
        set_recognition_failure(&conn, id, r#"{"error_code":"timeout"}"#).unwrap();
        let failed = get(&conn, id).unwrap().unwrap();
        assert_eq!(failed.recognize_status, "failed");
        assert!(failed.recognize_meta.unwrap().contains("timeout"));
        assert_eq!(
            list_recognition_failures(&conn, ModuleKey::Recitation)
                .unwrap()
                .len(),
            1
        );
        assert!(list_anomalies(&conn, ModuleKey::Recitation)
            .unwrap()
            .is_empty());
    }
}
