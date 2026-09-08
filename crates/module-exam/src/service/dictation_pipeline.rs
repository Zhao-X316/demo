//! 固定格式默写模板确认、评分策略冻结与学生页题区物化。
//!
//! 首版把“一行/一栏/一个题号”作为一个 assessment item，并要求每项只有一个已确认
//! rubric point。老师确认一次空白页模板后，后续学生页按该模板裁区；OCR 和老师校正
//! 另写追加式 transcription。只有老师接受建议或人工记分后才桥接 B0 grade decision，
//! 整卷仍需显式发布，发布后才产生正式学习证据。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::{ai_runs, artifacts, audit};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRunStatus, ArchiveStatus, ArtifactKind, AuditActorType, PrivacyClass};

use crate::dictation::{
    compare_dictation_point, DictationPointResult, DictationPointRule, DictationRecognitionState,
};
use crate::dictation_recognition::{
    DictationOcrOutput, DictationQuestionType, DictationRegionProposal, DictationTemplateOutput,
    DictationTemplateState, DICTATION_READY_CONFIDENCE,
};

use super::assessment::{decide_grade_in_transaction, GradeDecision, NewGradeDecision};
use super::papers::{
    self, AnswerRegionRevision, NewAnswerRegionRevision, NewPageAlignmentRevision,
    PageAlignmentRevision,
};

pub const DICTATION_STRICT_BATCH_CONFIDENCE: f64 = 0.95;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictationTemplateRevision {
    pub id: i64,
    pub public_id: String,
    pub assessment_version_id: i64,
    pub revision: i64,
    pub template_version: String,
    pub page_no: i64,
    pub template_json: String,
    pub source_ai_run_id: Option<i64>,
    pub confirmed_by: String,
    pub state: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictationPolicyRevision {
    pub id: i64,
    pub public_id: String,
    pub assessment_item_id: i64,
    pub revision: i64,
    pub answer_key_version_id: i64,
    pub rubric_version_id: i64,
    pub policy_hash: String,
    pub policy_json: String,
    pub confirmed_by: String,
    pub state: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictationTemplateConfirmation {
    pub template: DictationTemplateRevision,
    pub policies: Vec<DictationPolicyRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictationPageMaterialization {
    pub id: i64,
    pub public_id: String,
    pub page_id: i64,
    pub template_revision_id: i64,
    pub alignment_revision_id: i64,
    pub region_revision_ids: Vec<i64>,
    pub confirmed_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationPageMaterializationResult {
    pub materialization: DictationPageMaterialization,
    pub alignment: PageAlignmentRevision,
    pub regions: Vec<AnswerRegionRevision>,
}

type MaterializationRow = (i64, String, i64, i64, i64, String, String, String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationTemplateSource {
    pub template: DictationTemplateRevision,
    pub blank_artifact_id: i64,
    pub blank_artifact_sha256: String,
    pub regions: Vec<DictationRegionProposal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictationRegionArtifact {
    pub assessment_item_id: i64,
    pub region_index: i64,
    pub crop_artifact_id: i64,
}

pub struct MaterializeDictationPageInput<'a> {
    pub page_id: i64,
    pub template_revision_id: i64,
    pub aligned_artifact_id: i64,
    pub regions: &'a [DictationRegionArtifact],
    pub confirmed_by: &'a str,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationTranscriptionRevision {
    pub id: i64,
    pub public_id: String,
    pub attempt_id: i64,
    pub assessment_item_id: i64,
    pub answer_region_revision_id: i64,
    pub revision: i64,
    pub source_ai_run_id: Option<i64>,
    pub result_state: String,
    pub raw_ocr_text: Option<String>,
    pub normalized_text: Option<String>,
    pub teacher_corrected_text: Option<String>,
    pub confidence: Option<f64>,
    pub failure_meta_json: Option<String>,
    pub corrected_by: Option<String>,
    pub corrected_at: Option<String>,
    pub state: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationPointObservation {
    pub id: i64,
    pub transcription_revision_id: i64,
    pub policy_revision_id: i64,
    pub rubric_point_id: i64,
    pub result: String,
    pub evidence_text: Option<String>,
    pub rationale: String,
    pub suggested_score: Option<f64>,
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationTranscriptionResult {
    pub transcription: DictationTranscriptionRevision,
    pub observation: DictationPointObservation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationWorkbenchRow {
    pub assessment_id: i64,
    pub assessment_version_id: i64,
    pub assessment_title: String,
    pub attempt_id: i64,
    pub attempt_state: String,
    pub active_publication_id: Option<i64>,
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
    pub assessment_item_id: i64,
    pub order_index: i64,
    pub question_no: String,
    pub question_type: String,
    pub question_stem: String,
    pub max_score: f64,
    pub answer_region_revision_id: i64,
    pub crop_path: Option<String>,
    pub transcription_revision_id: i64,
    pub transcription_revision: i64,
    pub source_ai_run_id: Option<i64>,
    pub result_state: String,
    pub raw_ocr_text: Option<String>,
    pub normalized_text: Option<String>,
    pub teacher_corrected_text: Option<String>,
    pub confidence: Option<f64>,
    pub point_result: String,
    pub canonical_text: String,
    pub accepted_variants: Vec<String>,
    pub suggested_score: Option<f64>,
    pub requires_teacher_review: bool,
    pub grade_decision_id: Option<i64>,
    pub grade_decision_revision: Option<i64>,
    pub teacher_score: Option<f64>,
    pub confirmation_level: Option<String>,
    pub review_mode: Option<String>,
    pub current_transcription_confirmed: bool,
    pub decided_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationAttemptSummary {
    pub assessment_id: i64,
    pub assessment_version_id: i64,
    pub assessment_title: String,
    pub attempt_id: i64,
    pub attempt_state: String,
    pub active_publication_id: Option<i64>,
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
    pub item_count: i64,
    pub observed_count: i64,
    pub confirmed_count: i64,
    pub teacher_total_score: f64,
    pub max_total_score: f64,
    pub published_total_score: Option<f64>,
    pub can_publish: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationWorkbench {
    pub rows: Vec<DictationWorkbenchRow>,
    pub attempts: Vec<DictationAttemptSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictationBatchItemResult {
    pub transcription_revision_id: i64,
    pub outcome: String,
    pub reason_code: Option<String>,
    pub grade_decision_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictationReviewBatch {
    pub id: i64,
    pub public_id: String,
    pub assessment_version_id: i64,
    pub idempotency_key: String,
    pub requested_count: i64,
    pub confirmed_count: i64,
    pub excluded_count: i64,
    pub created_by: String,
    pub items: Vec<DictationBatchItemResult>,
}

pub struct StrictDictationBatchReview<'a> {
    pub transcription_revision_ids: &'a [i64],
    pub reviewed_by: &'a str,
    pub idempotency_key: &'a str,
}

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("默写{field}不能为空")))
    } else {
        Ok(())
    }
}

fn template_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DictationTemplateRevision> {
    Ok(DictationTemplateRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        assessment_version_id: row.get(2)?,
        revision: row.get(3)?,
        template_version: row.get(4)?,
        page_no: row.get(5)?,
        template_json: row.get(6)?,
        source_ai_run_id: row.get(7)?,
        confirmed_by: row.get(8)?,
        state: row.get(9)?,
        created_at: row.get(10)?,
    })
}

fn policy_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DictationPolicyRevision> {
    Ok(DictationPolicyRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        assessment_item_id: row.get(2)?,
        revision: row.get(3)?,
        answer_key_version_id: row.get(4)?,
        rubric_version_id: row.get(5)?,
        policy_hash: row.get(6)?,
        policy_json: row.get(7)?,
        confirmed_by: row.get(8)?,
        state: row.get(9)?,
        created_at: row.get(10)?,
    })
}

pub fn get_active_template(
    conn: &Connection,
    assessment_version_id: i64,
    page_no: i64,
) -> CoreResult<Option<DictationTemplateRevision>> {
    Ok(conn
        .query_row(
            "SELECT id,public_id,assessment_version_id,revision,template_version,page_no,
                    template_json,source_ai_run_id,confirmed_by,state,created_at
             FROM exam_dictation_template_revisions_v2
             WHERE assessment_version_id=?1 AND page_no=?2 AND state='active'",
            (assessment_version_id, page_no),
            template_row,
        )
        .optional()?)
}

pub fn get_active_policy(
    conn: &Connection,
    assessment_item_id: i64,
) -> CoreResult<Option<DictationPolicyRevision>> {
    Ok(conn
        .query_row(
            "SELECT id,public_id,assessment_item_id,revision,answer_key_version_id,
                    rubric_version_id,policy_hash,policy_json,confirmed_by,state,created_at
             FROM exam_dictation_policy_revisions_v2
             WHERE assessment_item_id=?1 AND state='active'",
            [assessment_item_id],
            policy_row,
        )
        .optional()?)
}

fn accepted_variants(raw: Option<String>) -> CoreResult<Vec<String>> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|error| CoreError::Parse(format!("默写可接受变体 JSON 无效：{error}")))?;
    let values = if let Some(array) = value.as_array() {
        array.clone()
    } else if let Some(array) = value.get("values").and_then(serde_json::Value::as_array) {
        array.clone()
    } else if let Some(array) = value
        .get("paraphrases")
        .and_then(serde_json::Value::as_array)
    {
        array.clone()
    } else {
        Vec::new()
    };
    Ok(values
        .into_iter()
        .filter_map(|value| value.as_str().map(str::trim).map(str::to_string))
        .filter(|value| !value.is_empty())
        .collect())
}

#[derive(Debug)]
struct PolicyDraft {
    assessment_item_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    policy_hash: String,
    policy_json: String,
}

fn build_policy_draft(conn: &Connection, assessment_item_id: i64) -> CoreResult<PolicyDraft> {
    let (answer_key_version_id, rubric_version_id, question_type, answer_state, rubric_state): (
        i64,
        i64,
        String,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT i.answer_key_version_id,i.rubric_version_id,q.question_type,
                    ak.state,rv.state
             FROM exam_assessment_items_v2 i
             JOIN k1_question_versions q ON q.id=i.question_version_id
             JOIN k1_answer_key_versions ak ON ak.id=i.answer_key_version_id
             JOIN k1_rubric_versions rv ON rv.id=i.rubric_version_id
             WHERE i.id=?1 AND i.state='active'",
            [assessment_item_id],
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
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("默写作业项#{assessment_item_id}")))?;
    if DictationQuestionType::from_db(&question_type).is_none()
        || answer_state != "confirmed"
        || rubric_state != "confirmed"
    {
        return Err(CoreError::Invalid(
            "固定默写只接受已确认答案/rubric 的填空或简答作业项".into(),
        ));
    }
    let mut stmt = conn.prepare(
        "SELECT id,stable_id,canonical_text,allowed_paraphrases_json,max_score
         FROM k1_rubric_points WHERE rubric_version_id=?1 ORDER BY order_index,id",
    )?;
    let rows = stmt.query_map([rubric_version_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, f64>(4)?,
        ))
    })?;
    let mut points = Vec::new();
    for row in rows {
        let (id, stable_id, canonical_text, variants, max_score) = row?;
        points.push(serde_json::json!({
            "rubric_point_id": id,
            "stable_id": stable_id,
            "canonical_text": canonical_text,
            "accepted_variants": accepted_variants(variants)?,
            "max_score": max_score,
        }));
    }
    if points.len() != 1 {
        return Err(CoreError::Invalid(
            "固定默写首版要求一行/一栏对应一个且仅一个评分点；多评分点请拆成多个默写项".into(),
        ));
    }
    let policy_json = serde_json::json!({
        "schema_version": 1,
        "typo_policy": "exact_or_teacher_confirmed_variant",
        "ordered_sequence": true,
        "points": points,
    })
    .to_string();
    let policy_hash = hashing::sha256_hex(policy_json.as_bytes());
    Ok(PolicyDraft {
        assessment_item_id,
        answer_key_version_id,
        rubric_version_id,
        policy_hash,
        policy_json,
    })
}

fn validate_template_output(output: &DictationTemplateOutput) -> CoreResult<()> {
    if output.state != DictationTemplateState::Ready
        || output.confidence < DICTATION_READY_CONFIDENCE
        || !output.issue_codes.is_empty()
        || output.regions.is_empty()
        || output
            .regions
            .iter()
            .any(|region| region.mapping_confidence < DICTATION_READY_CONFIDENCE)
    {
        return Err(CoreError::Invalid(
            "只有全部题区达到 ready 的固定默写模板才能确认".into(),
        ));
    }
    Ok(())
}

pub fn confirm_template_and_policies(
    conn: &mut Connection,
    output: &DictationTemplateOutput,
    source_ai_run_id: i64,
    confirmed_by: &str,
) -> CoreResult<DictationTemplateConfirmation> {
    required(confirmed_by, "模板确认人")?;
    validate_template_output(output)?;
    let run = ai_runs::get_by_id(conn, source_ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{source_ai_run_id}")))?;
    if run.status != AiRunStatus::Succeeded
        || run.run_type != "dictation_template"
        || run.input_artifact_id != Some(output.blank_artifact_id)
        || run.input_hash != output.input_hash
        || run.output_json.as_deref().is_none()
    {
        return Err(CoreError::Invalid("默写模板候选 run 不可确认".into()));
    }
    let artifact = artifacts::get_by_id(conn, output.blank_artifact_id)?
        .ok_or_else(|| CoreError::NotFound("默写空白模板 artifact".into()))?;
    if !matches!(artifact.kind, ArtifactKind::Image | ArtifactKind::Page)
        || artifact.privacy_class != PrivacyClass::TeachingContent
        || artifact.archive_status != ArchiveStatus::Ready
        || artifact.sha256 != output.blank_artifact_sha256
    {
        return Err(CoreError::Invalid("默写空白模板 artifact 不可用".into()));
    }
    let version: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT state,template_version FROM exam_assessment_versions_v2 WHERE id=?1",
            [output.assessment_version_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((state, template_version)) = version else {
        return Err(CoreError::NotFound("默写关联作业版本".into()));
    };
    if state != "confirmed"
        || template_version.as_deref().map(str::trim) != Some(output.template_version.trim())
    {
        return Err(CoreError::Invalid(
            "默写模板必须绑定当前确认的作业模板版本".into(),
        ));
    }
    if let Some(existing) = get_active_template(conn, output.assessment_version_id, output.page_no)?
    {
        if existing.source_ai_run_id == Some(source_ai_run_id) {
            let mut policies = Vec::new();
            for region in &output.regions {
                policies.push(
                    get_active_policy(conn, region.assessment_item_id)?.ok_or_else(|| {
                        CoreError::NotFound("已确认默写模板缺少 active policy".into())
                    })?,
                );
            }
            return Ok(DictationTemplateConfirmation {
                template: existing,
                policies,
            });
        }
    }
    let expected_items = output
        .regions
        .iter()
        .map(|region| region.assessment_item_id)
        .collect::<BTreeSet<_>>();
    let mut scoped = BTreeSet::new();
    let mut stmt = conn.prepare(
        "SELECT i.id,q.question_type
         FROM exam_assessment_items_v2 i
         JOIN k1_question_versions q ON q.id=i.question_version_id
         WHERE i.assessment_version_id=?1 AND i.state='active'
           AND COALESCE(CAST(json_extract(i.presentation_snapshot_json,'$.page_no') AS INTEGER),1)=?2",
    )?;
    let rows = stmt.query_map((output.assessment_version_id, output.page_no), |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (item_id, question_type) = row?;
        if DictationQuestionType::from_db(&question_type).is_none() {
            return Err(CoreError::Invalid(
                "默写页包含非填空/简答题，必须拆分材料".into(),
            ));
        }
        scoped.insert(item_id);
    }
    if scoped != expected_items {
        return Err(CoreError::Invalid(
            "默写模板候选未完整绑定当前页作业项".into(),
        ));
    }
    drop(stmt);
    let drafts = expected_items
        .iter()
        .map(|item_id| build_policy_draft(conn, *item_id))
        .collect::<CoreResult<Vec<_>>>()?;
    let template_json = serde_json::json!({
        "schema_version": 1,
        "blank_artifact_id": output.blank_artifact_id,
        "blank_artifact_sha256": output.blank_artifact_sha256,
        "canvas_width": output.canvas_width,
        "canvas_height": output.canvas_height,
        "regions": output.regions,
        "source_ai_run_id": source_ai_run_id,
    })
    .to_string();
    let tx = conn.transaction()?;
    let template_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_dictation_template_revisions_v2
         WHERE assessment_version_id=?1 AND page_no=?2",
        (output.assessment_version_id, output.page_no),
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE exam_dictation_template_revisions_v2 SET state='superseded'
         WHERE assessment_version_id=?1 AND page_no=?2 AND state='active'",
        (output.assessment_version_id, output.page_no),
    )?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    tx.execute(
        "INSERT INTO exam_dictation_template_revisions_v2
         (public_id,assessment_version_id,revision,template_version,page_no,template_json,
          source_ai_run_id,confirmed_by,state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'active',?9)",
        (
            &public_id,
            output.assessment_version_id,
            template_revision,
            output.template_version.trim(),
            output.page_no,
            &template_json,
            source_ai_run_id,
            confirmed_by.trim(),
            &now,
        ),
    )?;
    let template_id = tx.last_insert_rowid();
    let mut policies = Vec::with_capacity(drafts.len());
    for draft in drafts {
        let revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0)+1 FROM exam_dictation_policy_revisions_v2
             WHERE assessment_item_id=?1",
            [draft.assessment_item_id],
            |row| row.get(0),
        )?;
        tx.execute(
            "UPDATE exam_dictation_policy_revisions_v2 SET state='superseded'
             WHERE assessment_item_id=?1 AND state='active'",
            [draft.assessment_item_id],
        )?;
        let policy_public_id = ids::new_public_id();
        tx.execute(
            "INSERT INTO exam_dictation_policy_revisions_v2
             (public_id,assessment_item_id,revision,answer_key_version_id,rubric_version_id,
              policy_hash,policy_json,confirmed_by,state,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'active',?9)",
            (
                &policy_public_id,
                draft.assessment_item_id,
                revision,
                draft.answer_key_version_id,
                draft.rubric_version_id,
                &draft.policy_hash,
                &draft.policy_json,
                confirmed_by.trim(),
                &now,
            ),
        )?;
        policies.push(tx.query_row(
            "SELECT id,public_id,assessment_item_id,revision,answer_key_version_id,
                    rubric_version_id,policy_hash,policy_json,confirmed_by,state,created_at
             FROM exam_dictation_policy_revisions_v2 WHERE id=?1",
            [tx.last_insert_rowid()],
            policy_row,
        )?);
    }
    audit::append(
        &tx,
        &audit::NewAuditEvent {
            idempotency_key: &format!("exam:dictation-template:{source_ai_run_id}:confirmed"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(confirmed_by.trim()),
            action: "exam.dictation_template.confirmed",
            object_type: "exam_assessment_version",
            object_id: &output.assessment_version_id.to_string(),
            object_revision: Some(template_revision),
            note: None,
            meta_json: Some(r#"{"schema_version":1}"#),
            occurred_at: &now,
        },
    )?;
    let template = tx.query_row(
        "SELECT id,public_id,assessment_version_id,revision,template_version,page_no,
                template_json,source_ai_run_id,confirmed_by,state,created_at
         FROM exam_dictation_template_revisions_v2 WHERE id=?1",
        [template_id],
        template_row,
    )?;
    tx.commit()?;
    Ok(DictationTemplateConfirmation { template, policies })
}

fn template_regions(
    template: &DictationTemplateRevision,
) -> CoreResult<Vec<DictationRegionProposal>> {
    let value: serde_json::Value = serde_json::from_str(&template.template_json)
        .map_err(|error| CoreError::Parse(format!("默写模板 JSON 损坏：{error}")))?;
    serde_json::from_value(
        value
            .get("regions")
            .cloned()
            .ok_or_else(|| CoreError::Invalid("默写模板缺少 regions".into()))?,
    )
    .map_err(|error| CoreError::Parse(format!("默写模板题区损坏：{error}")))
}

pub fn get_active_template_for_page(
    conn: &Connection,
    page_id: i64,
) -> CoreResult<DictationTemplateRevision> {
    let scope: Option<(i64, i64)> = conn
        .query_row(
            "SELECT at.assessment_version_id,m.page_no
             FROM exam_ingest_pages_v2 p
             JOIN exam_page_quality_revisions_v2 q
               ON q.page_id=p.id AND q.state='active' AND q.result='pass'
             JOIN exam_page_match_revisions_v2 m
               ON m.page_id=p.id AND m.state='active' AND m.decision='teacher_confirmed'
             JOIN exam_attempts_v2 at ON at.id=m.attempt_id AND at.state<>'voided'
             JOIN exam_material_type_revisions_v2 mt
               ON mt.ingest_batch_id=p.batch_id AND mt.state='active'
                  AND mt.decision='teacher_confirmed' AND mt.material_type='dictation'
             WHERE p.id=?1 AND p.state<>'voided'",
            [page_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((assessment_version_id, page_no)) = scope else {
        return Err(CoreError::Invalid(
            "默写页面必须先通过质量、学生/页码和材料类型确认".into(),
        ));
    };
    get_active_template(conn, assessment_version_id, page_no)?
        .ok_or_else(|| CoreError::Invalid("当前默写页尚无老师确认的固定模板".into()))
}

fn materialization_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MaterializationRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ))
}

fn parse_materialization(raw: MaterializationRow) -> CoreResult<DictationPageMaterialization> {
    let value: serde_json::Value = serde_json::from_str(&raw.5)
        .map_err(|error| CoreError::Parse(format!("默写物化账本损坏：{error}")))?;
    let ids = serde_json::from_value(
        value
            .get("region_revision_ids")
            .cloned()
            .ok_or_else(|| CoreError::Invalid("默写物化账本缺少题区 ids".into()))?,
    )
    .map_err(|error| CoreError::Parse(format!("默写题区 ids 损坏：{error}")))?;
    Ok(DictationPageMaterialization {
        id: raw.0,
        public_id: raw.1,
        page_id: raw.2,
        template_revision_id: raw.3,
        alignment_revision_id: raw.4,
        region_revision_ids: ids,
        confirmed_by: raw.6,
        created_at: raw.7,
    })
}

pub fn get_page_materialization(
    conn: &Connection,
    page_id: i64,
    template_revision_id: i64,
) -> CoreResult<Option<DictationPageMaterialization>> {
    conn.query_row(
        "SELECT id,public_id,page_id,template_revision_id,alignment_revision_id,
                region_revision_ids_json,confirmed_by,created_at
         FROM exam_dictation_page_materializations_v2
         WHERE page_id=?1 AND template_revision_id=?2",
        (page_id, template_revision_id),
        materialization_row,
    )
    .optional()?
    .map(parse_materialization)
    .transpose()
}

fn load_alignment(conn: &Connection, id: i64) -> CoreResult<PageAlignmentRevision> {
    conn.query_row(
        "SELECT id,public_id,page_id,revision,match_revision_id,template_version,
                transform_json,confidence,aligned_artifact_id,decision,reason_code,
                confirmed_by,state FROM exam_page_alignment_revisions_v2 WHERE id=?1",
        [id],
        |row| {
            Ok(PageAlignmentRevision {
                id: row.get(0)?,
                public_id: row.get(1)?,
                page_id: row.get(2)?,
                revision: row.get(3)?,
                match_revision_id: row.get(4)?,
                template_version: row.get(5)?,
                transform_json: row.get(6)?,
                confidence: row.get(7)?,
                aligned_artifact_id: row.get(8)?,
                decision: row.get(9)?,
                reason_code: row.get(10)?,
                confirmed_by: row.get(11)?,
                state: row.get(12)?,
            })
        },
    )
    .map_err(Into::into)
}

fn load_region(conn: &Connection, id: i64) -> CoreResult<AnswerRegionRevision> {
    conn.query_row(
        "SELECT id,public_id,page_id,assessment_item_id,region_index,revision,
                alignment_revision_id,bbox_json,crop_artifact_id,mapping_confidence,
                decision,reason_code,confirmed_by,state
         FROM exam_answer_region_revisions_v2 WHERE id=?1",
        [id],
        |row| {
            Ok(AnswerRegionRevision {
                id: row.get(0)?,
                public_id: row.get(1)?,
                page_id: row.get(2)?,
                assessment_item_id: row.get(3)?,
                region_index: row.get(4)?,
                revision: row.get(5)?,
                alignment_revision_id: row.get(6)?,
                bbox_json: row.get(7)?,
                crop_artifact_id: row.get(8)?,
                mapping_confidence: row.get(9)?,
                decision: row.get(10)?,
                reason_code: row.get(11)?,
                confirmed_by: row.get(12)?,
                state: row.get(13)?,
            })
        },
    )
    .map_err(Into::into)
}

fn load_materialization_result(
    conn: &Connection,
    materialization: DictationPageMaterialization,
) -> CoreResult<DictationPageMaterializationResult> {
    let alignment = load_alignment(conn, materialization.alignment_revision_id)?;
    let regions = materialization
        .region_revision_ids
        .iter()
        .map(|id| load_region(conn, *id))
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(DictationPageMaterializationResult {
        materialization,
        alignment,
        regions,
    })
}

pub fn materialize_in_transaction(
    conn: &Connection,
    input: &MaterializeDictationPageInput<'_>,
) -> CoreResult<DictationPageMaterializationResult> {
    required(input.confirmed_by, "模板应用确认人")?;
    if let Some(existing) =
        get_page_materialization(conn, input.page_id, input.template_revision_id)?
    {
        return load_materialization_result(conn, existing);
    }
    let active = get_active_template_for_page(conn, input.page_id)?;
    if active.id != input.template_revision_id {
        return Err(CoreError::Invalid("默写页必须使用当前 active 模板".into()));
    }
    let proposals = template_regions(&active)?;
    let artifact_map = input
        .regions
        .iter()
        .map(|region| {
            (
                (region.assessment_item_id, region.region_index),
                region.crop_artifact_id,
            )
        })
        .collect::<BTreeMap<_, _>>();
    if artifact_map.len() != proposals.len() {
        return Err(CoreError::Invalid("默写题区裁剪与模板数量不一致".into()));
    }
    let transform_json = serde_json::json!({
        "schema_version": 1,
        "matrix": [1.0,0.0,0.0,0.0,1.0,0.0,0.0,0.0,1.0],
        "kind": "teacher_confirmed_fixed_dictation_template",
        "template_revision_id": active.id,
    })
    .to_string();
    let alignment = papers::record_page_alignment_in(
        conn,
        &NewPageAlignmentRevision {
            page_id: input.page_id,
            template_version: &active.template_version,
            transform_json: &transform_json,
            confidence: 1.0,
            aligned_artifact_id: Some(input.aligned_artifact_id),
            decision: "teacher_confirmed",
            reason_code: Some("FIXED_DICTATION_TEMPLATE_ACCEPTED"),
            confirmed_by: Some(input.confirmed_by.trim()),
        },
    )?;
    let mut regions = Vec::with_capacity(proposals.len());
    for proposal in proposals {
        let crop_artifact_id = artifact_map
            .get(&(proposal.assessment_item_id, proposal.region_index))
            .copied()
            .ok_or_else(|| CoreError::Invalid("默写模板题区缺少对应裁剪".into()))?;
        let bbox_json = serde_json::json!({
            "schema_version": 1,
            "x": proposal.bbox.x,
            "y": proposal.bbox.y,
            "width": proposal.bbox.width,
            "height": proposal.bbox.height,
            "template_revision_id": active.id,
        })
        .to_string();
        regions.push(papers::record_answer_region_in(
            conn,
            &NewAnswerRegionRevision {
                page_id: input.page_id,
                assessment_item_id: proposal.assessment_item_id,
                region_index: proposal.region_index,
                bbox_json: &bbox_json,
                crop_artifact_id: Some(crop_artifact_id),
                mapping_confidence: Some(proposal.mapping_confidence),
                decision: "teacher_confirmed",
                reason_code: Some("FIXED_DICTATION_TEMPLATE_ACCEPTED"),
                confirmed_by: Some(input.confirmed_by.trim()),
            },
        )?);
    }
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    let ids_json = serde_json::json!({
        "schema_version": 1,
        "region_revision_ids": regions.iter().map(|region| region.id).collect::<Vec<_>>(),
    })
    .to_string();
    conn.execute(
        "INSERT INTO exam_dictation_page_materializations_v2
         (public_id,page_id,template_revision_id,alignment_revision_id,
          region_revision_ids_json,confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        (
            &public_id,
            input.page_id,
            active.id,
            alignment.id,
            &ids_json,
            input.confirmed_by.trim(),
            &now,
        ),
    )?;
    let materialization = get_page_materialization(conn, input.page_id, active.id)?
        .ok_or_else(|| CoreError::NotFound("新建默写页物化账本".into()))?;
    Ok(DictationPageMaterializationResult {
        materialization,
        alignment,
        regions,
    })
}

#[derive(Debug, Deserialize)]
struct StoredPolicy {
    points: Vec<StoredPolicyPoint>,
}

#[derive(Debug, Deserialize)]
struct StoredPolicyPoint {
    rubric_point_id: i64,
    stable_id: String,
    canonical_text: String,
    #[serde(default)]
    accepted_variants: Vec<String>,
    max_score: f64,
}

#[derive(Debug)]
struct TranscriptionScope {
    attempt_id: i64,
    assessment_item_id: i64,
    policy: DictationPolicyRevision,
    crop_artifact_id: i64,
}

fn transcription_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DictationTranscriptionRevision> {
    Ok(DictationTranscriptionRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        attempt_id: row.get(2)?,
        assessment_item_id: row.get(3)?,
        answer_region_revision_id: row.get(4)?,
        revision: row.get(5)?,
        source_ai_run_id: row.get(6)?,
        result_state: row.get(7)?,
        raw_ocr_text: row.get(8)?,
        normalized_text: row.get(9)?,
        teacher_corrected_text: row.get(10)?,
        confidence: row.get(11)?,
        failure_meta_json: row.get(12)?,
        corrected_by: row.get(13)?,
        corrected_at: row.get(14)?,
        state: row.get(15)?,
        created_at: row.get(16)?,
    })
}

fn observation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DictationPointObservation> {
    Ok(DictationPointObservation {
        id: row.get(0)?,
        transcription_revision_id: row.get(1)?,
        policy_revision_id: row.get(2)?,
        rubric_point_id: row.get(3)?,
        result: row.get(4)?,
        evidence_text: row.get(5)?,
        rationale: row.get(6)?,
        suggested_score: row.get(7)?,
        confidence: row.get(8)?,
    })
}

fn recognition_state_name(state: DictationRecognitionState) -> &'static str {
    match state {
        DictationRecognitionState::Recognized => "recognized",
        DictationRecognitionState::NotWritten => "not_written",
        DictationRecognitionState::Unreadable => "unreadable",
        DictationRecognitionState::RecognizeFailed => "recognize_failed",
        DictationRecognitionState::AmbiguousFinal => "ambiguous_final",
    }
}

fn point_result_name(result: DictationPointResult) -> &'static str {
    match result {
        DictationPointResult::NotWritten => "not_written",
        DictationPointResult::Unreadable => "unreadable",
        DictationPointResult::RecognizeFailed => "recognize_failed",
        DictationPointResult::Exact => "exact",
        DictationPointResult::AcceptedVariant => "accepted_variant",
        DictationPointResult::Partial => "partial",
        DictationPointResult::Missing => "missing",
        DictationPointResult::Contradicted => "contradicted",
        DictationPointResult::ExtraWrong => "extra_wrong",
        DictationPointResult::AmbiguousFinal => "ambiguous_final",
        DictationPointResult::NeedsReview => "needs_review",
    }
}

fn load_transcription_scope(
    conn: &Connection,
    answer_region_revision_id: i64,
) -> CoreResult<TranscriptionScope> {
    let raw: Option<(i64, i64, i64, i64)> = conn
        .query_row(
            "SELECT m.attempt_id,r.assessment_item_id,p.id,r.crop_artifact_id
             FROM exam_answer_region_revisions_v2 r
             JOIN exam_page_alignment_revisions_v2 al
               ON al.id=r.alignment_revision_id AND al.state='active'
                  AND al.decision='teacher_confirmed'
             JOIN exam_page_match_revisions_v2 m
               ON m.id=al.match_revision_id AND m.state='active'
                  AND m.decision='teacher_confirmed'
             JOIN exam_attempts_v2 at ON at.id=m.attempt_id AND at.state<>'voided'
             JOIN exam_assessment_items_v2 i
               ON i.id=r.assessment_item_id
                  AND i.assessment_version_id=at.assessment_version_id AND i.state='active'
             JOIN exam_dictation_policy_revisions_v2 p
               ON p.assessment_item_id=i.id AND p.state='active'
             WHERE r.id=?1 AND r.state='active' AND r.decision='teacher_confirmed'
               AND r.crop_artifact_id IS NOT NULL",
            [answer_region_revision_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((attempt_id, assessment_item_id, policy_id, crop_artifact_id)) = raw else {
        return Err(CoreError::Invalid(
            "默写 OCR 只允许处理当前老师已确认且策略已冻结的题区".into(),
        ));
    };
    let policy = conn.query_row(
        "SELECT id,public_id,assessment_item_id,revision,answer_key_version_id,
                rubric_version_id,policy_hash,policy_json,confirmed_by,state,created_at
         FROM exam_dictation_policy_revisions_v2 WHERE id=?1",
        [policy_id],
        policy_row,
    )?;
    Ok(TranscriptionScope {
        attempt_id,
        assessment_item_id,
        policy,
        crop_artifact_id,
    })
}

fn load_active_transcription(
    conn: &Connection,
    scope: &TranscriptionScope,
    answer_region_revision_id: i64,
) -> CoreResult<Option<DictationTranscriptionRevision>> {
    Ok(conn
        .query_row(
            "SELECT id,public_id,attempt_id,assessment_item_id,answer_region_revision_id,
                    revision,source_ai_run_id,result_state,raw_ocr_text,normalized_text,
                    teacher_corrected_text,confidence,failure_meta_json,corrected_by,
                    corrected_at,state,created_at
             FROM exam_dictation_transcription_revisions_v2
             WHERE attempt_id=?1 AND assessment_item_id=?2
               AND answer_region_revision_id=?3 AND state='active'",
            (
                scope.attempt_id,
                scope.assessment_item_id,
                answer_region_revision_id,
            ),
            transcription_row,
        )
        .optional()?)
}

fn load_observation(
    conn: &Connection,
    transcription_revision_id: i64,
) -> CoreResult<DictationPointObservation> {
    conn.query_row(
        "SELECT id,transcription_revision_id,policy_revision_id,rubric_point_id,
                result,evidence_text,rationale,suggested_score,confidence
         FROM exam_dictation_point_observations_v2
         WHERE transcription_revision_id=?1",
        [transcription_revision_id],
        observation_row,
    )
    .map_err(Into::into)
}

fn policy_point(policy: &DictationPolicyRevision) -> CoreResult<StoredPolicyPoint> {
    let stored: StoredPolicy = serde_json::from_str(&policy.policy_json)
        .map_err(|error| CoreError::Parse(format!("默写策略 JSON 损坏：{error}")))?;
    if stored.points.len() != 1 {
        return Err(CoreError::Invalid("默写首版策略必须只有一个评分点".into()));
    }
    stored
        .points
        .into_iter()
        .next()
        .ok_or_else(|| CoreError::Invalid("默写策略没有评分点".into()))
}

struct NewTranscription<'a> {
    source_ai_run_id: Option<i64>,
    recognition_state: DictationRecognitionState,
    raw_ocr_text: Option<&'a str>,
    normalized_text: Option<&'a str>,
    teacher_corrected_text: Option<&'a str>,
    confidence: Option<f64>,
    failure_meta_json: Option<&'a str>,
    corrected_by: Option<&'a str>,
}

fn append_transcription(
    conn: &Connection,
    answer_region_revision_id: i64,
    input: &NewTranscription<'_>,
) -> CoreResult<DictationTranscriptionResult> {
    let scope = load_transcription_scope(conn, answer_region_revision_id)?;
    let point = policy_point(&scope.policy)?;
    let rule = DictationPointRule {
        stable_id: point.stable_id,
        canonical_text: point.canonical_text,
        accepted_variants: point.accepted_variants,
    };
    let comparison = compare_dictation_point(
        input.recognition_state,
        input.normalized_text,
        input.teacher_corrected_text,
        &rule,
        point.max_score,
    )?;
    let revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1
         FROM exam_dictation_transcription_revisions_v2
         WHERE attempt_id=?1 AND assessment_item_id=?2 AND answer_region_revision_id=?3",
        (
            scope.attempt_id,
            scope.assessment_item_id,
            answer_region_revision_id,
        ),
        |row| row.get(0),
    )?;
    conn.execute(
        "UPDATE exam_dictation_transcription_revisions_v2 SET state='superseded'
         WHERE attempt_id=?1 AND assessment_item_id=?2
           AND answer_region_revision_id=?3 AND state='active'",
        (
            scope.attempt_id,
            scope.assessment_item_id,
            answer_region_revision_id,
        ),
    )?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO exam_dictation_transcription_revisions_v2
         (public_id,attempt_id,assessment_item_id,answer_region_revision_id,revision,
          source_ai_run_id,result_state,raw_ocr_text,normalized_text,teacher_corrected_text,
          confidence,failure_meta_json,corrected_by,corrected_at,state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,'active',?15)",
        rusqlite::params![
            &public_id,
            scope.attempt_id,
            scope.assessment_item_id,
            answer_region_revision_id,
            revision,
            input.source_ai_run_id,
            recognition_state_name(input.recognition_state),
            input.raw_ocr_text,
            input.normalized_text,
            input.teacher_corrected_text,
            input.confidence,
            input.failure_meta_json,
            input.corrected_by,
            input.corrected_by.map(|_| now.as_str()),
            &now,
        ],
    )?;
    let transcription_id = conn.last_insert_rowid();
    let observation_public_id = ids::new_public_id();
    let rationale = match comparison.result {
        DictationPointResult::Exact => "与老师确认的标准内容精确一致",
        DictationPointResult::AcceptedVariant => "命中老师确认的可接受写法",
        DictationPointResult::NotWritten => "未检测到学生书写，需要老师确认",
        DictationPointResult::Unreadable => "字迹无法稳定识别，需要老师查看原图",
        DictationPointResult::RecognizeFailed => "识别失败，保留裁剪等待老师处理",
        DictationPointResult::AmbiguousFinal => "存在涂改且最终答案不明确",
        _ => "未精确命中已确认答案，不自动判错或给分",
    };
    conn.execute(
        "INSERT INTO exam_dictation_point_observations_v2
         (public_id,transcription_revision_id,policy_revision_id,rubric_point_id,result,
          evidence_text,rationale,suggested_score,confidence,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        rusqlite::params![
            &observation_public_id,
            transcription_id,
            scope.policy.id,
            point.rubric_point_id,
            point_result_name(comparison.result),
            comparison.compared_text,
            rationale,
            comparison.suggested_score,
            input.confidence,
            &now,
        ],
    )?;
    let transcription = conn.query_row(
        "SELECT id,public_id,attempt_id,assessment_item_id,answer_region_revision_id,
                revision,source_ai_run_id,result_state,raw_ocr_text,normalized_text,
                teacher_corrected_text,confidence,failure_meta_json,corrected_by,
                corrected_at,state,created_at
         FROM exam_dictation_transcription_revisions_v2 WHERE id=?1",
        [transcription_id],
        transcription_row,
    )?;
    let observation = load_observation(conn, transcription_id)?;
    Ok(DictationTranscriptionResult {
        transcription,
        observation,
    })
}

pub fn record_ocr_ai_run_transcription(
    conn: &mut Connection,
    ai_run_id: i64,
) -> CoreResult<DictationTranscriptionResult> {
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    if run.run_type != "dictation_ocr" || run.business_ref_type != "answer_region_revision" {
        return Err(CoreError::Invalid("当前 AI run 不是默写 OCR".into()));
    }
    let answer_region_revision_id = run
        .business_ref_id
        .parse::<i64>()
        .map_err(|_| CoreError::Invalid("默写 OCR 业务引用无效".into()))?;
    let scope = load_transcription_scope(conn, answer_region_revision_id)?;
    if run.input_artifact_id != Some(scope.crop_artifact_id) {
        return Err(CoreError::Invalid("默写 OCR run 与当前裁剪不一致".into()));
    }
    if let Some(existing) = load_active_transcription(conn, &scope, answer_region_revision_id)? {
        if existing.source_ai_run_id == Some(ai_run_id) {
            return Ok(DictationTranscriptionResult {
                observation: load_observation(conn, existing.id)?,
                transcription: existing,
            });
        }
    }
    let tx = conn.transaction()?;
    let result = match run.status {
        AiRunStatus::Succeeded => {
            let output: DictationOcrOutput = serde_json::from_str(
                run.output_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("成功的默写 OCR 缺少输出".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("默写 OCR 输出损坏：{error}")))?;
            if output.answer_region_revision_id != answer_region_revision_id
                || output.crop_artifact_id != scope.crop_artifact_id
                || output.input_hash != run.input_hash
            {
                return Err(CoreError::Invalid("默写 OCR 输出作用域不一致".into()));
            }
            append_transcription(
                &tx,
                answer_region_revision_id,
                &NewTranscription {
                    source_ai_run_id: Some(ai_run_id),
                    recognition_state: output.state,
                    raw_ocr_text: output.raw_text.as_deref(),
                    normalized_text: output.normalized_text.as_deref(),
                    teacher_corrected_text: None,
                    confidence: output.confidence,
                    failure_meta_json: None,
                    corrected_by: None,
                },
            )?
        }
        AiRunStatus::Failed => append_transcription(
            &tx,
            answer_region_revision_id,
            &NewTranscription {
                source_ai_run_id: Some(ai_run_id),
                recognition_state: DictationRecognitionState::RecognizeFailed,
                raw_ocr_text: None,
                normalized_text: None,
                teacher_corrected_text: None,
                confidence: None,
                failure_meta_json: Some(
                    run.error_meta_json
                        .as_deref()
                        .ok_or_else(|| CoreError::Invalid("失败的默写 OCR 缺少错误信息".into()))?,
                ),
                corrected_by: None,
            },
        )?,
        _ => return Err(CoreError::Invalid("默写 OCR 尚未形成最终结果".into())),
    };
    tx.commit()?;
    Ok(result)
}

pub fn teacher_correct_transcription(
    conn: &mut Connection,
    answer_region_revision_id: i64,
    corrected_text: &str,
    corrected_by: &str,
) -> CoreResult<DictationTranscriptionResult> {
    required(corrected_by, "校正人")?;
    required(corrected_text, "老师校正文本")?;
    let scope = load_transcription_scope(conn, answer_region_revision_id)?;
    let active = load_active_transcription(conn, &scope, answer_region_revision_id)?
        .ok_or_else(|| CoreError::NotFound("当前默写题区尚无转写结果".into()))?;
    if active.result_state != "recognized" {
        return Err(CoreError::Invalid(
            "识别失败、未写或无法辨认的题区请直接人工判定；只有已有 OCR 原文才能校正文案".into(),
        ));
    }
    let raw = active
        .raw_ocr_text
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("当前转写缺少原始 OCR".into()))?;
    let normalized = active
        .normalized_text
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("当前转写缺少规范化 OCR".into()))?;
    let tx = conn.transaction()?;
    let result = append_transcription(
        &tx,
        answer_region_revision_id,
        &NewTranscription {
            source_ai_run_id: active.source_ai_run_id,
            recognition_state: DictationRecognitionState::Recognized,
            raw_ocr_text: Some(raw),
            normalized_text: Some(normalized),
            teacher_corrected_text: Some(corrected_text.trim()),
            confidence: active.confidence,
            failure_meta_json: None,
            corrected_by: Some(corrected_by.trim()),
        },
    )?;
    audit::append(
        &tx,
        &audit::NewAuditEvent {
            idempotency_key: &format!(
                "exam:dictation-region:{answer_region_revision_id}:transcription:{}:corrected",
                result.transcription.revision
            ),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(corrected_by.trim()),
            action: "exam.dictation_transcription.corrected",
            object_type: "exam_answer_region_revision",
            object_id: &answer_region_revision_id.to_string(),
            object_revision: Some(result.transcription.revision),
            note: None,
            meta_json: Some(r#"{"schema_version":1}"#),
            occurred_at: &result.transcription.created_at,
        },
    )?;
    tx.commit()?;
    Ok(result)
}

#[derive(Debug)]
struct DictationReviewScope {
    assessment_version_id: i64,
    max_score: f64,
    transcription: DictationTranscriptionRevision,
    observation: DictationPointObservation,
    region_state: String,
    region_decision: String,
    policy_state: String,
}

fn review_scope(
    conn: &Connection,
    transcription_revision_id: i64,
) -> CoreResult<DictationReviewScope> {
    let raw = conn
        .query_row(
            "SELECT i.assessment_version_id,i.score,
                    t.id,t.public_id,t.attempt_id,t.assessment_item_id,
                    t.answer_region_revision_id,t.revision,t.source_ai_run_id,t.result_state,
                    t.raw_ocr_text,t.normalized_text,t.teacher_corrected_text,t.confidence,
                    t.failure_meta_json,t.corrected_by,t.corrected_at,t.state,t.created_at,
                    r.state,r.decision,p.state
             FROM exam_dictation_transcription_revisions_v2 t
             JOIN exam_dictation_point_observations_v2 o
               ON o.transcription_revision_id=t.id
             JOIN exam_dictation_policy_revisions_v2 p ON p.id=o.policy_revision_id
             JOIN exam_answer_region_revisions_v2 r ON r.id=t.answer_region_revision_id
             JOIN exam_attempts_v2 at ON at.id=t.attempt_id AND at.state<>'voided'
             JOIN exam_assessment_items_v2 i
               ON i.id=t.assessment_item_id
              AND i.assessment_version_id=at.assessment_version_id AND i.state='active'
             WHERE t.id=?1",
            [transcription_revision_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, f64>(1)?,
                    DictationTranscriptionRevision {
                        id: row.get(2)?,
                        public_id: row.get(3)?,
                        attempt_id: row.get(4)?,
                        assessment_item_id: row.get(5)?,
                        answer_region_revision_id: row.get(6)?,
                        revision: row.get(7)?,
                        source_ai_run_id: row.get(8)?,
                        result_state: row.get(9)?,
                        raw_ocr_text: row.get(10)?,
                        normalized_text: row.get(11)?,
                        teacher_corrected_text: row.get(12)?,
                        confidence: row.get(13)?,
                        failure_meta_json: row.get(14)?,
                        corrected_by: row.get(15)?,
                        corrected_at: row.get(16)?,
                        state: row.get(17)?,
                        created_at: row.get(18)?,
                    },
                    row.get::<_, String>(19)?,
                    row.get::<_, String>(20)?,
                    row.get::<_, String>(21)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::NotFound(format!(
                "dictation_transcription_revision#{transcription_revision_id}"
            ))
        })?;
    Ok(DictationReviewScope {
        assessment_version_id: raw.0,
        max_score: raw.1,
        observation: load_observation(conn, raw.2.id)?,
        transcription: raw.2,
        region_state: raw.3,
        region_decision: raw.4,
        policy_state: raw.5,
    })
}

fn active_review_reason(
    scope: &DictationReviewScope,
    strict_confidence: Option<f64>,
) -> Option<String> {
    if scope.transcription.state != "active" || scope.policy_state != "active" {
        return Some("SUPERSEDED_TRANSCRIPTION".into());
    }
    if scope.region_state != "active" || scope.region_decision != "teacher_confirmed" {
        return Some("REGION_NO_LONGER_CONFIRMED".into());
    }
    if !matches!(
        scope.observation.result.as_str(),
        "exact" | "accepted_variant"
    ) || scope.observation.suggested_score.is_none()
    {
        return Some(
            match scope.transcription.result_state.as_str() {
                "not_written" => "NOT_WRITTEN",
                "unreadable" => "UNREADABLE",
                "recognize_failed" => "RECOGNITION_FAILED",
                "ambiguous_final" => "AMBIGUOUS_FINAL",
                _ => "ANSWER_DIFFERS",
            }
            .into(),
        );
    }
    if let Some(threshold) = strict_confidence {
        if scope.transcription.result_state != "recognized" {
            return Some("NOT_BATCH_ELIGIBLE".into());
        }
        if scope
            .transcription
            .confidence
            .is_none_or(|value| value < threshold)
        {
            return Some("BELOW_BATCH_THRESHOLD".into());
        }
    }
    None
}

fn dictation_point_results(
    scope: &DictationReviewScope,
    source: &str,
    teacher_score: f64,
    teacher_evidence_text: Option<&str>,
) -> String {
    serde_json::json!({
        "schema_version": 1,
        "source": source,
        "transcription_revision_id": scope.transcription.id,
        "transcription_revision": scope.transcription.revision,
        "point_observation_id": scope.observation.id,
        "rubric_point_id": scope.observation.rubric_point_id,
        "result": scope.observation.result,
        "machine_evidence_text": scope.observation.evidence_text,
        "teacher_evidence_text": teacher_evidence_text,
        "suggested_score": scope.observation.suggested_score,
        "teacher_score": teacher_score,
        "max_score": scope.max_score
    })
    .to_string()
}

struct DictationDecisionSource<'a> {
    decision_id: i64,
    scope: &'a DictationReviewScope,
    review_mode: &'a str,
    batch_id: Option<i64>,
    teacher_evidence_text: Option<&'a str>,
    reviewed_by: &'a str,
    now: &'a str,
}

fn link_decision_source(conn: &Connection, input: &DictationDecisionSource<'_>) -> CoreResult<()> {
    let evidence = input
        .teacher_evidence_text
        .map(str::trim)
        .filter(|value| !value.is_empty());
    conn.execute(
        "INSERT INTO exam_grade_decision_dictation_sources_v2
         (grade_decision_id,transcription_revision_id,point_observation_id,
          review_mode,batch_id,teacher_evidence_text,reviewed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(grade_decision_id) DO NOTHING",
        (
            input.decision_id,
            input.scope.transcription.id,
            input.scope.observation.id,
            input.review_mode,
            input.batch_id,
            evidence,
            input.reviewed_by,
            input.now,
        ),
    )?;
    let existing: (i64, i64, String, Option<i64>, Option<String>, String) = conn.query_row(
        "SELECT transcription_revision_id,point_observation_id,review_mode,batch_id,
                teacher_evidence_text,reviewed_by
         FROM exam_grade_decision_dictation_sources_v2 WHERE grade_decision_id=?1",
        [input.decision_id],
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
    )?;
    if existing
        != (
            input.scope.transcription.id,
            input.scope.observation.id,
            input.review_mode.to_string(),
            input.batch_id,
            evidence.map(str::to_string),
            input.reviewed_by.to_string(),
        )
    {
        return Err(CoreError::Invalid(
            "评分 revision 已绑定不同的默写证据来源".into(),
        ));
    }
    Ok(())
}

pub fn accept_dictation_suggestion(
    conn: &Connection,
    transcription_revision_id: i64,
    reviewed_by: &str,
) -> CoreResult<GradeDecision> {
    required(reviewed_by, "默写终审人")?;
    let tx = conn.unchecked_transaction()?;
    let scope = review_scope(&tx, transcription_revision_id)?;
    if let Some(reason) = active_review_reason(&scope, None) {
        return Err(CoreError::Invalid(format!(
            "该默写结果不能直接接受：{reason}"
        )));
    }
    let score = scope.observation.suggested_score.expect("validated score");
    let point_results = dictation_point_results(&scope, "dictation_teacher_accept", score, None);
    let confirmation_level = if scope.transcription.corrected_by.is_some() {
        "teacher_corrected"
    } else {
        "teacher_accepted"
    };
    let decision = decide_grade_in_transaction(
        &tx,
        &NewGradeDecision {
            attempt_id: scope.transcription.attempt_id,
            assessment_item_id: scope.transcription.assessment_item_id,
            machine_grade_ai_run_id: None,
            teacher_score: score,
            point_results_json: &point_results,
            teacher_note: None,
            confirmation_level,
            decided_by: reviewed_by,
        },
    )?;
    let now = time::utc_now_rfc3339();
    link_decision_source(
        &tx,
        &DictationDecisionSource {
            decision_id: decision.id,
            scope: &scope,
            review_mode: "single",
            batch_id: None,
            teacher_evidence_text: None,
            reviewed_by: reviewed_by.trim(),
            now: &now,
        },
    )?;
    tx.commit()?;
    Ok(decision)
}

/// 老师查看原始裁剪后，可对分歧、未写、无法辨认或识别失败项人工记分。
/// `teacher_evidence_text` 只记录老师看到的实际内容，不改写原始 OCR。
pub fn correct_dictation_grade(
    conn: &Connection,
    transcription_revision_id: i64,
    teacher_score: f64,
    teacher_note: Option<&str>,
    teacher_evidence_text: Option<&str>,
    reviewed_by: &str,
) -> CoreResult<GradeDecision> {
    required(reviewed_by, "默写终审人")?;
    let note = teacher_note
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CoreError::Invalid("默写人工记分必须填写判定依据".into()))?;
    let evidence = teacher_evidence_text
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let tx = conn.unchecked_transaction()?;
    let scope = review_scope(&tx, transcription_revision_id)?;
    if scope.transcription.state != "active" || scope.policy_state != "active" {
        return Err(CoreError::Invalid("默写转写已被后续 revision 替代".into()));
    }
    if scope.region_state != "active" || scope.region_decision != "teacher_confirmed" {
        return Err(CoreError::Invalid(
            "答案区域已变化，请先重新核对页面证据".into(),
        ));
    }
    if !teacher_score.is_finite()
        || teacher_score < 0.0
        || teacher_score > scope.max_score + 0.000_001
    {
        return Err(CoreError::Invalid(format!(
            "人工得分必须位于 0~{} 分",
            scope.max_score
        )));
    }
    let point_results = dictation_point_results(
        &scope,
        "dictation_teacher_correction",
        teacher_score,
        evidence,
    );
    let decision = decide_grade_in_transaction(
        &tx,
        &NewGradeDecision {
            attempt_id: scope.transcription.attempt_id,
            assessment_item_id: scope.transcription.assessment_item_id,
            machine_grade_ai_run_id: None,
            teacher_score,
            point_results_json: &point_results,
            teacher_note: Some(note),
            confirmation_level: "teacher_corrected",
            decided_by: reviewed_by,
        },
    )?;
    let now = time::utc_now_rfc3339();
    link_decision_source(
        &tx,
        &DictationDecisionSource {
            decision_id: decision.id,
            scope: &scope,
            review_mode: "single",
            batch_id: None,
            teacher_evidence_text: evidence,
            reviewed_by: reviewed_by.trim(),
            now: &now,
        },
    )?;
    tx.commit()?;
    Ok(decision)
}

fn get_review_batch(conn: &Connection, id: i64) -> CoreResult<DictationReviewBatch> {
    let mut batch = conn.query_row(
        "SELECT id,public_id,assessment_version_id,idempotency_key,requested_count,
                confirmed_count,excluded_count,created_by
         FROM exam_dictation_review_batches_v2 WHERE id=?1",
        [id],
        |row| {
            Ok(DictationReviewBatch {
                id: row.get(0)?,
                public_id: row.get(1)?,
                assessment_version_id: row.get(2)?,
                idempotency_key: row.get(3)?,
                requested_count: row.get(4)?,
                confirmed_count: row.get(5)?,
                excluded_count: row.get(6)?,
                created_by: row.get(7)?,
                items: Vec::new(),
            })
        },
    )?;
    let mut stmt = conn.prepare(
        "SELECT transcription_revision_id,outcome,reason_code,grade_decision_id
         FROM exam_dictation_review_batch_items_v2 WHERE batch_id=?1 ORDER BY id",
    )?;
    batch.items = stmt
        .query_map([id], |row| {
            Ok(DictationBatchItemResult {
                transcription_revision_id: row.get(0)?,
                outcome: row.get(1)?,
                reason_code: row.get(2)?,
                grade_decision_id: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(batch)
}

pub fn strict_batch_accept(
    conn: &Connection,
    input: &StrictDictationBatchReview<'_>,
) -> CoreResult<DictationReviewBatch> {
    required(input.reviewed_by, "默写批量终审人")?;
    required(input.idempotency_key, "默写批量终审幂等键")?;
    let transcription_ids: Vec<i64> = input
        .transcription_revision_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if transcription_ids.is_empty() {
        return Err(CoreError::Invalid(
            "严格批量确认至少需要一条默写结果".into(),
        ));
    }
    let request = serde_json::json!({
        "schema_version": 1,
        "transcription_revision_ids": transcription_ids,
        "confidence_micros": (DICTATION_STRICT_BATCH_CONFIDENCE * 1_000_000.0).round() as i64,
        "reviewed_by": input.reviewed_by.trim()
    });
    let request_hash = hashing::sha256_hex(
        &serde_json::to_vec(&request)
            .map_err(|error| CoreError::Parse(format!("默写批量终审 hash 失败：{error}")))?,
    );
    if let Some((id, hash)) = conn
        .query_row(
            "SELECT id,request_hash FROM exam_dictation_review_batches_v2
             WHERE idempotency_key=?1",
            [input.idempotency_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if hash != request_hash {
            return Err(CoreError::Invalid(
                "默写批量终审幂等键已属于不同请求".into(),
            ));
        }
        return get_review_batch(conn, id);
    }
    let tx = conn.unchecked_transaction()?;
    let mut scopes = Vec::with_capacity(transcription_ids.len());
    let mut assessment_version_id = None;
    for id in &transcription_ids {
        let scope = review_scope(&tx, *id)?;
        match assessment_version_id {
            Some(version_id) if version_id != scope.assessment_version_id => {
                return Err(CoreError::Invalid(
                    "一次严格批量终审只能处理同一作业版本".into(),
                ));
            }
            None => assessment_version_id = Some(scope.assessment_version_id),
            _ => {}
        }
        scopes.push(scope);
    }
    let mut reasons = Vec::with_capacity(scopes.len());
    for scope in &scopes {
        let already_confirmed: bool = tx.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM exam_grade_decision_dictation_sources_v2 src
               JOIN exam_grade_decisions_v2 d ON d.id=src.grade_decision_id
               WHERE src.transcription_revision_id=?1 AND d.state='active'
             )",
            [scope.transcription.id],
            |row| row.get(0),
        )?;
        reasons.push(if already_confirmed {
            Some("ALREADY_CONFIRMED".into())
        } else {
            active_review_reason(scope, Some(DICTATION_STRICT_BATCH_CONFIDENCE))
        });
    }
    let confirmed_count = reasons.iter().filter(|reason| reason.is_none()).count() as i64;
    let excluded_count = reasons.len() as i64 - confirmed_count;
    let now = time::utc_now_rfc3339();
    tx.execute(
        "INSERT INTO exam_dictation_review_batches_v2
         (public_id,assessment_version_id,idempotency_key,request_hash,requested_count,
          confirmed_count,excluded_count,state,created_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,'completed',?8,?9)",
        (
            ids::new_public_id(),
            assessment_version_id.expect("non-empty scopes"),
            input.idempotency_key.trim(),
            &request_hash,
            scopes.len() as i64,
            confirmed_count,
            excluded_count,
            input.reviewed_by.trim(),
            &now,
        ),
    )?;
    let batch_id = tx.last_insert_rowid();
    for (scope, reason) in scopes.iter().zip(reasons) {
        let decision = if reason.is_none() {
            let score = scope.observation.suggested_score.expect("validated score");
            let point_results =
                dictation_point_results(scope, "dictation_strict_batch", score, None);
            let confirmation_level = if scope.transcription.corrected_by.is_some() {
                "teacher_corrected"
            } else {
                "teacher_accepted"
            };
            let decision = decide_grade_in_transaction(
                &tx,
                &NewGradeDecision {
                    attempt_id: scope.transcription.attempt_id,
                    assessment_item_id: scope.transcription.assessment_item_id,
                    machine_grade_ai_run_id: None,
                    teacher_score: score,
                    point_results_json: &point_results,
                    teacher_note: None,
                    confirmation_level,
                    decided_by: input.reviewed_by,
                },
            )?;
            link_decision_source(
                &tx,
                &DictationDecisionSource {
                    decision_id: decision.id,
                    scope,
                    review_mode: "strict_batch",
                    batch_id: Some(batch_id),
                    teacher_evidence_text: None,
                    reviewed_by: input.reviewed_by.trim(),
                    now: &now,
                },
            )?;
            Some(decision)
        } else {
            None
        };
        tx.execute(
            "INSERT INTO exam_dictation_review_batch_items_v2
             (batch_id,transcription_revision_id,outcome,reason_code,grade_decision_id,created_at)
             VALUES (?1,?2,?3,?4,?5,?6)",
            (
                batch_id,
                scope.transcription.id,
                if decision.is_some() {
                    "confirmed"
                } else {
                    "excluded"
                },
                reason.as_deref(),
                decision.as_ref().map(|value| value.id),
                &now,
            ),
        )?;
    }
    let batch = get_review_batch(&tx, batch_id)?;
    tx.commit()?;
    Ok(batch)
}

pub fn list_dictation_workbench(
    conn: &Connection,
    assessment_version_id: Option<i64>,
    limit: i64,
) -> CoreResult<DictationWorkbench> {
    list_dictation_workbench_scoped(conn, assessment_version_id, limit, None)
}

pub fn list_dictation_workbench_scoped(
    conn: &Connection,
    assessment_version_id: Option<i64>,
    limit: i64,
    attempt_ids: Option<&[i64]>,
) -> CoreResult<DictationWorkbench> {
    let scope = attempt_ids
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| CoreError::Parse(format!("批改任务范围无法读取：{error}")))?;
    let limit = if attempt_ids.is_some() {
        i64::MAX
    } else {
        limit.clamp(1, 2000)
    };
    let mut stmt = conn.prepare(
        "SELECT a.id,v.id,a.title,at.id,at.state,at.active_publication_id,
                st.id,st.student_no,st.name,i.id,i.order_index,
                COALESCE(CAST(json_extract(i.presentation_snapshot_json,'$.question_no') AS TEXT),
                         CAST(i.order_index + 1 AS TEXT)),
                q.question_type,q.stem,i.score,r.id,art.archived_path,
                t.id,t.revision,t.source_ai_run_id,t.result_state,t.raw_ocr_text,
                t.normalized_text,t.teacher_corrected_text,t.confidence,o.result,
                p.policy_json,o.suggested_score,
                d.id,d.revision,d.teacher_score,d.confirmation_level,src.review_mode,
                CASE WHEN src.transcription_revision_id=t.id THEN 1 ELSE 0 END,d.decided_at
         FROM exam_dictation_transcription_revisions_v2 t
         JOIN exam_dictation_point_observations_v2 o
           ON o.transcription_revision_id=t.id
         JOIN exam_dictation_policy_revisions_v2 p
           ON p.id=o.policy_revision_id AND p.state='active'
         JOIN exam_answer_region_revisions_v2 r
           ON r.id=t.answer_region_revision_id AND r.state='active'
         LEFT JOIN artifacts art ON art.id=r.crop_artifact_id
         JOIN exam_attempts_v2 at ON at.id=t.attempt_id AND at.state<>'voided'
         JOIN students st ON st.id=at.student_id
         JOIN exam_assessment_items_v2 i
           ON i.id=t.assessment_item_id AND i.state='active'
         JOIN exam_assessment_versions_v2 v ON v.id=i.assessment_version_id
         JOIN exam_assessments_v2 a ON a.id=v.assessment_id
         JOIN k1_question_versions q ON q.id=i.question_version_id
         LEFT JOIN exam_grade_decisions_v2 d
           ON d.attempt_id=at.id AND d.assessment_item_id=i.id AND d.state='active'
         LEFT JOIN exam_grade_decision_dictation_sources_v2 src
           ON src.grade_decision_id=d.id
         WHERE t.state='active' AND (?1 IS NULL OR v.id=?1)
           AND (?3 IS NULL OR at.id IN (SELECT value FROM json_each(?3)))
         ORDER BY v.id DESC,i.order_index,st.student_no,at.attempt_no
         LIMIT ?2",
    )?;
    let rows = stmt.query_map((assessment_version_id, limit, scope.as_deref()), |row| {
        Ok((
            DictationWorkbenchRow {
                assessment_id: row.get(0)?,
                assessment_version_id: row.get(1)?,
                assessment_title: row.get(2)?,
                attempt_id: row.get(3)?,
                attempt_state: row.get(4)?,
                active_publication_id: row.get(5)?,
                student_id: row.get(6)?,
                student_no: row.get(7)?,
                student_name: row.get(8)?,
                assessment_item_id: row.get(9)?,
                order_index: row.get(10)?,
                question_no: row.get(11)?,
                question_type: row.get(12)?,
                question_stem: row.get(13)?,
                max_score: row.get(14)?,
                answer_region_revision_id: row.get(15)?,
                crop_path: row.get(16)?,
                transcription_revision_id: row.get(17)?,
                transcription_revision: row.get(18)?,
                source_ai_run_id: row.get(19)?,
                result_state: row.get(20)?,
                raw_ocr_text: row.get(21)?,
                normalized_text: row.get(22)?,
                teacher_corrected_text: row.get(23)?,
                confidence: row.get(24)?,
                point_result: row.get(25)?,
                canonical_text: String::new(),
                accepted_variants: Vec::new(),
                suggested_score: row.get(27)?,
                requires_teacher_review: true,
                grade_decision_id: row.get(28)?,
                grade_decision_revision: row.get(29)?,
                teacher_score: row.get(30)?,
                confirmation_level: row.get(31)?,
                review_mode: row.get(32)?,
                current_transcription_confirmed: row.get(33)?,
                decided_at: row.get(34)?,
            },
            row.get::<_, String>(26)?,
        ))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (mut row, policy_json) = row?;
        let policy: StoredPolicy = serde_json::from_str(&policy_json)
            .map_err(|error| CoreError::Parse(format!("默写策略 JSON 损坏：{error}")))?;
        let point = policy
            .points
            .into_iter()
            .next()
            .ok_or_else(|| CoreError::Invalid("默写策略没有评分点".into()))?;
        row.requires_teacher_review =
            !matches!(row.point_result.as_str(), "exact" | "accepted_variant");
        row.canonical_text = point.canonical_text;
        row.accepted_variants = point.accepted_variants;
        result.push(row);
    }
    drop(stmt);

    let mut attempt_stmt = conn.prepare(
        "SELECT a.id,v.id,a.title,at.id,at.state,at.active_publication_id,
                st.id,st.student_no,st.name,
                (SELECT COUNT(*) FROM exam_assessment_items_v2 i
                 WHERE i.assessment_version_id=v.id AND i.state='active'),
                (SELECT COUNT(*) FROM exam_dictation_transcription_revisions_v2 t
                 WHERE t.attempt_id=at.id AND t.state='active'),
                (SELECT COUNT(*) FROM exam_grade_decisions_v2 d
                 JOIN exam_assessment_items_v2 i ON i.id=d.assessment_item_id
                 WHERE d.attempt_id=at.id AND d.state='active'
                   AND i.assessment_version_id=v.id AND i.state='active'),
                (SELECT COALESCE(SUM(d.teacher_score),0.0)
                 FROM exam_grade_decisions_v2 d
                 JOIN exam_assessment_items_v2 i ON i.id=d.assessment_item_id
                 WHERE d.attempt_id=at.id AND d.state='active'
                   AND i.assessment_version_id=v.id AND i.state='active'),
                (SELECT COALESCE(SUM(i.score),0.0)
                 FROM exam_assessment_items_v2 i
                 WHERE i.assessment_version_id=v.id AND i.state='active'),
                pi.total_score,
                CASE WHEN at.state='ready_to_publish' THEN 1 ELSE 0 END
         FROM exam_attempts_v2 at
         JOIN students st ON st.id=at.student_id
         JOIN exam_assessment_versions_v2 v ON v.id=at.assessment_version_id
         JOIN exam_assessments_v2 a ON a.id=v.assessment_id
         LEFT JOIN exam_grade_publication_items_v2 pi
           ON pi.publication_id=at.active_publication_id AND pi.attempt_id=at.id
         WHERE at.state<>'voided' AND (?1 IS NULL OR v.id=?1)
           AND EXISTS(SELECT 1 FROM exam_dictation_transcription_revisions_v2 t
                      WHERE t.attempt_id=at.id AND t.state='active')
           AND (?3 IS NULL OR at.id IN (SELECT value FROM json_each(?3)))
         ORDER BY v.id DESC,st.student_no,at.attempt_no
         LIMIT ?2",
    )?;
    let attempts = attempt_stmt
        .query_map((assessment_version_id, limit, scope.as_deref()), |row| {
            Ok(DictationAttemptSummary {
                assessment_id: row.get(0)?,
                assessment_version_id: row.get(1)?,
                assessment_title: row.get(2)?,
                attempt_id: row.get(3)?,
                attempt_state: row.get(4)?,
                active_publication_id: row.get(5)?,
                student_id: row.get(6)?,
                student_no: row.get(7)?,
                student_name: row.get(8)?,
                item_count: row.get(9)?,
                observed_count: row.get(10)?,
                confirmed_count: row.get(11)?,
                teacher_total_score: row.get(12)?,
                max_total_score: row.get(13)?,
                published_total_score: row.get(14)?,
                can_publish: row.get(15)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(DictationWorkbench {
        rows: result,
        attempts,
    })
}

pub fn template_source(
    conn: &Connection,
    assessment_version_id: i64,
    page_no: i64,
) -> CoreResult<Option<DictationTemplateSource>> {
    let Some(template) = get_active_template(conn, assessment_version_id, page_no)? else {
        return Ok(None);
    };
    let value: serde_json::Value = serde_json::from_str(&template.template_json)
        .map_err(|error| CoreError::Parse(format!("默写模板 JSON 损坏：{error}")))?;
    let blank_artifact_id = value
        .get("blank_artifact_id")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| CoreError::Invalid("默写模板缺少空白 artifact".into()))?;
    let hash = value
        .get("blank_artifact_sha256")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| CoreError::Invalid("默写模板缺少空白 hash".into()))?
        .to_string();
    let regions = template_regions(&template)?;
    Ok(Some(DictationTemplateSource {
        template,
        blank_artifact_id,
        blank_artifact_sha256: hash,
        regions,
    }))
}
