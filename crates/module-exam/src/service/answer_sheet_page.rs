//! 已确认答题卡模板应用到学生页面后的结构物化。
//!
//! 调用方负责在文件层完成四角校正与裁图；本服务在一个事务中核对当前页面、
//! 空白模板 revision 和派生 artifact，再创建老师确认的配准/题区与不可变账本。
//! 本步骤不识别答案、不判分、不发布，也不生成学习证据。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::{artifacts, audit};
use suite_core::domain::{ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::Artifact;
use suite_core::models::{ArchiveStatus, ArtifactKind, AuditActorType, PrivacyClass};

use crate::answer_sheet_recognition::{AnswerSheetTemplateDefinition, DetectedAnswerSheetAnchor};

use super::answer_sheet;
use super::answer_sheet::AnswerSheetTemplateRevision;
use super::papers::{
    self, AnswerRegionRevision, NewAnswerRegionRevision, NewPageAlignmentRevision,
    PageAlignmentRevision,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerSheetRegionArtifact {
    pub assessment_item_id: i64,
    pub region_index: i64,
    pub crop_artifact_id: i64,
}

pub struct MaterializeAnswerSheetPageInput<'a> {
    pub page_id: i64,
    pub template_revision_id: i64,
    pub aligned_artifact_id: i64,
    pub region_artifacts: &'a [AnswerSheetRegionArtifact],
    pub template_to_source: [f64; 9],
    pub detected_anchors: &'a [DetectedAnswerSheetAnchor],
    pub alignment_confidence: f64,
    pub confirmed_by: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerSheetPageMaterialization {
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
pub struct AnswerSheetPageMaterializationResult {
    pub materialization: AnswerSheetPageMaterialization,
    pub alignment: PageAlignmentRevision,
    pub regions: Vec<AnswerRegionRevision>,
    pub routes: Vec<AnswerSheetRegionRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerSheetRegionRoute {
    pub id: i64,
    pub materialization_id: i64,
    pub answer_region_revision_id: i64,
    pub assessment_item_id: i64,
    pub region_index: i64,
    pub recognition_route: String,
    pub question_type: String,
    pub created_at: String,
}

type MaterializationRow = (i64, String, i64, i64, i64, String, String, String);

#[derive(Debug, Clone)]
pub struct AnswerSheetPageMaterializationSource {
    pub source_artifact: Artifact,
    pub blank_artifact: Artifact,
    pub template_revision: AnswerSheetTemplateRevision,
    pub definition: AnswerSheetTemplateDefinition,
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

fn parse_materialization(raw: MaterializationRow) -> CoreResult<AnswerSheetPageMaterialization> {
    let value: serde_json::Value = serde_json::from_str(&raw.5)
        .map_err(|error| CoreError::Parse(format!("答题卡题区账本损坏：{error}")))?;
    let region_revision_ids = serde_json::from_value(
        value
            .get("region_revision_ids")
            .cloned()
            .ok_or_else(|| CoreError::Invalid("答题卡题区账本缺少 revision ids".into()))?,
    )
    .map_err(|error| CoreError::Parse(format!("答题卡题区 ids 无效：{error}")))?;
    Ok(AnswerSheetPageMaterialization {
        id: raw.0,
        public_id: raw.1,
        page_id: raw.2,
        template_revision_id: raw.3,
        alignment_revision_id: raw.4,
        region_revision_ids,
        confirmed_by: raw.6,
        created_at: raw.7,
    })
}

pub fn get_page_materialization(
    conn: &Connection,
    page_id: i64,
    template_revision_id: i64,
) -> CoreResult<Option<AnswerSheetPageMaterialization>> {
    let raw = conn
        .query_row(
            "SELECT id,public_id,page_id,template_revision_id,alignment_revision_id,
                    region_revision_ids_json,confirmed_by,created_at
             FROM exam_answer_sheet_page_materializations_v2
             WHERE page_id=?1 AND template_revision_id=?2",
            (page_id, template_revision_id),
            materialization_row,
        )
        .optional()?;
    raw.map(parse_materialization).transpose()
}

fn load_template(
    conn: &Connection,
    template_revision_id: i64,
) -> CoreResult<(AnswerSheetTemplateRevision, AnswerSheetTemplateDefinition)> {
    let revision = conn
        .query_row(
            "SELECT id,public_id,assessment_version_id,revision,template_version,page_no,
                    blank_artifact_id,template_hash,template_json,source_ai_run_id,
                    confirmed_by,state,created_at
             FROM exam_answer_sheet_template_revisions_v2
             WHERE id=?1 AND state='active'",
            [template_revision_id],
            |row| {
                Ok(AnswerSheetTemplateRevision {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    assessment_version_id: row.get(2)?,
                    revision: row.get(3)?,
                    template_version: row.get(4)?,
                    page_no: row.get(5)?,
                    blank_artifact_id: row.get(6)?,
                    template_hash: row.get(7)?,
                    template_json: row.get(8)?,
                    source_ai_run_id: row.get(9)?,
                    confirmed_by: row.get(10)?,
                    state: row.get(11)?,
                    created_at: row.get(12)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("答题卡必须使用当前 active 模板 revision".into()))?;
    let definition: AnswerSheetTemplateDefinition =
        serde_json::from_str(&revision.template_json)
            .map_err(|error| CoreError::Parse(format!("答题卡模板 JSON 无法解析：{error}")))?;
    definition.validate()?;
    if definition.template_hash()? != revision.template_hash {
        return Err(CoreError::Invalid(
            "答题卡模板 hash 与已确认 revision 不一致".into(),
        ));
    }
    Ok((revision, definition))
}

fn load_page_scope(conn: &Connection, page_id: i64) -> CoreResult<(i64, i64, i64, String)> {
    let scope = conn
        .query_row(
            "SELECT p.source_artifact_id,m.page_no,at.assessment_version_id,v.template_version
             FROM exam_ingest_pages_v2 p
             JOIN exam_page_quality_revisions_v2 q
               ON q.page_id=p.id AND q.state='active' AND q.result='pass'
             JOIN exam_page_match_revisions_v2 m
               ON m.page_id=p.id AND m.state='active' AND m.decision='teacher_confirmed'
             JOIN exam_attempts_v2 at ON at.id=m.attempt_id AND at.state<>'voided'
             JOIN exam_assessment_versions_v2 v
               ON v.id=at.assessment_version_id AND v.state='confirmed'
             JOIN exam_material_type_revisions_v2 mt
               ON mt.ingest_batch_id=p.batch_id AND mt.state='active'
                  AND mt.decision='teacher_confirmed' AND mt.material_type='answer_sheet'
             WHERE p.id=?1 AND p.state<>'voided'",
            [page_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid("当前页面质量、学生归属、作业版本或答题卡类型已发生变化".into())
        })?;
    let template_version = scope
        .3
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CoreError::Invalid("当前作业未固定答题卡模板版本".into()))?;
    Ok((scope.0, scope.1, scope.2, template_version))
}

fn validate_template_scope(
    revision: &AnswerSheetTemplateRevision,
    definition: &AnswerSheetTemplateDefinition,
    scope: &(i64, i64, i64, String),
) -> CoreResult<()> {
    if revision.assessment_version_id != scope.2
        || revision.page_no != scope.1
        || revision.template_version.trim() != scope.3.trim()
        || definition.assessment_version_id != scope.2
        || definition.page_no != scope.1
    {
        return Err(CoreError::Invalid(
            "答题卡模板 revision 与当前学生页面、页码或作业版本不一致".into(),
        ));
    }
    Ok(())
}

/// 文件准备层只读入口；真正写入时仍会在事务内再次验证全部作用域。
pub fn validate_materialization_source(
    conn: &Connection,
    page_id: i64,
    template_revision_id: i64,
) -> CoreResult<AnswerSheetPageMaterializationSource> {
    let (template_revision, definition) = load_template(conn, template_revision_id)?;
    let scope = load_page_scope(conn, page_id)?;
    validate_template_scope(&template_revision, &definition, &scope)?;
    let source_artifact = artifacts::get_by_id(conn, scope.0)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{}", scope.0)))?;
    let blank_artifact = artifacts::get_by_id(conn, definition.blank_artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{}", definition.blank_artifact_id)))?;
    if !matches!(
        source_artifact.kind,
        ArtifactKind::Page | ArtifactKind::Image
    ) || source_artifact.privacy_class != PrivacyClass::StudentSensitive
        || source_artifact.archive_status != ArchiveStatus::Ready
        || !matches!(
            blank_artifact.kind,
            ArtifactKind::Page | ArtifactKind::Image
        )
        || !matches!(
            blank_artifact.privacy_class,
            PrivacyClass::TeachingContent | PrivacyClass::PublicSafe
        )
        || blank_artifact.archive_status != ArchiveStatus::Ready
        || blank_artifact.sha256 != definition.blank_artifact_sha256
    {
        return Err(CoreError::Invalid(
            "答题卡学生原图或空白模板 artifact 当前不可派生".into(),
        ));
    }
    Ok(AnswerSheetPageMaterializationSource {
        source_artifact,
        blank_artifact,
        template_revision,
        definition,
    })
}

/// 零配置入口：按当前页面的已确认作业版本和页码找到唯一 active 答题卡模板。
pub fn get_active_template_for_page(
    conn: &Connection,
    page_id: i64,
) -> CoreResult<AnswerSheetTemplateRevision> {
    let scope = load_page_scope(conn, page_id)?;
    answer_sheet::get_active_answer_sheet_template(conn, scope.2, scope.1)?
        .ok_or_else(|| CoreError::Invalid("当前页还没有已确认的固定答题卡模板".into()))
}

fn load_result(
    conn: &Connection,
    materialization: AnswerSheetPageMaterialization,
) -> CoreResult<AnswerSheetPageMaterializationResult> {
    let alignment = conn
        .query_row(
            "SELECT id,public_id,page_id,revision,match_revision_id,template_version,
                    transform_json,confidence,aligned_artifact_id,decision,reason_code,
                    confirmed_by,state
             FROM exam_page_alignment_revisions_v2 WHERE id=?1",
            [materialization.alignment_revision_id],
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
        .ok_or_else(|| CoreError::NotFound("答题卡账本引用的配准 revision".into()))?;
    let mut regions = Vec::with_capacity(materialization.region_revision_ids.len());
    for region_id in &materialization.region_revision_ids {
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
            .ok_or_else(|| CoreError::NotFound(format!("答题卡题区 revision#{region_id}")))?;
        regions.push(region);
    }
    let mut stmt = conn.prepare(
        "SELECT id,materialization_id,answer_region_revision_id,assessment_item_id,
                region_index,recognition_route,question_type,created_at
         FROM exam_answer_sheet_region_routes_v2
         WHERE materialization_id=?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([materialization.id], |row| {
        Ok(AnswerSheetRegionRoute {
            id: row.get(0)?,
            materialization_id: row.get(1)?,
            answer_region_revision_id: row.get(2)?,
            assessment_item_id: row.get(3)?,
            region_index: row.get(4)?,
            recognition_route: row.get(5)?,
            question_type: row.get(6)?,
            created_at: row.get(7)?,
        })
    })?;
    let routes = rows.collect::<Result<Vec<_>, _>>()?;
    if routes.len() != regions.len() {
        return Err(CoreError::Invalid("答题卡题区识别路由账本不完整".into()));
    }
    Ok(AnswerSheetPageMaterializationResult {
        materialization,
        alignment,
        regions,
        routes,
    })
}

fn validate_geometry(input: &MaterializeAnswerSheetPageInput<'_>) -> CoreResult<()> {
    if !input.alignment_confidence.is_finite()
        || !(0.0..=1.0).contains(&input.alignment_confidence)
        || input
            .template_to_source
            .iter()
            .any(|value| !value.is_finite())
    {
        return Err(CoreError::Invalid("答题卡配准矩阵或置信度非法".into()));
    }
    let mut keys = BTreeSet::new();
    for anchor in input.detected_anchors {
        if !anchor.source_x.is_finite()
            || !anchor.source_y.is_finite()
            || !anchor.confidence.is_finite()
            || !(0.0..=1.0).contains(&anchor.confidence)
            || !keys.insert(anchor.key.trim().to_ascii_lowercase())
        {
            return Err(CoreError::Invalid("答题卡四角定位点非法或重复".into()));
        }
    }
    if keys
        != BTreeSet::from([
            "bottom_left".to_string(),
            "bottom_right".to_string(),
            "top_left".to_string(),
            "top_right".to_string(),
        ])
    {
        return Err(CoreError::Invalid("答题卡必须检测到四个页面角点".into()));
    }
    Ok(())
}

/// 在调用方事务中物化一页答题卡结构。
pub fn materialize_in_transaction(
    conn: &Connection,
    input: &MaterializeAnswerSheetPageInput<'_>,
) -> CoreResult<AnswerSheetPageMaterializationResult> {
    if input.page_id <= 0 || input.template_revision_id <= 0 || input.aligned_artifact_id <= 0 {
        return Err(CoreError::Invalid(
            "答题卡页面、模板和配准 artifact id 必须为正数".into(),
        ));
    }
    if input.confirmed_by.trim().is_empty() {
        return Err(CoreError::Invalid("答题卡模板应用确认人不能为空".into()));
    }
    validate_geometry(input)?;
    if let Some(existing) =
        get_page_materialization(conn, input.page_id, input.template_revision_id)?
    {
        return load_result(conn, existing);
    }

    let (template_revision, definition) = load_template(conn, input.template_revision_id)?;
    let scope = load_page_scope(conn, input.page_id)?;
    validate_template_scope(&template_revision, &definition, &scope)?;
    let scope_template_version = scope.3.clone();

    let already_aligned: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_page_alignment_revisions_v2
         WHERE page_id=?1 AND state='active' AND decision='teacher_confirmed')",
        [input.page_id],
        |row| row.get(0),
    )?;
    if already_aligned {
        return Err(CoreError::Invalid(
            "当前答题卡页面已有老师确认结构；纠正时必须新建结构修订".into(),
        ));
    }

    let aligned = artifacts::get_by_id(conn, input.aligned_artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{}", input.aligned_artifact_id)))?;
    if !matches!(aligned.kind, ArtifactKind::Page | ArtifactKind::Image)
        || aligned.parent_artifact_id != Some(scope.0)
        || aligned.privacy_class != PrivacyClass::StudentSensitive
        || aligned.archive_status != ArchiveStatus::Ready
    {
        return Err(CoreError::Invalid(
            "答题卡校正页必须是原图派生的已归档 student_sensitive 页面".into(),
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
    let template_keys = definition.region_keys();
    if artifact_map.len() != input.region_artifacts.len()
        || artifact_map.keys().copied().collect::<BTreeSet<_>>() != template_keys
    {
        return Err(CoreError::Invalid(
            "答题卡裁图清单必须与已确认模板题区逐项一致".into(),
        ));
    }

    let transform_json = serde_json::json!({
        "schema_version": 1,
        "template_revision_id": input.template_revision_id,
        "matrix": input.template_to_source,
        "matrix_direction": "template_to_source",
        "alignment_mode": definition.alignment_mode,
        "detected_anchors": input.detected_anchors,
    })
    .to_string();
    let alignment = papers::record_page_alignment_in(
        conn,
        &NewPageAlignmentRevision {
            page_id: input.page_id,
            template_version: &scope_template_version,
            transform_json: &transform_json,
            confidence: input.alignment_confidence,
            aligned_artifact_id: Some(input.aligned_artifact_id),
            decision: "teacher_confirmed",
            reason_code: Some("FIXED_ANSWER_SHEET_TEMPLATE_APPLIED"),
            confirmed_by: Some(input.confirmed_by),
        },
    )?;

    let mut regions = Vec::with_capacity(definition.region_count());
    for item in &definition.items {
        let crop_artifact_id = artifact_map
            .get(&(item.assessment_item_id, item.region_index))
            .copied()
            .ok_or_else(|| CoreError::Invalid("答题卡题区缺少对应裁图".into()))?;
        let bbox_json = serde_json::json!({
            "schema_version": 1,
            "x": item.region.x,
            "y": item.region.y,
            "width": item.region.width,
            "height": item.region.height,
            "region_role": "objective_mark",
            "mark_cells": item.cells,
            "template_revision_id": input.template_revision_id,
        })
        .to_string();
        regions.push(papers::record_answer_region_in(
            conn,
            &NewAnswerRegionRevision {
                page_id: input.page_id,
                assessment_item_id: item.assessment_item_id,
                region_index: item.region_index,
                bbox_json: &bbox_json,
                crop_artifact_id: Some(crop_artifact_id),
                mapping_confidence: Some(input.alignment_confidence),
                decision: "teacher_confirmed",
                reason_code: Some("FIXED_ANSWER_SHEET_TEMPLATE_APPLIED"),
                confirmed_by: Some(input.confirmed_by),
            },
        )?);
    }
    for item in &definition.subjective_regions {
        let crop_artifact_id = artifact_map
            .get(&(item.assessment_item_id, item.region_index))
            .copied()
            .ok_or_else(|| CoreError::Invalid("答题卡主观题区缺少对应裁图".into()))?;
        let bbox_json = serde_json::json!({
            "schema_version": 1,
            "x": item.region.x,
            "y": item.region.y,
            "width": item.region.width,
            "height": item.region.height,
            "region_role": "handwritten_answer",
            "question_type": item.question_type.as_db(),
            "template_revision_id": input.template_revision_id,
        })
        .to_string();
        regions.push(papers::record_answer_region_in(
            conn,
            &NewAnswerRegionRevision {
                page_id: input.page_id,
                assessment_item_id: item.assessment_item_id,
                region_index: item.region_index,
                bbox_json: &bbox_json,
                crop_artifact_id: Some(crop_artifact_id),
                mapping_confidence: Some(input.alignment_confidence),
                decision: "teacher_confirmed",
                reason_code: Some("FIXED_ANSWER_SHEET_SUBJECTIVE_REGION"),
                confirmed_by: Some(input.confirmed_by),
            },
        )?);
    }

    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    let region_revision_ids = regions.iter().map(|region| region.id).collect::<Vec<_>>();
    let ids_json = serde_json::json!({
        "schema_version": 1,
        "region_revision_ids": region_revision_ids,
    })
    .to_string();
    conn.execute(
        "INSERT INTO exam_answer_sheet_page_materializations_v2
         (public_id,page_id,template_revision_id,alignment_revision_id,
          region_revision_ids_json,confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        (
            &public_id,
            input.page_id,
            input.template_revision_id,
            alignment.id,
            &ids_json,
            input.confirmed_by.trim(),
            &created_at,
        ),
    )?;
    let materialization_id = conn.last_insert_rowid();
    let mut route_specs = BTreeMap::new();
    for item in &definition.items {
        route_specs.insert(
            (item.assessment_item_id, item.region_index),
            ("objective_omr", item.question_type.as_str()),
        );
    }
    for item in &definition.subjective_regions {
        route_specs.insert(
            (item.assessment_item_id, item.region_index),
            ("handwriting_ocr", item.question_type.as_db()),
        );
    }
    for region in &regions {
        let (recognition_route, question_type) = route_specs
            .get(&(region.assessment_item_id, region.region_index))
            .copied()
            .ok_or_else(|| CoreError::Invalid("答题卡题区缺少识别路由".into()))?;
        conn.execute(
            "INSERT INTO exam_answer_sheet_region_routes_v2
             (materialization_id,answer_region_revision_id,assessment_item_id,region_index,
              recognition_route,question_type,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            (
                materialization_id,
                region.id,
                region.assessment_item_id,
                region.region_index,
                recognition_route,
                question_type,
                &created_at,
            ),
        )?;
    }
    audit::append(
        conn,
        &audit::NewAuditEvent {
            idempotency_key: &format!(
                "exam:answer-sheet-page-materialization:{}:{}",
                input.page_id, input.template_revision_id
            ),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.confirmed_by.trim()),
            action: "exam.answer_sheet_template.applied",
            object_type: "exam_ingest_page",
            object_id: &input.page_id.to_string(),
            object_revision: Some(alignment.revision),
            note: None,
            meta_json: Some(r#"{"schema_version":1}"#),
            occurred_at: &created_at,
        },
    )?;
    let materialization =
        get_page_materialization(conn, input.page_id, input.template_revision_id)?
            .ok_or_else(|| CoreError::NotFound("刚创建的答题卡页面物化账本".into()))?;
    load_result(conn, materialization)
}
