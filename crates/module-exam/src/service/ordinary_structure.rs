//! 普通试卷整页 AI 结构候选的老师确认与物化。
//!
//! succeeded `ai_run` 仍只是机器建议；只有老师显式确认后，才会在一个事务里创建
//! teacher-confirmed alignment/region revisions，并把来源 run 固定到不可变账本。

use std::collections::{BTreeMap, BTreeSet};

use image::codecs::jpeg::JpegEncoder;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::{ai_runs, artifacts, audit};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRunStatus, ArchiveStatus, ArtifactKind, AuditActorType, PrivacyClass};

use crate::ordinary_paper_recognition::{
    NormalizedRect, OrdinaryPaperQualityResult, OrdinaryPaperQuestionType,
    OrdinaryPaperRecognitionOutput, OrdinaryPaperRecognitionState, ORDINARY_PAPER_READY_CONFIDENCE,
};

use super::papers::{
    self, AnswerRegionRevision, NewAnswerRegionRevision, NewPageAlignmentRevision,
    PageAlignmentRevision,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrdinaryRegionArtifact {
    pub assessment_item_id: i64,
    pub region_index: i64,
    pub crop_artifact_id: i64,
}

pub struct ConfirmOrdinaryStructureInput<'a> {
    pub page_id: i64,
    pub ai_run_id: i64,
    pub aligned_artifact_id: i64,
    pub region_artifacts: &'a [OrdinaryRegionArtifact],
    pub confirmed_by: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdinaryStructureConfirmation {
    pub id: i64,
    pub public_id: String,
    pub ai_run_id: i64,
    pub page_id: i64,
    pub alignment_revision_id: i64,
    pub region_revision_ids: Vec<i64>,
    pub confirmed_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrdinaryStructureConfirmationResult {
    pub confirmation: OrdinaryStructureConfirmation,
    pub alignment: PageAlignmentRevision,
    pub regions: Vec<AnswerRegionRevision>,
}

type ConfirmationRow = (i64, String, i64, i64, i64, String, String, String);

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!(
            "普通试卷结构确认 {field} 不能为空"
        )))
    } else {
        Ok(())
    }
}

fn confirmation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ConfirmationRow> {
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

fn parse_confirmation(raw: ConfirmationRow) -> CoreResult<OrdinaryStructureConfirmation> {
    let value: serde_json::Value = serde_json::from_str(&raw.5)
        .map_err(|error| CoreError::Parse(format!("普通试卷结构题区账本损坏：{error}")))?;
    let region_revision_ids = serde_json::from_value(
        value
            .get("region_revision_ids")
            .cloned()
            .ok_or_else(|| CoreError::Invalid("普通试卷结构题区账本缺少 revision ids".into()))?,
    )
    .map_err(|error| CoreError::Parse(format!("普通试卷结构题区 ids 无效：{error}")))?;
    Ok(OrdinaryStructureConfirmation {
        id: raw.0,
        public_id: raw.1,
        ai_run_id: raw.2,
        page_id: raw.3,
        alignment_revision_id: raw.4,
        region_revision_ids,
        confirmed_by: raw.6,
        created_at: raw.7,
    })
}

pub fn get_confirmation_by_ai_run(
    conn: &Connection,
    ai_run_id: i64,
) -> CoreResult<Option<OrdinaryStructureConfirmation>> {
    let raw = conn
        .query_row(
            "SELECT id,public_id,ai_run_id,page_id,alignment_revision_id,
                    region_revision_ids_json,confirmed_by,created_at
             FROM exam_ordinary_structure_confirmations_v2 WHERE ai_run_id=?1",
            [ai_run_id],
            confirmation_row,
        )
        .optional()?;
    raw.map(parse_confirmation).transpose()
}

fn load_result(
    conn: &Connection,
    confirmation: OrdinaryStructureConfirmation,
) -> CoreResult<OrdinaryStructureConfirmationResult> {
    let alignment = conn
        .query_row(
            "SELECT id,public_id,page_id,revision,match_revision_id,template_version,
                    transform_json,confidence,aligned_artifact_id,decision,reason_code,
                    confirmed_by,state
             FROM exam_page_alignment_revisions_v2 WHERE id=?1",
            [confirmation.alignment_revision_id],
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
        .optional()?
        .ok_or_else(|| CoreError::NotFound("普通试卷结构账本引用的配准 revision".into()))?;
    let mut regions = Vec::with_capacity(confirmation.region_revision_ids.len());
    for region_id in &confirmation.region_revision_ids {
        let region = conn
            .query_row(
                "SELECT id,public_id,page_id,assessment_item_id,region_index,revision,
                        alignment_revision_id,bbox_json,crop_artifact_id,mapping_confidence,
                        decision,reason_code,confirmed_by,state
                 FROM exam_answer_region_revisions_v2 WHERE id=?1",
                [region_id],
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
            .optional()?
            .ok_or_else(|| CoreError::NotFound(format!("普通试卷题区 revision#{region_id}")))?;
        regions.push(region);
    }
    Ok(OrdinaryStructureConfirmationResult {
        confirmation,
        alignment,
        regions,
    })
}

fn validate_ready_output(output: &OrdinaryPaperRecognitionOutput) -> CoreResult<()> {
    if output.state != OrdinaryPaperRecognitionState::Ready
        || output.quality.result != OrdinaryPaperQualityResult::Pass
        || output.confidence < ORDINARY_PAPER_READY_CONFIDENCE
        || !output.issue_codes.is_empty()
        || match &output.alignment {
            Some(alignment) => alignment.confidence < ORDINARY_PAPER_READY_CONFIDENCE,
            None => true,
        }
        || output.regions.is_empty()
        || output.regions.iter().any(|region| {
            region.mapping_confidence < ORDINARY_PAPER_READY_CONFIDENCE
                || region.mark_cells.len() < 2
        })
    {
        return Err(CoreError::Invalid(
            "只有质量、配准、全部题区和答题格均达到 ready 的普通试卷候选才能确认".into(),
        ));
    }
    Ok(())
}

fn question_type_from_db(value: &str) -> CoreResult<OrdinaryPaperQuestionType> {
    OrdinaryPaperQuestionType::from_db(value)
        .ok_or_else(|| CoreError::Invalid("普通试卷结构候选包含非客观题".into()))
}

/// 在调用方持有的事务中物化一页普通试卷结构。
pub fn confirm_in_transaction(
    conn: &Connection,
    input: &ConfirmOrdinaryStructureInput<'_>,
) -> CoreResult<OrdinaryStructureConfirmationResult> {
    if input.page_id <= 0 || input.ai_run_id <= 0 || input.aligned_artifact_id <= 0 {
        return Err(CoreError::Invalid(
            "普通试卷结构确认的页面、run 与配准 artifact id 必须为正数".into(),
        ));
    }
    required(input.confirmed_by, "确认人")?;
    if let Some(existing) = get_confirmation_by_ai_run(conn, input.ai_run_id)? {
        if existing.page_id != input.page_id {
            return Err(CoreError::Invalid(
                "普通试卷结构 run 已被用于另一页面，拒绝复用".into(),
            ));
        }
        return load_result(conn, existing);
    }

    let run = ai_runs::get_by_id(conn, input.ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{}", input.ai_run_id)))?;
    if run.status != AiRunStatus::Succeeded
        || run.run_type != "ordinary_paper_structure"
        || run.source_module != "exam"
        || run.business_ref_type != "ingest_page"
        || run.business_ref_id != input.page_id.to_string()
    {
        return Err(CoreError::Invalid(
            "普通试卷结构确认必须引用当前页面 succeeded 结构分析 run".into(),
        ));
    }
    let output_json = run
        .output_json
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("普通试卷成功 run 缺少输出".into()))?;
    let calculated_output_hash = hashing::sha256_hex(output_json.as_bytes());
    if run.output_hash.as_deref() != Some(calculated_output_hash.as_str()) {
        return Err(CoreError::Invalid(
            "普通试卷结构 run 输出 hash 不一致".into(),
        ));
    }
    let output: OrdinaryPaperRecognitionOutput = serde_json::from_str(output_json)
        .map_err(|error| CoreError::Parse(format!("普通试卷结构输出无法解析：{error}")))?;
    validate_ready_output(&output)?;

    let scope = conn
        .query_row(
            "SELECT p.source_artifact_id,a.sha256,m.page_no,at.assessment_version_id,
                    v.template_version
             FROM exam_ingest_pages_v2 p
             JOIN artifacts a ON a.id=p.source_artifact_id AND a.archive_status='ready'
             JOIN exam_page_quality_revisions_v2 q
               ON q.page_id=p.id AND q.state='active' AND q.result='pass'
             JOIN exam_page_match_revisions_v2 m
               ON m.page_id=p.id AND m.state='active' AND m.decision='teacher_confirmed'
             JOIN exam_attempts_v2 at ON at.id=m.attempt_id AND at.state<>'voided'
             JOIN exam_assessment_versions_v2 v
               ON v.id=at.assessment_version_id AND v.state='confirmed'
             JOIN exam_material_type_revisions_v2 mt
               ON mt.ingest_batch_id=p.batch_id AND mt.state='active'
                  AND mt.decision='teacher_confirmed' AND mt.material_type='ordinary_paper'
             WHERE p.id=?1 AND p.state<>'voided'",
            [input.page_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid("当前页面的质量、学生归属、作业版本或普通试卷类型已发生变化".into())
        })?;
    let template_version = scope
        .4
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CoreError::Invalid("当前作业未固定普通试卷模板版本".into()))?;
    if run.input_artifact_id != Some(scope.0)
        || output.page_id != input.page_id
        || output.input_artifact_id != scope.0
        || output.input_artifact_sha256 != scope.1
        || output.input_hash != run.input_hash
        || output.expected_page_no != scope.2
        || output.descriptor.provider != run.provider
        || output.descriptor.model_name != run.model_name
        || output.descriptor.model_version != run.model_version
        || output.descriptor.config_version != run.config_version
        || output.descriptor.rule_version != run.prompt_or_rule_version
        || match &output.alignment {
            Some(alignment) => alignment.template_version.trim() != template_version.trim(),
            None => true,
        }
    {
        return Err(CoreError::Invalid(
            "普通试卷结构候选与当前页面、模板或模型版本不一致".into(),
        ));
    }

    let already_confirmed: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_page_alignment_revisions_v2
         WHERE page_id=?1 AND state='active' AND decision='teacher_confirmed')",
        [input.page_id],
        |row| row.get(0),
    )?;
    if already_confirmed {
        return Err(CoreError::Invalid(
            "当前页面已有老师确认的结构；如需纠正请走结构修订，不能重复覆盖".into(),
        ));
    }

    let mut stmt = conn.prepare(
        "SELECT i.id,q.question_type
         FROM exam_assessment_items_v2 i
         JOIN k1_question_versions q ON q.id=i.question_version_id
         WHERE i.assessment_version_id=?1 AND i.state='active'
           AND COALESCE(CAST(json_extract(i.presentation_snapshot_json,'$.page_no') AS INTEGER),1)=?2
         ORDER BY i.order_index,i.id",
    )?;
    let rows = stmt.query_map((scope.3, scope.2), |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut current_items = BTreeMap::new();
    for row in rows {
        let (item_id, question_type) = row?;
        current_items.insert(item_id, question_type_from_db(&question_type)?);
    }
    drop(stmt);
    let output_items = output
        .regions
        .iter()
        .map(|region| region.assessment_item_id)
        .collect::<BTreeSet<_>>();
    if output_items != current_items.keys().copied().collect() {
        return Err(CoreError::Invalid(
            "普通试卷结构候选未完整覆盖当前页题目，拒绝确认".into(),
        ));
    }

    let artifact_map = input
        .region_artifacts
        .iter()
        .map(|artifact| {
            (
                (artifact.assessment_item_id, artifact.region_index),
                artifact.crop_artifact_id,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let output_keys = output
        .regions
        .iter()
        .map(|region| (region.assessment_item_id, region.region_index))
        .collect::<BTreeSet<_>>();
    if artifact_map.len() != input.region_artifacts.len()
        || artifact_map.keys().copied().collect::<BTreeSet<_>>() != output_keys
    {
        return Err(CoreError::Invalid(
            "普通试卷裁图清单必须与 AI 题区候选逐项一致".into(),
        ));
    }

    let alignment_candidate = output.alignment.as_ref().expect("ready checked above");
    let transform_json = serde_json::json!({
        "schema_version": 1,
        "matrix": alignment_candidate.matrix,
        "source_ai_run_id": input.ai_run_id,
    })
    .to_string();
    let alignment = papers::record_page_alignment_in(
        conn,
        &NewPageAlignmentRevision {
            page_id: input.page_id,
            template_version: &template_version,
            transform_json: &transform_json,
            confidence: alignment_candidate.confidence,
            aligned_artifact_id: Some(input.aligned_artifact_id),
            decision: "teacher_confirmed",
            reason_code: Some("AI_READY_STRUCTURE_ACCEPTED"),
            confirmed_by: Some(input.confirmed_by),
        },
    )?;

    let mut region_revisions = Vec::with_capacity(output.regions.len());
    for candidate in &output.regions {
        let crop_artifact_id = artifact_map
            .get(&(candidate.assessment_item_id, candidate.region_index))
            .copied()
            .ok_or_else(|| CoreError::Invalid("普通试卷题区缺少对应裁图".into()))?;
        let bbox_json = serde_json::json!({
            "schema_version": 1,
            "x": candidate.bbox.x,
            "y": candidate.bbox.y,
            "width": candidate.bbox.width,
            "height": candidate.bbox.height,
            "mark_cells": candidate.mark_cells,
            "source_ai_run_id": input.ai_run_id,
        })
        .to_string();
        region_revisions.push(papers::record_answer_region_in(
            conn,
            &NewAnswerRegionRevision {
                page_id: input.page_id,
                assessment_item_id: candidate.assessment_item_id,
                region_index: candidate.region_index,
                bbox_json: &bbox_json,
                crop_artifact_id: Some(crop_artifact_id),
                mapping_confidence: Some(candidate.mapping_confidence),
                decision: "teacher_confirmed",
                reason_code: Some("AI_READY_STRUCTURE_ACCEPTED"),
                confirmed_by: Some(input.confirmed_by),
            },
        )?);
    }

    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    let region_revision_ids = region_revisions
        .iter()
        .map(|region| region.id)
        .collect::<Vec<_>>();
    let ids_json = serde_json::json!({
        "schema_version": 1,
        "region_revision_ids": region_revision_ids,
    })
    .to_string();
    conn.execute(
        "INSERT INTO exam_ordinary_structure_confirmations_v2
         (public_id,ai_run_id,page_id,alignment_revision_id,region_revision_ids_json,
          confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        (
            &public_id,
            input.ai_run_id,
            input.page_id,
            alignment.id,
            &ids_json,
            input.confirmed_by.trim(),
            &created_at,
        ),
    )?;
    let confirmation_id = conn.last_insert_rowid();
    audit::append(
        conn,
        &audit::NewAuditEvent {
            idempotency_key: &format!("exam:ordinary-structure-confirmation:{}", input.ai_run_id),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.confirmed_by.trim()),
            action: "exam.ordinary_structure.confirmed",
            object_type: "exam_ingest_page",
            object_id: &input.page_id.to_string(),
            object_revision: Some(alignment.revision),
            note: None,
            meta_json: Some(r#"{"schema_version":1}"#),
            occurred_at: &created_at,
        },
    )?;
    let confirmation = get_confirmation_by_ai_run(conn, input.ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("结构确认#{confirmation_id}")))?;
    Ok(OrdinaryStructureConfirmationResult {
        confirmation,
        alignment,
        regions: region_revisions,
    })
}

/// 按整页 0~1 题区坐标裁剪，并统一编码为 JPEG；不在图片中写入任何学生标识。
pub fn crop_normalized_jpeg(source_bytes: &[u8], rect: &NormalizedRect) -> CoreResult<Vec<u8>> {
    rect.validate("待裁剪题区")?;
    let image = image::load_from_memory(source_bytes)
        .map_err(|error| CoreError::Parse(format!("普通试卷图片无法解码：{error}")))?;
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return Err(CoreError::Invalid("普通试卷图片尺寸无效".into()));
    }
    let x = (rect.x * width as f64).floor() as u32;
    let y = (rect.y * height as f64).floor() as u32;
    let right = ((rect.x + rect.width) * width as f64).ceil() as u32;
    let bottom = ((rect.y + rect.height) * height as f64).ceil() as u32;
    let crop_width = right.min(width).saturating_sub(x.min(width));
    let crop_height = bottom.min(height).saturating_sub(y.min(height));
    if crop_width == 0 || crop_height == 0 {
        return Err(CoreError::Invalid("普通试卷题区裁剪结果为空".into()));
    }
    let crop = image.crop_imm(x, y, crop_width, crop_height).to_rgb8();
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, 92)
        .encode_image(&crop)
        .map_err(|error| CoreError::Parse(format!("普通试卷题区 JPEG 编码失败：{error}")))?;
    Ok(bytes)
}

/// 供文件准备层验证来源 artifact，避免派生文件挂错父节点。
pub fn validate_source_artifact(
    conn: &Connection,
    page_id: i64,
    ai_run_id: i64,
) -> CoreResult<(i64, String, String, OrdinaryPaperRecognitionOutput)> {
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    if run.status != AiRunStatus::Succeeded
        || run.run_type != "ordinary_paper_structure"
        || run.business_ref_id != page_id.to_string()
    {
        return Err(CoreError::Invalid(
            "当前 run 不是本页成功的普通试卷结构候选".into(),
        ));
    }
    let output: OrdinaryPaperRecognitionOutput = serde_json::from_str(
        run.output_json
            .as_deref()
            .ok_or_else(|| CoreError::Invalid("普通试卷成功 run 缺少输出".into()))?,
    )
    .map_err(|error| CoreError::Parse(format!("普通试卷结构输出无法解析：{error}")))?;
    validate_ready_output(&output)?;
    let artifact_id = run
        .input_artifact_id
        .ok_or_else(|| CoreError::Invalid("普通试卷结构 run 缺少输入 artifact".into()))?;
    let artifact = artifacts::get_by_id(conn, artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{artifact_id}")))?;
    if artifact.archive_status != ArchiveStatus::Ready
        || !matches!(artifact.kind, ArtifactKind::Page | ArtifactKind::Image)
        || artifact.privacy_class != PrivacyClass::StudentSensitive
    {
        return Err(CoreError::Invalid(
            "普通试卷来源 artifact 当前不可派生".into(),
        ));
    }
    Ok((artifact.id, artifact.sha256, artifact.archived_path, output))
}
