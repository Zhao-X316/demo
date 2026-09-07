//! 评分编排（见 背诵批改系统 §8.4）。
//!
//! 串起：取内容答案 → 评分(正确率+熟练度) → 写机器建议；
//! 老师人工终审后才推进复习或生成补背。
//! 词级时间戳由调用方从 ASR 结果解析后传入。

use chrono::{Duration, NaiveDate};
use rusqlite::Connection;
use std::collections::HashSet;
use suite_core::db::repo::{decision_effects, memory_cards, submissions, tasks, verdicts};
use suite_core::domain::accuracy::AccuracyCfg;
use suite_core::domain::normalize::NormalizeCfg;
use suite_core::domain::scheduler::{LadderScheduler, ReviewQuality};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{MemoryCard, ModuleKey, Task, TaskStatus};
use suite_core::ports::{GradeResult, Grader, RecognizedWord};
use suite_core::services::review::{self, ReviewRef};

use crate::db::contents;
use crate::db::point_reviews::{self, TeacherPointReviewInput};
use crate::db::retention;
use crate::domain::fluency::FluencyCfg;
use crate::grader::{RecitationGradeInput, RecitationGrader};
use crate::service::learning_evidence;
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
    /// 机器评分完成，等待老师终审；无排程副作用。
    AwaitingHumanReview,
    /// 重复提交相同人工结论；仅更新备注，不重复应用副作用。
    Unchanged,
    /// 未终审提交被老师重开，等待新录音。
    Reopened,
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

pub struct HumanDecisionRequest<'a> {
    pub result: &'a str,
    pub note: Option<&'a str>,
    pub decided_by: Option<&'a str>,
    pub point_review: Option<&'a TeacherPointReviewInput>,
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
    _today: NaiveDate,
    cfg: &ScoreCfg,
) -> CoreResult<ScoreOutcome> {
    let tx = conn.unchecked_transaction()?;
    let outcome = score_submission_inner(&tx, submission_id, words, cfg)?;
    tx.commit()?;
    Ok(outcome)
}

pub(crate) fn score_submission_inner(
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
        next: NextAction::AwaitingHumanReview,
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
    let tx = conn.unchecked_transaction()?;
    let outcome = score_submission_inner(&tx, submission_id, words, cfg)?;
    tx.commit()?;
    Ok(outcome)
}

/// ASR 成功收尾：识别文本、机器判定、提交状态与异常清理一次提交。
pub fn finish_recognition_and_score(
    conn: &Connection,
    submission_id: i64,
    recognized_text: &str,
    duration_ms: Option<i64>,
    words: &[RecognizedWord],
    cfg: &ScoreCfg,
) -> CoreResult<ScoreOutcome> {
    let tx = conn.unchecked_transaction()?;
    submissions::set_recognition(
        &tx,
        submission_id,
        Some(recognized_text),
        "ok",
        None,
        duration_ms,
    )?;
    let outcome = score_submission_inner(&tx, submission_id, words, cfg)?;
    submissions::clear_anomaly(&tx, submission_id)?;
    tx.commit()?;
    Ok(outcome)
}

fn json_encode<T: serde::Serialize>(value: &T, label: &str) -> CoreResult<String> {
    serde_json::to_string(value)
        .map_err(|err| CoreError::Invalid(format!("{label} 序列化失败: {err}")))
}

fn json_decode<T: serde::de::DeserializeOwned>(raw: &str, label: &str) -> CoreResult<T> {
    serde_json::from_str(raw).map_err(|err| CoreError::Invalid(format!("{label} 解析失败: {err}")))
}

fn validate_review_evidence(
    submission: &suite_core::models::Submission,
    content: &contents::RecContent,
    verdict: &suite_core::models::Verdict,
) -> CoreResult<()> {
    if submission
        .recognized_text
        .as_deref()
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        return Err(CoreError::Invalid(
            "ASR 原文为空，不能只凭机器分数终审".into(),
        ));
    }
    let evidence_path = submission
        .archived_path
        .as_deref()
        .filter(|path| std::path::Path::new(path).is_file())
        .unwrap_or(&submission.file_path);
    std::fs::File::open(evidence_path).map_err(|err| {
        CoreError::Invalid(format!(
            "录音文件不可读，不能终审；请重新定位或重开任务: {err}"
        ))
    })?;
    if content.answer_text.trim().is_empty() {
        return Err(CoreError::Invalid("标准答案为空，不能终审".into()));
    }
    if content.answer_version != verdict.answer_version {
        return Err(CoreError::Invalid(format!(
            "答案版本已变化（当前 v{}，评分 v{}），请先重新评分再终审",
            content.answer_version, verdict.answer_version
        )));
    }
    Ok(())
}

fn restore_effect(conn: &Connection, effect: &decision_effects::DecisionEffect) -> CoreResult<()> {
    let current_card = memory_cards::get(
        conn,
        effect.module,
        effect.student_id,
        &effect.ref_type,
        effect.ref_id,
    )?;
    let current_tasks = tasks::list_for_scope(
        conn,
        effect.module,
        effect.student_id,
        &effect.ref_type,
        effect.ref_id,
    )?;
    if json_encode(&current_card, "当前卡片状态")? != effect.card_after_json
        || json_encode(&current_tasks, "当前任务状态")? != effect.task_after_json
    {
        return Err(CoreError::Invalid(
            "终审后任务或复习卡已被其他操作修改，拒绝自动覆盖；请先人工核对".into(),
        ));
    }

    let card_before: Option<MemoryCard> = json_decode(&effect.card_before_json, "卡片前态")?;
    let tasks_before: Vec<Task> = json_decode(&effect.task_before_json, "任务前态")?;
    let created_makeup_ids: Vec<i64> =
        json_decode(&effect.created_makeup_task_ids_json, "补背任务账本")?;

    memory_cards::restore(
        conn,
        effect.module,
        effect.student_id,
        &effect.ref_type,
        effect.ref_id,
        card_before.as_ref(),
    )?;
    tasks::restore_statuses(conn, &tasks_before)?;
    for task_id in created_makeup_ids {
        tasks::set_status(conn, task_id, TaskStatus::Closed)?;
    }
    Ok(())
}

struct EffectApplication<'a, 'r> {
    verdict_id: i64,
    review_ref: &'a ReviewRef<'r>,
    task_id: Option<i64>,
    result: &'a str,
    quality: &'a str,
    today: NaiveDate,
    cfg: &'a ScoreCfg,
}

struct AppliedEffect {
    next: NextAction,
    effect_id: i64,
}

fn apply_and_record_effect(
    conn: &Connection,
    application: &EffectApplication<'_, '_>,
) -> CoreResult<AppliedEffect> {
    let EffectApplication {
        verdict_id,
        review_ref,
        task_id,
        result,
        quality,
        today,
        cfg,
    } = application;
    let card_before = memory_cards::get(
        conn,
        review_ref.module,
        review_ref.student_id,
        review_ref.ref_type,
        review_ref.ref_id,
    )?;
    let tasks_before = tasks::list_for_scope(
        conn,
        review_ref.module,
        review_ref.student_id,
        review_ref.ref_type,
        review_ref.ref_id,
    )?;
    let before_ids: HashSet<i64> = tasks_before.iter().map(|task| task.id).collect();

    let next = apply_outcome(
        conn,
        review_ref,
        *task_id,
        *result == "pass",
        quality,
        *today,
        cfg,
    )?;
    let created_makeup_ids = match &next {
        NextAction::Makeup {
            task_id: Some(id), ..
        } if !before_ids.contains(id) => vec![*id],
        _ => Vec::new(),
    };

    let card_after = memory_cards::get(
        conn,
        review_ref.module,
        review_ref.student_id,
        review_ref.ref_type,
        review_ref.ref_id,
    )?;
    let tasks_after = tasks::list_for_scope(
        conn,
        review_ref.module,
        review_ref.student_id,
        review_ref.ref_type,
        review_ref.ref_id,
    )?;

    let card_before_json = json_encode(&card_before, "卡片前态")?;
    let task_before_json = json_encode(&tasks_before, "任务前态")?;
    let card_after_json = json_encode(&card_after, "卡片后态")?;
    let task_after_json = json_encode(&tasks_after, "任务后态")?;
    let created_makeup_task_ids_json = json_encode(&created_makeup_ids, "补背任务账本")?;
    let effect_id = decision_effects::insert(
        conn,
        &decision_effects::NewDecisionEffect {
            verdict_id: *verdict_id,
            revision: decision_effects::next_revision(conn, *verdict_id)?,
            module: review_ref.module,
            student_id: review_ref.student_id,
            ref_type: review_ref.ref_type,
            ref_id: review_ref.ref_id,
            result,
            card_before_json: &card_before_json,
            task_before_json: &task_before_json,
            card_after_json: &card_after_json,
            task_after_json: &task_after_json,
            created_makeup_task_ids_json: &created_makeup_task_ids_json,
        },
    )?;
    Ok(AppliedEffect { next, effect_id })
}

/// 人工最终判定（pass|fail|reopen）。所有多表副作用与效果账本在一个事务内提交。
pub fn human_decide(
    conn: &Connection,
    submission_id: i64,
    result: &str,
    note: Option<&str>,
    decided_by: Option<&str>,
    today: NaiveDate,
    cfg: &ScoreCfg,
) -> CoreResult<NextAction> {
    human_decide_with_point_review(
        conn,
        submission_id,
        &HumanDecisionRequest {
            result,
            note,
            decided_by,
            point_review: None,
        },
        today,
        cfg,
    )
}

/// 人工最终判定，可选地显式接受或修正本次结构化评分的全部评分点。
///
/// `point_review=None` 只表示老师确认总体 pass/fail，不会把机器逐点结果伪装成人工确认。
pub fn human_decide_with_point_review(
    conn: &Connection,
    submission_id: i64,
    request: &HumanDecisionRequest<'_>,
    today: NaiveDate,
    cfg: &ScoreCfg,
) -> CoreResult<NextAction> {
    let result = request.result;
    let note = request.note;
    let decided_by = request.decided_by;
    let point_review = request.point_review;
    if !matches!(result, "pass" | "fail" | "reopen") {
        return Err(CoreError::Invalid(format!("未知人工结论: {result}")));
    }
    if result == "reopen" && point_review.is_some() {
        return Err(CoreError::Invalid(
            "重开待重交不能同时确认结构化评分点".into(),
        ));
    }
    let point_review_actor = if point_review.is_some() {
        Some(
            decided_by
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| CoreError::Invalid("逐点评审必须记录确认人".into()))?,
        )
    } else {
        None
    };
    let decision_actor = decided_by
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("teacher");

    let tx = conn.unchecked_transaction()?;
    let sub = submissions::get(&tx, submission_id)?
        .ok_or_else(|| CoreError::NotFound(format!("submission {submission_id}")))?;
    let student_id = sub
        .student_id
        .ok_or_else(|| CoreError::Invalid("提交缺少学生".into()))?;
    let content_id = sub
        .ref_id
        .ok_or_else(|| CoreError::Invalid("提交缺少内容".into()))?;

    let verdict = verdicts::get_by_submission(&tx, submission_id)?
        .ok_or_else(|| CoreError::NotFound("尚无判定，无法人工确认".into()))?;
    let content = contents::get_by_id(&tx, content_id)?
        .ok_or_else(|| CoreError::NotFound(format!("content {content_id}")))?;
    let decision_task = match sub.task_id {
        Some(task_id) => Some(
            tasks::get(&tx, task_id)?
                .ok_or_else(|| CoreError::NotFound(format!("task {task_id}")))?,
        ),
        None => None,
    };
    let existing_result = verdict.human_result.as_deref();
    let prior_submission_effect =
        decision_effects::latest_active_for_submission(&tx, submission_id)?;

    if result == "reopen" {
        if matches!(existing_result, Some("pass" | "fail")) || prior_submission_effect.is_some() {
            return Err(CoreError::Invalid(
                "已终审记录不能直接重开，请使用改判".into(),
            ));
        }
        if existing_result.is_none() {
            if let Some(task_id) = sub.task_id {
                let task = tasks::get(&tx, task_id)?
                    .ok_or_else(|| CoreError::NotFound(format!("task {task_id}")))?;
                if matches!(task.status, TaskStatus::Passed | TaskStatus::Failed) {
                    return Err(CoreError::Invalid(
                        "检测到旧版机器评分已产生副作用，请先执行旧数据修复后再重开".into(),
                    ));
                }
            }
        }
        if let Some(task_id) = sub.task_id {
            tasks::set_status(&tx, task_id, TaskStatus::Reopened)?;
        }
        verdicts::set_human_result(&tx, verdict.id, result, note, decided_by)?;
        submissions::set_status(&tx, submission_id, "confirmed")?;
        tx.commit()?;
        return Ok(if existing_result == Some("reopen") {
            NextAction::Unchanged
        } else {
            NextAction::Reopened
        });
    }

    let review_ref = ReviewRef {
        module: MODULE,
        student_id,
        ref_type: REF_TYPE,
        ref_id: content_id,
    };
    let active_effect = decision_effects::active_for_verdict(&tx, verdict.id)?;

    if existing_result == Some(result) {
        let active_effect = active_effect.ok_or_else(|| {
            CoreError::Invalid("终审结果缺少效果账本，拒绝猜测历史状态；请先执行旧数据修复".into())
        })?;
        verdicts::set_human_result(&tx, verdict.id, result, note, decided_by)?;
        let previous_review = point_reviews::active_review_for_submission(&tx, submission_id)?;
        let recorded_review =
            if let (Some(review), Some(actor)) = (point_review, point_review_actor) {
                let recorded = point_reviews::record_review_inner(
                    &tx,
                    submission_id,
                    verdict.id,
                    active_effect.id,
                    result,
                    actor,
                    review,
                )?;
                if let Some(previous) = previous_review.as_ref() {
                    if previous.id != recorded.id {
                        learning_evidence::supersede_for_review(&tx, previous)?;
                    }
                }
                Some(recorded)
            } else {
                None
            };
        learning_evidence::activate_for_decision(
            &tx,
            &learning_evidence::DecisionEvidenceInput {
                submission_id,
                student_id,
                verdict: &verdict,
                effect: &active_effect,
                review: recorded_review.as_ref(),
                timing: None,
                actor: decision_actor,
            },
        )?;
        submissions::set_status(&tx, submission_id, "confirmed")?;
        tx.commit()?;
        return Ok(NextAction::Unchanged);
    }

    if existing_result == Some("reopen") {
        return Err(CoreError::Invalid(
            "已重开的旧提交不能再次终审，请对新提交进行判定".into(),
        ));
    }

    if existing_result != Some(result) {
        validate_review_evidence(&sub, &content, &verdict)?;
    }

    let effect_to_replace = active_effect.or_else(|| {
        if existing_result.is_none() {
            prior_submission_effect
        } else {
            None
        }
    });

    if let Some(effect) = effect_to_replace {
        let latest = decision_effects::latest_active_for_scope(
            &tx, MODULE, student_id, REF_TYPE, content_id,
        )?
        .ok_or_else(|| CoreError::Invalid("效果账本状态不完整".into()))?;
        if latest.id != effect.id {
            return Err(CoreError::Invalid(
                "该记录之后已有终审，请先逆序改判较新的记录".into(),
            ));
        }
        restore_effect(&tx, &effect)?;
        learning_evidence::revert_for_effect(&tx, &effect)?;
        retention::revert_for_effect(&tx, &effect)?;
        point_reviews::revert_for_effect_inner(&tx, effect.id)?;
        decision_effects::mark_reverted(&tx, effect.id)?;
    } else if existing_result.is_some() {
        return Err(CoreError::Invalid(
            "历史终审缺少效果账本，拒绝猜测并覆盖聚合状态".into(),
        ));
    } else if let Some(task_id) = sub.task_id {
        let task = tasks::get(&tx, task_id)?
            .ok_or_else(|| CoreError::NotFound(format!("task {task_id}")))?;
        if matches!(task.status, TaskStatus::Passed | TaskStatus::Failed) {
            return Err(CoreError::Invalid(
                "检测到旧版机器评分已产生副作用，请先执行旧数据修复后再终审".into(),
            ));
        }
    }

    let quality = verdict.quality.clone().unwrap_or_else(|| "C".to_string());
    let applied = apply_and_record_effect(
        &tx,
        &EffectApplication {
            verdict_id: verdict.id,
            review_ref: &review_ref,
            task_id: sub.task_id,
            result,
            quality: &quality,
            today,
            cfg,
        },
    )?;
    verdicts::set_human_result(&tx, verdict.id, result, note, decided_by)?;
    let recorded_review = if let (Some(review), Some(actor)) = (point_review, point_review_actor) {
        Some(point_reviews::record_review_inner(
            &tx,
            submission_id,
            verdict.id,
            applied.effect_id,
            result,
            actor,
            review,
        )?)
    } else {
        None
    };
    let active_effect = decision_effects::active_for_verdict(&tx, verdict.id)?
        .filter(|effect| effect.id == applied.effect_id)
        .ok_or_else(|| CoreError::Db("新终审效果账本写入后无法读取".into()))?;
    learning_evidence::activate_for_decision(
        &tx,
        &learning_evidence::DecisionEvidenceInput {
            submission_id,
            student_id,
            verdict: &verdict,
            effect: &active_effect,
            review: recorded_review.as_ref(),
            timing: Some(learning_evidence::DecisionTiming {
                decision_date: today,
                task: decision_task.as_ref(),
            }),
            actor: decision_actor,
        },
    )?;
    submissions::set_status(&tx, submission_id, "confirmed")?;
    tx.commit()?;
    Ok(applied.next)
}

#[cfg(test)]
#[path = "scoring_tests.rs"]
mod tests;
