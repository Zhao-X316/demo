//! 固定普通试卷印刷题面到 M2.5 私有候选管线的来源接线。
//!
//! 一份作业的同一页在全班会出现很多次；这里只选择一张老师已确认结构的学生页，
//! 把视觉模型已分离出的印刷文本送入既有 question_ingest。学生卷裁图永远不复用，
//! 隐私未通过时不会保存题干。题库同步失败也不回滚页面结构或批改结果。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::ai_runs;
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AiRunStatus;

use crate::ordinary_paper_recognition::{
    OrdinaryPaperPrintedQuestion, OrdinaryPaperQuestionType, OrdinaryPaperRecognitionOutput,
    OrdinaryPaperRecognitionState, ORDINARY_PAPER_READY_CONFIDENCE,
};

use super::ordinary_structure;
use super::question_ingest::{
    self, NewQuestionIngest, PrivacyScan, QuestionDraft, QuestionOptionDraft,
};

const SOURCE_RULE_VERSION: &str = "ordinary-printed-question-source-v1";
const MAX_WORKER_STEPS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdinaryQuestionSyncSummary {
    pub schema_version: i64,
    pub state: String,
    pub assessment_version_id: i64,
    pub page_no: i64,
    pub source_page_id: i64,
    pub source_ai_run_id: i64,
    pub printed_question_count: i64,
    pub eligible_count: i64,
    pub enqueued_count: i64,
    pub matched_count: i64,
    pub candidate_created_count: i64,
    pub needs_review_count: i64,
    pub privacy_rejected_count: i64,
    pub low_confidence_skipped_count: i64,
    pub failed_count: i64,
    pub reused_existing_source: bool,
}

struct SourceScope {
    assessment_version_id: i64,
    page_no: i64,
}

struct ItemBinding {
    region_revision_id: i64,
    question_type: String,
    max_score: f64,
}

struct SourceClaim {
    id: i64,
    state: String,
    summary_json: Option<String>,
    source_page_id: i64,
    source_ai_run_id: i64,
}

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!(
            "普通试卷题目沉淀 {field} 不能为空"
        )))
    } else {
        Ok(())
    }
}

fn extraction_version(output: &OrdinaryPaperRecognitionOutput) -> CoreResult<String> {
    let identity = serde_json::json!({
        "schema_version": 1,
        "source_rule_version": SOURCE_RULE_VERSION,
        "provider": output.descriptor.provider,
        "model_name": output.descriptor.model_name,
        "model_version": output.descriptor.model_version,
        "config_version": output.descriptor.config_version,
        "rule_version": output.descriptor.rule_version,
    });
    let hash = hashing::sha256_hex(
        &serde_json::to_vec(&identity)
            .map_err(|error| CoreError::Parse(format!("题面提取版本序列化失败：{error}")))?,
    );
    Ok(format!("{SOURCE_RULE_VERSION}:{}", &hash[..20]))
}

fn claim_key(scope: &SourceScope, owner_id: &str, extraction_version: &str) -> CoreResult<String> {
    let identity = serde_json::json!({
        "schema_version": 1,
        "assessment_version_id": scope.assessment_version_id,
        "page_no": scope.page_no,
        "owner_id": owner_id.trim(),
        "extraction_version": extraction_version,
    });
    let hash = hashing::sha256_hex(
        &serde_json::to_vec(&identity)
            .map_err(|error| CoreError::Parse(format!("题面来源 claim 序列化失败：{error}")))?,
    );
    Ok(format!("m2.5:ordinary-question-source:{hash}"))
}

fn load_run_and_scope(
    conn: &Connection,
    page_id: i64,
    ai_run_id: i64,
) -> CoreResult<(OrdinaryPaperRecognitionOutput, SourceScope)> {
    if page_id <= 0 || ai_run_id <= 0 {
        return Err(CoreError::Invalid(
            "普通试卷题目沉淀页面和 run id 必须为正数".into(),
        ));
    }
    let confirmation = ordinary_structure::get_confirmation_by_ai_run(conn, ai_run_id)?
        .ok_or_else(|| CoreError::Invalid("必须先由老师确认当前普通卷页面结构".into()))?;
    if confirmation.page_id != page_id {
        return Err(CoreError::Invalid(
            "普通试卷题目沉淀 run 不属于当前确认页面".into(),
        ));
    }
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    if run.status != AiRunStatus::Succeeded
        || run.run_type != "ordinary_paper_structure"
        || run.source_module != "exam"
        || run.business_ref_type != "ingest_page"
        || run.business_ref_id != page_id.to_string()
    {
        return Err(CoreError::Invalid(
            "普通试卷题目沉淀必须引用当前页面 succeeded 结构分析 run".into(),
        ));
    }
    let output_json = run
        .output_json
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("普通试卷成功 run 缺少输出".into()))?;
    if run.output_hash.as_deref() != Some(hashing::sha256_hex(output_json.as_bytes()).as_str()) {
        return Err(CoreError::Invalid(
            "普通试卷题面来源 run 输出 hash 已漂移".into(),
        ));
    }
    let output: OrdinaryPaperRecognitionOutput = serde_json::from_str(output_json)
        .map_err(|error| CoreError::Parse(format!("普通试卷题面来源输出无效：{error}")))?;
    if output.state != OrdinaryPaperRecognitionState::Ready
        || output.page_id != page_id
        || output.descriptor.provider != run.provider
        || output.descriptor.model_name != run.model_name
        || output.descriptor.model_version != run.model_version
        || output.descriptor.config_version != run.config_version
        || output.descriptor.rule_version != run.prompt_or_rule_version
    {
        return Err(CoreError::Invalid(
            "普通试卷题面来源与已确认结构 run 不一致".into(),
        ));
    }
    let scope = conn
        .query_row(
            "SELECT at.assessment_version_id,m.page_no
             FROM exam_ingest_pages_v2 p
             JOIN exam_page_match_revisions_v2 m
               ON m.page_id=p.id AND m.state='active' AND m.decision='teacher_confirmed'
             JOIN exam_attempts_v2 at ON at.id=m.attempt_id AND at.state<>'voided'
             JOIN exam_assessment_versions_v2 v
               ON v.id=at.assessment_version_id AND v.state='confirmed'
             WHERE p.id=?1 AND p.state<>'voided'",
            [page_id],
            |row| {
                Ok(SourceScope {
                    assessment_version_id: row.get(0)?,
                    page_no: row.get(1)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("普通试卷题面来源的作业版本或学生页归属已失效".into()))?;
    Ok((output, scope))
}

fn load_bindings(
    conn: &Connection,
    page_id: i64,
    assessment_version_id: i64,
    printed: &[OrdinaryPaperPrintedQuestion],
) -> CoreResult<BTreeMap<i64, ItemBinding>> {
    let confirmation_items = conn
        .prepare(
            "SELECT r.assessment_item_id,r.id,i.score,q.question_type
             FROM exam_answer_region_revisions_v2 r
             JOIN exam_assessment_items_v2 i
               ON i.id=r.assessment_item_id AND i.state='active'
                  AND i.assessment_version_id=?2
             JOIN k1_question_versions q ON q.id=i.question_version_id
             WHERE r.page_id=?1 AND r.state='active'
               AND r.decision='teacher_confirmed'
             ORDER BY r.assessment_item_id,r.region_index,r.id",
        )?
        .query_map((page_id, assessment_version_id), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                ItemBinding {
                    region_revision_id: row.get(1)?,
                    max_score: row.get(2)?,
                    question_type: row.get(3)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let printed_ids = printed
        .iter()
        .map(|question| question.assessment_item_id)
        .collect::<BTreeSet<_>>();
    if !printed_ids.is_subset(&confirmation_items.keys().copied().collect()) {
        return Err(CoreError::Invalid(
            "普通试卷印刷题面未能绑定到老师确认的当前题区".into(),
        ));
    }
    Ok(confirmation_items)
}

fn question_type(value: &str) -> CoreResult<OrdinaryPaperQuestionType> {
    OrdinaryPaperQuestionType::from_db(value)
        .ok_or_else(|| CoreError::Invalid("普通试卷印刷题面仅支持选择与判断题".into()))
}

fn draft_for(
    printed: &OrdinaryPaperPrintedQuestion,
    binding: &ItemBinding,
) -> CoreResult<QuestionDraft> {
    let current_type = question_type(&binding.question_type)?;
    let question_type = match current_type {
        OrdinaryPaperQuestionType::Single => "single",
        OrdinaryPaperQuestionType::Multiple => "multiple",
        OrdinaryPaperQuestionType::TrueFalse => "true_false",
    };
    Ok(QuestionDraft {
        question_type: question_type.into(),
        stem: printed.stem.trim().into(),
        material_text: printed
            .material_text
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        max_score: binding.max_score,
        options: printed
            .options
            .iter()
            .map(|option| QuestionOptionDraft {
                label: option.label.trim().into(),
                content: option.content.trim().into(),
                order_index: option.order_index,
            })
            .collect(),
    })
}

fn empty_summary(
    state: &str,
    scope: &SourceScope,
    page_id: i64,
    ai_run_id: i64,
    printed: &[OrdinaryPaperPrintedQuestion],
) -> OrdinaryQuestionSyncSummary {
    OrdinaryQuestionSyncSummary {
        schema_version: 1,
        state: state.into(),
        assessment_version_id: scope.assessment_version_id,
        page_no: scope.page_no,
        source_page_id: page_id,
        source_ai_run_id: ai_run_id,
        printed_question_count: printed.len() as i64,
        eligible_count: printed
            .iter()
            .filter(|question| {
                question.extraction_confidence >= ORDINARY_PAPER_READY_CONFIDENCE
                    && question.privacy.passed()
            })
            .count() as i64,
        enqueued_count: 0,
        matched_count: 0,
        candidate_created_count: 0,
        needs_review_count: 0,
        privacy_rejected_count: printed
            .iter()
            .filter(|question| {
                question.extraction_confidence >= ORDINARY_PAPER_READY_CONFIDENCE
                    && !question.privacy.passed()
            })
            .count() as i64,
        low_confidence_skipped_count: printed
            .iter()
            .filter(|question| question.extraction_confidence < ORDINARY_PAPER_READY_CONFIDENCE)
            .count() as i64,
        failed_count: 0,
        reused_existing_source: false,
    }
}

fn existing_summary(conn: &Connection, claim_key: &str) -> CoreResult<Option<SourceClaim>> {
    Ok(conn
        .query_row(
            "SELECT id,state,summary_json,source_page_id,source_ai_run_id
             FROM exam_ordinary_question_source_syncs_v2 WHERE claim_key=?1",
            [claim_key],
            |row| {
                Ok(SourceClaim {
                    id: row.get(0)?,
                    state: row.get(1)?,
                    summary_json: row.get(2)?,
                    source_page_id: row.get(3)?,
                    source_ai_run_id: row.get(4)?,
                })
            },
        )
        .optional()?)
}

fn persist_completed(
    conn: &Connection,
    claim_id: i64,
    summary: &OrdinaryQuestionSyncSummary,
) -> CoreResult<()> {
    let now = time::utc_now_rfc3339();
    let summary_json = serde_json::to_string(summary)
        .map_err(|error| CoreError::Parse(format!("题面同步摘要序列化失败：{error}")))?;
    conn.execute(
        "UPDATE exam_ordinary_question_source_syncs_v2
         SET state='completed',summary_json=?1,error_meta_json=NULL,updated_at=?2
         WHERE id=?3 AND state IN ('processing','failed')",
        params![summary_json, now, claim_id],
    )?;
    Ok(())
}

fn persist_failed(conn: &Connection, claim_id: i64, error: &CoreError) -> CoreResult<()> {
    let now = time::utc_now_rfc3339();
    let error_meta = serde_json::json!({
        "schema_version": 1,
        "error_code": "ORDINARY_QUESTION_SYNC_FAILED",
        "retryable": true,
        "safe_message": error.to_string(),
    })
    .to_string();
    conn.execute(
        "UPDATE exam_ordinary_question_source_syncs_v2
         SET state='failed',error_meta_json=?1,updated_at=?2 WHERE id=?3",
        params![error_meta, now, claim_id],
    )?;
    Ok(())
}

/// 在老师确认页面结构后自动调用。没有安全印刷层时返回跳过摘要，不影响批改。
pub fn sync_confirmed_printed_questions(
    conn: &Connection,
    page_id: i64,
    ai_run_id: i64,
    owner_id: &str,
) -> CoreResult<OrdinaryQuestionSyncSummary> {
    required(owner_id, "题库所有者")?;
    let (output, scope) = load_run_and_scope(conn, page_id, ai_run_id)?;
    let mut summary = empty_summary(
        if output.printed_questions.is_empty() {
            "no_printed_questions"
        } else {
            "no_safe_print_layer"
        },
        &scope,
        page_id,
        ai_run_id,
        &output.printed_questions,
    );
    if summary.eligible_count == 0 {
        return Ok(summary);
    }

    let extraction_version = extraction_version(&output)?;
    let claim_key = claim_key(&scope, owner_id, &extraction_version)?;
    if let Some(existing_claim) = existing_summary(conn, &claim_key)? {
        if existing_claim.state == "completed" {
            let mut existing: OrdinaryQuestionSyncSummary = serde_json::from_str(
                existing_claim
                    .summary_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("题面同步完成记录缺少摘要".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("题面同步摘要损坏：{error}")))?;
            existing.reused_existing_source = true;
            return Ok(existing);
        }
        if existing_claim.source_page_id != page_id || existing_claim.source_ai_run_id != ai_run_id
        {
            summary.state = format!("source_{}", existing_claim.state);
            summary.source_page_id = existing_claim.source_page_id;
            summary.source_ai_run_id = existing_claim.source_ai_run_id;
            summary.reused_existing_source = true;
            return Ok(summary);
        }
    }

    let now = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO exam_ordinary_question_source_syncs_v2
         (public_id,claim_key,assessment_version_id,page_no,owner_id,extraction_version,
          source_page_id,source_ai_run_id,state,created_at,updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'processing',?9,?9)
         ON CONFLICT(claim_key) DO NOTHING",
        params![
            ids::new_public_id(),
            &claim_key,
            scope.assessment_version_id,
            scope.page_no,
            owner_id.trim(),
            &extraction_version,
            page_id,
            ai_run_id,
            &now,
        ],
    )?;
    let selected_claim = existing_summary(conn, &claim_key)?
        .ok_or_else(|| CoreError::Db("题面来源 claim 未创建".into()))?;
    if selected_claim.source_page_id != page_id || selected_claim.source_ai_run_id != ai_run_id {
        summary.state = "source_processing".into();
        summary.source_page_id = selected_claim.source_page_id;
        summary.source_ai_run_id = selected_claim.source_ai_run_id;
        summary.reused_existing_source = true;
        return Ok(summary);
    }
    let claim_id = selected_claim.id;

    let result = (|| -> CoreResult<OrdinaryQuestionSyncSummary> {
        let bindings = load_bindings(
            conn,
            page_id,
            scope.assessment_version_id,
            &output.printed_questions,
        )?;
        let mut job_ids = Vec::new();
        for printed in &output.printed_questions {
            if printed.extraction_confidence < ORDINARY_PAPER_READY_CONFIDENCE {
                continue;
            }
            let binding = bindings
                .get(&printed.assessment_item_id)
                .ok_or_else(|| CoreError::Invalid("普通试卷印刷题面缺少当前题区绑定".into()))?;
            let draft = draft_for(printed, binding)?;
            let privacy = PrivacyScan {
                schema_version: printed.privacy.schema_version,
                sanitized: printed.privacy.sanitized,
                student_identity_detected: printed.privacy.student_identity_detected,
                student_answer_detected: printed.privacy.student_answer_detected,
                teacher_mark_detected: printed.privacy.teacher_mark_detected,
                score_detected: printed.privacy.score_detected,
            };
            let job = question_ingest::enqueue_question_ingest(
                conn,
                &NewQuestionIngest {
                    answer_region_revision_id: binding.region_revision_id,
                    owner_id: owner_id.trim(),
                    source_type: "student_paper",
                    extraction_version: &extraction_version,
                    draft: &draft,
                    privacy_scan: &privacy,
                    reusable_artifact_id: None,
                },
            )?;
            job_ids.push(job.id);
        }
        summary.enqueued_count = job_ids.len() as i64;

        for _ in 0..MAX_WORKER_STEPS {
            let remaining: i64 = conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM exam_question_ingest_jobs_v2
                     WHERE id IN ({}) AND state IN ('pending','processing')",
                    std::iter::repeat_n("?", job_ids.len())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
                rusqlite::params_from_iter(job_ids.iter()),
                |row| row.get(0),
            )?;
            if remaining == 0 {
                break;
            }
            match question_ingest::process_next_question_ingest(conn) {
                Ok(Some(_)) => {}
                Ok(None) => break,
                // 单个 job 已由 worker 记录为 failed；继续处理其余题目。
                Err(_) => {}
            }
        }

        for job_id in &job_ids {
            let row: (String, String, Option<String>) = conn.query_row(
                "SELECT j.state,j.privacy_status,c.status
                 FROM exam_question_ingest_jobs_v2 j
                 LEFT JOIN exam_question_candidates_v2 c ON c.job_id=j.id
                 WHERE j.id=?1",
                [job_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
            if row.1 == "rejected" {
                // 已在 empty_summary 中按 provider 声明计数，避免重复累加。
                continue;
            }
            match row.2.as_deref() {
                Some("matched") => summary.matched_count += 1,
                Some("candidate_created") => summary.candidate_created_count += 1,
                Some("needs_review") => summary.needs_review_count += 1,
                _ if row.0 == "failed" || row.0 == "pending" || row.0 == "processing" => {
                    summary.failed_count += 1;
                }
                _ => {}
            }
        }
        summary.state = if summary.failed_count == 0 {
            "completed".into()
        } else {
            "completed_with_failures".into()
        };
        Ok(summary)
    })();

    match result {
        Ok(summary) => {
            persist_completed(conn, claim_id, &summary)?;
            Ok(summary)
        }
        Err(error) => {
            persist_failed(conn, claim_id, &error)?;
            Err(error)
        }
    }
}
