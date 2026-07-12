//! 评分编排（见 背诵批改系统 §8.4）。
//!
//! 串起：取内容答案 → 评分(正确率+熟练度) → 写判定 →
//! 通过则推进梯度复习；未通过则脱档 + 生成次日补背。
//! 词级时间戳由调用方从 ASR 结果解析后传入。

use chrono::{Duration, NaiveDate};
use rusqlite::Connection;
use suite_core::db::repo::{submissions, tasks, verdicts};
use suite_core::domain::accuracy::AccuracyCfg;
use suite_core::domain::normalize::NormalizeCfg;
use suite_core::domain::scheduler::{LadderScheduler, ReviewQuality};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{ModuleKey, TaskStatus};
use suite_core::ports::{GradeResult, Grader, RecognizedWord};
use suite_core::services::review::{self, ReviewRef};

use crate::db::contents;
use crate::domain::fluency::FluencyCfg;
use crate::grader::{RecitationGradeInput, RecitationGrader};
use crate::service::tasks as task_service;

const MODULE: ModuleKey = ModuleKey::Recitation;
const REF_TYPE: &str = "content";

pub struct ScoreCfg {
    pub normalize: NormalizeCfg,
    pub accuracy: AccuracyCfg,
    pub fluency: FluencyCfg,
    pub makeup_offset_days: i64,
}

impl Default for ScoreCfg {
    fn default() -> Self {
        Self {
            normalize: NormalizeCfg::default(),
            accuracy: AccuracyCfg::default(),
            fluency: FluencyCfg::default(),
            makeup_offset_days: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextAction {
    /// 通过 → 排入下次复习。
    Scheduled { due_date: String, stage: i32 },
    /// 未通过 → 生成补背（task_id 为 None 表示去重命中未新建）。
    Makeup {
        task_id: Option<i64>,
        due_date: String,
    },
}

#[derive(Debug, Clone)]
pub struct ScoreOutcome {
    pub verdict_id: i64,
    pub accuracy: f64,
    pub pass: bool,
    pub fluency: f64,
    pub quality: String,
    pub next: NextAction,
}

fn quality_to_review(q: &str) -> ReviewQuality {
    match q {
        "A" => ReviewQuality::Good,
        "B" => ReviewQuality::Ok,
        _ => ReviewQuality::Hard,
    }
}

pub fn score_submission(
    conn: &Connection,
    submission_id: i64,
    words: &[RecognizedWord],
    today: NaiveDate,
    cfg: &ScoreCfg,
) -> CoreResult<ScoreOutcome> {
    let sub = submissions::get(conn, submission_id)?
        .ok_or_else(|| CoreError::NotFound(format!("submission {submission_id}")))?;
    let student_id = sub
        .student_id
        .ok_or_else(|| CoreError::Invalid("提交缺少学生".into()))?;
    let content_id = sub
        .ref_id
        .ok_or_else(|| CoreError::Invalid("提交缺少内容".into()))?;
    let asr_text = sub.recognized_text.clone().unwrap_or_default();
    let duration = sub.duration_ms.unwrap_or(0).max(0) as u64;

    let content = contents::get_by_id(conn, content_id)?
        .ok_or_else(|| CoreError::NotFound(format!("content {content_id}")))?;

    let g = grade_and_record(
        conn,
        submission_id,
        &content,
        &asr_text,
        words,
        duration,
        cfg,
    )?;

    let review_ref = ReviewRef {
        module: MODULE,
        student_id,
        ref_type: REF_TYPE,
        ref_id: content_id,
    };
    let next = apply_outcome(
        conn,
        &review_ref,
        sub.task_id,
        g.grade.pass,
        &g.quality,
        today,
        cfg,
    )?;

    Ok(ScoreOutcome {
        verdict_id: g.verdict_id,
        accuracy: g.grade.primary_score,
        pass: g.grade.pass,
        fluency: g.grade.secondary_score.unwrap_or(0.0),
        quality: g.quality,
        next,
    })
}

struct Graded {
    grade: GradeResult,
    quality: String,
    verdict_id: i64,
}

/// 评分 + 写新判定 + 提交转 scored（不含复习/补背副作用）。
fn grade_and_record(
    conn: &Connection,
    submission_id: i64,
    content: &crate::db::contents::RecContent,
    asr_text: &str,
    words: &[RecognizedWord],
    duration_ms: u64,
    cfg: &ScoreCfg,
) -> CoreResult<Graded> {
    let grade = RecitationGrader.grade(RecitationGradeInput {
        answer_text: &content.answer_text,
        asr_text,
        words,
        duration_ms,
        normalize_cfg: &cfg.normalize,
        accuracy_cfg: &cfg.accuracy,
        fluency_cfg: &cfg.fluency,
    })?;
    let quality = grade.quality.clone().unwrap_or_else(|| "C".to_string());
    let verdict_id = verdicts::insert(
        conn,
        &verdicts::NewVerdict {
            submission_id,
            module: MODULE,
            primary_score: Some(grade.primary_score),
            pass: Some(grade.pass),
            secondary_score: grade.secondary_score,
            quality: Some(quality.as_str()),
            confidence: Some(grade.confidence),
            answer_version: content.answer_version,
            metrics_json: Some(grade.metrics_json.as_str()),
            machine_note: Some(grade.note.as_str()),
        },
    )?;
    submissions::set_status(conn, submission_id, "scored")?;
    Ok(Graded {
        grade,
        quality,
        verdict_id,
    })
}

/// 应用通过/未通过的副作用：通过→排复习+任务passed+作废残留补背；未通过→脱档+任务failed+生成补背。
fn apply_outcome(
    conn: &Connection,
    review_ref: &ReviewRef<'_>,
    task_id: Option<i64>,
    pass: bool,
    quality: &str,
    today: NaiveDate,
    cfg: &ScoreCfg,
) -> CoreResult<NextAction> {
    if pass {
        let sched = LadderScheduler::default();
        let out = review::record_pass(conn, review_ref, quality_to_review(quality), today, &sched)?;
        if let Some(tid) = task_id {
            tasks::set_status(conn, tid, TaskStatus::Passed)?;
        }
        // 通过 → 作废该 学生+内容 残留的未关闭补背（如人工把 fail 改判为 pass）
        tasks::close_open_kind(
            conn,
            MODULE,
            review_ref.student_id,
            review_ref.ref_id,
            suite_core::models::TaskKind::Makeup,
        )?;
        let due = (today + Duration::days(out.interval_days as i64))
            .format("%Y-%m-%d")
            .to_string();
        Ok(NextAction::Scheduled {
            due_date: due,
            stage: out.stage,
        })
    } else {
        review::record_lapse(conn, review_ref, today)?;
        if let Some(tid) = task_id {
            tasks::set_status(conn, tid, TaskStatus::Failed)?;
        }
        let due = (today + Duration::days(cfg.makeup_offset_days))
            .format("%Y-%m-%d")
            .to_string();
        let mk = match task_id {
            Some(tid) => task_service::ensure_makeup(
                conn,
                review_ref.student_id,
                review_ref.ref_id,
                tid,
                &due,
            )?,
            None => None,
        };
        Ok(NextAction::Makeup {
            task_id: mk,
            due_date: due,
        })
    }
}

/// 重判（答案版本变更后）：重新评分并写新判定，但**不**改动既有复习/补背排程
/// （如需改变排程由人工 `human_decide`）。
pub fn rescore(
    conn: &Connection,
    submission_id: i64,
    words: &[RecognizedWord],
    cfg: &ScoreCfg,
) -> CoreResult<ScoreOutcome> {
    let sub = submissions::get(conn, submission_id)?
        .ok_or_else(|| CoreError::NotFound(format!("submission {submission_id}")))?;
    let content_id = sub
        .ref_id
        .ok_or_else(|| CoreError::Invalid("提交缺少内容".into()))?;
    let asr_text = sub.recognized_text.clone().unwrap_or_default();
    let duration = sub.duration_ms.unwrap_or(0).max(0) as u64;
    let content = contents::get_by_id(conn, content_id)?
        .ok_or_else(|| CoreError::NotFound(format!("content {content_id}")))?;

    let g = grade_and_record(
        conn,
        submission_id,
        &content,
        &asr_text,
        words,
        duration,
        cfg,
    )?;
    Ok(ScoreOutcome {
        verdict_id: g.verdict_id,
        accuracy: g.grade.primary_score,
        pass: g.grade.pass,
        fluency: g.grade.secondary_score.unwrap_or(0.0),
        quality: g.quality,
        next: NextAction::Scheduled {
            due_date: String::new(),
            stage: -1,
        }, // 占位：rescore 不改排程
    })
}

/// 人工最终判定（pass|fail|reopen）。写入 human_result 并据此驱动复习/补背。
pub fn human_decide(
    conn: &Connection,
    submission_id: i64,
    result: &str,
    note: Option<&str>,
    decided_by: Option<&str>,
    today: NaiveDate,
    cfg: &ScoreCfg,
) -> CoreResult<NextAction> {
    let sub = submissions::get(conn, submission_id)?
        .ok_or_else(|| CoreError::NotFound(format!("submission {submission_id}")))?;
    let student_id = sub
        .student_id
        .ok_or_else(|| CoreError::Invalid("提交缺少学生".into()))?;
    let content_id = sub
        .ref_id
        .ok_or_else(|| CoreError::Invalid("提交缺少内容".into()))?;

    let verdict = verdicts::get_by_submission(conn, submission_id)?
        .ok_or_else(|| CoreError::NotFound("尚无判定，无法人工确认".into()))?;
    verdicts::set_human_result(conn, verdict.id, result, note, decided_by)?;
    submissions::set_status(conn, submission_id, "confirmed")?;

    let quality = verdict.quality.clone().unwrap_or_else(|| "C".to_string());
    let review_ref = ReviewRef {
        module: MODULE,
        student_id,
        ref_type: REF_TYPE,
        ref_id: content_id,
    };
    let current_task_status = match sub.task_id {
        Some(tid) => tasks::get(conn, tid)?.map(|t| t.status),
        None => None,
    };

    match result {
        "pass" if current_task_status == Some(TaskStatus::Passed) => Ok(NextAction::Scheduled {
            due_date: String::new(),
            stage: -1,
        }),
        "fail" if current_task_status == Some(TaskStatus::Failed) => Ok(NextAction::Makeup {
            task_id: None,
            due_date: String::new(),
        }),
        "pass" => apply_outcome(conn, &review_ref, sub.task_id, true, &quality, today, cfg),
        "fail" => apply_outcome(conn, &review_ref, sub.task_id, false, &quality, today, cfg),
        "reopen" => {
            if let Some(tid) = sub.task_id {
                tasks::set_status(conn, tid, TaskStatus::Reopened)?;
            }
            Ok(NextAction::Makeup {
                task_id: None,
                due_date: String::new(),
            })
        }
        other => Err(CoreError::Invalid(format!("未知人工结论: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::import::{import_one, ImportItem, ImportOutcome};
    use suite_core::db::repo::memory_cards;
    use suite_core::db::repo::students::{upsert as upsert_student, StudentInput};
    use suite_core::db::repo::tasks::NewTask;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::TaskKind;

    fn setup_imported(answer: &str) -> (Connection, i64, i64, i64) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, crate::recitation_migrations()).unwrap();
        let s = upsert_student(
            &conn,
            &StudentInput {
                student_no: "2023001",
                name: "张三",
                class_id: None,
                enabled: true,
            },
        )
        .unwrap();
        let c = contents::upsert(
            &conn,
            &contents::ContentInput {
                content_no: "C012",
                title: "静夜思",
                answer_text: answer,
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap();
        tasks::insert(
            &conn,
            &NewTask {
                module: MODULE,
                student_id: s.id,
                subject_id: None,
                ref_type: REF_TYPE,
                ref_id: c.id,
                kind: TaskKind::Normal,
                due_date: "2026-06-25",
                source_task_id: None,
                card_id: None,
            },
        )
        .unwrap();
        let out = import_one(
            &conn,
            &ImportItem {
                file_path: "/x.m4a",
                file_stem: "20260625_2023001_张三_C012",
                file_hash: "h1",
                duration_ms: Some(5000),
            },
        )
        .unwrap();
        let sub_id = match out {
            ImportOutcome::Imported { submission_id, .. } => submission_id,
            o => panic!("import failed: {o:?}"),
        };
        (conn, s.id, c.id, sub_id)
    }

    #[test]
    fn pass_schedules_review_and_marks_task_passed() {
        let answer = "床前明月光";
        let (conn, sid, cid, sub) = setup_imported(answer);
        // ASR 完美识别
        submissions::set_recognition(&conn, sub, Some(answer), "ok", None, Some(5000)).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let out = score_submission(&conn, sub, &[], today, &ScoreCfg::default()).unwrap();

        assert!(out.pass);
        assert_eq!(out.accuracy, 100.0);
        assert!(matches!(out.next, NextAction::Scheduled { .. }));
        // 卡片已建立、due 已写
        let card = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();
        assert!(card.due_date.is_some());
        // 任务标记通过
        let t = tasks::get(&conn, score_task_id(&conn, sub))
            .unwrap()
            .unwrap();
        assert_eq!(t.status, TaskStatus::Passed);
    }

    #[test]
    fn fail_generates_makeup_and_marks_task_failed() {
        // 答案两句，只背一句 → < 95%
        let (conn, sid, cid, sub) = setup_imported("床前明月光疑是地上霜");
        submissions::set_recognition(&conn, sub, Some("床前明月光"), "ok", None, Some(3000))
            .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let out = score_submission(&conn, sub, &[], today, &ScoreCfg::default()).unwrap();

        assert!(!out.pass);
        match out.next {
            NextAction::Makeup { task_id, due_date } => {
                assert!(task_id.is_some());
                assert_eq!(due_date, "2026-06-26"); // 次日
            }
            other => panic!("expected Makeup, got {other:?}"),
        }
        assert!(tasks::exists_open_kind(&conn, MODULE, sid, cid, TaskKind::Makeup).unwrap());
    }

    fn score_task_id(conn: &Connection, sub: i64) -> i64 {
        submissions::get(conn, sub)
            .unwrap()
            .unwrap()
            .task_id
            .unwrap()
    }
}
