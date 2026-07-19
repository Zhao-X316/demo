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
mod tests {
    use super::*;
    use crate::db::structured::{
        confirm_rubric, create_rubric_draft, current_answer_version, CreateRubricDraftInput,
        RubricPointDraftInput,
    };
    use crate::service::ai_pipeline::{
        begin_asr_run, finish_asr_success, AsrRunDescriptor, BeginAsrRun, FinishAsrSuccessInput,
    };
    use crate::service::import::{import_one, ImportItem, ImportOutcome};
    use module_knowledge::db::taxonomy::{
        create_knowledge_map, create_knowledge_node, create_textbook_edition, NewKnowledgeMap,
        NewKnowledgeNode, NewTextbookEdition,
    };
    use rusqlite::params;
    use suite_core::db::repo::students::{upsert as upsert_student, StudentInput};
    use suite_core::db::repo::tasks::NewTask;
    use suite_core::db::repo::{learning_evidence as evidence_repo, memory_cards};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::domain::hashing;
    use suite_core::models::TaskKind;
    use suite_core::ports::RecognizedWord;

    const EMPTY_ITEMS: &str = r#"{"schema_version":1,"items":[]}"#;

    fn setup_imported(answer: &str) -> (Connection, i64, i64, i64) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
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
        let audio_path = std::env::temp_dir().join("jiaofu-suite-scoring-test.m4a");
        std::fs::write(&audio_path, b"test audio evidence").unwrap();
        let audio_path = audio_path.to_string_lossy().to_string();
        let out = import_one(
            &conn,
            &ImportItem {
                file_path: &audio_path,
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

    fn import_scored_task(
        conn: &Connection,
        student_id: i64,
        content_id: i64,
        answer: &str,
        date: NaiveDate,
        kind: TaskKind,
        hash: &str,
    ) -> i64 {
        let due_date = date.format("%Y-%m-%d").to_string();
        tasks::insert(
            conn,
            &NewTask {
                module: MODULE,
                student_id,
                subject_id: None,
                ref_type: REF_TYPE,
                ref_id: content_id,
                kind,
                due_date: &due_date,
                source_task_id: None,
                card_id: None,
            },
        )
        .unwrap();
        let audio_path = std::env::temp_dir().join(format!("jiaofu-retention-{hash}.m4a"));
        std::fs::write(&audio_path, format!("retention audio {hash}")).unwrap();
        let file_stem = format!("{}_2023001_张三_C012", date.format("%Y%m%d"));
        let imported = import_one(
            conn,
            &ImportItem {
                file_path: &audio_path.to_string_lossy(),
                file_stem: &file_stem,
                file_hash: hash,
                duration_ms: Some(5_000),
            },
        )
        .unwrap();
        let submission_id = match imported {
            ImportOutcome::Imported { submission_id, .. } => submission_id,
            other => panic!("retention import failed: {other:?}"),
        };
        submissions::set_recognition(conn, submission_id, Some(answer), "ok", None, Some(5_000))
            .unwrap();
        score_submission(conn, submission_id, &[], date, &ScoreCfg::default()).unwrap();
        submission_id
    }

    type StructuredSetup = (
        Connection,
        i64,
        i64,
        i64,
        i64,
        i64,
        Option<String>,
        Option<String>,
    );

    fn setup_structured_scored_inner(
        answer: &str,
        knowledge_map_state: Option<&'static str>,
    ) -> StructuredSetup {
        let (conn, student_id, content_id, submission_id) = setup_imported(answer);
        let link = if let Some(map_state) = knowledge_map_state {
            conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])
                .unwrap();
            let edition = create_textbook_edition(
                &conn,
                &NewTextbookEdition {
                    subject_id: 1,
                    publisher_code: "PEP",
                    edition_code: "2024",
                    title: "中国历史八年级上册",
                    grade: "8",
                    volume: "upper",
                    curriculum_region: Some("CN"),
                },
            )
            .unwrap();
            let map = create_knowledge_map(
                &conn,
                &NewKnowledgeMap {
                    textbook_edition_id: edition.id,
                    revision: 1,
                    state: map_state,
                    supersedes_map_id: None,
                },
            )
            .unwrap();
            let node = create_knowledge_node(
                &conn,
                &NewKnowledgeNode {
                    stable_id: None,
                    knowledge_map_id: map.id,
                    curriculum_node_id: None,
                    parent_id: None,
                    code: Some("K-MAIN"),
                    title: "背诵主评分点",
                    description: None,
                    order_index: 0,
                },
            )
            .unwrap();
            Some((
                node.id,
                node.public_id,
                format!("{}:r{}", map.public_id, map.revision),
            ))
        } else {
            None
        };
        let answer_version = current_answer_version(&conn, content_id).unwrap().unwrap();
        let points = [RubricPointDraftInput {
            stable_key: "main-point",
            canonical_text: answer,
            required_entities_json: EMPTY_ITEMS,
            allowed_paraphrases_json: EMPTY_ITEMS,
            contradiction_rules_json: EMPTY_ITEMS,
            required: true,
            weight: 1.0,
            order_index: 0,
            knowledge_node_id: link.as_ref().map(|value| value.0),
            knowledge_link_state: if link.is_some() { "confirmed" } else { "none" },
            verified_by: link.as_ref().map(|_| "teacher"),
            verified_at: link.as_ref().map(|_| "2026-06-25T00:00:00.000Z"),
        }];
        let rubric = create_rubric_draft(
            &conn,
            &CreateRubricDraftInput {
                answer_version_id: answer_version.id,
                generated_by_ai_run_id: None,
                created_by: "teacher",
                points: &points,
            },
        )
        .unwrap();
        confirm_rubric(&conn, rubric.id, "teacher").unwrap();

        let input_hash = hashing::sha256_hex(format!("audio:{submission_id}").as_bytes());
        let descriptor = AsrRunDescriptor {
            provider: "test",
            model_name: "test-asr",
            model_version: "1",
            config_version: "1",
            prompt_or_rule_version: "1",
        };
        let ai_run_id =
            match begin_asr_run(&conn, submission_id, &input_hash, &descriptor, false).unwrap() {
                BeginAsrRun::Execute { ai_run_id, .. } => ai_run_id,
                BeginAsrRun::Cached { .. } => panic!("first test ASR must execute"),
            };
        let words = [RecognizedWord {
            text: answer.to_string(),
            start_ms: 100,
            end_ms: 4_000,
        }];
        finish_asr_success(
            &conn,
            ai_run_id,
            &FinishAsrSuccessInput {
                submission_id,
                raw_transcript: answer,
                normalized_transcript: answer,
                normalization_version: "test-v1",
                words: &words,
                duration_ms: 5_000,
            },
        )
        .unwrap();
        submissions::set_recognition(&conn, submission_id, Some(answer), "ok", None, Some(5_000))
            .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        score_submission(&conn, submission_id, &words, today, &ScoreCfg::default()).unwrap();
        let score = crate::service::structured_scoring::record_score_if_ready(
            &conn,
            submission_id,
            &ScoreCfg::default(),
        )
        .unwrap()
        .unwrap();
        let point_result = point_reviews::list_point_results(&conn, score.id)
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        (
            conn,
            student_id,
            content_id,
            submission_id,
            score.id,
            point_result.id,
            link.as_ref().map(|value| value.1.clone()),
            link.as_ref().map(|value| value.2.clone()),
        )
    }

    fn setup_structured_scored(answer: &str) -> (Connection, i64, i64, i64, i64, i64) {
        let (conn, student_id, content_id, submission_id, score_id, point_result_id, _, _) =
            setup_structured_scored_inner(answer, None);
        (
            conn,
            student_id,
            content_id,
            submission_id,
            score_id,
            point_result_id,
        )
    }

    fn accepted_review(score_run_id: i64, point_result_id: i64) -> TeacherPointReviewInput {
        TeacherPointReviewInput {
            score_run_id,
            items: vec![point_reviews::TeacherPointReviewItemInput {
                point_result_id,
                confirmation_level: "accepted".into(),
                corrected_state: None,
                corrected_evidence_spans_json: None,
                teacher_note: None,
            }],
        }
    }

    #[test]
    fn overall_confirmation_does_not_silently_confirm_machine_points() {
        let (conn, _sid, _cid, submission_id, _score_id, _point_result_id) =
            setup_structured_scored("床前明月光");
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        human_decide(
            &conn,
            submission_id,
            "pass",
            Some("只确认总体"),
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        assert!(
            point_reviews::active_review_for_submission(&conn, submission_id)
                .unwrap()
                .is_none()
        );
        let review_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM rec_point_review_revisions WHERE submission_id=?1",
                [submission_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(review_count, 0);
        let evidence_counts: (i64, i64, i64) = conn
            .query_row(
                "SELECT count(*),
                        sum(CASE WHEN source_type='recitation_overall'
                                      AND evidence_kind='accuracy' THEN 1 ELSE 0 END),
                        sum(CASE WHEN source_type='recitation_fluency'
                                      AND evidence_kind='fluency' THEN 1 ELSE 0 END)
                 FROM learning_evidence
                 WHERE source_module='recitation' AND state='active'
                   AND confirmation_level='teacher_overall'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(evidence_counts, (2, 1, 1));
    }

    #[test]
    fn database_refuses_to_seal_incomplete_point_review() {
        let (conn, _sid, _cid, submission_id, score_id, _point_result_id) =
            setup_structured_scored("床前明月光");
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        human_decide(
            &conn,
            submission_id,
            "pass",
            Some("只确认总体"),
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        let verdict = verdicts::get_by_submission(&conn, submission_id)
            .unwrap()
            .unwrap();
        let effect = decision_effects::active_for_verdict(&conn, verdict.id)
            .unwrap()
            .unwrap();

        let tx = conn.unchecked_transaction().unwrap();
        tx.execute(
            "INSERT INTO rec_point_review_revisions
              (public_id,submission_id,score_run_id,verdict_id,decision_effect_id,revision,
               item_count,definition_hash,overall_result,created_by,created_at)
             VALUES (?1,?2,?3,?4,?5,1,2,?6,'pass','teacher',?7)",
            params![
                "test-incomplete-point-review",
                submission_id,
                score_id,
                verdict.id,
                effect.id,
                "a".repeat(64),
                "2026-06-25T00:00:00Z",
            ],
        )
        .unwrap();
        let review_id = tx.last_insert_rowid();
        let error = tx
            .execute(
                "UPDATE rec_point_review_revisions SET state='active' WHERE id=?1",
                [review_id],
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("M1_POINT_REVIEW_ITEMS_INCOMPLETE"));
        tx.rollback().unwrap();
    }

    #[test]
    fn explicit_point_acceptance_is_idempotent_and_audited() {
        let (conn, _sid, _cid, submission_id, score_id, point_result_id) =
            setup_structured_scored("床前明月光");
        let review = accepted_review(score_id, point_result_id);
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        human_decide_with_point_review(
            &conn,
            submission_id,
            &HumanDecisionRequest {
                result: "pass",
                note: Some("总体和逐点均确认"),
                decided_by: Some("teacher"),
                point_review: Some(&review),
            },
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        let first = point_reviews::active_review_for_submission(&conn, submission_id)
            .unwrap()
            .unwrap();
        let items = point_reviews::list_review_items(&conn, first.id).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].confirmation_level, "accepted");

        human_decide_with_point_review(
            &conn,
            submission_id,
            &HumanDecisionRequest {
                result: "pass",
                note: Some("重复提交"),
                decided_by: Some("teacher"),
                point_review: Some(&review),
            },
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM rec_point_review_revisions WHERE submission_id=?1",
                [submission_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        let audit_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM audit_events
                 WHERE object_type='recitation_point_review' AND action='recitation.point_review.recorded'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_count, 1);
        let evidence_counts: (i64, i64, i64, i64) = conn
            .query_row(
                "SELECT count(*),
                        sum(CASE WHEN confirmation_level='teacher_overall' THEN 1 ELSE 0 END),
                        sum(CASE WHEN confirmation_level='teacher_accepted' THEN 1 ELSE 0 END),
                        sum(CASE WHEN source_type='recitation_rubric_point'
                                      AND knowledge_node_id IS NULL THEN 1 ELSE 0 END)
                 FROM learning_evidence
                 WHERE source_module='recitation' AND state='active'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(evidence_counts, (4, 2, 2, 2));
        let evidence_events: (i64, i64) = conn
            .query_row(
                "SELECT
                   (SELECT count(*) FROM outbox_events
                    WHERE event_type='learning_evidence_changed'
                      AND aggregate_type='learning_evidence'),
                   (SELECT count(*) FROM audit_events
                    WHERE action='recitation.learning_evidence.activated'
                      AND object_type='learning_evidence')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(evidence_events, (4, 4));
    }

    #[test]
    fn confirmed_rubric_link_projects_public_knowledge_identity() {
        let (
            conn,
            _student_id,
            _content_id,
            submission_id,
            score_id,
            point_result_id,
            node_public_id,
            map_version,
        ) = setup_structured_scored_inner("床前明月光", Some("confirmed"));
        let review = accepted_review(score_id, point_result_id);
        human_decide_with_point_review(
            &conn,
            submission_id,
            &HumanDecisionRequest {
                result: "pass",
                note: Some("确认总体、逐点和知识链接"),
                decided_by: Some("teacher"),
                point_review: Some(&review),
            },
            NaiveDate::from_ymd_opt(2026, 6, 25).unwrap(),
            &ScoreCfg::default(),
        )
        .unwrap();
        let targets: Vec<(String, String)> = {
            let mut statement = conn
                .prepare(
                    "SELECT knowledge_node_id,knowledge_map_version
                     FROM learning_evidence
                     WHERE source_type='recitation_rubric_point' AND state='active'
                     ORDER BY evidence_kind",
                )
                .unwrap();
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().all(|target| {
            target.0 == node_public_id.as_deref().unwrap()
                && target.1 == map_version.as_deref().unwrap()
        }));
    }

    #[test]
    fn draft_knowledge_map_is_not_projected_as_formal_point_identity() {
        let (conn, _, _, submission_id, score_id, point_result_id, _, _) =
            setup_structured_scored_inner("床前明月光", Some("draft"));
        let review = accepted_review(score_id, point_result_id);
        human_decide_with_point_review(
            &conn,
            submission_id,
            &HumanDecisionRequest {
                result: "pass",
                note: Some("确认逐点，但知识地图尚未确认"),
                decided_by: Some("teacher"),
                point_review: Some(&review),
            },
            NaiveDate::from_ymd_opt(2026, 6, 25).unwrap(),
            &ScoreCfg::default(),
        )
        .unwrap();
        let targets: Vec<(Option<String>, String)> = {
            let mut statement = conn
                .prepare(
                    "SELECT knowledge_node_id,knowledge_map_version
                     FROM learning_evidence
                     WHERE source_type='recitation_rubric_point' AND state='active'
                     ORDER BY evidence_kind",
                )
                .unwrap();
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(targets.len(), 2);
        assert!(targets
            .iter()
            .all(|target| target.0.is_none() && target.1 == "unmapped:r1"));
    }

    #[test]
    fn invalid_point_correction_rolls_back_overall_effects() {
        let (conn, student_id, content_id, submission_id, score_id, point_result_id) =
            setup_structured_scored("床前明月光");
        let invalid = TeacherPointReviewInput {
            score_run_id: score_id,
            items: vec![point_reviews::TeacherPointReviewItemInput {
                point_result_id,
                confirmation_level: "corrected".into(),
                corrected_state: Some("omitted".into()),
                corrected_evidence_spans_json: Some("[]".into()),
                teacher_note: None,
            }],
        };
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let error = human_decide_with_point_review(
            &conn,
            submission_id,
            &HumanDecisionRequest {
                result: "pass",
                note: None,
                decided_by: Some("teacher"),
                point_review: Some(&invalid),
            },
            today,
            &ScoreCfg::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("必须填写简短说明"));
        assert!(
            memory_cards::get(&conn, MODULE, student_id, REF_TYPE, content_id)
                .unwrap()
                .is_none()
        );
        let verdict = verdicts::get_by_submission(&conn, submission_id)
            .unwrap()
            .unwrap();
        assert!(verdict.human_result.is_none());
        assert!(decision_effects::active_for_verdict(&conn, verdict.id)
            .unwrap()
            .is_none());
        assert!(
            point_reviews::active_review_for_submission(&conn, submission_id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn returning_to_an_older_point_definition_creates_a_new_revision() {
        let (conn, _sid, _cid, submission_id, score_id, point_result_id) =
            setup_structured_scored("床前明月光");
        let accepted = accepted_review(score_id, point_result_id);
        let corrected = TeacherPointReviewInput {
            score_run_id: score_id,
            items: vec![point_reviews::TeacherPointReviewItemInput {
                point_result_id,
                confirmation_level: "corrected".into(),
                corrected_state: Some("omitted".into()),
                corrected_evidence_spans_json: Some("[]".into()),
                teacher_note: Some("回听后确认未完整说出".into()),
            }],
        };
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        for review in [&accepted, &corrected, &accepted] {
            human_decide_with_point_review(
                &conn,
                submission_id,
                &HumanDecisionRequest {
                    result: "pass",
                    note: None,
                    decided_by: Some("teacher"),
                    point_review: Some(review),
                },
                today,
                &ScoreCfg::default(),
            )
            .unwrap();
        }
        let active = point_reviews::active_review_for_submission(&conn, submission_id)
            .unwrap()
            .unwrap();
        assert_eq!(active.revision, 3);
        let items = point_reviews::list_review_items(&conn, active.id).unwrap();
        assert_eq!(items[0].confirmation_level, "accepted");
        let superseded: i64 = conn
            .query_row(
                "SELECT count(*) FROM rec_point_review_revisions
                 WHERE submission_id=?1 AND state='superseded'",
                [submission_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(superseded, 2);
        let evidence_states: (i64, i64, i64) = conn
            .query_row(
                "SELECT count(*),
                        sum(CASE WHEN state='active' THEN 1 ELSE 0 END),
                        sum(CASE WHEN state='superseded' THEN 1 ELSE 0 END)
                 FROM learning_evidence
                 WHERE source_module='recitation'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(evidence_states, (7, 4, 3));
    }

    #[test]
    fn overall_change_reverts_prior_point_review_before_new_revision() {
        let (conn, _sid, _cid, submission_id, score_id, point_result_id) =
            setup_structured_scored("床前明月光");
        let review = accepted_review(score_id, point_result_id);
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        human_decide_with_point_review(
            &conn,
            submission_id,
            &HumanDecisionRequest {
                result: "pass",
                note: None,
                decided_by: Some("teacher"),
                point_review: Some(&review),
            },
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        let first = point_reviews::active_review_for_submission(&conn, submission_id)
            .unwrap()
            .unwrap();

        human_decide_with_point_review(
            &conn,
            submission_id,
            &HumanDecisionRequest {
                result: "fail",
                note: Some("改判并复核逐点"),
                decided_by: Some("teacher"),
                point_review: Some(&review),
            },
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        let second = point_reviews::active_review_for_submission(&conn, submission_id)
            .unwrap()
            .unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(second.overall_result, "fail");
        assert_ne!(second.decision_effect_id, first.decision_effect_id);
        assert_eq!(
            point_reviews::get_review(&conn, first.id)
                .unwrap()
                .unwrap()
                .state,
            "reverted"
        );
        let evidence_states: (i64, i64) = conn
            .query_row(
                "SELECT sum(CASE WHEN state='active' THEN 1 ELSE 0 END),
                        sum(CASE WHEN state='reverted' THEN 1 ELSE 0 END)
                 FROM learning_evidence WHERE source_module='recitation'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(evidence_states, (4, 4));
    }

    #[test]
    fn database_blocks_effect_revert_while_recitation_evidence_is_active() {
        let (conn, _sid, _cid, submission_id, _score_id, _point_result_id) =
            setup_structured_scored("床前明月光");
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        human_decide(
            &conn,
            submission_id,
            "pass",
            Some("确认总体"),
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        let verdict = verdicts::get_by_submission(&conn, submission_id)
            .unwrap()
            .unwrap();
        let effect = decision_effects::active_for_verdict(&conn, verdict.id)
            .unwrap()
            .unwrap();
        let error = decision_effects::mark_reverted(&conn, effect.id)
            .unwrap_err()
            .to_string();
        assert!(error.contains("M1_LEARNING_EVIDENCE_MUST_REVERT_FIRST"));
        assert_eq!(
            learning_evidence::revert_for_effect(&conn, &effect).unwrap(),
            2
        );
        let context_error = decision_effects::mark_reverted(&conn, effect.id)
            .unwrap_err()
            .to_string();
        assert!(context_error.contains("M1_RETENTION_CONTEXT_MUST_REVERT_FIRST"));
        assert_eq!(retention::revert_for_effect(&conn, &effect).unwrap(), 1);
        decision_effects::mark_reverted(&conn, effect.id).unwrap();
    }

    #[test]
    fn evidence_write_failure_rolls_back_decision_effect_and_schedule() {
        let (conn, student_id, content_id, submission_id, _score_id, _point_result_id) =
            setup_structured_scored("床前明月光");
        conn.execute_batch(
            "CREATE TEMP TRIGGER fail_recitation_fluency_evidence
             BEFORE INSERT ON learning_evidence
             WHEN NEW.source_module='recitation' AND NEW.evidence_kind='fluency'
             BEGIN
               SELECT RAISE(ABORT,'TEST_FAIL_RECITATION_EVIDENCE');
             END;",
        )
        .unwrap();
        let error = human_decide(
            &conn,
            submission_id,
            "pass",
            Some("触发证据写入失败"),
            Some("teacher"),
            NaiveDate::from_ymd_opt(2026, 6, 25).unwrap(),
            &ScoreCfg::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("TEST_FAIL_RECITATION_EVIDENCE"));
        let verdict = verdicts::get_by_submission(&conn, submission_id)
            .unwrap()
            .unwrap();
        assert!(verdict.human_result.is_none());
        assert!(decision_effects::active_for_verdict(&conn, verdict.id)
            .unwrap()
            .is_none());
        assert!(
            memory_cards::get(&conn, MODULE, student_id, REF_TYPE, content_id)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            evidence_repo::list_active_for_student(&conn, student_id, false)
                .unwrap()
                .len(),
            0
        );
    }

    #[test]
    fn machine_pass_waits_for_human_then_schedules_review() {
        let answer = "床前明月光";
        let (conn, sid, cid, sub) = setup_imported(answer);
        // ASR 完美识别
        submissions::set_recognition(&conn, sub, Some(answer), "ok", None, Some(5000)).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let out = score_submission(&conn, sub, &[], today, &ScoreCfg::default()).unwrap();

        assert!(out.pass);
        assert_eq!(out.accuracy, 100.0);
        assert_eq!(out.next, NextAction::AwaitingHumanReview);
        assert!(memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .is_none());
        let task_id = score_task_id(&conn, sub);
        assert_eq!(
            tasks::get(&conn, task_id).unwrap().unwrap().status,
            TaskStatus::Submitted
        );

        let next = human_decide(
            &conn,
            sub,
            "pass",
            Some("确认通过"),
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        assert!(matches!(next, NextAction::Scheduled { .. }));
        let card = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();
        assert!(card.due_date.is_some());
        let t = tasks::get(&conn, task_id).unwrap().unwrap();
        assert_eq!(t.status, TaskStatus::Passed);
    }

    #[test]
    fn human_decision_requires_asr_text_and_readable_audio() {
        let answer = "床前明月光";
        let (conn, _sid, _cid, sub) = setup_imported(answer);
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        submissions::set_recognition(&conn, sub, Some(""), "ok", None, Some(5000)).unwrap();
        score_submission(&conn, sub, &[], today, &ScoreCfg::default()).unwrap();
        let empty_asr = human_decide(
            &conn,
            sub,
            "pass",
            None,
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(empty_asr.contains("ASR 原文为空"));

        submissions::set_recognition(&conn, sub, Some(answer), "ok", None, Some(5000)).unwrap();
        submissions::set_file_path(&conn, sub, "/path/does/not/exist.m4a").unwrap();
        let missing_audio = human_decide(
            &conn,
            sub,
            "pass",
            None,
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(missing_audio.contains("录音文件不可读"));
    }

    #[test]
    fn human_decision_rejects_changed_answer_version() {
        let answer = "床前明月光";
        let (conn, _sid, _cid, sub) = setup_imported(answer);
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        submissions::set_recognition(&conn, sub, Some(answer), "ok", None, Some(5000)).unwrap();
        score_submission(&conn, sub, &[], today, &ScoreCfg::default()).unwrap();

        contents::upsert(
            &conn,
            &contents::ContentInput {
                content_no: "C012",
                title: "静夜思",
                answer_text: "床前明月光疑是地上霜",
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap();

        let err = human_decide(
            &conn,
            sub,
            "pass",
            None,
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("答案版本已变化"));
    }

    #[test]
    fn machine_fail_waits_for_human_then_generates_makeup() {
        // 答案两句，只背一句 → < 95%
        let (conn, sid, cid, sub) = setup_imported("床前明月光疑是地上霜");
        submissions::set_recognition(&conn, sub, Some("床前明月光"), "ok", None, Some(3000))
            .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        let out = score_submission(&conn, sub, &[], today, &ScoreCfg::default()).unwrap();

        assert!(!out.pass);
        assert_eq!(out.next, NextAction::AwaitingHumanReview);
        assert!(!tasks::exists_open_kind(&conn, MODULE, sid, cid, TaskKind::Makeup).unwrap());

        let next = human_decide(
            &conn,
            sub,
            "fail",
            Some("确认未通过"),
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        match next {
            NextAction::Makeup { task_id, due_date } => {
                assert!(task_id.is_some());
                assert_eq!(due_date, "2026-06-26"); // 次日
            }
            other => panic!("expected Makeup, got {other:?}"),
        }
        assert!(tasks::exists_open_kind(&conn, MODULE, sid, cid, TaskKind::Makeup).unwrap());
        let card = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();
        assert_eq!(card.state, suite_core::models::CardState::Lapsed);
        assert_eq!(card.stage, 0);
        assert_eq!(card.reps, 0);
        assert_eq!(card.lapses, 1);
    }

    #[test]
    fn makeup_good_pass_restarts_schedule_without_erasing_lapse() {
        let answer = "字".repeat(20);
        let (conn, sid, cid, first_sub) = setup_imported(&answer);
        let day1 = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        submissions::set_recognition(
            &conn,
            first_sub,
            Some(&"字".repeat(10)),
            "ok",
            None,
            Some(2500),
        )
        .unwrap();
        score_submission(&conn, first_sub, &[], day1, &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            first_sub,
            "fail",
            None,
            Some("teacher"),
            day1,
            &ScoreCfg::default(),
        )
        .unwrap();

        let audio_path = std::env::temp_dir().join("jiaofu-suite-makeup-pass-test.m4a");
        std::fs::write(&audio_path, b"makeup test audio evidence").unwrap();
        let audio_path = audio_path.to_string_lossy().to_string();
        let imported = import_one(
            &conn,
            &ImportItem {
                file_path: &audio_path,
                file_stem: "20260626_2023001_张三_C012",
                file_hash: "h2",
                duration_ms: Some(5000),
            },
        )
        .unwrap();
        let makeup_sub = match imported {
            ImportOutcome::Imported { submission_id, .. } => submission_id,
            other => panic!("makeup import failed: {other:?}"),
        };
        submissions::set_recognition(&conn, makeup_sub, Some(&answer), "ok", None, Some(5000))
            .unwrap();
        let words: Vec<_> = (0..20)
            .map(|index| RecognizedWord {
                text: "字".into(),
                start_ms: index * 250,
                end_ms: index * 250 + 250,
            })
            .collect();
        let day2 = NaiveDate::from_ymd_opt(2026, 6, 26).unwrap();
        let scored =
            score_submission(&conn, makeup_sub, &words, day2, &ScoreCfg::default()).unwrap();
        assert!(scored.pass);
        assert_eq!(scored.quality, "A");
        let next = human_decide(
            &conn,
            makeup_sub,
            "pass",
            None,
            Some("teacher"),
            day2,
            &ScoreCfg::default(),
        )
        .unwrap();
        assert_eq!(
            next,
            NextAction::Scheduled {
                due_date: "2026-06-28".into(),
                stage: 1,
            }
        );

        let card = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();
        assert_eq!(card.state, suite_core::models::CardState::Review);
        assert_eq!(card.stage, 1);
        assert_eq!(card.interval_days, 2);
        assert_eq!(card.reps, 1);
        assert_eq!(card.lapses, 1);
        assert_eq!(card.due_date.as_deref(), Some("2026-06-28"));
        assert_eq!(
            tasks::get(&conn, score_task_id(&conn, makeup_sub))
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Passed
        );
        let retention: (i64, i64, i64, i64, String, f64) = conn
            .query_row(
                "SELECT actual_interval_days,planned_interval_days,recovered_after_lapse,
                        revision,result,evidence.value
                 FROM rec_retention_windows window
                 JOIN learning_evidence evidence
                   ON evidence.source_ref_type='recitation_retention_window'
                  AND evidence.source_ref_id=window.public_id
                  AND evidence.source_revision=window.revision
                 WHERE window.state='active' AND evidence.state='active'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(retention, (1, 1, 1, 1, "pass".into(), 1.0));
        let prior_failure: i64 = conn
            .query_row(
                "SELECT count(*) FROM learning_evidence
                 WHERE source_module='recitation'
                   AND source_type='recitation_overall'
                   AND evidence_kind='accuracy'
                   AND value=0.0
                   AND state='active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(prior_failure, 1, "补背恢复不能抹去此前老师确认的失败事实");
    }

    #[test]
    fn retention_strengthens_across_two_seven_and_twenty_one_day_windows() {
        let answer = "床前明月光";
        let (conn, sid, cid, first_sub) = setup_imported(answer);
        let day1 = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        submissions::set_recognition(&conn, first_sub, Some(answer), "ok", None, Some(5_000))
            .unwrap();
        score_submission(&conn, first_sub, &[], day1, &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            first_sub,
            "pass",
            None,
            Some("teacher"),
            day1,
            &ScoreCfg::default(),
        )
        .unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM rec_retention_windows", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0,
            "首次确认只有日期上下文，不能伪造保持证据"
        );

        for (date, expected_days, expected_quality, hash) in [
            (
                NaiveDate::from_ymd_opt(2026, 6, 27).unwrap(),
                2,
                0.6,
                "retention-2d",
            ),
            (
                NaiveDate::from_ymd_opt(2026, 7, 4).unwrap(),
                7,
                0.8,
                "retention-7d",
            ),
            (
                NaiveDate::from_ymd_opt(2026, 7, 25).unwrap(),
                21,
                1.0,
                "retention-21d",
            ),
        ] {
            let submission_id =
                import_scored_task(&conn, sid, cid, answer, date, TaskKind::Review, hash);
            human_decide(
                &conn,
                submission_id,
                "pass",
                None,
                Some("teacher"),
                date,
                &ScoreCfg::default(),
            )
            .unwrap();
            let (actual_days, planned_days, value, quality, confidence): (i64, i64, f64, f64, f64) =
                conn.query_row(
                    "SELECT window.actual_interval_days,window.planned_interval_days,
                            evidence.value,evidence.evidence_quality,verdict.confidence
                     FROM rec_retention_windows window
                     JOIN learning_evidence evidence
                       ON evidence.source_ref_type='recitation_retention_window'
                      AND evidence.source_ref_id=window.public_id
                      AND evidence.source_revision=window.revision
                     JOIN decision_effects effect ON effect.id=window.current_effect_id
                     JOIN verdicts verdict ON verdict.id=effect.verdict_id
                     WHERE window.window_date=?1
                       AND window.state='active' AND evidence.state='active'",
                    [date.format("%Y-%m-%d").to_string()],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .unwrap();
            assert_eq!(
                (actual_days, planned_days, value),
                (expected_days, expected_days, 1.0)
            );
            let expected_quality = expected_quality * (0.5 + 0.5 * confidence);
            assert!(
                (quality - expected_quality).abs() < 0.000_001,
                "{expected_days} 天窗口质量应为 {expected_quality}，实际为 {quality}"
            );
        }
        let active_retention: i64 = conn
            .query_row(
                "SELECT count(*) FROM learning_evidence
                 WHERE source_module='recitation'
                   AND source_type='recitation_retention'
                   AND state='active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active_retention, 3);
    }

    #[test]
    fn same_day_repeat_supersedes_retention_instead_of_counting_twice() {
        let answer = "床前明月光";
        let (conn, sid, cid, first_sub) = setup_imported(answer);
        let day1 = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        submissions::set_recognition(&conn, first_sub, Some(answer), "ok", None, Some(5_000))
            .unwrap();
        score_submission(&conn, first_sub, &[], day1, &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            first_sub,
            "pass",
            None,
            Some("teacher"),
            day1,
            &ScoreCfg::default(),
        )
        .unwrap();

        let day2 = NaiveDate::from_ymd_opt(2026, 6, 27).unwrap();
        let first_day2 = import_scored_task(
            &conn,
            sid,
            cid,
            answer,
            day2,
            TaskKind::Review,
            "retention-same-day-1",
        );
        human_decide(
            &conn,
            first_day2,
            "pass",
            None,
            Some("teacher"),
            day2,
            &ScoreCfg::default(),
        )
        .unwrap();
        let second_day2 = import_scored_task(
            &conn,
            sid,
            cid,
            answer,
            day2,
            TaskKind::Normal,
            "retention-same-day-2",
        );
        human_decide(
            &conn,
            second_day2,
            "pass",
            None,
            Some("teacher"),
            day2,
            &ScoreCfg::default(),
        )
        .unwrap();

        let window_states: (i64, i64, i64) = conn
            .query_row(
                "SELECT count(*),
                        sum(CASE WHEN state='active' THEN 1 ELSE 0 END),
                        sum(CASE WHEN state='superseded' THEN 1 ELSE 0 END)
                 FROM rec_retention_windows WHERE window_date='2026-06-27'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(window_states, (2, 1, 1));
        let evidence_states: (i64, i64) = conn
            .query_row(
                "SELECT sum(CASE WHEN state='active' THEN 1 ELSE 0 END),
                        sum(CASE WHEN state='superseded' THEN 1 ELSE 0 END)
                 FROM learning_evidence
                 WHERE source_module='recitation'
                   AND source_type='recitation_retention'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(evidence_states, (1, 1));
        let active_revision: i64 = conn
            .query_row(
                "SELECT revision FROM rec_retention_windows
                 WHERE window_date='2026-06-27' AND state='active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active_revision, 2);
    }

    #[test]
    fn latest_retention_rejudge_reverts_old_window_and_appends_revision() {
        let answer = "床前明月光";
        let (conn, sid, cid, first_sub) = setup_imported(answer);
        let day1 = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        submissions::set_recognition(&conn, first_sub, Some(answer), "ok", None, Some(5_000))
            .unwrap();
        score_submission(&conn, first_sub, &[], day1, &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            first_sub,
            "pass",
            None,
            Some("teacher"),
            day1,
            &ScoreCfg::default(),
        )
        .unwrap();

        let day2 = NaiveDate::from_ymd_opt(2026, 6, 27).unwrap();
        let second_sub = import_scored_task(
            &conn,
            sid,
            cid,
            answer,
            day2,
            TaskKind::Review,
            "retention-rejudge",
        );
        human_decide(
            &conn,
            second_sub,
            "pass",
            None,
            Some("teacher"),
            day2,
            &ScoreCfg::default(),
        )
        .unwrap();
        human_decide(
            &conn,
            second_sub,
            "fail",
            Some("老师改判"),
            Some("teacher"),
            day2,
            &ScoreCfg::default(),
        )
        .unwrap();

        let windows: (i64, i64) = conn
            .query_row(
                "SELECT sum(CASE WHEN state='active' THEN 1 ELSE 0 END),
                        sum(CASE WHEN state='reverted' THEN 1 ELSE 0 END)
                 FROM rec_retention_windows WHERE window_date='2026-06-27'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(windows, (1, 1));
        let evidence: (i64, i64, f64) = conn
            .query_row(
                "SELECT sum(CASE WHEN state='active' THEN 1 ELSE 0 END),
                        sum(CASE WHEN state='reverted' THEN 1 ELSE 0 END),
                        max(CASE WHEN state='active' THEN value END)
                 FROM learning_evidence
                 WHERE source_module='recitation'
                   AND source_type='recitation_retention'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(evidence, (1, 1, 0.0));
        let contexts: (i64, i64) = conn
            .query_row(
                "SELECT sum(CASE WHEN state='active' THEN 1 ELSE 0 END),
                        sum(CASE WHEN state='reverted' THEN 1 ELSE 0 END)
                 FROM rec_decision_contexts WHERE decision_date='2026-06-27'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(contexts, (1, 1));
    }

    #[test]
    fn retention_evidence_failure_rolls_back_window_effect_and_schedule() {
        let answer = "床前明月光";
        let (conn, sid, cid, first_sub) = setup_imported(answer);
        let day1 = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        submissions::set_recognition(&conn, first_sub, Some(answer), "ok", None, Some(5_000))
            .unwrap();
        score_submission(&conn, first_sub, &[], day1, &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            first_sub,
            "pass",
            None,
            Some("teacher"),
            day1,
            &ScoreCfg::default(),
        )
        .unwrap();
        let card_before = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();

        let day2 = NaiveDate::from_ymd_opt(2026, 6, 27).unwrap();
        let second_sub = import_scored_task(
            &conn,
            sid,
            cid,
            answer,
            day2,
            TaskKind::Review,
            "retention-failure",
        );
        conn.execute_batch(
            "CREATE TEMP TRIGGER fail_retention_evidence
             BEFORE INSERT ON learning_evidence
             WHEN NEW.source_module='recitation'
               AND NEW.source_type='recitation_retention'
             BEGIN SELECT RAISE(ABORT,'forced retention evidence failure'); END;",
        )
        .unwrap();
        let error = human_decide(
            &conn,
            second_sub,
            "pass",
            None,
            Some("teacher"),
            day2,
            &ScoreCfg::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("forced retention evidence failure"));

        let verdict = verdicts::get_by_submission(&conn, second_sub)
            .unwrap()
            .unwrap();
        assert!(verdict.human_result.is_none());
        assert!(decision_effects::active_for_verdict(&conn, verdict.id)
            .unwrap()
            .is_none());
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM rec_decision_contexts
                 WHERE decision_date='2026-06-27'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM rec_retention_windows", [], |row| row
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
        let card_after = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();
        assert_eq!(card_after.stage, card_before.stage);
        assert_eq!(card_after.reps, card_before.reps);
        assert_eq!(
            tasks::get(&conn, score_task_id(&conn, second_sub))
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Submitted
        );
    }

    #[test]
    fn repeated_human_decision_is_idempotent() {
        let answer = "床前明月光";
        let (conn, sid, cid, sub) = setup_imported(answer);
        submissions::set_recognition(&conn, sub, Some(answer), "ok", None, Some(5000)).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        score_submission(&conn, sub, &[], today, &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            sub,
            "pass",
            None,
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        let first = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();

        let next = human_decide(
            &conn,
            sub,
            "pass",
            Some("补充备注"),
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        assert_eq!(next, NextAction::Unchanged);
        let second = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();
        assert_eq!(second.reps, first.reps);
        assert_eq!(second.stage, first.stage);
        let effects: i64 = conn
            .query_row(
                "SELECT count(*) FROM decision_effects WHERE state='active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(effects, 1);
    }

    #[test]
    fn pass_to_fail_restores_before_state_then_applies_new_effect() {
        let answer = "床前明月光";
        let (conn, sid, cid, sub) = setup_imported(answer);
        submissions::set_recognition(&conn, sub, Some(answer), "ok", None, Some(5000)).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        score_submission(&conn, sub, &[], today, &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            sub,
            "pass",
            None,
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();

        let changed = human_decide(
            &conn,
            sub,
            "fail",
            Some("改判未通过"),
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        assert!(matches!(changed, NextAction::Makeup { .. }));
        let card = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();
        assert_eq!(card.state, suite_core::models::CardState::Lapsed);
        assert_eq!(card.stage, 0);
        assert_eq!(card.reps, 0);
        assert_eq!(card.lapses, 1);
        assert_eq!(
            tasks::get(&conn, score_task_id(&conn, sub))
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Failed
        );
        assert!(tasks::exists_open_kind(&conn, MODULE, sid, cid, TaskKind::Makeup).unwrap());
        let (active, reverted): (i64, i64) = conn
            .query_row(
                "SELECT sum(state='active'), sum(state='reverted') FROM decision_effects",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((active, reverted), (1, 1));
    }

    #[test]
    fn fail_to_pass_closes_effect_created_makeup() {
        let (conn, sid, cid, sub) = setup_imported("床前明月光疑是地上霜");
        submissions::set_recognition(&conn, sub, Some("床前明月光"), "ok", None, Some(3000))
            .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        score_submission(&conn, sub, &[], today, &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            sub,
            "fail",
            None,
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        assert!(tasks::exists_open_kind(&conn, MODULE, sid, cid, TaskKind::Makeup).unwrap());

        let changed = human_decide(
            &conn,
            sub,
            "pass",
            Some("改判通过"),
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();
        assert!(matches!(changed, NextAction::Scheduled { .. }));
        assert!(!tasks::exists_open_kind(&conn, MODULE, sid, cid, TaskKind::Makeup).unwrap());
        let card = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();
        assert_eq!(card.state, suite_core::models::CardState::Review);
        assert_eq!(card.stage, 0);
        assert_eq!(card.interval_days, 1);
        assert_eq!(card.reps, 1);
        assert_eq!(card.lapses, 0);
    }

    #[test]
    fn human_decision_rolls_back_when_effect_write_fails() {
        let answer = "床前明月光";
        let (conn, sid, cid, sub) = setup_imported(answer);
        submissions::set_recognition(&conn, sub, Some(answer), "ok", None, Some(5000)).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        score_submission(&conn, sub, &[], today, &ScoreCfg::default()).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_decision_effect_insert
             BEFORE INSERT ON decision_effects
             BEGIN
               SELECT RAISE(ABORT, 'forced decision effect failure');
             END;",
        )
        .unwrap();

        assert!(human_decide(
            &conn,
            sub,
            "pass",
            None,
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .is_err());
        assert!(memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .is_none());
        assert_eq!(
            tasks::get(&conn, score_task_id(&conn, sub))
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Submitted
        );
        assert!(verdicts::get_by_submission(&conn, sub)
            .unwrap()
            .unwrap()
            .human_result
            .is_none());
    }

    #[test]
    fn machine_scoring_rolls_back_verdict_when_status_write_fails() {
        let answer = "床前明月光";
        let (conn, _sid, _cid, sub) = setup_imported(answer);
        submissions::set_recognition(&conn, sub, Some(answer), "ok", None, Some(5000)).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_scored_status
             BEFORE UPDATE OF status ON submissions
             WHEN NEW.status='scored'
             BEGIN
               SELECT RAISE(ABORT, 'forced scored status failure');
             END;",
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();

        assert!(score_submission(&conn, sub, &[], today, &ScoreCfg::default()).is_err());
        let verdicts_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM verdicts WHERE submission_id=?1",
                [sub],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(verdicts_count, 0);
        assert_eq!(
            submissions::get(&conn, sub).unwrap().unwrap().status,
            "pending"
        );
    }

    #[test]
    fn recognition_finish_rolls_back_text_when_verdict_write_fails() {
        let answer = "床前明月光";
        let (conn, _sid, _cid, sub) = setup_imported(answer);
        submissions::claim_recognition(&conn, sub).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_finish_verdict
             BEFORE INSERT ON verdicts
             BEGIN SELECT RAISE(ABORT, 'injected verdict failure'); END;",
        )
        .unwrap();

        assert!(finish_recognition_and_score(
            &conn,
            sub,
            answer,
            Some(5000),
            &[],
            &ScoreCfg::default(),
        )
        .is_err());
        let submission = submissions::get(&conn, sub).unwrap().unwrap();
        assert_eq!(submission.recognize_status, "processing");
        assert_eq!(submission.recognized_text, None);
        assert_eq!(submission.status, "pending");
        let verdicts_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM verdicts WHERE submission_id=?1",
                [sub],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(verdicts_count, 0);
    }

    #[test]
    fn older_decision_cannot_change_after_newer_scope_effect() {
        let answer = "床前明月光";
        let (conn, sid, cid, first_sub) = setup_imported(answer);
        let day1 = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        submissions::set_recognition(&conn, first_sub, Some(answer), "ok", None, Some(5000))
            .unwrap();
        score_submission(&conn, first_sub, &[], day1, &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            first_sub,
            "pass",
            None,
            Some("teacher"),
            day1,
            &ScoreCfg::default(),
        )
        .unwrap();

        tasks::insert(
            &conn,
            &NewTask {
                module: MODULE,
                student_id: sid,
                subject_id: None,
                ref_type: REF_TYPE,
                ref_id: cid,
                kind: TaskKind::Normal,
                due_date: "2026-06-26",
                source_task_id: None,
                card_id: None,
            },
        )
        .unwrap();
        let second_audio = std::env::temp_dir().join("jiaofu-suite-scoring-test-2.m4a");
        std::fs::write(&second_audio, b"second test audio evidence").unwrap();
        let second_audio = second_audio.to_string_lossy().to_string();
        let imported = import_one(
            &conn,
            &ImportItem {
                file_path: &second_audio,
                file_stem: "20260626_2023001_张三_C012",
                file_hash: "h2",
                duration_ms: Some(5000),
            },
        )
        .unwrap();
        let second_sub = match imported {
            ImportOutcome::Imported { submission_id, .. } => submission_id,
            other => panic!("second import failed: {other:?}"),
        };
        let day2 = NaiveDate::from_ymd_opt(2026, 6, 26).unwrap();
        submissions::set_recognition(&conn, second_sub, Some(answer), "ok", None, Some(5000))
            .unwrap();
        score_submission(&conn, second_sub, &[], day2, &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            second_sub,
            "pass",
            None,
            Some("teacher"),
            day2,
            &ScoreCfg::default(),
        )
        .unwrap();
        let before = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();

        let err = human_decide(
            &conn,
            first_sub,
            "fail",
            None,
            Some("teacher"),
            day2,
            &ScoreCfg::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("先逆序改判较新的记录"));
        let after = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();
        assert_eq!(after.reps, before.reps);
        assert_eq!(after.stage, before.stage);
    }

    #[test]
    fn rescored_submission_replaces_its_prior_effect_instead_of_double_applying() {
        let answer = "床前明月光";
        let (conn, sid, cid, sub) = setup_imported(answer);
        submissions::set_recognition(&conn, sub, Some(answer), "ok", None, Some(5000)).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        score_submission(&conn, sub, &[], today, &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            sub,
            "pass",
            None,
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();

        // 同一 submission 重评分会产生新的 machine verdict，但不能叠加旧终审效果。
        rescore(&conn, sub, &[], &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            sub,
            "fail",
            Some("按新答案改判"),
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();

        let card = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();
        assert_eq!(card.state, suite_core::models::CardState::Lapsed);
        let (active, reverted): (i64, i64) = conn
            .query_row(
                "SELECT sum(state='active'), sum(state='reverted') FROM decision_effects",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((active, reverted), (1, 1));
    }

    #[test]
    fn changed_scope_after_confirmation_blocks_automatic_reversal() {
        let answer = "床前明月光";
        let (conn, sid, cid, sub) = setup_imported(answer);
        submissions::set_recognition(&conn, sub, Some(answer), "ok", None, Some(5000)).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        score_submission(&conn, sub, &[], today, &ScoreCfg::default()).unwrap();
        human_decide(
            &conn,
            sub,
            "pass",
            None,
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap();

        let task_id = score_task_id(&conn, sub);
        tasks::set_status(&conn, task_id, TaskStatus::Closed).unwrap();
        let before = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();
        let err = human_decide(
            &conn,
            sub,
            "fail",
            None,
            Some("teacher"),
            today,
            &ScoreCfg::default(),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("拒绝自动覆盖"));
        assert_eq!(
            tasks::get(&conn, task_id).unwrap().unwrap().status,
            TaskStatus::Closed
        );
        let after = memory_cards::get(&conn, MODULE, sid, REF_TYPE, cid)
            .unwrap()
            .unwrap();
        assert_eq!(after.reps, before.reps);
        assert_eq!(
            verdicts::get_by_submission(&conn, sub)
                .unwrap()
                .unwrap()
                .human_result
                .as_deref(),
            Some("pass")
        );
    }

    fn score_task_id(conn: &Connection, sub: i64) -> i64 {
        submissions::get(conn, sub)
            .unwrap()
            .unwrap()
            .task_id
            .unwrap()
    }
}
