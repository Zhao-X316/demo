//! ASR 状态机与失败后的人工操作。网络调用留在 Tauri 外壳，本服务只持有短事务。

use chrono::{FixedOffset, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::{ai_runs, submissions, tasks, verdicts};
use suite_core::domain::time;
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{ModuleKey, TaskStatus};

const MODULE: ModuleKey = ModuleKey::Recitation;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecognitionFailureMeta {
    pub error_code: String,
    pub error_message: String,
    pub retryable: bool,
    pub failed_at: String,
    pub attempts: u32,
}

fn now_shanghai() -> String {
    let offset = FixedOffset::east_opt(8 * 60 * 60).expect("valid UTC+8 offset");
    Utc::now().with_timezone(&offset).to_rfc3339()
}

pub fn parse_failure_meta(raw: Option<&str>) -> Option<RecognitionFailureMeta> {
    raw.and_then(|value| serde_json::from_str(value).ok())
}

pub fn classify_failure(message: &str) -> (&'static str, bool) {
    let lower = message.to_lowercase();
    if message.contains("未配置火山凭据")
        || message.contains("凭据")
        || lower.contains("http 401")
        || lower.contains("http 403")
    {
        ("credentials", true)
    } else if message.contains("识别过程中退出") {
        ("interrupted", true)
    } else if message.contains("读取音频失败")
        || message.contains("No such file")
        || message.contains("不存在")
    {
        ("file_missing", false)
    } else if message.contains("超时") || lower.contains("timeout") {
        ("timeout", true)
    } else if message.contains("限流") || lower.contains("429") || lower.contains("rate") {
        ("rate_limited", true)
    } else if message.contains("请求失败") || message.contains("HTTP 5") {
        ("network", true)
    } else if message.contains("格式") || message.contains("解码") || message.contains("返回为空")
    {
        ("invalid_audio", false)
    } else {
        ("asr_error", false)
    }
}

pub fn claim(conn: &Connection, submission_id: i64) -> CoreResult<()> {
    submissions::claim_recognition(conn, submission_id)
}

pub fn mark_failed(
    conn: &Connection,
    submission_id: i64,
    message: &str,
) -> CoreResult<RecognitionFailureMeta> {
    let submission = submissions::get(conn, submission_id)?
        .ok_or_else(|| CoreError::NotFound(format!("submission {submission_id}")))?;
    let attempts = parse_failure_meta(submission.recognize_meta.as_deref())
        .map(|meta| meta.attempts)
        .unwrap_or(0)
        .saturating_add(1);
    let (error_code, retryable) = classify_failure(message);
    let meta = RecognitionFailureMeta {
        error_code: error_code.into(),
        error_message: message.chars().take(500).collect(),
        retryable,
        failed_at: now_shanghai(),
        attempts,
    };
    let encoded = serde_json::to_string(&meta)
        .map_err(|err| CoreError::Invalid(format!("识别失败元数据序列化失败: {err}")))?;
    submissions::set_recognition_failure(conn, submission_id, &encoded)?;
    Ok(meta)
}

pub fn recover_stale_processing(conn: &Connection) -> CoreResult<usize> {
    let processing = submissions::list_processing(conn, MODULE)?;
    for submission in &processing {
        let transaction = conn.unchecked_transaction()?;
        let ai_run_id = transaction
            .query_row(
                "SELECT id FROM ai_runs
                 WHERE run_type='asr'
                   AND source_module='recitation'
                   AND business_ref_type='submission'
                   AND business_ref_id=?1
                   AND status='processing'
                 ORDER BY id DESC LIMIT 1",
                [submission.id.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if let Some(ai_run_id) = ai_run_id {
            let error_meta = serde_json::json!({
                "schema_version": 1,
                "error_code": "interrupted",
                "error_message": "应用上次在识别过程中退出，已恢复为可重试失败",
                "retryable": true
            })
            .to_string();
            ai_runs::finalize_failed(
                &transaction,
                ai_run_id,
                &error_meta,
                &time::utc_now_rfc3339(),
            )?;
        }
        mark_failed(
            &transaction,
            submission.id,
            "应用上次在识别过程中退出，已恢复为可重试失败",
        )?;
        transaction.commit()?;
    }
    Ok(processing.len())
}

pub fn relocate_same_hash(
    conn: &Connection,
    submission_id: i64,
    new_path: &std::path::Path,
) -> CoreResult<()> {
    let submission = submissions::get(conn, submission_id)?
        .ok_or_else(|| CoreError::NotFound(format!("submission {submission_id}")))?;
    if submission.status == "voided" || submission.status == "confirmed" {
        return Err(CoreError::Invalid("作废或已确认提交不能重新定位".into()));
    }
    let actual_hash = suite_core::domain::hashing::sha256_file(new_path)?;
    if actual_hash != submission.file_hash {
        return Err(CoreError::Invalid(
            "所选文件与原录音 hash 不一致；不同录音必须新建提交".into(),
        ));
    }
    let path = new_path
        .to_str()
        .ok_or_else(|| CoreError::Invalid("文件路径不是有效 UTF-8".into()))?;
    submissions::set_file_path(conn, submission_id, path)
}

/// 作废未确认提交。若它是任务唯一仍有效的提交，则任务回到 reopened 等待新录音。
pub fn void_unconfirmed(conn: &Connection, submission_id: i64) -> CoreResult<bool> {
    let tx = conn.unchecked_transaction()?;
    let submission = submissions::get(&tx, submission_id)?
        .ok_or_else(|| CoreError::NotFound(format!("submission {submission_id}")))?;
    let human_result =
        verdicts::get_by_submission(&tx, submission_id)?.and_then(|verdict| verdict.human_result);
    if submission.status == "confirmed" || matches!(human_result.as_deref(), Some("pass" | "fail"))
    {
        return Err(CoreError::Invalid("已终审提交不能作废，请使用改判".into()));
    }
    submissions::set_status(&tx, submission_id, "voided")?;

    let reopened = if let Some(task_id) = submission.task_id {
        if submissions::has_other_active_for_task(&tx, task_id, submission_id)? {
            false
        } else {
            tasks::set_status(&tx, task_id, TaskStatus::Reopened)?;
            true
        }
    } else {
        false
    };
    tx.commit()?;
    Ok(reopened)
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::repo::ai_runs::NewAiRun;
    use suite_core::db::repo::students::{upsert as upsert_student, StudentInput};
    use suite_core::db::repo::submissions::NewSubmission;
    use suite_core::db::repo::tasks::NewTask;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{MediaType, TaskKind};

    fn setup() -> (Connection, i64) {
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
        (conn, student.id)
    }

    fn insert_submission(conn: &Connection, task_id: Option<i64>, hash: &str) -> i64 {
        submissions::insert(
            conn,
            &NewSubmission {
                module: MODULE,
                task_id,
                student_id: None,
                ref_id: None,
                media_type: MediaType::Audio,
                file_path: "/missing.m4a",
                file_hash: hash,
                duration_ms: None,
                parsed_meta: None,
                anomaly_type: None,
                status: "pending",
            },
        )
        .unwrap()
    }

    #[test]
    fn failure_attempts_increment_and_processing_recovers() {
        let (conn, _sid) = setup();
        let submission_id = insert_submission(&conn, None, "h1");
        claim(&conn, submission_id).unwrap();
        let first = mark_failed(&conn, submission_id, "火山识别超时").unwrap();
        assert_eq!(first.error_code, "timeout");
        assert!(first.retryable);
        assert_eq!(first.attempts, 1);

        claim(&conn, submission_id).unwrap();
        let run = ai_runs::create_or_get(
            &conn,
            &NewAiRun {
                idempotency_key: "recognition-recovery-asr",
                run_type: "asr",
                source_module: "recitation",
                business_ref_type: "submission",
                business_ref_id: &submission_id.to_string(),
                input_artifact_id: None,
                provider: "fixture",
                model_name: "fixture",
                model_version: "1",
                config_version: "1",
                prompt_or_rule_version: "1",
                input_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                retry_of_ai_run_id: None,
            },
        )
        .unwrap();
        ai_runs::start(&conn, run.id, "2026-07-17T08:00:00.000Z", None).unwrap();
        assert_eq!(recover_stale_processing(&conn).unwrap(), 1);
        let recovered = submissions::get(&conn, submission_id).unwrap().unwrap();
        let meta = parse_failure_meta(recovered.recognize_meta.as_deref()).unwrap();
        assert_eq!(meta.error_code, "interrupted");
        assert!(meta.retryable);
        assert_eq!(meta.attempts, 2);
        assert_eq!(
            ai_runs::get_by_id(&conn, run.id).unwrap().unwrap().status,
            suite_core::models::AiRunStatus::Failed
        );
        let count: i64 = conn
            .query_row("SELECT count(*) FROM submissions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1, "重试必须复用原 submission");
    }

    #[test]
    fn relocate_requires_same_hash() {
        let (conn, _sid) = setup();
        let good_path = std::env::temp_dir().join("jiaofu-relocate-good.m4a");
        std::fs::write(&good_path, b"same audio").unwrap();
        let hash = suite_core::domain::hashing::sha256_file(&good_path).unwrap();
        let submission_id = insert_submission(&conn, None, &hash);
        relocate_same_hash(&conn, submission_id, &good_path).unwrap();
        assert_eq!(
            submissions::get(&conn, submission_id)
                .unwrap()
                .unwrap()
                .file_path,
            good_path.to_string_lossy()
        );

        let other_path = std::env::temp_dir().join("jiaofu-relocate-other.m4a");
        std::fs::write(&other_path, b"different audio").unwrap();
        let err = relocate_same_hash(&conn, submission_id, &other_path)
            .unwrap_err()
            .to_string();
        assert!(err.contains("hash 不一致"));
    }

    #[test]
    fn void_reopens_task_only_when_submission_is_unique() {
        let (conn, sid) = setup();
        let task_id = tasks::insert(
            &conn,
            &NewTask {
                module: MODULE,
                student_id: sid,
                subject_id: None,
                ref_type: "content",
                ref_id: 7,
                kind: TaskKind::Normal,
                due_date: "2026-06-25",
                source_task_id: None,
                card_id: None,
            },
        )
        .unwrap();
        tasks::set_status(&conn, task_id, TaskStatus::Submitted).unwrap();
        let only = insert_submission(&conn, Some(task_id), "h1");
        assert!(void_unconfirmed(&conn, only).unwrap());
        assert_eq!(
            tasks::get(&conn, task_id).unwrap().unwrap().status,
            TaskStatus::Reopened
        );

        tasks::set_status(&conn, task_id, TaskStatus::Submitted).unwrap();
        let first = insert_submission(&conn, Some(task_id), "h2");
        let _other = insert_submission(&conn, Some(task_id), "h3");
        assert!(!void_unconfirmed(&conn, first).unwrap());
        assert_eq!(
            tasks::get(&conn, task_id).unwrap().unwrap().status,
            TaskStatus::Submitted
        );

        let unbound = insert_submission(&conn, None, "h4");
        assert!(!void_unconfirmed(&conn, unbound).unwrap());
        assert_eq!(
            submissions::get(&conn, unbound).unwrap().unwrap().status,
            "voided"
        );

        let confirmed = insert_submission(&conn, None, "h5");
        submissions::set_status(&conn, confirmed, "confirmed").unwrap();
        let err = void_unconfirmed(&conn, confirmed).unwrap_err().to_string();
        assert!(err.contains("已终审提交不能作废"));
    }
}
