//! 答案资料结构化候选的复核与一次确认。
//!
//! `ai_run` 保存不可变模型输出，逐题草稿复用 B3a 的答案权威候选表。老师可以确认
//! “与当前作业答案一致”、明确沿用当前答案，或把完整高置信冲突另存为新 K1/作业版本。
//! 无论选择哪条路径，当前 ingest batch 的 assessment version 都不会被原地改写。

use std::collections::BTreeSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::{ai_runs, audit};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRunStatus, AuditActorType};

use crate::answer_source_recognition::{
    AnswerSourceRecognitionOutput, AnswerSourceState, ANSWER_SOURCE_SCHEMA_VERSION,
};

use super::fixed_paper::{record_answer_authority_candidate, NewAnswerAuthorityCandidate};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSourceReviewItem {
    pub assessment_item_id: i64,
    pub order_index: i64,
    pub question_no: String,
    pub question_type: String,
    pub question_stem: String,
    pub bound_answer_key_version_id: i64,
    pub bound_answer_json: String,
    pub candidate_id: Option<i64>,
    pub candidate_answer_json: Option<String>,
    pub source_anchor_json: Option<String>,
    pub match_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSourceReviewSummary {
    pub ingest_batch_id: i64,
    pub source_ai_run_id: i64,
    pub source_state: String,
    pub route: String,
    pub matched_count: i64,
    pub conflict_count: i64,
    pub missing_count: i64,
    pub resolution: Option<String>,
    pub adoption: Option<AnswerSourceAdoptionSummary>,
    pub items: Vec<AnswerSourceReviewItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSourceAdoptionSummary {
    pub source_assessment_version_id: i64,
    pub adopted_assessment_version_id: i64,
    pub adopted_assessment_version_public_id: String,
    pub adopted_assessment_revision: i64,
    pub changed_item_count: i64,
    pub current_batch_unchanged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerSourcePreflightGate {
    Ready,
    ReviewRequired,
    Blocked,
}

fn parse_object(json: &str, label: &str) -> CoreResult<Value> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| CoreError::Parse(format!("{label}无法解析：{error}")))?;
    if !value.is_object()
        || value
            .get("schema_version")
            .and_then(Value::as_i64)
            .is_none()
    {
        return Err(CoreError::Parse(format!("{label}缺少 schema_version")));
    }
    Ok(value)
}

fn output_for_run(
    conn: &Connection,
    batch_id: i64,
    run_id: i64,
) -> CoreResult<AnswerSourceRecognitionOutput> {
    let run = ai_runs::get_by_id(conn, run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{run_id}")))?;
    if run.run_type != "answer_source_structure"
        || run.business_ref_type != "fixed_answer_source"
        || run.business_ref_id != batch_id.to_string()
        || run.status != AiRunStatus::Succeeded
    {
        return Err(CoreError::Invalid(
            "答案结构化 run 不属于当前批次或尚未成功".into(),
        ));
    }
    let output: AnswerSourceRecognitionOutput = serde_json::from_str(
        run.output_json
            .as_deref()
            .ok_or_else(|| CoreError::Invalid("答案结构化成功 run 缺少输出".into()))?,
    )
    .map_err(|error| CoreError::Parse(format!("答案结构化 run 输出无法解析：{error}")))?;
    if output.schema_version != ANSWER_SOURCE_SCHEMA_VERSION
        || output.ingest_batch_id != batch_id
        || output.source_artifact_id != run.input_artifact_id.unwrap_or_default()
    {
        return Err(CoreError::Invalid("答案结构化 run 输出身份链不一致".into()));
    }
    Ok(output)
}

pub fn materialize_ai_drafts(
    conn: &Connection,
    output: &AnswerSourceRecognitionOutput,
    source_ai_run_id: i64,
) -> CoreResult<()> {
    let source_artifact_id: i64 = conn.query_row(
        "SELECT source_artifact_id FROM exam_fixed_input_documents_v2
         WHERE ingest_batch_id=?1 AND document_role='answer_source' AND state<>'voided'",
        [output.ingest_batch_id],
        |row| row.get(0),
    )?;
    for entry in &output.entries {
        let mut anchor = entry.source_anchor.clone();
        let object = anchor
            .as_object_mut()
            .ok_or_else(|| CoreError::Invalid("答案来源锚点必须是对象".into()))?;
        object.insert("source_ai_run_id".into(), Value::from(source_ai_run_id));
        object.insert("confidence".into(), Value::from(entry.confidence));
        record_answer_authority_candidate(
            conn,
            &NewAnswerAuthorityCandidate {
                ingest_batch_id: output.ingest_batch_id,
                assessment_item_id: entry.assessment_item_id,
                source_kind: "ai_draft",
                answer_key_version_id: None,
                source_artifact_id: Some(source_artifact_id),
                candidate_answer_json: &entry.answer_json.to_string(),
                source_anchor_json: &anchor.to_string(),
                teacher_confirmed: false,
                idempotency_key: &format!(
                    "answer-source-run:{source_ai_run_id}:item:{}",
                    entry.assessment_item_id
                ),
                created_by_type: "system",
                created_by: None,
            },
        )?;
    }
    Ok(())
}

pub fn review_summary(
    conn: &Connection,
    batch_id: i64,
    source_ai_run_id: i64,
) -> CoreResult<AnswerSourceReviewSummary> {
    let output = output_for_run(conn, batch_id, source_ai_run_id)?;
    let resolution: Option<String> = conn
        .query_row(
            "SELECT decision FROM exam_answer_source_resolutions_v2 WHERE source_ai_run_id=?1",
            [source_ai_run_id],
            |row| row.get(0),
        )
        .optional()?;
    let adoption: Option<AnswerSourceAdoptionSummary> = conn
        .query_row(
            "SELECT d.source_assessment_version_id,d.adopted_assessment_version_id,
                    v.public_id,v.revision,d.changed_item_count
             FROM exam_answer_source_adoptions_v2 d
             JOIN exam_assessment_versions_v2 v ON v.id=d.adopted_assessment_version_id
             WHERE d.source_ai_run_id=?1",
            [source_ai_run_id],
            |row| {
                Ok(AnswerSourceAdoptionSummary {
                    source_assessment_version_id: row.get(0)?,
                    adopted_assessment_version_id: row.get(1)?,
                    adopted_assessment_version_public_id: row.get(2)?,
                    adopted_assessment_revision: row.get(3)?,
                    changed_item_count: row.get(4)?,
                    current_batch_unchanged: true,
                })
            },
        )
        .optional()?;
    let mut stmt = conn.prepare(
        "SELECT i.id,i.order_index,
                COALESCE(json_extract(i.presentation_snapshot_json,'$.question_no'),CAST(i.order_index+1 AS TEXT)),
                q.question_type,q.stem,i.answer_key_version_id,a.answer_json,
                c.id,c.candidate_answer_json,c.source_anchor_json
         FROM exam_ingest_batches_v2 b
         JOIN exam_assessment_items_v2 i
           ON i.assessment_version_id=b.assessment_version_id AND i.state='active'
         JOIN k1_question_versions q ON q.id=i.question_version_id
         JOIN k1_answer_key_versions a ON a.id=i.answer_key_version_id AND a.state='confirmed'
         LEFT JOIN exam_answer_authority_candidates_v2 c
           ON c.ingest_batch_id=b.id AND c.assessment_item_id=i.id
          AND c.source_kind='ai_draft'
          AND CAST(json_extract(c.source_anchor_json,'$.source_ai_run_id') AS INTEGER)=?2
         WHERE b.id=?1
         ORDER BY i.order_index,i.id",
    )?;
    let rows = stmt.query_map((batch_id, source_ai_run_id), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, Option<i64>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
        ))
    })?;
    let mut items = Vec::new();
    let mut matched_count = 0;
    let mut conflict_count = 0;
    let mut missing_count = 0;
    for row in rows {
        let (
            item_id,
            order_index,
            question_no,
            question_type,
            question_stem,
            bound_id,
            bound_json,
            candidate_id,
            candidate_json,
            anchor_json,
        ) = row?;
        let match_state = match candidate_json.as_deref() {
            None => {
                missing_count += 1;
                "missing"
            }
            Some(candidate)
                if parse_object(candidate, "答案候选")?
                    == parse_object(&bound_json, "当前答案")? =>
            {
                matched_count += 1;
                "matched"
            }
            Some(_) => {
                conflict_count += 1;
                "conflict"
            }
        };
        items.push(AnswerSourceReviewItem {
            assessment_item_id: item_id,
            order_index,
            question_no,
            question_type,
            question_stem,
            bound_answer_key_version_id: bound_id,
            bound_answer_json: bound_json,
            candidate_id,
            candidate_answer_json: candidate_json,
            source_anchor_json: anchor_json,
            match_state: match_state.into(),
        });
    }
    let source_state = match output.state {
        AnswerSourceState::Ready => "ready",
        AnswerSourceState::NeedsReview => "needs_review",
        AnswerSourceState::Blocked => "blocked",
    };
    let route = match (adoption.as_ref(), resolution.as_deref()) {
        (Some(_), _) => "adopted_new_version",
        (None, Some("confirmed_matches")) => "confirmed",
        (None, Some("kept_bound")) => "kept_bound",
        (None, _) if source_state != "ready" || conflict_count > 0 || missing_count > 0 => {
            "blocked"
        }
        (None, _) => "ready_to_confirm",
    };
    Ok(AnswerSourceReviewSummary {
        ingest_batch_id: batch_id,
        source_ai_run_id,
        source_state: source_state.into(),
        route: route.into(),
        matched_count,
        conflict_count,
        missing_count,
        resolution,
        adoption,
        items,
    })
}

fn write_resolution_rows(
    conn: &Connection,
    summary: &AnswerSourceReviewSummary,
    decision: &str,
    confirmed_by: &str,
) -> CoreResult<()> {
    if confirmed_by.trim().is_empty() {
        return Err(CoreError::Invalid("答案资料确认人不能为空".into()));
    }
    if let Some(existing) = summary.resolution.as_deref() {
        if existing == decision {
            return Ok(());
        }
        return Err(CoreError::Invalid(
            "该答案资料已经作出另一项不可变决议".into(),
        ));
    }
    if decision == "confirmed_matches"
        && (summary.route != "ready_to_confirm"
            || summary.conflict_count > 0
            || summary.missing_count > 0)
    {
        return Err(CoreError::Invalid("只有全部逐题一致时才能一次确认".into()));
    }
    if !matches!(decision, "confirmed_matches" | "kept_bound") {
        return Err(CoreError::Invalid("答案资料决议非法".into()));
    }
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let summary_json = serde_json::json!({
        "schema_version": 1,
        "source_ai_run_id": summary.source_ai_run_id,
        "decision": decision,
        "matched_count": summary.matched_count,
        "conflict_count": summary.conflict_count,
        "missing_count": summary.missing_count,
    })
    .to_string();
    let summary_hash = hashing::sha256_hex(summary_json.as_bytes());
    if decision == "confirmed_matches" {
        for item in &summary.items {
            let candidate_id = item
                .candidate_id
                .ok_or_else(|| CoreError::Invalid("一致答案缺少机器候选".into()))?;
            let (artifact_id, candidate_json, fingerprint, anchor_json): (i64, String, String, String) =
                conn.query_row(
                    "SELECT source_artifact_id,candidate_answer_json,answer_fingerprint,source_anchor_json
                     FROM exam_answer_authority_candidates_v2
                     WHERE id=?1 AND source_kind='ai_draft' AND state='active'",
                    [candidate_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
            conn.execute(
                "INSERT INTO exam_answer_authority_candidates_v2
                 (public_id,ingest_batch_id,assessment_item_id,source_kind,source_priority,
                  answer_key_version_id,source_artifact_id,candidate_answer_json,
                  answer_fingerprint,source_anchor_json,teacher_confirmed,idempotency_key,
                  state,created_by_type,created_by,created_at)
                 VALUES (?1,?2,?3,'uploaded_official',3,?4,?5,?6,?7,?8,1,?9,
                         'active','teacher',?10,?11)",
                params![
                    ids::new_public_id(),
                    summary.ingest_batch_id,
                    item.assessment_item_id,
                    item.bound_answer_key_version_id,
                    artifact_id,
                    candidate_json,
                    fingerprint,
                    anchor_json,
                    format!(
                        "answer-source-run:{}:confirmed-item:{}",
                        summary.source_ai_run_id, item.assessment_item_id
                    ),
                    confirmed_by.trim(),
                    &now,
                ],
            )?;
        }
    }
    conn.execute(
        "UPDATE exam_answer_authority_candidates_v2 SET state='rejected'
         WHERE ingest_batch_id=?1 AND source_kind='ai_draft' AND state='active'
           AND CAST(json_extract(source_anchor_json,'$.source_ai_run_id') AS INTEGER)=?2",
        (summary.ingest_batch_id, summary.source_ai_run_id),
    )?;
    conn.execute(
        "INSERT INTO exam_answer_source_resolutions_v2
         (public_id,ingest_batch_id,source_ai_run_id,decision,summary_hash,summary_json,
          confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        (
            &public_id,
            summary.ingest_batch_id,
            summary.source_ai_run_id,
            decision,
            &summary_hash,
            &summary_json,
            confirmed_by.trim(),
            &now,
        ),
    )?;
    audit::append(
        conn,
        &audit::NewAuditEvent {
            idempotency_key: &format!("exam:answer-source-resolution:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(confirmed_by.trim()),
            action: "exam.answer_source.resolved",
            object_type: "exam_answer_source_resolution",
            object_id: &public_id,
            object_revision: None,
            note: None,
            meta_json: Some(&summary_json),
            occurred_at: &now,
        },
    )?;
    Ok(())
}

fn write_resolution(
    conn: &mut Connection,
    summary: &AnswerSourceReviewSummary,
    decision: &str,
    confirmed_by: &str,
) -> CoreResult<()> {
    let tx = conn.transaction()?;
    write_resolution_rows(&tx, summary, decision, confirmed_by)?;
    tx.commit()?;
    Ok(())
}

#[derive(Debug)]
struct AdoptedAnswerSlot {
    stable_id: String,
    order_index: i64,
    canonical_answers_json: String,
    normalization_rules_json: Option<String>,
    max_score: f64,
}

#[derive(Serialize)]
struct AdoptedItemHashInput {
    item_id: i64,
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
    order_index: i64,
    score_millis: i64,
}

fn non_empty_string_array<'a>(value: &'a Value, field: &str) -> CoreResult<Vec<&'a str>> {
    let values = value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Invalid(format!("上传答案缺少 {field}")))?;
    let mut unique = BTreeSet::new();
    let mut result = Vec::new();
    for value in values {
        let text = value.as_str().map(str::trim).unwrap_or_default();
        if text.is_empty() || !unique.insert(text) {
            return Err(CoreError::Invalid(format!(
                "上传答案 {field} 含空值或重复值"
            )));
        }
        result.push(text);
    }
    if result.is_empty() {
        return Err(CoreError::Invalid(format!("上传答案 {field} 不能为空")));
    }
    Ok(result)
}

fn validate_adoptable_candidate(
    conn: &Connection,
    question_type: &str,
    question_version_id: i64,
    bound_answer_key_version_id: i64,
    candidate: &Value,
) -> CoreResult<Vec<AdoptedAnswerSlot>> {
    if candidate.get("schema_version").and_then(Value::as_i64) != Some(1) {
        return Err(CoreError::Invalid(
            "上传答案 schema_version 当前只接受 1".into(),
        ));
    }
    match question_type {
        "single" | "multiple" => {
            let labels = non_empty_string_array(candidate, "correct_labels")?;
            if question_type == "single" && labels.len() != 1 {
                return Err(CoreError::Invalid("单选题必须且只能有一个正确选项".into()));
            }
            if question_type == "multiple" && labels.len() < 2 {
                return Err(CoreError::Invalid("多选题至少需要两个正确选项".into()));
            }
            let mut stmt =
                conn.prepare("SELECT label FROM k1_question_options WHERE question_version_id=?1")?;
            let allowed = stmt
                .query_map([question_version_id], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<BTreeSet<_>>>()?;
            if allowed.is_empty() || labels.iter().any(|label| !allowed.contains(*label)) {
                return Err(CoreError::Invalid(
                    "上传答案包含题目中不存在的选项，请老师核对".into(),
                ));
            }
            Ok(Vec::new())
        }
        "true_false" => {
            if candidate.get("correct").and_then(Value::as_bool).is_none() {
                return Err(CoreError::Invalid("判断题上传答案必须明确为对或错".into()));
            }
            Ok(Vec::new())
        }
        "fill_blank" => {
            let slots = candidate
                .get("slots")
                .and_then(Value::as_array)
                .ok_or_else(|| CoreError::Invalid("填空题上传答案缺少答案槽位".into()))?;
            let mut stmt = conn.prepare(
                "SELECT stable_id,order_index,normalization_rules_json,max_score
                 FROM k1_answer_slots WHERE answer_key_version_id=?1
                 ORDER BY order_index,id",
            )?;
            let current = stmt
                .query_map([bound_answer_key_version_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, f64>(3)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if slots.is_empty() || slots.len() != current.len() {
                return Err(CoreError::Invalid(
                    "填空题上传答案槽位数与当前评分结构不一致，请老师补录确认".into(),
                ));
            }
            let mut result = Vec::with_capacity(slots.len());
            let mut seen = BTreeSet::new();
            for slot in slots {
                let order_index = slot
                    .get("order_index")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| CoreError::Invalid("填空题答案槽位缺少顺序".into()))?;
                if !seen.insert(order_index) {
                    return Err(CoreError::Invalid("填空题答案槽位顺序重复".into()));
                }
                let answers = non_empty_string_array(slot, "canonical_answers")?;
                let (stable_id, current_order, normalization, max_score) = current
                    .iter()
                    .find(|(_, current_order, _, _)| *current_order == order_index)
                    .ok_or_else(|| {
                        CoreError::Invalid("填空题上传答案槽位无法对应当前评分结构".into())
                    })?;
                result.push(AdoptedAnswerSlot {
                    stable_id: stable_id.clone(),
                    order_index: *current_order,
                    canonical_answers_json: serde_json::json!({
                        "schema_version": 1,
                        "answers": answers,
                    })
                    .to_string(),
                    normalization_rules_json: normalization.clone(),
                    max_score: *max_score,
                });
            }
            result.sort_by_key(|slot| slot.order_index);
            Ok(result)
        }
        "short_answer" => Err(CoreError::Invalid(
            "简答题答案会影响评分点与知识链接，请先由老师补录并确认评分点，不能一键采用".into(),
        )),
        _ => Err(CoreError::Invalid(
            "当前题型不能从上传答案一键建立新版本".into(),
        )),
    }
}

fn insert_confirmed_answer_version(
    conn: &Connection,
    question_version_id: i64,
    bound_answer_key_version_id: i64,
    answer_json: &str,
    slots: &[AdoptedAnswerSlot],
    confirmed_by: &str,
    now: &str,
) -> CoreResult<i64> {
    let revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_answer_key_versions
         WHERE question_version_id=?1",
        [question_version_id],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO k1_answer_key_versions
         (public_id,question_version_id,revision,answer_json,state,
          supersedes_answer_key_id,created_at,confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,'confirmed',?5,?6,?7,?6)",
        (
            ids::new_public_id(),
            question_version_id,
            revision,
            answer_json,
            bound_answer_key_version_id,
            now,
            confirmed_by,
        ),
    )?;
    let answer_key_version_id = conn.last_insert_rowid();
    for slot in slots {
        conn.execute(
            "INSERT INTO k1_answer_slots
             (public_id,stable_id,answer_key_version_id,order_index,
              canonical_answers_json,normalization_rules_json,max_score,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                ids::new_public_id(),
                &slot.stable_id,
                answer_key_version_id,
                slot.order_index,
                &slot.canonical_answers_json,
                slot.normalization_rules_json.as_deref(),
                slot.max_score,
                now,
            ],
        )?;
    }
    Ok(answer_key_version_id)
}

/// 将全部高置信冲突答案保存为新的 K1 答案版本和新的作业版本。
///
/// 当前 ingest batch 始终继续引用原作业版本；该操作同时把当前批次明确决议为
/// `kept_bound`，新版本只供之后新建批改批次使用。
pub fn adopt_conflicts_as_new_version(
    conn: &mut Connection,
    batch_id: i64,
    source_ai_run_id: i64,
    confirmed_by: &str,
) -> CoreResult<AnswerSourceReviewSummary> {
    if confirmed_by.trim().is_empty() {
        return Err(CoreError::Invalid("答案版本确认人不能为空".into()));
    }
    let summary = review_summary(conn, batch_id, source_ai_run_id)?;
    if summary.adoption.is_some() {
        return Ok(summary);
    }
    if summary.resolution.is_some() {
        return Err(CoreError::Invalid(
            "该答案资料已经选择沿用或确认，不能再创建另一套版本".into(),
        ));
    }
    if summary.source_state != "ready" || summary.missing_count > 0 || summary.conflict_count <= 0 {
        return Err(CoreError::Invalid(
            "只有来源完整、高置信且至少存在一项冲突时，才能建立新版本".into(),
        ));
    }

    let now = time::utc_now_rfc3339();
    let tx = conn.transaction()?;
    let (source_assessment_version_id, assessment_id, template_version, source_state): (
        i64,
        i64,
        Option<String>,
        String,
    ) = tx.query_row(
        "SELECT b.assessment_version_id,v.assessment_id,v.template_version,v.state
         FROM exam_ingest_batches_v2 b
         JOIN exam_assessment_versions_v2 v ON v.id=b.assessment_version_id
         WHERE b.id=?1",
        [batch_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    if source_state != "confirmed" {
        return Err(CoreError::Invalid(
            "只有已确认的作业版本才能派生新答案版本".into(),
        ));
    }
    let adopted_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_assessment_versions_v2
         WHERE assessment_id=?1",
        [assessment_id],
        |row| row.get(0),
    )?;
    let adopted_public_id = ids::new_public_id();
    tx.execute(
        "INSERT INTO exam_assessment_versions_v2
         (public_id,assessment_id,revision,template_version,state,supersedes_version_id,created_at)
         VALUES (?1,?2,?3,?4,'draft',?5,?6)",
        (
            &adopted_public_id,
            assessment_id,
            adopted_revision,
            template_version.as_deref(),
            source_assessment_version_id,
            &now,
        ),
    )?;
    let adopted_assessment_version_id = tx.last_insert_rowid();

    let mut stmt = tx.prepare(
        "SELECT i.id,i.question_version_id,i.answer_key_version_id,i.rubric_version_id,
                i.link_set_id,i.order_index,i.score,i.option_order_json,
                i.presentation_snapshot_json,q.question_type
         FROM exam_assessment_items_v2 i
         JOIN k1_question_versions q ON q.id=i.question_version_id
         WHERE i.assessment_version_id=?1 AND i.state='active'
         ORDER BY i.order_index,i.id",
    )?;
    let rows = stmt
        .query_map([source_assessment_version_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, f64>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut hash_items = Vec::with_capacity(rows.len());
    let mut changed_items = Vec::new();
    for (
        source_item_id,
        question_version_id,
        bound_answer_key_version_id,
        rubric_version_id,
        link_set_id,
        order_index,
        score,
        option_order_json,
        presentation_snapshot_json,
        question_type,
    ) in rows
    {
        let review_item = summary
            .items
            .iter()
            .find(|item| item.assessment_item_id == source_item_id)
            .ok_or_else(|| CoreError::Invalid("答案核对题目集合与作业版本不一致".into()))?;
        let answer_key_version_id = if review_item.match_state == "conflict" {
            let candidate_json = review_item
                .candidate_answer_json
                .as_deref()
                .ok_or_else(|| CoreError::Invalid("冲突项缺少上传答案候选".into()))?;
            let candidate = parse_object(candidate_json, "上传答案")?;
            let slots = validate_adoptable_candidate(
                &tx,
                &question_type,
                question_version_id,
                bound_answer_key_version_id,
                &candidate,
            )?;
            let new_answer_key_version_id = insert_confirmed_answer_version(
                &tx,
                question_version_id,
                bound_answer_key_version_id,
                &candidate.to_string(),
                &slots,
                confirmed_by.trim(),
                &now,
            )?;
            changed_items.push(serde_json::json!({
                "source_assessment_item_id": source_item_id,
                "question_version_id": question_version_id,
                "old_answer_key_version_id": bound_answer_key_version_id,
                "new_answer_key_version_id": new_answer_key_version_id,
            }));
            new_answer_key_version_id
        } else {
            bound_answer_key_version_id
        };
        tx.execute(
            "INSERT INTO exam_assessment_items_v2
             (public_id,assessment_version_id,question_version_id,answer_key_version_id,
              rubric_version_id,link_set_id,order_index,score,option_order_json,
              presentation_snapshot_json,state,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11)",
            params![
                ids::new_public_id(),
                adopted_assessment_version_id,
                question_version_id,
                answer_key_version_id,
                rubric_version_id,
                link_set_id,
                order_index,
                score,
                option_order_json.as_deref(),
                &presentation_snapshot_json,
                &now,
            ],
        )?;
        hash_items.push(AdoptedItemHashInput {
            item_id: tx.last_insert_rowid(),
            question_version_id,
            answer_key_version_id,
            rubric_version_id,
            link_set_id,
            order_index,
            score_millis: (score * 1000.0).round() as i64,
        });
    }
    if changed_items.len() as i64 != summary.conflict_count {
        return Err(CoreError::Invalid(
            "上传冲突项没有全部进入新版本，已回滚".into(),
        ));
    }
    let item_set_hash = hashing::sha256_hex(
        &serde_json::to_vec(&hash_items)
            .map_err(|error| CoreError::Parse(format!("新作业版本 hash 失败：{error}")))?,
    );
    tx.execute(
        "UPDATE exam_assessment_versions_v2
         SET item_set_hash=?1,state='confirmed',confirmed_by=?2,confirmed_at=?3
         WHERE id=?4 AND state='draft'",
        (
            &item_set_hash,
            confirmed_by.trim(),
            &now,
            adopted_assessment_version_id,
        ),
    )?;
    tx.execute(
        "UPDATE exam_assessments_v2 SET updated_at=?1 WHERE id=?2",
        (&now, assessment_id),
    )?;

    let adoption_public_id = ids::new_public_id();
    let details_json = serde_json::json!({
        "schema_version": 1,
        "source_ai_run_id": source_ai_run_id,
        "source_assessment_version_id": source_assessment_version_id,
        "adopted_assessment_version_id": adopted_assessment_version_id,
        "current_batch_unchanged": true,
        "changed_items": changed_items,
    })
    .to_string();
    tx.execute(
        "INSERT INTO exam_answer_source_adoptions_v2
         (public_id,ingest_batch_id,source_ai_run_id,source_assessment_version_id,
          adopted_assessment_version_id,changed_item_count,details_json,confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        (
            &adoption_public_id,
            batch_id,
            source_ai_run_id,
            source_assessment_version_id,
            adopted_assessment_version_id,
            summary.conflict_count,
            &details_json,
            confirmed_by.trim(),
            &now,
        ),
    )?;
    audit::append(
        &tx,
        &audit::NewAuditEvent {
            idempotency_key: &format!("exam:answer-source-adoption:{adoption_public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(confirmed_by.trim()),
            action: "exam.answer_source.adopted_new_version",
            object_type: "exam_assessment_version",
            object_id: &adopted_public_id,
            object_revision: Some(adopted_revision),
            note: Some("当前批次继续引用原作业版本"),
            meta_json: Some(&details_json),
            occurred_at: &now,
        },
    )?;
    write_resolution_rows(&tx, &summary, "kept_bound", confirmed_by.trim())?;
    tx.commit()?;
    review_summary(conn, batch_id, source_ai_run_id)
}

pub fn confirm_matches(
    conn: &mut Connection,
    batch_id: i64,
    source_ai_run_id: i64,
    confirmed_by: &str,
) -> CoreResult<AnswerSourceReviewSummary> {
    let summary = review_summary(conn, batch_id, source_ai_run_id)?;
    write_resolution(conn, &summary, "confirmed_matches", confirmed_by)?;
    review_summary(conn, batch_id, source_ai_run_id)
}

pub fn keep_bound_answers(
    conn: &mut Connection,
    batch_id: i64,
    source_ai_run_id: i64,
    confirmed_by: &str,
) -> CoreResult<AnswerSourceReviewSummary> {
    let summary = review_summary(conn, batch_id, source_ai_run_id)?;
    write_resolution(conn, &summary, "kept_bound", confirmed_by)?;
    review_summary(conn, batch_id, source_ai_run_id)
}

pub fn latest_preflight_gate(
    conn: &Connection,
    batch_id: i64,
) -> CoreResult<(AnswerSourcePreflightGate, Option<&'static str>)> {
    let has_source: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_fixed_input_documents_v2
         WHERE ingest_batch_id=?1 AND document_role='answer_source' AND state<>'voided')",
        [batch_id],
        |row| row.get(0),
    )?;
    if !has_source {
        return Ok((AnswerSourcePreflightGate::Ready, None));
    }
    let (item_count, confirmed_uploaded_count): (i64, i64) = conn.query_row(
        "SELECT COUNT(DISTINCT i.id),
                COUNT(DISTINCT CASE WHEN c.teacher_confirmed=1 AND c.state='active' THEN i.id END)
         FROM exam_ingest_batches_v2 b
         JOIN exam_assessment_items_v2 i
           ON i.assessment_version_id=b.assessment_version_id AND i.state='active'
         LEFT JOIN exam_answer_authority_candidates_v2 c
           ON c.ingest_batch_id=b.id AND c.assessment_item_id=i.id
          AND c.source_kind='uploaded_official'
         WHERE b.id=?1",
        [batch_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if item_count > 0 && confirmed_uploaded_count == item_count {
        return Ok((AnswerSourcePreflightGate::Ready, None));
    }
    let latest: Option<(i64, String)> = conn
        .query_row(
            "SELECT id,status FROM ai_runs
             WHERE run_type='answer_source_structure'
               AND business_ref_type='fixed_answer_source' AND business_ref_id=?1
             ORDER BY id DESC LIMIT 1",
            [batch_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((run_id, status)) = latest else {
        return Ok((
            AnswerSourcePreflightGate::Blocked,
            Some("ANSWER_SOURCE_STRUCTURE_PENDING"),
        ));
    };
    if status != "succeeded" {
        return Ok((
            AnswerSourcePreflightGate::Blocked,
            Some("ANSWER_SOURCE_STRUCTURE_FAILED"),
        ));
    }
    let summary = review_summary(conn, batch_id, run_id)?;
    match summary.route.as_str() {
        "confirmed" | "kept_bound" | "adopted_new_version" => {
            Ok((AnswerSourcePreflightGate::Ready, None))
        }
        "ready_to_confirm" => Ok((
            AnswerSourcePreflightGate::ReviewRequired,
            Some("ANSWER_SOURCE_CONFIRMATION_REQUIRED"),
        )),
        _ => Ok((
            AnswerSourcePreflightGate::Blocked,
            Some("ANSWER_SOURCE_CONFLICT_OR_MISSING"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::answer_source_recognition::{AnswerSourceEntry, AnswerSourceRecognizerDescriptor};
    use crate::service::fixed_paper::{register_fixed_input_document, NewFixedInputDocument};
    use suite_core::db::repo::artifacts;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

    struct Fixture {
        conn: Connection,
        batch_id: i64,
        run_id: i64,
    }

    fn fixture(candidate_correct: bool) -> Fixture {
        fixture_with_candidate(
            "true_false",
            serde_json::json!({"schema_version":1,"correct":candidate_correct}),
        )
    }

    fn fixture_with_candidate(question_type: &str, candidate_answer: Value) -> Fixture {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        let hash = "a".repeat(64);
        conn.execute_batch(&format!(
            r#"INSERT INTO subjects(name) VALUES ('历史');
               INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
               INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('answer-source-edition',1,'PEP','2024','中国历史八上','8','upper',
                         'active','2026-07-15T10:00:00Z');
               INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('answer-source-map',1,1,'confirmed','2026-07-15T10:00:00Z',
                         '2026-07-15T10:00:00Z');
               INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('answer-source-q','personal','teacher','unknown',0,'2026-07-15T10:00:00Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES ('answer-source-qv',1,1,'{question_type}','鸦片战争爆发于1840年。',1,
                         '{hash}','L2','published','2026-07-15T10:00:00Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-source-ak',1,1,'{{"schema_version":1,"correct":true}}',
                         'confirmed','2026-07-15T10:00:00Z','teacher','2026-07-15T10:00:00Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-source-r',1,1,1,'confirmed','2026-07-15T10:00:00Z',
                         'teacher','2026-07-15T10:00:00Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-source-links',1,1,1,'confirmed','2026-07-15T10:00:00Z',
                         'teacher','2026-07-15T10:00:00Z');
               INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,
                  created_by,created_at,updated_at)
                 VALUES ('answer-source-a','答案资料作业',1,'quiz','include','active','teacher',
                         '2026-07-15T10:00:00Z','2026-07-15T10:00:00Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,
                  created_at,confirmed_by,confirmed_at)
                 VALUES ('answer-source-av',1,1,'{hash}','answer-source-template','confirmed',
                         '2026-07-15T10:00:00Z','teacher','2026-07-15T10:00:00Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,state,created_at)
                 VALUES ('answer-source-item',1,1,1,1,1,0,1,
                         '{{"schema_version":1,"question_no":"1"}}','active',
                         '2026-07-15T10:00:00Z');
               INSERT INTO exam_ingest_batches_v2
                 (public_id,assessment_version_id,source_kind,idempotency_key,state,
                  created_by,created_at,updated_at)
                 VALUES ('answer-source-batch',1,'image_folder','answer-source-batch','processing',
                         'teacher','2026-07-15T10:00:00Z','2026-07-15T10:00:00Z');"#
        ))
        .unwrap();
        let artifact = artifacts::create_or_get(
            &conn,
            &artifacts::NewArtifact {
                kind: ArtifactKind::Document,
                sha256: &"b".repeat(64),
                mime_type: "text/plain",
                byte_size: 4,
                original_name: Some("answer.txt"),
                original_path: None,
                archived_path: "/tmp/answer-source.txt",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: "answer-source-test-v1",
                privacy_class: PrivacyClass::TeachingContent,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        register_fixed_input_document(
            &conn,
            &NewFixedInputDocument {
                ingest_batch_id: 1,
                source_artifact_id: artifact.id,
                document_role: "answer_source",
                source_format: "text",
                import_index: 0,
                page_count: 1,
                idempotency_key: "answer-source-doc",
                created_by: "teacher",
            },
        )
        .unwrap();
        let descriptor = AnswerSourceRecognizerDescriptor {
            provider: "fixture".into(),
            model_name: "fixture".into(),
            model_version: "v1".into(),
            config_version: "v1".into(),
            rule_version: "v1".into(),
        };
        let output = AnswerSourceRecognitionOutput {
            schema_version: 1,
            ingest_batch_id: 1,
            source_artifact_id: artifact.id,
            source_artifact_sha256: artifact.sha256.clone(),
            input_hash: "c".repeat(64),
            descriptor,
            state: AnswerSourceState::Ready,
            entries: vec![AnswerSourceEntry {
                assessment_item_id: 1,
                answer_json: candidate_answer,
                source_anchor: serde_json::json!({"schema_version":1,"line":1}),
                confidence: 0.99,
            }],
            confidence: 0.99,
            issue_codes: vec![],
        };
        let run = ai_runs::create_or_get(
            &conn,
            &ai_runs::NewAiRun {
                idempotency_key: "answer-source-run",
                run_type: "answer_source_structure",
                source_module: "exam",
                business_ref_type: "fixed_answer_source",
                business_ref_id: "1",
                input_artifact_id: Some(artifact.id),
                provider: "fixture",
                model_name: "fixture",
                model_version: "v1",
                config_version: "v1",
                prompt_or_rule_version: "v1",
                input_hash: &output.input_hash,
                retry_of_ai_run_id: None,
            },
        )
        .unwrap();
        ai_runs::start(&conn, run.id, "2026-07-15T10:00:01Z", None).unwrap();
        let output_json = serde_json::to_string(&output).unwrap();
        ai_runs::finalize_succeeded(
            &conn,
            run.id,
            &hashing::sha256_hex(output_json.as_bytes()),
            Some(0.99),
            &output_json,
            "2026-07-15T10:00:02Z",
        )
        .unwrap();
        materialize_ai_drafts(&conn, &output, run.id).unwrap();
        Fixture {
            conn,
            batch_id: 1,
            run_id: run.id,
        }
    }

    #[test]
    fn matching_source_requires_one_teacher_confirmation_then_reuses_bound_k1() {
        let mut fixture = fixture(true);
        let summary = review_summary(&fixture.conn, fixture.batch_id, fixture.run_id).unwrap();
        assert_eq!(summary.route, "ready_to_confirm");
        assert_eq!(
            latest_preflight_gate(&fixture.conn, fixture.batch_id)
                .unwrap()
                .0,
            AnswerSourcePreflightGate::ReviewRequired
        );
        let confirmed = confirm_matches(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
        )
        .unwrap();
        assert_eq!(confirmed.route, "confirmed");
        let official: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_answer_authority_candidates_v2
                 WHERE source_kind='uploaded_official' AND teacher_confirmed=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(official, 1);
        assert_eq!(
            latest_preflight_gate(&fixture.conn, fixture.batch_id)
                .unwrap()
                .0,
            AnswerSourcePreflightGate::Ready
        );
        let effects: (i64, i64) = fixture
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_publications_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(effects, (0, 0));
    }

    #[test]
    fn conflict_stays_blocked_until_teacher_explicitly_keeps_bound_answer() {
        let mut fixture = fixture(false);
        let summary = review_summary(&fixture.conn, fixture.batch_id, fixture.run_id).unwrap();
        assert_eq!(summary.route, "blocked");
        assert_eq!(summary.conflict_count, 1);
        assert!(confirm_matches(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher"
        )
        .is_err());
        let resolved = keep_bound_answers(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
        )
        .unwrap();
        assert_eq!(resolved.route, "kept_bound");
        assert_eq!(
            latest_preflight_gate(&fixture.conn, fixture.batch_id)
                .unwrap()
                .0,
            AnswerSourcePreflightGate::Ready
        );
    }

    #[test]
    fn conflict_can_create_new_k1_and_assessment_version_without_rebinding_current_batch() {
        let mut fixture = fixture(false);
        let adopted = adopt_conflicts_as_new_version(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
        )
        .unwrap();
        assert_eq!(adopted.route, "adopted_new_version");
        let adoption = adopted.adoption.clone().unwrap();
        assert_eq!(adoption.source_assessment_version_id, 1);
        assert_eq!(adoption.adopted_assessment_revision, 2);
        assert_eq!(adoption.changed_item_count, 1);
        assert!(adoption.current_batch_unchanged);
        assert_eq!(adopted.resolution.as_deref(), Some("kept_bound"));

        let batch_version: i64 = fixture
            .conn
            .query_row(
                "SELECT assessment_version_id FROM exam_ingest_batches_v2 WHERE id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(batch_version, 1);
        let adopted_state: (String, bool) = fixture
            .conn
            .query_row(
                "SELECT state,item_set_hash IS NOT NULL FROM exam_assessment_versions_v2 WHERE id=?1",
                [adoption.adopted_assessment_version_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(adopted_state, ("confirmed".into(), true));
        let (new_answer_id, new_answer_json, supersedes): (i64, String, i64) = fixture
            .conn
            .query_row(
                "SELECT i.answer_key_version_id,a.answer_json,a.supersedes_answer_key_id
                 FROM exam_assessment_items_v2 i
                 JOIN k1_answer_key_versions a ON a.id=i.answer_key_version_id
                 WHERE i.assessment_version_id=?1",
                [adoption.adopted_assessment_version_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_ne!(new_answer_id, 1);
        assert_eq!(supersedes, 1);
        assert_eq!(
            serde_json::from_str::<Value>(&new_answer_json).unwrap(),
            serde_json::json!({"schema_version":1,"correct":false})
        );
        let old_answer_json: String = fixture
            .conn
            .query_row(
                "SELECT answer_json FROM k1_answer_key_versions WHERE id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&old_answer_json).unwrap(),
            serde_json::json!({"schema_version":1,"correct":true})
        );
        let effects: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_publications_v2),
                        (SELECT COUNT(*) FROM learning_evidence)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(effects, (0, 0, 0));
        assert_eq!(
            latest_preflight_gate(&fixture.conn, fixture.batch_id)
                .unwrap()
                .0,
            AnswerSourcePreflightGate::Ready
        );

        let repeated = adopt_conflicts_as_new_version(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
        )
        .unwrap();
        assert_eq!(
            repeated
                .adoption
                .as_ref()
                .unwrap()
                .adopted_assessment_version_id,
            adoption.adopted_assessment_version_id
        );
        let counts: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                        (SELECT COUNT(*) FROM k1_answer_key_versions),
                        (SELECT COUNT(*) FROM exam_answer_source_adoptions_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(counts, (2, 2, 1));
        assert!(fixture
            .conn
            .execute(
                "UPDATE exam_answer_source_adoptions_v2 SET changed_item_count=2 WHERE id=1",
                [],
            )
            .is_err());
    }

    #[test]
    fn short_answer_conflict_requires_teacher_rubric_instead_of_partial_version() {
        let mut fixture = fixture_with_candidate(
            "short_answer",
            serde_json::json!({
                "schema_version": 1,
                "reference_answer": "需要新的参考答案",
                "rubric_points": [{"canonical_text":"评分点一","max_score":1}],
            }),
        );
        let error = adopt_conflicts_as_new_version(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
        )
        .unwrap_err();
        assert!(error.to_string().contains("评分点"));
        let counts: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                        (SELECT COUNT(*) FROM k1_answer_key_versions),
                        (SELECT COUNT(*) FROM exam_answer_source_adoptions_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(counts, (1, 1, 0));
    }

    #[test]
    fn fill_blank_adoption_preserves_slot_identity_and_scoring_rules() {
        let mut fixture = fixture_with_candidate(
            "fill_blank",
            serde_json::json!({
                "schema_version": 1,
                "slots": [{
                    "order_index": 0,
                    "canonical_answers": ["1842年", "一八四二年"]
                }]
            }),
        );
        fixture
            .conn
            .execute(
                "INSERT INTO k1_answer_slots
                 (public_id,stable_id,answer_key_version_id,order_index,
                  canonical_answers_json,normalization_rules_json,max_score,created_at)
                 VALUES ('old-slot-public','stable-slot',1,0,
                         '{\"schema_version\":1,\"answers\":[\"1842\"]}',
                         '{\"schema_version\":1,\"trim\":true}',1,'2026-07-15T10:00:00Z')",
                [],
            )
            .unwrap();
        let adopted = adopt_conflicts_as_new_version(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
        )
        .unwrap();
        let new_version = adopted.adoption.unwrap().adopted_assessment_version_id;
        let (stable_id, canonical, normalization, max_score): (String, String, String, f64) =
            fixture
                .conn
                .query_row(
                    "SELECT s.stable_id,s.canonical_answers_json,s.normalization_rules_json,s.max_score
                     FROM exam_assessment_items_v2 i
                     JOIN k1_answer_slots s ON s.answer_key_version_id=i.answer_key_version_id
                     WHERE i.assessment_version_id=?1",
                    [new_version],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .unwrap();
        assert_eq!(stable_id, "stable-slot");
        assert_eq!(
            serde_json::from_str::<Value>(&canonical).unwrap(),
            serde_json::json!({"schema_version":1,"answers":["1842年","一八四二年"]})
        );
        assert_eq!(
            serde_json::from_str::<Value>(&normalization).unwrap(),
            serde_json::json!({"schema_version":1,"trim":true})
        );
        assert_eq!(max_score, 1.0);
    }
}
