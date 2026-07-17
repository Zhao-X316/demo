//! 答案资料结构化候选的复核与一次确认。
//!
//! `ai_run` 保存不可变模型输出，逐题草稿复用 B3a 的答案权威候选表。老师可以确认
//! “与当前作业答案一致”、明确沿用当前答案，或把完整高置信冲突另存为新 K1/作业版本。
//! 无论选择哪条路径，当前 ingest batch 的 assessment version 都不会被原地改写。

use std::collections::{BTreeMap, BTreeSet};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSourceReviewItem {
    pub assessment_item_id: i64,
    pub order_index: i64,
    pub question_no: String,
    pub question_type: String,
    pub question_stem: String,
    pub bound_answer_key_version_id: i64,
    pub bound_answer_json: String,
    pub bound_rubric_version_id: i64,
    pub bound_link_set_id: i64,
    pub bound_rubric_points: Vec<AnswerSourceRubricPointReview>,
    pub candidate_id: Option<i64>,
    pub candidate_answer_json: Option<String>,
    pub source_anchor_json: Option<String>,
    pub match_state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSourceRubricPointReview {
    pub stable_id: String,
    pub order_index: i64,
    pub canonical_text: String,
    pub max_score: f64,
    pub confirmed_knowledge_titles: Vec<String>,
    pub confirmed_ability_titles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub changed_rubric_count: i64,
    pub carried_knowledge_link_count: i64,
    pub carried_ability_link_count: i64,
    pub new_rubric_point_count: i64,
    pub retired_rubric_point_count: i64,
    pub unlinked_new_rubric_point_count: i64,
    pub dropped_knowledge_link_count: i64,
    pub dropped_ability_link_count: i64,
    pub current_batch_unchanged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RubricPointMappingAction {
    ReuseExisting,
    NewPoint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RubricPointMappingInput {
    pub assessment_item_id: i64,
    pub candidate_order_index: i64,
    pub action: RubricPointMappingAction,
    pub previous_stable_id: Option<String>,
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

fn split_titles(value: String) -> Vec<String> {
    value
        .split('\u{1f}')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

fn rubric_points_for_review(
    conn: &Connection,
    rubric_version_id: i64,
    link_set_id: i64,
) -> CoreResult<Vec<AnswerSourceRubricPointReview>> {
    let mut stmt = conn.prepare(
        "SELECT rp.stable_id,rp.order_index,rp.canonical_text,rp.max_score,
                COALESCE((SELECT group_concat(kn.title,CHAR(31))
                          FROM k1_knowledge_links kl
                          JOIN k1_knowledge_nodes kn ON kn.id=kl.knowledge_node_id
                          WHERE kl.link_set_id=?2 AND kl.source_type='rubric_point'
                            AND kl.source_public_id=rp.public_id
                            AND kl.confirmation_level='teacher_confirmed'),''),
                COALESCE((SELECT group_concat(ad.title,CHAR(31))
                          FROM k1_ability_links al
                          JOIN k1_ability_dimensions ad ON ad.id=al.ability_dimension_id
                          WHERE al.link_set_id=?2 AND al.source_type='rubric_point'
                            AND al.source_public_id=rp.public_id
                            AND al.confirmation_level='teacher_confirmed'),'')
         FROM k1_rubric_points rp
         WHERE rp.rubric_version_id=?1 ORDER BY rp.order_index,rp.id",
    )?;
    let rows = stmt.query_map((rubric_version_id, link_set_id), |row| {
        Ok(AnswerSourceRubricPointReview {
            stable_id: row.get(0)?,
            order_index: row.get(1)?,
            canonical_text: row.get(2)?,
            max_score: row.get(3)?,
            confirmed_knowledge_titles: split_titles(row.get(4)?),
            confirmed_ability_titles: split_titles(row.get(5)?),
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
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
                    v.public_id,v.revision,d.changed_item_count,d.details_json
             FROM exam_answer_source_adoptions_v2 d
             JOIN exam_assessment_versions_v2 v ON v.id=d.adopted_assessment_version_id
             WHERE d.source_ai_run_id=?1",
            [source_ai_run_id],
            |row| {
                let details_json: String = row.get(5)?;
                let details: Value = serde_json::from_str(&details_json).unwrap_or(Value::Null);
                Ok(AnswerSourceAdoptionSummary {
                    source_assessment_version_id: row.get(0)?,
                    adopted_assessment_version_id: row.get(1)?,
                    adopted_assessment_version_public_id: row.get(2)?,
                    adopted_assessment_revision: row.get(3)?,
                    changed_item_count: row.get(4)?,
                    changed_rubric_count: details
                        .get("changed_rubric_count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    carried_knowledge_link_count: details
                        .get("carried_knowledge_link_count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    carried_ability_link_count: details
                        .get("carried_ability_link_count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    new_rubric_point_count: details
                        .get("new_rubric_point_count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    retired_rubric_point_count: details
                        .get("retired_rubric_point_count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    unlinked_new_rubric_point_count: details
                        .get("unlinked_new_rubric_point_count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    dropped_knowledge_link_count: details
                        .get("dropped_knowledge_link_count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    dropped_ability_link_count: details
                        .get("dropped_ability_link_count")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    current_batch_unchanged: true,
                })
            },
        )
        .optional()?;
    let mut stmt = conn.prepare(
        "SELECT i.id,i.order_index,
                COALESCE(json_extract(i.presentation_snapshot_json,'$.question_no'),CAST(i.order_index+1 AS TEXT)),
                q.question_type,q.stem,i.answer_key_version_id,a.answer_json,
                i.rubric_version_id,i.link_set_id,
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
            row.get::<_, i64>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, Option<i64>>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, Option<String>>(11)?,
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
            bound_rubric_version_id,
            bound_link_set_id,
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
            bound_rubric_version_id,
            bound_link_set_id,
            bound_rubric_points: rubric_points_for_review(
                conn,
                bound_rubric_version_id,
                bound_link_set_id,
            )?,
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

#[derive(Debug)]
struct AdoptedRubricPoint {
    stable_id: String,
    old_public_id: Option<String>,
    order_index: i64,
    canonical_text: String,
    allowed_paraphrases_json: Option<String>,
    required_concepts_json: Option<String>,
    max_score: f64,
}

#[derive(Debug)]
struct RetiredRubricPoint {
    old_public_id: String,
}

#[derive(Debug)]
struct ValidatedRubricChange {
    points: Vec<AdoptedRubricPoint>,
    retired_points: Vec<RetiredRubricPoint>,
    new_point_count: i64,
}

#[derive(Debug)]
struct InsertedRubricPointMapping {
    candidate_order_index: i64,
    previous_rubric_point_public_id: Option<String>,
    adopted_rubric_point_public_id: String,
}

#[derive(Debug)]
struct PendingRubricMappingLedgerRow {
    mapping_action: &'static str,
    candidate_order_index: Option<i64>,
    previous_rubric_point_public_id: Option<String>,
    adopted_rubric_point_public_id: Option<String>,
}

#[derive(Debug)]
struct RubricMappingLedgerRow {
    source_assessment_item_id: i64,
    adopted_assessment_item_id: i64,
    mapping_action: &'static str,
    candidate_order_index: Option<i64>,
    previous_rubric_point_public_id: Option<String>,
    adopted_rubric_point_public_id: Option<String>,
}

#[derive(Debug, Default)]
struct CarriedLinkCounts {
    knowledge: i64,
    ability: i64,
    dropped_knowledge: i64,
    dropped_ability: i64,
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

fn optional_string_array_json(value: &Value, field: &str) -> CoreResult<Option<String>> {
    let Some(values) = value.get(field) else {
        return Ok(None);
    };
    let values = values
        .as_array()
        .ok_or_else(|| CoreError::Invalid(format!("上传评分点 {field} 必须是数组")))?;
    let mut unique = BTreeSet::new();
    let mut result = Vec::new();
    for value in values {
        let text = value.as_str().map(str::trim).unwrap_or_default();
        if text.is_empty() || !unique.insert(text) {
            return Err(CoreError::Invalid(format!(
                "上传评分点 {field} 含空值或重复值"
            )));
        }
        result.push(text);
    }
    if result.is_empty() {
        Ok(None)
    } else {
        Ok(Some(serde_json::to_string(&result).map_err(|error| {
            CoreError::Parse(format!("上传评分点 {field} 序列化失败：{error}"))
        })?))
    }
}

fn validate_short_answer_candidate(
    conn: &Connection,
    bound_rubric_version_id: i64,
    item_score: f64,
    candidate: &Value,
    mappings: &[RubricPointMappingInput],
) -> CoreResult<ValidatedRubricChange> {
    let reference_answer = candidate
        .get("reference_answer")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if reference_answer.is_empty() {
        return Err(CoreError::Invalid("简答题上传答案缺少参考答案".into()));
    }
    let candidate_points = candidate
        .get("rubric_points")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Invalid("简答题上传答案缺少评分点".into()))?;
    let (rubric_score, rubric_state): (f64, String) = conn.query_row(
        "SELECT max_score,state FROM k1_rubric_versions WHERE id=?1",
        [bound_rubric_version_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if rubric_state != "confirmed" || (rubric_score - item_score).abs() > 0.000_001 {
        return Err(CoreError::Invalid(
            "当前简答题 rubric 尚未确认或分值与作业不一致".into(),
        ));
    }
    let mut stmt = conn.prepare(
        "SELECT stable_id,public_id,order_index FROM k1_rubric_points
         WHERE rubric_version_id=?1 ORDER BY order_index,id",
    )?;
    let current_points = stmt
        .query_map([bound_rubric_version_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if candidate_points.is_empty() {
        return Err(CoreError::Invalid("上传简答评分点不能为空".into()));
    }
    let automatic_order_mapping = mappings.is_empty();
    if automatic_order_mapping && candidate_points.len() != current_points.len() {
        return Err(CoreError::Invalid(
            "上传简答评分点数量发生变化，请逐点选择沿用旧评分点或新增评分点".into(),
        ));
    }
    let mut mappings_by_order = BTreeMap::new();
    if !automatic_order_mapping {
        if mappings.len() != candidate_points.len() {
            return Err(CoreError::Invalid(
                "评分点结构映射必须覆盖上传答案的每一个评分点".into(),
            ));
        }
        for mapping in mappings {
            if mapping.candidate_order_index < 0
                || mappings_by_order
                    .insert(mapping.candidate_order_index, mapping)
                    .is_some()
            {
                return Err(CoreError::Invalid(
                    "评分点结构映射包含非法或重复的候选顺序".into(),
                ));
            }
        }
    }
    let mut result = Vec::with_capacity(candidate_points.len());
    let mut seen = BTreeSet::new();
    let mut reused_stable_ids = BTreeSet::new();
    let mut new_point_count = 0;
    let mut total = 0.0;
    for (index, point) in candidate_points.iter().enumerate() {
        let order_index = point
            .get("order_index")
            .and_then(Value::as_i64)
            .unwrap_or(index as i64);
        if order_index < 0 || !seen.insert(order_index) {
            return Err(CoreError::Invalid("上传简答评分点顺序非法或重复".into()));
        }
        let canonical_text = point
            .get("canonical_text")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        let max_score = point
            .get("max_score")
            .and_then(Value::as_f64)
            .unwrap_or_default();
        if canonical_text.is_empty() || !max_score.is_finite() || max_score <= 0.0 {
            return Err(CoreError::Invalid("上传简答评分点表述或分值非法".into()));
        }
        let (stable_id, old_public_id) = if automatic_order_mapping {
            let (stable_id, old_public_id, _) = current_points
                .iter()
                .find(|(_, _, current_order)| *current_order == order_index)
                .ok_or_else(|| {
                    CoreError::Invalid("上传简答评分点无法按顺序对应当前知识链接".into())
                })?;
            reused_stable_ids.insert(stable_id.clone());
            (stable_id.clone(), Some(old_public_id.clone()))
        } else {
            let mapping = mappings_by_order
                .get(&order_index)
                .ok_or_else(|| CoreError::Invalid("评分点结构映射缺少上传评分点".into()))?;
            match mapping.action {
                RubricPointMappingAction::ReuseExisting => {
                    let previous_stable_id = mapping
                        .previous_stable_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .ok_or_else(|| {
                            CoreError::Invalid("沿用旧评分点时必须明确选择旧评分点".into())
                        })?;
                    let (stable_id, old_public_id, _) = current_points
                        .iter()
                        .find(|(stable_id, _, _)| stable_id == previous_stable_id)
                        .ok_or_else(|| {
                            CoreError::Invalid("评分点结构映射引用了不存在的旧评分点".into())
                        })?;
                    if !reused_stable_ids.insert(stable_id.clone()) {
                        return Err(CoreError::Invalid(
                            "同一个旧评分点不能对应多个新评分点".into(),
                        ));
                    }
                    (stable_id.clone(), Some(old_public_id.clone()))
                }
                RubricPointMappingAction::NewPoint => {
                    if mapping
                        .previous_stable_id
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty())
                    {
                        return Err(CoreError::Invalid(
                            "新增评分点不能同时沿用旧评分点身份".into(),
                        ));
                    }
                    new_point_count += 1;
                    (format!("rubric-point-{}", ids::new_public_id()), None)
                }
            }
        };
        total += max_score;
        result.push(AdoptedRubricPoint {
            stable_id,
            old_public_id,
            order_index,
            canonical_text: canonical_text.into(),
            allowed_paraphrases_json: optional_string_array_json(point, "allowed_paraphrases")?,
            required_concepts_json: optional_string_array_json(point, "required_concepts")?,
            max_score,
        });
    }
    if (total - rubric_score).abs() > 0.000_001 {
        return Err(CoreError::Invalid(
            "上传简答评分点分值之和必须等于本题总分".into(),
        ));
    }
    result.sort_by_key(|point| point.order_index);
    let retired_points = current_points
        .into_iter()
        .filter(|(stable_id, _, _)| !reused_stable_ids.contains(stable_id))
        .map(|(_, old_public_id, _)| RetiredRubricPoint { old_public_id })
        .collect();
    Ok(ValidatedRubricChange {
        points: result,
        retired_points,
        new_point_count,
    })
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

fn insert_confirmed_rubric_version(
    conn: &Connection,
    question_version_id: i64,
    bound_rubric_version_id: i64,
    points: &[AdoptedRubricPoint],
    confirmed_by: &str,
    now: &str,
) -> CoreResult<(
    i64,
    BTreeMap<String, String>,
    Vec<InsertedRubricPointMapping>,
)> {
    let revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_rubric_versions
         WHERE question_version_id=?1",
        [question_version_id],
        |row| row.get(0),
    )?;
    let max_score: f64 = points.iter().map(|point| point.max_score).sum();
    conn.execute(
        "INSERT INTO k1_rubric_versions
         (public_id,question_version_id,revision,max_score,state,supersedes_rubric_id,
          created_at,confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,'confirmed',?5,?6,?7,?6)",
        (
            ids::new_public_id(),
            question_version_id,
            revision,
            max_score,
            bound_rubric_version_id,
            now,
            confirmed_by,
        ),
    )?;
    let rubric_version_id = conn.last_insert_rowid();
    let mut public_id_map = BTreeMap::new();
    let mut inserted_mappings = Vec::with_capacity(points.len());
    for point in points {
        let public_id = ids::new_public_id();
        conn.execute(
            "INSERT INTO k1_rubric_points
             (public_id,stable_id,rubric_version_id,order_index,canonical_text,
              allowed_paraphrases_json,required_concepts_json,max_score,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                &public_id,
                &point.stable_id,
                rubric_version_id,
                point.order_index,
                &point.canonical_text,
                point.allowed_paraphrases_json.as_deref(),
                point.required_concepts_json.as_deref(),
                point.max_score,
                now,
            ],
        )?;
        if let Some(old_public_id) = point.old_public_id.as_ref() {
            public_id_map.insert(old_public_id.clone(), public_id.clone());
        }
        inserted_mappings.push(InsertedRubricPointMapping {
            candidate_order_index: point.order_index,
            previous_rubric_point_public_id: point.old_public_id.clone(),
            adopted_rubric_point_public_id: public_id,
        });
    }
    Ok((rubric_version_id, public_id_map, inserted_mappings))
}

fn carry_forward_link_set(
    conn: &Connection,
    question_version_id: i64,
    bound_link_set_id: i64,
    rubric_point_public_ids: &BTreeMap<String, String>,
    retired_rubric_point_public_ids: &BTreeSet<String>,
    confirmed_by: &str,
    now: &str,
) -> CoreResult<(i64, CarriedLinkCounts)> {
    let (knowledge_map_id, state): (i64, String) = conn.query_row(
        "SELECT knowledge_map_id,state FROM k1_link_sets WHERE id=?1 AND question_version_id=?2",
        (bound_link_set_id, question_version_id),
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if state != "confirmed" {
        return Err(CoreError::Invalid(
            "当前简答题知识链接集尚未确认，不能自动沿用".into(),
        ));
    }
    let revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_link_sets WHERE question_version_id=?1",
        [question_version_id],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO k1_link_sets
         (public_id,question_version_id,knowledge_map_id,revision,state,supersedes_link_set_id,
          created_at,confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,'confirmed',?5,?6,?7,?6)",
        (
            ids::new_public_id(),
            question_version_id,
            knowledge_map_id,
            revision,
            bound_link_set_id,
            now,
            confirmed_by,
        ),
    )?;
    let link_set_id = conn.last_insert_rowid();
    let mut counts = CarriedLinkCounts::default();

    let mut knowledge_stmt = conn.prepare(
        "SELECT source_type,source_public_id,knowledge_node_id,relation_type,confirmation_level
         FROM k1_knowledge_links WHERE link_set_id=?1 ORDER BY id",
    )?;
    let knowledge_rows = knowledge_stmt
        .query_map([bound_link_set_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(knowledge_stmt);
    for (source_type, old_source_public_id, target_id, relation_type, confirmation_level) in
        knowledge_rows
    {
        let source_public_id = match source_type.as_str() {
            "rubric_point" => match rubric_point_public_ids.get(&old_source_public_id) {
                Some(value) => value.clone(),
                None if retired_rubric_point_public_ids.contains(&old_source_public_id) => {
                    counts.dropped_knowledge += 1;
                    continue;
                }
                None => {
                    return Err(CoreError::Invalid(
                        "当前知识链接无法对应新的简答评分点，请老师逐点补录".into(),
                    ))
                }
            },
            "answer_slot" => {
                return Err(CoreError::Invalid(
                    "简答题知识链接不能引用填空答案槽位".into(),
                ))
            }
            _ => old_source_public_id,
        };
        let teacher_confirmed = confirmation_level == "teacher_confirmed";
        conn.execute(
            "INSERT INTO k1_knowledge_links
             (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,
              relation_type,confirmation_level,verified_by,verified_at,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                ids::new_public_id(),
                link_set_id,
                &source_type,
                &source_public_id,
                target_id,
                &relation_type,
                &confirmation_level,
                teacher_confirmed.then_some(confirmed_by),
                teacher_confirmed.then_some(now),
                now,
            ],
        )?;
        counts.knowledge += 1;
    }

    let mut ability_stmt = conn.prepare(
        "SELECT source_type,source_public_id,ability_dimension_id,evidence_strength,
                response_mode,confirmation_level
         FROM k1_ability_links WHERE link_set_id=?1 ORDER BY id",
    )?;
    let ability_rows = ability_stmt
        .query_map([bound_link_set_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(ability_stmt);
    for (
        source_type,
        old_source_public_id,
        target_id,
        evidence_strength,
        response_mode,
        confirmation_level,
    ) in ability_rows
    {
        let source_public_id = match source_type.as_str() {
            "rubric_point" => match rubric_point_public_ids.get(&old_source_public_id) {
                Some(value) => value.clone(),
                None if retired_rubric_point_public_ids.contains(&old_source_public_id) => {
                    counts.dropped_ability += 1;
                    continue;
                }
                None => {
                    return Err(CoreError::Invalid(
                        "当前能力链接无法对应新的简答评分点，请老师逐点补录".into(),
                    ))
                }
            },
            "answer_slot" => {
                return Err(CoreError::Invalid(
                    "简答题能力链接不能引用填空答案槽位".into(),
                ))
            }
            _ => old_source_public_id,
        };
        let teacher_confirmed = confirmation_level == "teacher_confirmed";
        conn.execute(
            "INSERT INTO k1_ability_links
             (public_id,link_set_id,source_type,source_public_id,ability_dimension_id,
              evidence_strength,response_mode,confirmation_level,verified_by,verified_at,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                ids::new_public_id(),
                link_set_id,
                &source_type,
                &source_public_id,
                target_id,
                evidence_strength,
                &response_mode,
                &confirmation_level,
                teacher_confirmed.then_some(confirmed_by),
                teacher_confirmed.then_some(now),
                now,
            ],
        )?;
        counts.ability += 1;
    }
    Ok((link_set_id, counts))
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
    adopt_conflicts_as_new_version_with_mappings(
        conn,
        batch_id,
        source_ai_run_id,
        confirmed_by,
        &[],
    )
}

pub fn adopt_conflicts_as_new_version_with_mappings(
    conn: &mut Connection,
    batch_id: i64,
    source_ai_run_id: i64,
    confirmed_by: &str,
    rubric_mappings: &[RubricPointMappingInput],
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

    let mut requested_mapping_keys = BTreeSet::new();
    for mapping in rubric_mappings {
        if mapping.assessment_item_id <= 0
            || mapping.candidate_order_index < 0
            || !requested_mapping_keys
                .insert((mapping.assessment_item_id, mapping.candidate_order_index))
        {
            return Err(CoreError::Invalid(
                "评分点结构映射包含非法题目、顺序或重复项".into(),
            ));
        }
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
    let mut changed_rubric_count = 0;
    let mut carried_knowledge_link_count = 0;
    let mut carried_ability_link_count = 0;
    let mut new_rubric_point_count = 0;
    let mut retired_rubric_point_count = 0;
    let mut unlinked_new_rubric_point_count = 0;
    let mut dropped_knowledge_link_count = 0;
    let mut dropped_ability_link_count = 0;
    let mut used_mapping_keys = BTreeSet::new();
    let mut rubric_mapping_rows = Vec::new();
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
        let mut item_rubric_mapping_rows = Vec::new();
        let mut item_new_point_count = 0;
        let mut item_retired_point_count = 0;
        let (answer_key_version_id, rubric_version_id, link_set_id) =
            if review_item.match_state == "conflict" {
                let candidate_json = review_item
                    .candidate_answer_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("冲突项缺少上传答案候选".into()))?;
                let candidate = parse_object(candidate_json, "上传答案")?;
                let (slots, rubric_change) = if question_type == "short_answer" {
                    let item_mappings = rubric_mappings
                        .iter()
                        .filter(|mapping| mapping.assessment_item_id == source_item_id)
                        .cloned()
                        .collect::<Vec<_>>();
                    used_mapping_keys.extend(item_mappings.iter().map(|mapping| {
                        (mapping.assessment_item_id, mapping.candidate_order_index)
                    }));
                    (
                        Vec::new(),
                        Some(validate_short_answer_candidate(
                            &tx,
                            rubric_version_id,
                            score,
                            &candidate,
                            &item_mappings,
                        )?),
                    )
                } else {
                    (
                        validate_adoptable_candidate(
                            &tx,
                            &question_type,
                            question_version_id,
                            bound_answer_key_version_id,
                            &candidate,
                        )?,
                        None,
                    )
                };
                let new_answer_key_version_id = insert_confirmed_answer_version(
                    &tx,
                    question_version_id,
                    bound_answer_key_version_id,
                    &candidate.to_string(),
                    &slots,
                    confirmed_by.trim(),
                    &now,
                )?;
                let (new_rubric_version_id, new_link_set_id, link_counts) = if let Some(change) =
                    rubric_change
                {
                    let retired_public_ids = change
                        .retired_points
                        .iter()
                        .map(|point| point.old_public_id.clone())
                        .collect::<BTreeSet<_>>();
                    let (new_rubric_version_id, point_public_ids, inserted_mappings) =
                        insert_confirmed_rubric_version(
                            &tx,
                            question_version_id,
                            rubric_version_id,
                            &change.points,
                            confirmed_by.trim(),
                            &now,
                        )?;
                    let (new_link_set_id, link_counts) = carry_forward_link_set(
                        &tx,
                        question_version_id,
                        link_set_id,
                        &point_public_ids,
                        &retired_public_ids,
                        confirmed_by.trim(),
                        &now,
                    )?;
                    for mapping in inserted_mappings {
                        let action = if mapping.previous_rubric_point_public_id.is_some() {
                            "reuse_existing"
                        } else {
                            "new_point"
                        };
                        item_rubric_mapping_rows.push(PendingRubricMappingLedgerRow {
                            mapping_action: action,
                            candidate_order_index: Some(mapping.candidate_order_index),
                            previous_rubric_point_public_id: mapping
                                .previous_rubric_point_public_id,
                            adopted_rubric_point_public_id: Some(
                                mapping.adopted_rubric_point_public_id,
                            ),
                        });
                    }
                    for retired in &change.retired_points {
                        item_rubric_mapping_rows.push(PendingRubricMappingLedgerRow {
                            mapping_action: "retire_existing",
                            candidate_order_index: None,
                            previous_rubric_point_public_id: Some(retired.old_public_id.clone()),
                            adopted_rubric_point_public_id: None,
                        });
                    }
                    item_new_point_count = change.new_point_count;
                    item_retired_point_count = change.retired_points.len() as i64;
                    changed_rubric_count += 1;
                    carried_knowledge_link_count += link_counts.knowledge;
                    carried_ability_link_count += link_counts.ability;
                    new_rubric_point_count += item_new_point_count;
                    retired_rubric_point_count += item_retired_point_count;
                    unlinked_new_rubric_point_count += item_new_point_count;
                    dropped_knowledge_link_count += link_counts.dropped_knowledge;
                    dropped_ability_link_count += link_counts.dropped_ability;
                    (new_rubric_version_id, new_link_set_id, link_counts)
                } else {
                    (rubric_version_id, link_set_id, CarriedLinkCounts::default())
                };
                changed_items.push(serde_json::json!({
                    "source_assessment_item_id": source_item_id,
                    "question_version_id": question_version_id,
                    "old_answer_key_version_id": bound_answer_key_version_id,
                    "new_answer_key_version_id": new_answer_key_version_id,
                    "old_rubric_version_id": rubric_version_id,
                    "new_rubric_version_id": new_rubric_version_id,
                    "old_link_set_id": link_set_id,
                    "new_link_set_id": new_link_set_id,
                    "carried_knowledge_link_count": link_counts.knowledge,
                    "carried_ability_link_count": link_counts.ability,
                    "new_rubric_point_count": item_new_point_count,
                    "retired_rubric_point_count": item_retired_point_count,
                    "dropped_knowledge_link_count": link_counts.dropped_knowledge,
                    "dropped_ability_link_count": link_counts.dropped_ability,
                }));
                (
                    new_answer_key_version_id,
                    new_rubric_version_id,
                    new_link_set_id,
                )
            } else {
                (bound_answer_key_version_id, rubric_version_id, link_set_id)
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
        let adopted_assessment_item_id = tx.last_insert_rowid();
        for mapping in item_rubric_mapping_rows {
            rubric_mapping_rows.push(RubricMappingLedgerRow {
                source_assessment_item_id: source_item_id,
                adopted_assessment_item_id,
                mapping_action: mapping.mapping_action,
                candidate_order_index: mapping.candidate_order_index,
                previous_rubric_point_public_id: mapping.previous_rubric_point_public_id,
                adopted_rubric_point_public_id: mapping.adopted_rubric_point_public_id,
            });
        }
        hash_items.push(AdoptedItemHashInput {
            item_id: adopted_assessment_item_id,
            question_version_id,
            answer_key_version_id,
            rubric_version_id,
            link_set_id,
            order_index,
            score_millis: (score * 1000.0).round() as i64,
        });
    }
    if used_mapping_keys != requested_mapping_keys {
        return Err(CoreError::Invalid(
            "评分点结构映射包含不属于当前冲突简答题的项目".into(),
        ));
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
        "changed_rubric_count": changed_rubric_count,
        "carried_knowledge_link_count": carried_knowledge_link_count,
        "carried_ability_link_count": carried_ability_link_count,
        "new_rubric_point_count": new_rubric_point_count,
        "retired_rubric_point_count": retired_rubric_point_count,
        "unlinked_new_rubric_point_count": unlinked_new_rubric_point_count,
        "dropped_knowledge_link_count": dropped_knowledge_link_count,
        "dropped_ability_link_count": dropped_ability_link_count,
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
    let answer_source_adoption_id = tx.last_insert_rowid();
    for mapping in &rubric_mapping_rows {
        tx.execute(
            "INSERT INTO exam_rubric_point_mappings_v2
             (public_id,answer_source_adoption_id,source_ai_run_id,
              source_assessment_item_id,adopted_assessment_item_id,mapping_action,
              candidate_order_index,previous_rubric_point_public_id,
              adopted_rubric_point_public_id,created_by,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                ids::new_public_id(),
                answer_source_adoption_id,
                source_ai_run_id,
                mapping.source_assessment_item_id,
                mapping.adopted_assessment_item_id,
                mapping.mapping_action,
                mapping.candidate_order_index,
                mapping.previous_rubric_point_public_id.as_deref(),
                mapping.adopted_rubric_point_public_id.as_deref(),
                confirmed_by.trim(),
                &now,
            ],
        )?;
    }
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
        if question_type == "short_answer" {
            conn.execute_batch(
                r#"INSERT INTO k1_rubric_points
                     (public_id,stable_id,rubric_version_id,order_index,canonical_text,
                      allowed_paraphrases_json,required_concepts_json,max_score,created_at)
                   VALUES ('answer-source-rp','stable-rp',1,0,'旧评分点',
                           '["旧允许表达"]','["旧概念"]',1,'2026-07-15T10:00:00Z');
                   INSERT INTO k1_knowledge_nodes
                     (public_id,stable_id,knowledge_map_id,code,title,order_index,state,created_at)
                   VALUES ('answer-source-kn','stable-kn',1,'KN-1','鸦片战争影响',0,'active',
                           '2026-07-15T10:00:00Z');
                   INSERT INTO k1_ability_dimensions
                     (public_id,stable_id,subject_id,revision,code,title,state,created_at)
                   VALUES ('answer-source-ad','stable-ad',1,1,'CAUSE','因果分析','active',
                           '2026-07-15T10:00:00Z');
                   INSERT INTO k1_knowledge_links
                     (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,
                      relation_type,confirmation_level,verified_by,verified_at,created_at)
                   VALUES ('answer-source-kl',1,'rubric_point','answer-source-rp',1,
                           'rubric_basis','teacher_confirmed','teacher',
                           '2026-07-15T10:00:00Z','2026-07-15T10:00:00Z');
                   INSERT INTO k1_ability_links
                     (public_id,link_set_id,source_type,source_public_id,ability_dimension_id,
                      evidence_strength,response_mode,confirmation_level,verified_by,verified_at,created_at)
                   VALUES ('answer-source-al',1,'rubric_point','answer-source-rp',1,0.8,
                           'structured_response','teacher_confirmed','teacher',
                           '2026-07-15T10:00:00Z','2026-07-15T10:00:00Z');"#,
            )
            .unwrap();
        }
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
    fn short_answer_conflict_creates_confirmed_rubric_and_carries_verified_links() {
        let mut fixture = fixture_with_candidate(
            "short_answer",
            serde_json::json!({
                "schema_version": 1,
                "reference_answer": "需要新的参考答案",
                "rubric_points": [{
                    "order_index": 0,
                    "canonical_text":"新评分点一",
                    "allowed_paraphrases":["等价表述"],
                    "required_concepts":["核心概念"],
                    "max_score":1
                }],
            }),
        );
        let adopted = adopt_conflicts_as_new_version(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
        )
        .unwrap();
        let adoption = adopted.adoption.unwrap();
        assert_eq!(adoption.changed_rubric_count, 1);
        assert_eq!(adoption.carried_knowledge_link_count, 1);
        assert_eq!(adoption.carried_ability_link_count, 1);
        let new_version_id = adoption.adopted_assessment_version_id;
        let adopted_versions: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT answer_key_version_id,rubric_version_id,link_set_id
                 FROM exam_assessment_items_v2 WHERE assessment_version_id=?1",
                [new_version_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_ne!(adopted_versions.0, 1);
        assert_ne!(adopted_versions.1, 1);
        assert_ne!(adopted_versions.2, 1);
        let point: (String, String, String, String, f64) = fixture
            .conn
            .query_row(
                "SELECT stable_id,canonical_text,allowed_paraphrases_json,
                        required_concepts_json,max_score
                 FROM k1_rubric_points WHERE rubric_version_id=?1",
                [adopted_versions.1],
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
        assert_eq!(point.0, "stable-rp");
        assert_eq!(point.1, "新评分点一");
        assert_eq!(point.2, "[\"等价表述\"]");
        assert_eq!(point.3, "[\"核心概念\"]");
        assert_eq!(point.4, 1.0);
        let carried: (i64, i64, String, String) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM k1_knowledge_links WHERE link_set_id=?1),
                   (SELECT COUNT(*) FROM k1_ability_links WHERE link_set_id=?1),
                   (SELECT source_public_id FROM k1_knowledge_links WHERE link_set_id=?1),
                   (SELECT confirmation_level FROM k1_knowledge_links WHERE link_set_id=?1)",
                [adopted_versions.2],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            (carried.0, carried.1, carried.3.as_str()),
            (1, 1, "teacher_confirmed")
        );
        assert_ne!(carried.2, "answer-source-rp");
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
        let current_batch_version: i64 = fixture
            .conn
            .query_row(
                "SELECT assessment_version_id FROM exam_ingest_batches_v2 WHERE id=?1",
                [fixture.batch_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(current_batch_version, 1);
    }

    #[test]
    fn short_answer_point_shape_change_rolls_back_every_new_version() {
        let mut fixture = fixture_with_candidate(
            "short_answer",
            serde_json::json!({
                "schema_version": 1,
                "reference_answer": "需要新的参考答案",
                "rubric_points": [
                    {"order_index":0,"canonical_text":"评分点一","max_score":0.5},
                    {"order_index":1,"canonical_text":"评分点二","max_score":0.5}
                ],
            }),
        );
        let error = adopt_conflicts_as_new_version(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
        )
        .unwrap_err();
        assert!(error.to_string().contains("数量"));
        let counts: (i64, i64, i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                        (SELECT COUNT(*) FROM k1_answer_key_versions),
                        (SELECT COUNT(*) FROM k1_rubric_versions),
                        (SELECT COUNT(*) FROM k1_link_sets),
                        (SELECT COUNT(*) FROM exam_answer_source_adoptions_v2)",
                [],
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
        assert_eq!(counts, (1, 1, 1, 1, 0));
    }

    #[test]
    fn teacher_mapping_can_reuse_one_point_and_add_one_without_guessing_links() {
        let mut fixture = fixture_with_candidate(
            "short_answer",
            serde_json::json!({
                "schema_version": 1,
                "reference_answer": "需要新的参考答案",
                "rubric_points": [
                    {"order_index":0,"canonical_text":"沿用旧知识点的新表述","max_score":0.5},
                    {"order_index":1,"canonical_text":"新增评分点","max_score":0.5}
                ],
            }),
        );
        let adopted = adopt_conflicts_as_new_version_with_mappings(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
            &[
                RubricPointMappingInput {
                    assessment_item_id: 1,
                    candidate_order_index: 0,
                    action: RubricPointMappingAction::ReuseExisting,
                    previous_stable_id: Some("stable-rp".into()),
                },
                RubricPointMappingInput {
                    assessment_item_id: 1,
                    candidate_order_index: 1,
                    action: RubricPointMappingAction::NewPoint,
                    previous_stable_id: None,
                },
            ],
        )
        .unwrap();
        let adoption = adopted.adoption.unwrap();
        assert_eq!(adoption.changed_rubric_count, 1);
        assert_eq!(adoption.new_rubric_point_count, 1);
        assert_eq!(adoption.retired_rubric_point_count, 0);
        assert_eq!(adoption.unlinked_new_rubric_point_count, 1);
        assert_eq!(adoption.carried_knowledge_link_count, 1);
        assert_eq!(adoption.carried_ability_link_count, 1);
        assert_eq!(adoption.dropped_knowledge_link_count, 0);
        assert_eq!(adoption.dropped_ability_link_count, 0);

        let new_rubric_version_id: i64 = fixture
            .conn
            .query_row(
                "SELECT rubric_version_id FROM exam_assessment_items_v2
                 WHERE assessment_version_id=?1",
                [adoption.adopted_assessment_version_id],
                |row| row.get(0),
            )
            .unwrap();
        let (point_count, reused_count, generated_count): (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT COUNT(*),
                        SUM(CASE WHEN stable_id='stable-rp' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN stable_id LIKE 'rubric-point-%' THEN 1 ELSE 0 END)
                 FROM k1_rubric_points WHERE rubric_version_id=?1",
                [new_rubric_version_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!((point_count, reused_count, generated_count), (2, 1, 1));

        let mapping_counts: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT COUNT(*),
                        SUM(CASE WHEN mapping_action='reuse_existing' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN mapping_action='new_point' THEN 1 ELSE 0 END)
                 FROM exam_rubric_point_mappings_v2",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(mapping_counts, (2, 1, 1));
        assert!(fixture
            .conn
            .execute(
                "UPDATE exam_rubric_point_mappings_v2 SET created_by='other' WHERE id=1",
                [],
            )
            .is_err());
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
    }

    #[test]
    fn incomplete_or_duplicate_teacher_mapping_rolls_back_every_new_version() {
        let mut fixture = fixture_with_candidate(
            "short_answer",
            serde_json::json!({
                "schema_version": 1,
                "reference_answer": "需要新的参考答案",
                "rubric_points": [
                    {"order_index":0,"canonical_text":"评分点一","max_score":0.5},
                    {"order_index":1,"canonical_text":"评分点二","max_score":0.5}
                ],
            }),
        );
        let incomplete = adopt_conflicts_as_new_version_with_mappings(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
            &[RubricPointMappingInput {
                assessment_item_id: 1,
                candidate_order_index: 0,
                action: RubricPointMappingAction::ReuseExisting,
                previous_stable_id: Some("stable-rp".into()),
            }],
        )
        .unwrap_err();
        assert!(incomplete
            .to_string()
            .contains("必须覆盖上传答案的每一个评分点"));

        let duplicate = adopt_conflicts_as_new_version_with_mappings(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
            &[
                RubricPointMappingInput {
                    assessment_item_id: 1,
                    candidate_order_index: 0,
                    action: RubricPointMappingAction::ReuseExisting,
                    previous_stable_id: Some("stable-rp".into()),
                },
                RubricPointMappingInput {
                    assessment_item_id: 1,
                    candidate_order_index: 1,
                    action: RubricPointMappingAction::ReuseExisting,
                    previous_stable_id: Some("stable-rp".into()),
                },
            ],
        )
        .unwrap_err();
        assert!(duplicate
            .to_string()
            .contains("同一个旧评分点不能对应多个新评分点"));

        let counts: (i64, i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                        (SELECT COUNT(*) FROM k1_rubric_versions),
                        (SELECT COUNT(*) FROM exam_answer_source_adoptions_v2),
                        (SELECT COUNT(*) FROM exam_rubric_point_mappings_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(counts, (1, 1, 0, 0));
    }

    #[test]
    fn teacher_can_retire_an_old_point_without_carrying_its_graph_links() {
        let mut fixture = fixture_with_candidate(
            "short_answer",
            serde_json::json!({
                "schema_version": 1,
                "reference_answer": "新的参考答案",
                "rubric_points": [{
                    "order_index":0,
                    "canonical_text":"完全新增的评分点",
                    "max_score":1
                }],
            }),
        );
        let adopted = adopt_conflicts_as_new_version_with_mappings(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
            &[RubricPointMappingInput {
                assessment_item_id: 1,
                candidate_order_index: 0,
                action: RubricPointMappingAction::NewPoint,
                previous_stable_id: None,
            }],
        )
        .unwrap();
        let adoption = adopted.adoption.unwrap();
        assert_eq!(adoption.new_rubric_point_count, 1);
        assert_eq!(adoption.retired_rubric_point_count, 1);
        assert_eq!(adoption.unlinked_new_rubric_point_count, 1);
        assert_eq!(adoption.carried_knowledge_link_count, 0);
        assert_eq!(adoption.carried_ability_link_count, 0);
        assert_eq!(adoption.dropped_knowledge_link_count, 1);
        assert_eq!(adoption.dropped_ability_link_count, 1);

        let (new_link_set_id, new_rubric_version_id): (i64, i64) = fixture
            .conn
            .query_row(
                "SELECT link_set_id,rubric_version_id FROM exam_assessment_items_v2
                 WHERE assessment_version_id=?1",
                [adoption.adopted_assessment_version_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let (knowledge_links, ability_links, new_points, mapping_rows): (i64, i64, i64, i64) =
            fixture
                .conn
                .query_row(
                    "SELECT
                       (SELECT COUNT(*) FROM k1_knowledge_links WHERE link_set_id=?1),
                       (SELECT COUNT(*) FROM k1_ability_links WHERE link_set_id=?1),
                       (SELECT COUNT(*) FROM k1_rubric_points WHERE rubric_version_id=?2),
                       (SELECT COUNT(*) FROM exam_rubric_point_mappings_v2)",
                    (new_link_set_id, new_rubric_version_id),
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .unwrap();
        assert_eq!(
            (knowledge_links, ability_links, new_points, mapping_rows),
            (0, 0, 1, 2)
        );
        let actions = fixture
            .conn
            .prepare(
                "SELECT mapping_action FROM exam_rubric_point_mappings_v2 ORDER BY mapping_action",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(actions, vec!["new_point", "retire_existing"]);
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
