//! AI 后台任务编排：claim/finalize 与 `ai_runs` 在同一短事务中切换状态。

use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;

use crate::db::repo::ai_runs::{self, NewAiRun};
use crate::db::repo::background_jobs::{self, NewBackgroundJob};
use crate::domain::{ids, time};
use crate::error::{CoreError, CoreResult};
use crate::models::{AiRun, BackgroundJob};

pub struct NewAiJob<'a> {
    pub run: NewAiRun<'a>,
    pub job_type: &'a str,
    pub business_key: &'a str,
    pub stage: &'a str,
    pub max_attempts: i64,
    pub next_retry_at: Option<&'a str>,
}

#[derive(Debug)]
pub struct ClaimedAiJob {
    pub job: BackgroundJob,
    pub ai_run: AiRun,
}

/// run 和 job 要么一起存在，要么都不写入。
pub fn enqueue_ai_job(
    conn: &Connection,
    input: &NewAiJob<'_>,
) -> CoreResult<(AiRun, BackgroundJob)> {
    let tx = conn.unchecked_transaction()?;
    let run = ai_runs::create_or_get(&tx, &input.run)?;
    let job = background_jobs::create_or_get(
        &tx,
        &NewBackgroundJob {
            job_type: input.job_type,
            business_key: input.business_key,
            source_module: input.run.source_module,
            business_ref_type: input.run.business_ref_type,
            business_ref_id: input.run.business_ref_id,
            ai_run_id: Some(run.id),
            stage: input.stage,
            max_attempts: input.max_attempts,
            next_retry_at: input.next_retry_at,
        },
    )?;
    tx.commit()?;
    Ok((run, job))
}

/// 原子 claim 后立刻把 job/run 切为 processing，随后调用方必须释放数据库锁再做网络请求。
pub fn claim_ai_job(
    conn: &Connection,
    job_type: &str,
    now: DateTime<Utc>,
    lease_duration_seconds: i64,
) -> CoreResult<Option<ClaimedAiJob>> {
    if lease_duration_seconds <= 0 {
        return Err(CoreError::Invalid(
            "job lease duration must be positive".into(),
        ));
    }
    let now_text = time::utc_rfc3339(now);
    let lease_end = time::utc_rfc3339(now + Duration::seconds(lease_duration_seconds));
    let lease_token = ids::new_public_id();
    let tx = conn.unchecked_transaction()?;
    let Some(claimed) =
        background_jobs::claim_next(&tx, job_type, &now_text, &lease_token, &lease_end)?
    else {
        tx.commit()?;
        return Ok(None);
    };
    let ai_run_id = claimed
        .ai_run_id
        .ok_or_else(|| CoreError::Invalid(format!("AI job {} has no ai_run_id", claimed.id)))?;
    let ai_run = ai_runs::start(&tx, ai_run_id, &now_text, None)?;
    let job = background_jobs::mark_processing(&tx, claimed.id, &lease_token, &now_text)?;
    tx.commit()?;
    Ok(Some(ClaimedAiJob { job, ai_run }))
}

#[allow(clippy::too_many_arguments)]
pub fn finalize_ai_job_succeeded(
    conn: &Connection,
    job_id: i64,
    lease_token: &str,
    output_hash: &str,
    confidence: Option<f64>,
    output_json: &str,
    progress_json: Option<&str>,
    finished_at: DateTime<Utc>,
) -> CoreResult<ClaimedAiJob> {
    let finished_at = time::utc_rfc3339(finished_at);
    let tx = conn.unchecked_transaction()?;
    let job = background_jobs::get_by_id(&tx, job_id)?
        .ok_or_else(|| CoreError::NotFound(format!("background job {job_id}")))?;
    let ai_run_id = job
        .ai_run_id
        .ok_or_else(|| CoreError::Invalid(format!("AI job {job_id} has no ai_run_id")))?;
    let ai_run = ai_runs::finalize_succeeded(
        &tx,
        ai_run_id,
        output_hash,
        confidence,
        output_json,
        &finished_at,
    )?;
    let job =
        background_jobs::finalize_succeeded(&tx, job_id, lease_token, progress_json, &finished_at)?;
    tx.commit()?;
    Ok(ClaimedAiJob { job, ai_run })
}

pub fn finalize_ai_job_failed(
    conn: &Connection,
    job_id: i64,
    lease_token: &str,
    error_meta_json: &str,
    finished_at: DateTime<Utc>,
) -> CoreResult<ClaimedAiJob> {
    let finished_at = time::utc_rfc3339(finished_at);
    let tx = conn.unchecked_transaction()?;
    let job = background_jobs::get_by_id(&tx, job_id)?
        .ok_or_else(|| CoreError::NotFound(format!("background job {job_id}")))?;
    let ai_run_id = job
        .ai_run_id
        .ok_or_else(|| CoreError::Invalid(format!("AI job {job_id} has no ai_run_id")))?;
    let ai_run = ai_runs::finalize_failed(&tx, ai_run_id, error_meta_json, &finished_at)?;
    let job =
        background_jobs::finalize_failed(&tx, job_id, lease_token, error_meta_json, &finished_at)?;
    tx.commit()?;
    Ok(ClaimedAiJob { job, ai_run })
}

/// 应用启动时把过期 lease 标记失败，不自动重排可能产生费用的调用。
pub fn fail_expired_ai_jobs(conn: &Connection, now: DateTime<Utc>) -> CoreResult<usize> {
    let now_text = time::utc_rfc3339(now);
    let error_meta = r#"{"schema_version":1,"code":"LEASE_EXPIRED","retryable":false}"#;
    let tx = conn.unchecked_transaction()?;
    let expired = background_jobs::list_expired_active(&tx, &now_text)?;
    for job in &expired {
        let lease_token = job.lease_token.as_deref().ok_or_else(|| {
            CoreError::Invalid(format!("active job {} has no lease token", job.id))
        })?;
        if let Some(ai_run_id) = job.ai_run_id {
            ai_runs::finalize_failed(&tx, ai_run_id, error_meta, &now_text)?;
        }
        background_jobs::finalize_failed(&tx, job.id, lease_token, error_meta, &now_text)?;
    }
    tx.commit()?;
    Ok(expired.len())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use crate::models::{AiRunStatus, BackgroundJobStatus};

    const INPUT_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OUTPUT_HASH: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 13, 8, 0, 0).unwrap()
    }

    fn input() -> NewAiJob<'static> {
        NewAiJob {
            run: NewAiRun {
                idempotency_key: "asr:submission:42:v1",
                run_type: "asr",
                source_module: "recitation",
                business_ref_type: "submission",
                business_ref_id: "42",
                input_artifact_id: None,
                provider: "fixture",
                model_name: "asr-fixture",
                model_version: "1",
                config_version: "1",
                prompt_or_rule_version: "1",
                input_hash: INPUT_HASH,
                retry_of_ai_run_id: None,
            },
            job_type: "asr",
            business_key: "asr-job:submission:42:v1",
            stage: "recognize",
            max_attempts: 1,
            next_retry_at: None,
        }
    }

    fn setup_claimed() -> (Connection, ClaimedAiJob) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        enqueue_ai_job(&conn, &input()).unwrap();
        let claimed = claim_ai_job(&conn, "asr", now(), 60).unwrap().unwrap();
        (conn, claimed)
    }

    #[test]
    fn enqueue_and_claim_move_job_and_run_together() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let (run, job) = enqueue_ai_job(&conn, &input()).unwrap();
        let (same_run, same_job) = enqueue_ai_job(&conn, &input()).unwrap();
        assert_eq!(run.id, same_run.id);
        assert_eq!(job.id, same_job.id);

        let claimed = claim_ai_job(&conn, "asr", now(), 60).unwrap().unwrap();
        assert_eq!(claimed.job.status, BackgroundJobStatus::Processing);
        assert_eq!(claimed.ai_run.status, AiRunStatus::Processing);
        assert!(claim_ai_job(&conn, "asr", now(), 60).unwrap().is_none());
    }

    #[test]
    fn success_finalize_is_atomic_across_both_ledgers() {
        let (conn, claimed) = setup_claimed();
        let result = finalize_ai_job_succeeded(
            &conn,
            claimed.job.id,
            claimed.job.lease_token.as_deref().unwrap(),
            OUTPUT_HASH,
            Some(0.9),
            r#"{"schema_version":1,"text":"ok"}"#,
            Some(r#"{"schema_version":1,"percent":100}"#),
            now() + Duration::seconds(1),
        )
        .unwrap();
        assert_eq!(result.job.status, BackgroundJobStatus::Succeeded);
        assert_eq!(result.ai_run.status, AiRunStatus::Succeeded);
    }

    #[test]
    fn finalize_rolls_back_ai_run_when_job_write_fails() {
        let (conn, claimed) = setup_claimed();
        conn.execute_batch(
            "CREATE TRIGGER fail_job_finalize
             BEFORE UPDATE OF status ON background_jobs
             WHEN NEW.status='succeeded'
             BEGIN
               SELECT RAISE(ABORT, 'injected job finalize failure');
             END;",
        )
        .unwrap();

        assert!(finalize_ai_job_succeeded(
            &conn,
            claimed.job.id,
            claimed.job.lease_token.as_deref().unwrap(),
            OUTPUT_HASH,
            Some(0.9),
            r#"{"schema_version":1,"text":"ok"}"#,
            None,
            now() + Duration::seconds(1),
        )
        .is_err());
        assert_eq!(
            ai_runs::get_by_id(&conn, claimed.ai_run.id)
                .unwrap()
                .unwrap()
                .status,
            AiRunStatus::Processing
        );
    }

    #[test]
    fn expired_lease_fails_without_automatic_retry() {
        let (conn, claimed) = setup_claimed();
        assert_eq!(
            fail_expired_ai_jobs(&conn, now() + Duration::seconds(61)).unwrap(),
            1
        );
        assert_eq!(
            background_jobs::get_by_id(&conn, claimed.job.id)
                .unwrap()
                .unwrap()
                .status,
            BackgroundJobStatus::Failed
        );
        assert_eq!(
            ai_runs::get_by_id(&conn, claimed.ai_run.id)
                .unwrap()
                .unwrap()
                .status,
            AiRunStatus::Failed
        );
        assert!(
            claim_ai_job(&conn, "asr", now() + Duration::seconds(62), 60)
                .unwrap()
                .is_none()
        );
    }
}
