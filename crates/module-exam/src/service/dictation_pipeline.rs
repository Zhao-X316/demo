//! 固定格式默写模板确认、评分策略冻结与学生页题区物化。
//!
//! 首版把“一行/一栏/一个题号”作为一个 assessment item，并要求每项只有一个已确认
//! rubric point。老师确认一次空白页模板后，后续学生页按该模板裁区；OCR 和老师校正
//! 另写追加式 transcription，不在本服务中产生分数、发布或学习证据。

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

use super::papers::{
    self, AnswerRegionRevision, NewAnswerRegionRevision, NewPageAlignmentRevision,
    PageAlignmentRevision,
};

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
    pub assessment_version_id: i64,
    pub assessment_title: String,
    pub attempt_id: i64,
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

pub fn list_dictation_workbench(
    conn: &Connection,
    assessment_version_id: Option<i64>,
    limit: i64,
) -> CoreResult<Vec<DictationWorkbenchRow>> {
    let limit = limit.clamp(1, 2000);
    let mut stmt = conn.prepare(
        "SELECT v.id,a.title,at.id,st.id,st.student_no,st.name,
                i.id,i.order_index,
                COALESCE(CAST(json_extract(i.presentation_snapshot_json,'$.question_no') AS TEXT),
                         CAST(i.order_index + 1 AS TEXT)),
                q.question_type,q.stem,i.score,r.id,art.archived_path,
                t.id,t.revision,t.source_ai_run_id,t.result_state,t.raw_ocr_text,
                t.normalized_text,t.teacher_corrected_text,t.confidence,o.result,
                p.policy_json,o.suggested_score
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
         WHERE t.state='active' AND (?1 IS NULL OR v.id=?1)
         ORDER BY v.id DESC,i.order_index,st.student_no,at.attempt_no
         LIMIT ?2",
    )?;
    let rows = stmt.query_map((assessment_version_id, limit), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, f64>(11)?,
            row.get::<_, i64>(12)?,
            row.get::<_, Option<String>>(13)?,
            row.get::<_, i64>(14)?,
            row.get::<_, i64>(15)?,
            row.get::<_, Option<i64>>(16)?,
            row.get::<_, String>(17)?,
            row.get::<_, Option<String>>(18)?,
            row.get::<_, Option<String>>(19)?,
            row.get::<_, Option<String>>(20)?,
            row.get::<_, Option<f64>>(21)?,
            row.get::<_, String>(22)?,
            row.get::<_, String>(23)?,
            row.get::<_, Option<f64>>(24)?,
        ))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let row = row?;
        let policy: StoredPolicy = serde_json::from_str(&row.23)
            .map_err(|error| CoreError::Parse(format!("默写策略 JSON 损坏：{error}")))?;
        let point = policy
            .points
            .into_iter()
            .next()
            .ok_or_else(|| CoreError::Invalid("默写策略没有评分点".into()))?;
        result.push(DictationWorkbenchRow {
            assessment_version_id: row.0,
            assessment_title: row.1,
            attempt_id: row.2,
            student_id: row.3,
            student_no: row.4,
            student_name: row.5,
            assessment_item_id: row.6,
            order_index: row.7,
            question_no: row.8,
            question_type: row.9,
            question_stem: row.10,
            max_score: row.11,
            answer_region_revision_id: row.12,
            crop_path: row.13,
            transcription_revision_id: row.14,
            transcription_revision: row.15,
            source_ai_run_id: row.16,
            result_state: row.17,
            raw_ocr_text: row.18,
            normalized_text: row.19,
            teacher_corrected_text: row.20,
            confidence: row.21,
            point_result: row.22.clone(),
            canonical_text: point.canonical_text,
            accepted_variants: point.accepted_variants,
            suggested_score: row.24,
            requires_teacher_review: !matches!(row.22.as_str(), "exact" | "accepted_variant"),
        });
    }
    Ok(result)
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
