//! 导入流水线（见 背诵批改系统 §6.2）。
//!
//! 幂等：hash 去重 → 解析文件名 → 匹配 学生/内容/任务 →
//! 成功建提交并触发后续 ASR/评分；失败进异常池（仍建提交+记账本，便于改派/防重复导入）。
//!
//! 为便于测试，`import_one` 接收已算好的 `file_hash`（外壳用 `core::domain::hashing` 计算）。

use rusqlite::Connection;
use suite_core::db::repo::{file_ledger, submissions, tasks};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{MediaType, ModuleKey, TaskStatus};
use suite_core::ports::RecognizedWord;

use crate::db::contents;
use crate::domain::filename;
use crate::service::scoring::{self, ScoreCfg, ScoreOutcome};

const MODULE: ModuleKey = ModuleKey::Recitation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportOutcome {
    /// 成功导入。`warning` 用于姓名不一致等非阻塞提示。
    Imported {
        submission_id: i64,
        task_id: i64,
        warning: Option<String>,
    },
    /// 同 hash 已导入过。
    Duplicate { existing_submission_id: Option<i64> },
    /// 进入异常池。
    Anomaly {
        submission_id: i64,
        anomaly_type: String,
    },
}

pub struct ImportItem<'a> {
    pub file_path: &'a str,
    pub file_stem: &'a str,
    pub file_hash: &'a str,
    pub duration_ms: Option<i64>,
}

fn insert_anomaly(
    conn: &Connection,
    item: &ImportItem<'_>,
    student_id: Option<i64>,
    ref_id: Option<i64>,
    parsed_meta: Option<&str>,
    anomaly_type: &str,
) -> CoreResult<ImportOutcome> {
    let sub_id = submissions::insert(
        conn,
        &submissions::NewSubmission {
            module: MODULE,
            task_id: None,
            student_id,
            ref_id,
            media_type: MediaType::Audio,
            file_path: item.file_path,
            file_hash: item.file_hash,
            duration_ms: item.duration_ms,
            parsed_meta,
            anomaly_type: Some(anomaly_type),
            status: "anomaly",
        },
    )?;
    file_ledger::record(conn, item.file_hash, item.file_path, sub_id)?;
    Ok(ImportOutcome::Anomaly {
        submission_id: sub_id,
        anomaly_type: anomaly_type.to_string(),
    })
}

pub fn import_one(conn: &Connection, item: &ImportItem<'_>) -> CoreResult<ImportOutcome> {
    let tx = conn.unchecked_transaction()?;
    let outcome = import_one_inner(&tx, item)?;
    tx.commit()?;
    Ok(outcome)
}

fn import_one_inner(conn: &Connection, item: &ImportItem<'_>) -> CoreResult<ImportOutcome> {
    // 1. 去重
    if let Some(hit) = file_ledger::get(conn, item.file_hash)? {
        file_ledger::bump(conn, item.file_hash)?;
        return Ok(ImportOutcome::Duplicate {
            existing_submission_id: hit.submission_id,
        });
    }

    // 2. 解析文件名
    let parsed = match filename::parse(item.file_stem) {
        Ok(p) => p,
        Err(_) => return insert_anomaly(conn, item, None, None, None, "parse_error"),
    };
    let meta = serde_json::json!({
        "date": parsed.date, "student_no": parsed.student_no,
        "name": parsed.name, "content_no": parsed.content_no, "seq": parsed.seq,
    })
    .to_string();

    // 3. 学生（不存在/停用 → 异常）
    let student = match suite_core::db::repo::students::get_by_no(conn, &parsed.student_no)? {
        Some(s) if s.enabled => s,
        _ => return insert_anomaly(conn, item, None, None, Some(&meta), "student_not_found"),
    };

    // 4. 内容（不存在/停用 → 异常）
    let content = match contents::get_by_no(conn, &parsed.content_no)? {
        Some(c) if c.enabled => c,
        _ => {
            return insert_anomaly(
                conn,
                item,
                Some(student.id),
                None,
                Some(&meta),
                "content_not_found",
            )
        }
    };

    // 5. 任务匹配
    let task = match tasks::find_open_match(conn, MODULE, student.id, content.id, &parsed.date)? {
        Some(t) => t,
        None => {
            return insert_anomaly(
                conn,
                item,
                Some(student.id),
                Some(content.id),
                Some(&meta),
                "task_not_found",
            )
        }
    };

    // 6. 姓名校验（非阻塞）
    let warning = if student.name != parsed.name {
        Some(format!(
            "姓名不一致：文件 {} / 档案 {}",
            parsed.name, student.name
        ))
    } else {
        None
    };

    // 7. 建提交 + 记账本 + 任务转 submitted
    let sub_id = submissions::insert(
        conn,
        &submissions::NewSubmission {
            module: MODULE,
            task_id: Some(task.id),
            student_id: Some(student.id),
            ref_id: Some(content.id),
            media_type: MediaType::Audio,
            file_path: item.file_path,
            file_hash: item.file_hash,
            duration_ms: item.duration_ms,
            parsed_meta: Some(&meta),
            anomaly_type: None,
            status: "pending",
        },
    )?;
    file_ledger::record(conn, item.file_hash, item.file_path, sub_id)?;
    tasks::set_status(conn, task.id, TaskStatus::Submitted)?;

    Ok(ImportOutcome::Imported {
        submission_id: sub_id,
        task_id: task.id,
        warning,
    })
}

/// 已解析身份的导入（用于"内容识别→自动命名"）：直接按 学生/内容 建提交、挂开放任务、记账本。
/// 返回 (submission_id, task_id)。调用方随后写识别结果并评分。
pub fn import_resolved(
    conn: &Connection,
    file_path: &str,
    file_hash: &str,
    student_id: i64,
    content_id: i64,
    duration_ms: Option<i64>,
) -> CoreResult<(i64, Option<i64>)> {
    let tx = conn.unchecked_transaction()?;
    let outcome = import_resolved_inner(
        &tx,
        file_path,
        file_hash,
        student_id,
        content_id,
        duration_ms,
    )?;
    tx.commit()?;
    Ok(outcome)
}

fn import_resolved_inner(
    conn: &Connection,
    file_path: &str,
    file_hash: &str,
    student_id: i64,
    content_id: i64,
    duration_ms: Option<i64>,
) -> CoreResult<(i64, Option<i64>)> {
    let task = tasks::find_latest_open(conn, MODULE, student_id, content_id)?;
    let task_id = task.as_ref().map(|t| t.id);
    let sub_id = submissions::insert(
        conn,
        &submissions::NewSubmission {
            module: MODULE,
            task_id,
            student_id: Some(student_id),
            ref_id: Some(content_id),
            media_type: MediaType::Audio,
            file_path,
            file_hash,
            duration_ms,
            parsed_meta: None,
            anomaly_type: None,
            status: "pending",
        },
    )?;
    file_ledger::record(conn, file_hash, file_path, sub_id)?;
    if let Some(tid) = task_id {
        tasks::set_status(conn, tid, TaskStatus::Submitted)?;
    }
    Ok((sub_id, task_id))
}

/// 改派异常提交：给定正确的 学生/内容 id（任一可沿用原值），重设关联并挂上开放任务。
/// 异常状态保留到后续识别和评分成功，失败时仍能在待处理区继续操作。
/// 返回关联到的 task_id（无开放任务则 None，仍可单独识别评分）。
pub fn reassign(
    conn: &Connection,
    submission_id: i64,
    student_id: Option<i64>,
    ref_id: Option<i64>,
) -> CoreResult<Option<i64>> {
    let tx = conn.unchecked_transaction()?;
    let task_id = reassign_inner(&tx, submission_id, student_id, ref_id)?;
    tx.commit()?;
    Ok(task_id)
}

fn reassign_inner(
    conn: &Connection,
    submission_id: i64,
    student_id: Option<i64>,
    ref_id: Option<i64>,
) -> CoreResult<Option<i64>> {
    let sub = submissions::get(conn, submission_id)?
        .ok_or_else(|| CoreError::NotFound(format!("submission {submission_id}")))?;
    let sid = student_id
        .or(sub.student_id)
        .ok_or_else(|| CoreError::Invalid("缺少学生，无法改派".into()))?;
    let rid = ref_id
        .or(sub.ref_id)
        .ok_or_else(|| CoreError::Invalid("缺少内容，无法改派".into()))?;
    let task_id = tasks::find_latest_open(conn, MODULE, sid, rid)?.map(|t| t.id);
    submissions::reassign(conn, submission_id, sid, rid, task_id)?;
    if let Some(tid) = task_id {
        tasks::set_status(conn, tid, TaskStatus::Submitted)?;
    }
    Ok(task_id)
}

/// 智能导入在网络调用前的短事务：新记录同时写 submission + ledger，并抢占 processing。
pub fn prepare_tracking_submission(
    conn: &Connection,
    existing_submission_id: Option<i64>,
    file_path: &str,
    file_hash: &str,
    archived_path: Option<&str>,
) -> CoreResult<i64> {
    let tx = conn.unchecked_transaction()?;
    let submission_id = match existing_submission_id {
        Some(id) => id,
        None => {
            let id = submissions::insert(
                &tx,
                &submissions::NewSubmission {
                    module: MODULE,
                    task_id: None,
                    student_id: None,
                    ref_id: None,
                    media_type: MediaType::Audio,
                    file_path,
                    file_hash,
                    duration_ms: None,
                    parsed_meta: None,
                    anomaly_type: None,
                    status: "pending",
                },
            )?;
            file_ledger::record(&tx, file_hash, file_path, id)?;
            id
        }
    };
    if let Some(path) = archived_path {
        submissions::set_archived_path(&tx, submission_id, path)?;
    }
    submissions::claim_recognition(&tx, submission_id)?;
    tx.commit()?;
    Ok(submission_id)
}

/// 智能导入匹配成功后的收尾事务：识别、关联任务、机器判定与异常清理要么全成，要么全退。
pub struct ResolvedRecognition<'a> {
    pub submission_id: i64,
    pub student_id: i64,
    pub content_id: i64,
    pub recognized_text: &'a str,
    pub duration_ms: Option<i64>,
    pub words: &'a [RecognizedWord],
    pub cfg: &'a ScoreCfg,
}

pub fn finish_resolved_recognition(
    conn: &Connection,
    input: &ResolvedRecognition<'_>,
) -> CoreResult<ScoreOutcome> {
    let tx = conn.unchecked_transaction()?;
    let existing = submissions::get(&tx, input.submission_id)?
        .ok_or_else(|| CoreError::NotFound(format!("submission {}", input.submission_id)))?;
    submissions::set_recognition(
        &tx,
        input.submission_id,
        Some(input.recognized_text),
        "ok",
        None,
        input.duration_ms,
    )?;
    if existing.status == "anomaly" || existing.task_id.is_none() {
        reassign_inner(
            &tx,
            input.submission_id,
            Some(input.student_id),
            Some(input.content_id),
        )?;
    } else {
        submissions::set_resolution_preserve_task(
            &tx,
            input.submission_id,
            input.student_id,
            input.content_id,
        )?;
    }
    let outcome =
        scoring::score_submission_inner(&tx, input.submission_id, input.words, input.cfg)?;
    submissions::clear_anomaly(&tx, input.submission_id)?;
    tx.commit()?;
    Ok(outcome)
}

/// 未匹配收尾：识别结果与异常状态同一事务提交。
pub fn finish_unmatched_recognition(
    conn: &Connection,
    submission_id: i64,
    recognized_text: &str,
    duration_ms: Option<i64>,
    mark_anomaly: bool,
) -> CoreResult<()> {
    let tx = conn.unchecked_transaction()?;
    submissions::set_recognition(
        &tx,
        submission_id,
        Some(recognized_text),
        "ok",
        None,
        duration_ms,
    )?;
    if mark_anomaly {
        let meta = serde_json::json!({ "asr": recognized_text }).to_string();
        submissions::mark_anomaly(&tx, submission_id, "autoname_unmatched", Some(&meta))?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::repo::students::{upsert as upsert_student, StudentInput};
    use suite_core::db::repo::tasks::NewTask;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::TaskKind;

    fn setup() -> Connection {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, crate::recitation_migrations()).unwrap();
        conn
    }

    fn seed_full(conn: &Connection) -> (i64, i64) {
        let s = upsert_student(
            conn,
            &StudentInput {
                student_no: "2023001",
                name: "张三",
                class_id: None,
                enabled: true,
            },
        )
        .unwrap();
        let c = contents::upsert(
            conn,
            &contents::ContentInput {
                content_no: "C012",
                title: "静夜思",
                answer_text: "床前明月光",
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap();
        let tid = tasks::insert(
            conn,
            &NewTask {
                module: MODULE,
                student_id: s.id,
                subject_id: None,
                ref_type: "content",
                ref_id: c.id,
                kind: TaskKind::Normal,
                due_date: "2026-06-25",
                source_task_id: None,
                card_id: None,
            },
        )
        .unwrap();
        (s.id, tid)
    }

    #[test]
    fn reassign_attaches_task_but_preserves_anomaly_until_success() {
        let conn = setup();
        let (sid, _tid) = seed_full(&conn);
        let cid = contents::get_by_no(&conn, "C012").unwrap().unwrap().id;
        // 未知学生 → 进异常池
        let item = ImportItem {
            file_path: "/x/20260625_9999999_王五_C012_1.m4a",
            file_stem: "20260625_9999999_王五_C012_1",
            file_hash: "h-anom",
            duration_ms: None,
        };
        let subid = match import_one(&conn, &item).unwrap() {
            ImportOutcome::Anomaly { submission_id, .. } => submission_id,
            o => panic!("应进异常池: {o:?}"),
        };
        // 改派到张三 + C012
        let task = reassign(&conn, subid, Some(sid), Some(cid)).unwrap();
        assert!(task.is_some(), "应挂上开放任务");
        let sub = submissions::get(&conn, subid).unwrap().unwrap();
        assert_eq!(sub.status, "anomaly");
        assert_eq!(sub.student_id, Some(sid));
        assert_eq!(sub.ref_id, Some(cid));
        assert_eq!(sub.anomaly_type.as_deref(), Some("student_not_found"));
    }

    #[test]
    fn happy_path_imports_and_marks_task_submitted() {
        let conn = setup();
        let (_sid, tid) = seed_full(&conn);
        let out = import_one(
            &conn,
            &ImportItem {
                file_path: "/x/20260625_2023001_张三_C012_1.m4a",
                file_stem: "20260625_2023001_张三_C012_1",
                file_hash: "h1",
                duration_ms: Some(3000),
            },
        )
        .unwrap();
        match out {
            ImportOutcome::Imported {
                task_id, warning, ..
            } => {
                assert_eq!(task_id, tid);
                assert!(warning.is_none());
            }
            other => panic!("expected Imported, got {other:?}"),
        }
        let t = tasks::get(&conn, tid).unwrap().unwrap();
        assert_eq!(t.status, TaskStatus::Submitted);
    }

    #[test]
    fn duplicate_hash_is_idempotent() {
        let conn = setup();
        seed_full(&conn);
        let item = ImportItem {
            file_path: "/x/a.m4a",
            file_stem: "20260625_2023001_张三_C012_1",
            file_hash: "h1",
            duration_ms: None,
        };
        import_one(&conn, &item).unwrap();
        let out = import_one(&conn, &item).unwrap();
        assert!(matches!(out, ImportOutcome::Duplicate { .. }));
    }

    #[test]
    fn parse_error_goes_to_anomaly() {
        let conn = setup();
        let out = import_one(
            &conn,
            &ImportItem {
                file_path: "/x/bad.m4a",
                file_stem: "bad-name",
                file_hash: "h2",
                duration_ms: None,
            },
        )
        .unwrap();
        assert!(
            matches!(out, ImportOutcome::Anomaly { ref anomaly_type, .. } if anomaly_type == "parse_error")
        );
    }

    #[test]
    fn unknown_student_and_task_not_found() {
        let conn = setup();
        // 没有任何 seed → 学生不存在
        let out = import_one(
            &conn,
            &ImportItem {
                file_path: "/x/a.m4a",
                file_stem: "20260625_2023001_张三_C012",
                file_hash: "h3",
                duration_ms: None,
            },
        )
        .unwrap();
        assert!(
            matches!(out, ImportOutcome::Anomaly { ref anomaly_type, .. } if anomaly_type == "student_not_found")
        );

        // 有学生有内容但无任务 → task_not_found
        upsert_student(
            &conn,
            &StudentInput {
                student_no: "2023002",
                name: "李四",
                class_id: None,
                enabled: true,
            },
        )
        .unwrap();
        contents::upsert(
            &conn,
            &contents::ContentInput {
                content_no: "C099",
                title: "x",
                answer_text: "y",
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap();
        let out = import_one(
            &conn,
            &ImportItem {
                file_path: "/x/b.m4a",
                file_stem: "20260625_2023002_李四_C099",
                file_hash: "h4",
                duration_ms: None,
            },
        )
        .unwrap();
        assert!(
            matches!(out, ImportOutcome::Anomaly { ref anomaly_type, .. } if anomaly_type == "task_not_found")
        );
    }

    #[test]
    fn name_mismatch_warns_but_imports() {
        let conn = setup();
        seed_full(&conn); // 学号2023001 档案姓名"张三"
        let out = import_one(
            &conn,
            &ImportItem {
                file_path: "/x/a.m4a",
                file_stem: "20260625_2023001_张三丰_C012",
                file_hash: "h5",
                duration_ms: None,
            },
        )
        .unwrap();
        match out {
            ImportOutcome::Imported { warning, .. } => assert!(warning.is_some()),
            other => panic!("expected Imported with warning, got {other:?}"),
        }
    }

    #[test]
    fn import_rolls_back_submission_when_ledger_write_fails() {
        let conn = setup();
        let (_sid, task_id) = seed_full(&conn);
        conn.execute_batch(
            "CREATE TRIGGER fail_import_ledger
             BEFORE INSERT ON file_ledger
             BEGIN SELECT RAISE(ABORT, 'injected ledger failure'); END;",
        )
        .unwrap();

        let result = import_one(
            &conn,
            &ImportItem {
                file_path: "/x/20260625_2023001_张三_C012_1.m4a",
                file_stem: "20260625_2023001_张三_C012_1",
                file_hash: "rollback-hash",
                duration_ms: None,
            },
        );
        assert!(result.is_err());
        let submissions_count: i64 = conn
            .query_row("SELECT count(*) FROM submissions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(submissions_count, 0);
        assert_eq!(
            tasks::get(&conn, task_id).unwrap().unwrap().status,
            TaskStatus::Open
        );
    }

    #[test]
    fn resolved_import_rolls_back_submission_when_ledger_write_fails() {
        let conn = setup();
        let (student_id, task_id) = seed_full(&conn);
        let content_id = contents::get_by_no(&conn, "C012").unwrap().unwrap().id;
        conn.execute_batch(
            "CREATE TRIGGER fail_resolved_ledger
             BEFORE INSERT ON file_ledger
             BEGIN SELECT RAISE(ABORT, 'injected ledger failure'); END;",
        )
        .unwrap();

        assert!(import_resolved(
            &conn,
            "/x/resolved.m4a",
            "resolved-rollback",
            student_id,
            content_id,
            None,
        )
        .is_err());
        let submissions_count: i64 = conn
            .query_row("SELECT count(*) FROM submissions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(submissions_count, 0);
        assert_eq!(
            tasks::get(&conn, task_id).unwrap().unwrap().status,
            TaskStatus::Open
        );
    }

    #[test]
    fn reassign_rolls_back_submission_when_task_update_fails() {
        let conn = setup();
        let (student_id, _task_id) = seed_full(&conn);
        let content_id = contents::get_by_no(&conn, "C012").unwrap().unwrap().id;
        let anomaly_id = match import_one(
            &conn,
            &ImportItem {
                file_path: "/x/bad.m4a",
                file_stem: "bad-name",
                file_hash: "reassign-rollback",
                duration_ms: None,
            },
        )
        .unwrap()
        {
            ImportOutcome::Anomaly { submission_id, .. } => submission_id,
            other => panic!("expected anomaly, got {other:?}"),
        };
        conn.execute_batch(
            "CREATE TRIGGER fail_reassign_task
             BEFORE UPDATE OF status ON tasks
             WHEN NEW.status='submitted'
             BEGIN SELECT RAISE(ABORT, 'injected task failure'); END;",
        )
        .unwrap();

        assert!(reassign(&conn, anomaly_id, Some(student_id), Some(content_id)).is_err());
        let submission = submissions::get(&conn, anomaly_id).unwrap().unwrap();
        assert_eq!(submission.student_id, None);
        assert_eq!(submission.ref_id, None);
        assert_eq!(submission.task_id, None);
        assert_eq!(submission.status, "anomaly");
    }

    #[test]
    fn resolved_recognition_rolls_back_resolution_when_scoring_fails() {
        let conn = setup();
        let (student_id, task_id) = seed_full(&conn);
        let content_id = contents::get_by_no(&conn, "C012").unwrap().unwrap().id;
        let submission_id = prepare_tracking_submission(
            &conn,
            None,
            "/x/recording.m4a",
            "finish-rollback",
            Some("/archive/finish-rollback.m4a"),
        )
        .unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_resolved_verdict
             BEFORE INSERT ON verdicts
             BEGIN SELECT RAISE(ABORT, 'injected verdict failure'); END;",
        )
        .unwrap();

        assert!(finish_resolved_recognition(
            &conn,
            &ResolvedRecognition {
                submission_id,
                student_id,
                content_id,
                recognized_text: "床前明月光",
                duration_ms: Some(5000),
                words: &[],
                cfg: &ScoreCfg::default(),
            },
        )
        .is_err());
        let submission = submissions::get(&conn, submission_id).unwrap().unwrap();
        assert_eq!(submission.recognize_status, "processing");
        assert_eq!(submission.recognized_text, None);
        assert_eq!(submission.student_id, None);
        assert_eq!(submission.ref_id, None);
        assert_eq!(submission.task_id, None);
        assert_eq!(
            submission.archived_path.as_deref(),
            Some("/archive/finish-rollback.m4a")
        );
        assert_eq!(
            tasks::get(&conn, task_id).unwrap().unwrap().status,
            TaskStatus::Open
        );
    }
}
