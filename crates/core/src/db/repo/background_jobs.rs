//! 可恢复后台任务仓储。claim/finalize 都是短 SQL 状态转换。

use std::io;

use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::ids;
use crate::error::{CoreError, CoreResult};
use crate::models::{BackgroundJob, BackgroundJobStatus};

const COLS: &str = "id, public_id, job_type, business_key, source_module, business_ref_type, \
                    business_ref_id, ai_run_id, stage, status, attempts, max_attempts, lease_token, \
                    lease_expires_at, next_retry_at, progress_json, error_meta_json, created_at, updated_at";

#[derive(Clone, Copy)]
pub struct NewBackgroundJob<'a> {
    pub job_type: &'a str,
    pub business_key: &'a str,
    pub source_module: &'a str,
    pub business_ref_type: &'a str,
    pub business_ref_id: &'a str,
    pub ai_run_id: Option<i64>,
    pub stage: &'a str,
    pub max_attempts: i64,
    pub next_retry_at: Option<&'a str>,
}

fn invalid_status(value: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        9,
        Type::Text,
        Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid background job status: {value}"),
        )),
    )
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<BackgroundJob> {
    let status_raw: String = row.get(9)?;
    Ok(BackgroundJob {
        id: row.get(0)?,
        public_id: row.get(1)?,
        job_type: row.get(2)?,
        business_key: row.get(3)?,
        source_module: row.get(4)?,
        business_ref_type: row.get(5)?,
        business_ref_id: row.get(6)?,
        ai_run_id: row.get(7)?,
        stage: row.get(8)?,
        status: BackgroundJobStatus::from_db(&status_raw)
            .ok_or_else(|| invalid_status(status_raw))?,
        attempts: row.get(10)?,
        max_attempts: row.get(11)?,
        lease_token: row.get(12)?,
        lease_expires_at: row.get(13)?,
        next_retry_at: row.get(14)?,
        progress_json: row.get(15)?,
        error_meta_json: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}

fn validate_new(input: &NewBackgroundJob<'_>) -> CoreResult<()> {
    for (value, field) in [
        (input.job_type, "job_type"),
        (input.business_key, "business_key"),
        (input.source_module, "source_module"),
        (input.business_ref_type, "business_ref_type"),
        (input.business_ref_id, "business_ref_id"),
        (input.stage, "stage"),
    ] {
        if value.trim().is_empty() {
            return Err(CoreError::Invalid(format!(
                "background job {field} is required"
            )));
        }
    }
    if input.max_attempts < 1 {
        return Err(CoreError::Invalid(
            "background job max_attempts must be at least 1".into(),
        ));
    }
    Ok(())
}

fn same_identity(existing: &BackgroundJob, input: &NewBackgroundJob<'_>) -> bool {
    existing.job_type == input.job_type.trim()
        && existing.source_module == input.source_module.trim()
        && existing.business_ref_type == input.business_ref_type.trim()
        && existing.business_ref_id == input.business_ref_id.trim()
        && existing.ai_run_id == input.ai_run_id
        && existing.stage == input.stage.trim()
        && existing.max_attempts == input.max_attempts
        && existing.next_retry_at.as_deref() == input.next_retry_at
}

pub fn create_or_get(conn: &Connection, input: &NewBackgroundJob<'_>) -> CoreResult<BackgroundJob> {
    validate_new(input)?;
    conn.execute(
        "INSERT INTO background_jobs
            (public_id, job_type, business_key, source_module, business_ref_type,
             business_ref_id, ai_run_id, stage, status, max_attempts, next_retry_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'queued', ?9, ?10)
         ON CONFLICT(business_key) DO NOTHING",
        params![
            ids::new_public_id(),
            input.job_type.trim(),
            input.business_key.trim(),
            input.source_module.trim(),
            input.business_ref_type.trim(),
            input.business_ref_id.trim(),
            input.ai_run_id,
            input.stage.trim(),
            input.max_attempts,
            input.next_retry_at,
        ],
    )?;
    let job = get_by_business_key(conn, input.business_key.trim())?
        .ok_or_else(|| CoreError::Db("background job insert did not produce a row".into()))?;
    if !same_identity(&job, input) {
        return Err(CoreError::Invalid(
            "background job business_key already belongs to different input".into(),
        ));
    }
    Ok(job)
}

pub fn get_by_id(conn: &Connection, id: i64) -> CoreResult<Option<BackgroundJob>> {
    let sql = format!("SELECT {COLS} FROM background_jobs WHERE id=?1");
    Ok(conn.query_row(&sql, [id], row_to_job).optional()?)
}

pub fn get_by_business_key(conn: &Connection, key: &str) -> CoreResult<Option<BackgroundJob>> {
    let sql = format!("SELECT {COLS} FROM background_jobs WHERE business_key=?1");
    Ok(conn.query_row(&sql, [key], row_to_job).optional()?)
}

/// 原子 claim 最早可运行任务；并发 worker 最多一个拿到该行。
pub fn claim_next(
    conn: &Connection,
    job_type: &str,
    now: &str,
    lease_token: &str,
    lease_expires_at: &str,
) -> CoreResult<Option<BackgroundJob>> {
    let sql = format!(
        "UPDATE background_jobs
         SET status='claimed', attempts=attempts+1, lease_token=?3,
             lease_expires_at=?4, updated_at=?2
         WHERE id=(
             SELECT id FROM background_jobs
             WHERE job_type=?1 AND status='queued' AND attempts < max_attempts
               AND (next_retry_at IS NULL OR next_retry_at <= ?2)
             ORDER BY created_at, id
             LIMIT 1
         )
         RETURNING {COLS}"
    );
    Ok(conn
        .query_row(
            &sql,
            params![job_type, now, lease_token, lease_expires_at],
            row_to_job,
        )
        .optional()?)
}

fn transition_error(conn: &Connection, id: i64, expected: &str) -> CoreError {
    match get_by_id(conn, id) {
        Ok(None) => CoreError::NotFound(format!("background job {id}")),
        Ok(Some(job)) => CoreError::Invalid(format!(
            "background job {id} is {}, expected {expected} with matching lease",
            job.status.as_str()
        )),
        Err(err) => err,
    }
}

pub fn mark_processing(
    conn: &Connection,
    id: i64,
    lease_token: &str,
    updated_at: &str,
) -> CoreResult<BackgroundJob> {
    let changed = conn.execute(
        "UPDATE background_jobs SET status='processing', updated_at=?3
         WHERE id=?1 AND status='claimed' AND lease_token=?2",
        params![id, lease_token, updated_at],
    )?;
    if changed != 1 {
        return Err(transition_error(conn, id, "claimed"));
    }
    get_by_id(conn, id)?.ok_or_else(|| CoreError::NotFound(format!("background job {id}")))
}

pub fn update_progress(
    conn: &Connection,
    id: i64,
    lease_token: &str,
    progress_json: &str,
    updated_at: &str,
) -> CoreResult<BackgroundJob> {
    let changed = conn.execute(
        "UPDATE background_jobs SET progress_json=?3, updated_at=?4
         WHERE id=?1 AND status IN ('claimed', 'processing') AND lease_token=?2",
        params![id, lease_token, progress_json, updated_at],
    )?;
    if changed != 1 {
        return Err(transition_error(conn, id, "active"));
    }
    get_by_id(conn, id)?.ok_or_else(|| CoreError::NotFound(format!("background job {id}")))
}

pub fn finalize_succeeded(
    conn: &Connection,
    id: i64,
    lease_token: &str,
    progress_json: Option<&str>,
    updated_at: &str,
) -> CoreResult<BackgroundJob> {
    let changed = conn.execute(
        "UPDATE background_jobs
         SET status='succeeded', lease_token=NULL, lease_expires_at=NULL,
             next_retry_at=NULL, progress_json=COALESCE(?3, progress_json),
             error_meta_json=NULL, updated_at=?4
         WHERE id=?1 AND status IN ('claimed', 'processing') AND lease_token=?2",
        params![id, lease_token, progress_json, updated_at],
    )?;
    if changed != 1 {
        return Err(transition_error(conn, id, "active"));
    }
    get_by_id(conn, id)?.ok_or_else(|| CoreError::NotFound(format!("background job {id}")))
}

pub fn finalize_failed(
    conn: &Connection,
    id: i64,
    lease_token: &str,
    error_meta_json: &str,
    updated_at: &str,
) -> CoreResult<BackgroundJob> {
    let changed = conn.execute(
        "UPDATE background_jobs
         SET status='failed', lease_token=NULL, lease_expires_at=NULL,
             error_meta_json=?3, updated_at=?4
         WHERE id=?1 AND status IN ('claimed', 'processing') AND lease_token=?2",
        params![id, lease_token, error_meta_json, updated_at],
    )?;
    if changed != 1 {
        return Err(transition_error(conn, id, "active"));
    }
    get_by_id(conn, id)?.ok_or_else(|| CoreError::NotFound(format!("background job {id}")))
}

pub fn list_expired_active(conn: &Connection, now: &str) -> CoreResult<Vec<BackgroundJob>> {
    let sql = format!(
        "SELECT {COLS} FROM background_jobs
         WHERE status IN ('claimed', 'processing') AND lease_expires_at <= ?1
         ORDER BY id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([now], row_to_job)?;
    let mut jobs = Vec::new();
    for row in rows {
        jobs.push(row?);
    }
    Ok(jobs)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use super::*;
    use crate::db::{open, open_in_memory, run_migrations, CORE_MIGRATIONS};

    const NOW: &str = "2026-07-13T08:00:00.000Z";
    const LEASE_END: &str = "2026-07-13T08:05:00.000Z";

    fn input(key: &'static str) -> NewBackgroundJob<'static> {
        NewBackgroundJob {
            job_type: "asr",
            business_key: key,
            source_module: "recitation",
            business_ref_type: "submission",
            business_ref_id: "42",
            ai_run_id: None,
            stage: "recognize",
            max_attempts: 1,
            next_retry_at: None,
        }
    }

    #[test]
    fn business_key_is_idempotent_but_not_rebindable() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let first = create_or_get(&conn, &input("job:42")).unwrap();
        let same = create_or_get(&conn, &input("job:42")).unwrap();
        assert_eq!(first.id, same.id);
        assert!(create_or_get(
            &conn,
            &NewBackgroundJob {
                business_ref_id: "43",
                ..input("job:42")
            }
        )
        .is_err());
    }

    #[test]
    fn lease_token_guards_processing_and_finalize() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        create_or_get(&conn, &input("job:42")).unwrap();
        let claimed = claim_next(&conn, "asr", NOW, "lease-1", LEASE_END)
            .unwrap()
            .unwrap();
        assert_eq!(claimed.attempts, 1);
        assert!(mark_processing(&conn, claimed.id, "wrong", NOW).is_err());
        mark_processing(&conn, claimed.id, "lease-1", NOW).unwrap();
        assert!(finalize_succeeded(&conn, claimed.id, "wrong", None, NOW).is_err());
        let done = finalize_succeeded(
            &conn,
            claimed.id,
            "lease-1",
            Some(r#"{"schema_version":1,"percent":100}"#),
            NOW,
        )
        .unwrap();
        assert_eq!(done.status, BackgroundJobStatus::Succeeded);
        assert_eq!(
            claim_next(&conn, "asr", NOW, "lease-2", LEASE_END).unwrap(),
            None
        );
    }

    #[test]
    fn atomic_claim_allows_only_one_worker() {
        let db_path = std::env::temp_dir().join(format!("jiaofu-job-{}.db", ids::new_public_id()));
        let conn = open(&db_path).unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        create_or_get(&conn, &input("job:atomic")).unwrap();
        drop(conn);

        let barrier = Arc::new(Barrier::new(3));
        let handles: Vec<_> = (0..2)
            .map(|worker| {
                let path = db_path.clone();
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let conn = open(&path).unwrap();
                    barrier.wait();
                    claim_next(&conn, "asr", NOW, &format!("lease-{worker}"), LEASE_END)
                        .unwrap()
                        .is_some()
                })
            })
            .collect();
        barrier.wait();
        let claimed = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .filter(|value| *value)
            .count();
        assert_eq!(claimed, 1);
        std::fs::remove_file(db_path).unwrap();
    }
}
