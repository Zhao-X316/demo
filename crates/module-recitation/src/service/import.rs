//! 导入流水线（见 背诵批改系统 §6.2）。
//!
//! 幂等：hash 去重 → 解析文件名 → 匹配 学生/内容/任务 →
//! 成功建提交并触发后续 ASR/评分；失败进异常池（仍建提交+记账本，便于改派/防重复导入）。
//!
//! 为便于测试，`import_one` 接收已算好的 `file_hash`（外壳用 `core::domain::hashing` 计算）。

use rusqlite::Connection;
use suite_core::db::repo::{file_ledger, submissions, tasks};
use suite_core::error::CoreResult;
use suite_core::models::{MediaType, ModuleKey, TaskStatus};

use crate::db::contents;
use crate::domain::filename;

const MODULE: ModuleKey = ModuleKey::Recitation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportOutcome {
    /// 成功导入。`warning` 用于姓名不一致等非阻塞提示。
    Imported { submission_id: i64, task_id: i64, warning: Option<String> },
    /// 同 hash 已导入过。
    Duplicate { existing_submission_id: Option<i64> },
    /// 进入异常池。
    Anomaly { submission_id: i64, anomaly_type: String },
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
    Ok(ImportOutcome::Anomaly { submission_id: sub_id, anomaly_type: anomaly_type.to_string() })
}

pub fn import_one(conn: &Connection, item: &ImportItem<'_>) -> CoreResult<ImportOutcome> {
    // 1. 去重
    if let Some(hit) = file_ledger::get(conn, item.file_hash)? {
        file_ledger::bump(conn, item.file_hash)?;
        return Ok(ImportOutcome::Duplicate { existing_submission_id: hit.submission_id });
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
            return insert_anomaly(conn, item, Some(student.id), None, Some(&meta), "content_not_found")
        }
    };

    // 5. 任务匹配
    let task = match tasks::find_open_match(conn, MODULE, student.id, content.id, &parsed.date)? {
        Some(t) => t,
        None => {
            return insert_anomaly(
                conn, item, Some(student.id), Some(content.id), Some(&meta), "task_not_found",
            )
        }
    };

    // 6. 姓名校验（非阻塞）
    let warning = if student.name != parsed.name {
        Some(format!("姓名不一致：文件 {} / 档案 {}", parsed.name, student.name))
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

    Ok(ImportOutcome::Imported { submission_id: sub_id, task_id: task.id, warning })
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
            &StudentInput { student_no: "2023001", name: "张三", class_id: None, enabled: true },
        )
        .unwrap();
        let c = contents::upsert(
            conn,
            &contents::ContentInput { content_no: "C012", title: "静夜思", answer_text: "床前明月光", subject_id: None, enabled: true },
        )
        .unwrap();
        let tid = tasks::insert(
            conn,
            &NewTask {
                module: MODULE, student_id: s.id, subject_id: None, ref_type: "content",
                ref_id: c.id, kind: TaskKind::Normal, due_date: "2026-06-25",
                source_task_id: None, card_id: None,
            },
        )
        .unwrap();
        (s.id, tid)
    }

    #[test]
    fn happy_path_imports_and_marks_task_submitted() {
        let conn = setup();
        let (_sid, tid) = seed_full(&conn);
        let out = import_one(
            &conn,
            &ImportItem { file_path: "/x/20260625_2023001_张三_C012_1.m4a", file_stem: "20260625_2023001_张三_C012_1", file_hash: "h1", duration_ms: Some(3000) },
        )
        .unwrap();
        match out {
            ImportOutcome::Imported { task_id, warning, .. } => {
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
        let item = ImportItem { file_path: "/x/a.m4a", file_stem: "20260625_2023001_张三_C012_1", file_hash: "h1", duration_ms: None };
        import_one(&conn, &item).unwrap();
        let out = import_one(&conn, &item).unwrap();
        assert!(matches!(out, ImportOutcome::Duplicate { .. }));
    }

    #[test]
    fn parse_error_goes_to_anomaly() {
        let conn = setup();
        let out = import_one(
            &conn,
            &ImportItem { file_path: "/x/bad.m4a", file_stem: "bad-name", file_hash: "h2", duration_ms: None },
        )
        .unwrap();
        assert!(matches!(out, ImportOutcome::Anomaly { ref anomaly_type, .. } if anomaly_type == "parse_error"));
    }

    #[test]
    fn unknown_student_and_task_not_found() {
        let conn = setup();
        // 没有任何 seed → 学生不存在
        let out = import_one(
            &conn,
            &ImportItem { file_path: "/x/a.m4a", file_stem: "20260625_2023001_张三_C012", file_hash: "h3", duration_ms: None },
        )
        .unwrap();
        assert!(matches!(out, ImportOutcome::Anomaly { ref anomaly_type, .. } if anomaly_type == "student_not_found"));

        // 有学生有内容但无任务 → task_not_found
        upsert_student(&conn, &StudentInput { student_no: "2023002", name: "李四", class_id: None, enabled: true }).unwrap();
        contents::upsert(&conn, &contents::ContentInput { content_no: "C099", title: "x", answer_text: "y", subject_id: None, enabled: true }).unwrap();
        let out = import_one(
            &conn,
            &ImportItem { file_path: "/x/b.m4a", file_stem: "20260625_2023002_李四_C099", file_hash: "h4", duration_ms: None },
        )
        .unwrap();
        assert!(matches!(out, ImportOutcome::Anomaly { ref anomaly_type, .. } if anomaly_type == "task_not_found"));
    }

    #[test]
    fn name_mismatch_warns_but_imports() {
        let conn = setup();
        seed_full(&conn); // 学号2023001 档案姓名"张三"
        let out = import_one(
            &conn,
            &ImportItem { file_path: "/x/a.m4a", file_stem: "20260625_2023001_张三丰_C012", file_hash: "h5", duration_ms: None },
        )
        .unwrap();
        match out {
            ImportOutcome::Imported { warning, .. } => assert!(warning.is_some()),
            other => panic!("expected Imported with warning, got {other:?}"),
        }
    }
}
