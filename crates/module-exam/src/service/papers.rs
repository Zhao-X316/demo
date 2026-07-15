//! M2-B1 纸面证据导入、质量闸门与页面匹配。
//!
//! 固定原则：原图使用 core `artifacts`；页面不直接持有学生身份；机器只追加匹配建议，
//! 只有 `teacher_confirmed` revision 才把页面推进到 `matched`。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::{artifacts, audit};
use suite_core::domain::{ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{ArchiveStatus, ArtifactKind, AuditActorType, PrivacyClass};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestBatch {
    pub id: i64,
    pub public_id: String,
    pub assessment_version_id: i64,
    pub source_kind: String,
    pub idempotency_key: String,
    pub state: String,
    pub created_by: String,
}

pub struct NewIngestBatch<'a> {
    pub assessment_version_id: i64,
    pub source_kind: &'a str,
    pub idempotency_key: &'a str,
    pub created_by: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestPage {
    pub id: i64,
    pub public_id: String,
    pub batch_id: i64,
    pub source_artifact_id: i64,
    pub import_index: i64,
    pub expected_page_no: Option<i64>,
    pub state: String,
}

pub struct NewIngestPage {
    pub batch_id: i64,
    pub source_artifact_id: i64,
    pub import_index: i64,
    pub expected_page_no: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageQualityRevision {
    pub id: i64,
    pub public_id: String,
    pub page_id: i64,
    pub revision: i64,
    pub blur_score: f64,
    pub glare_score: f64,
    pub brightness_score: f64,
    pub perspective_score: f64,
    pub rotation_degrees: f64,
    pub crop_complete: bool,
    pub result: String,
    pub issue_codes_json: String,
    pub checked_by_type: String,
    pub checked_by: Option<String>,
    pub state: String,
}

pub struct NewPageQualityRevision<'a> {
    pub page_id: i64,
    pub blur_score: f64,
    pub glare_score: f64,
    pub brightness_score: f64,
    pub perspective_score: f64,
    pub rotation_degrees: f64,
    pub crop_complete: bool,
    pub result: &'a str,
    pub issue_codes_json: &'a str,
    pub checked_by_type: &'a str,
    pub checked_by: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageMatchRevision {
    pub id: i64,
    pub public_id: String,
    pub page_id: i64,
    pub revision: i64,
    pub attempt_id: Option<i64>,
    pub page_no: Option<i64>,
    pub student_confidence: Option<f64>,
    pub page_no_confidence: Option<f64>,
    pub template_confidence: Option<f64>,
    pub decision: String,
    pub reason_code: Option<String>,
    pub confirmed_by: Option<String>,
    pub state: String,
}

pub struct NewPageMatchRevision<'a> {
    pub page_id: i64,
    pub attempt_id: Option<i64>,
    pub page_no: Option<i64>,
    pub student_confidence: Option<f64>,
    pub page_no_confidence: Option<f64>,
    pub template_confidence: Option<f64>,
    pub decision: &'a str,
    pub reason_code: Option<&'a str>,
    pub confirmed_by: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageAlignmentRevision {
    pub id: i64,
    pub public_id: String,
    pub page_id: i64,
    pub revision: i64,
    pub match_revision_id: i64,
    pub template_version: String,
    pub transform_json: String,
    pub confidence: f64,
    pub aligned_artifact_id: Option<i64>,
    pub decision: String,
    pub reason_code: Option<String>,
    pub confirmed_by: Option<String>,
    pub state: String,
}

pub struct NewPageAlignmentRevision<'a> {
    pub page_id: i64,
    pub template_version: &'a str,
    pub transform_json: &'a str,
    pub confidence: f64,
    pub aligned_artifact_id: Option<i64>,
    pub decision: &'a str,
    pub reason_code: Option<&'a str>,
    pub confirmed_by: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerRegionRevision {
    pub id: i64,
    pub public_id: String,
    pub page_id: i64,
    pub assessment_item_id: i64,
    pub region_index: i64,
    pub revision: i64,
    pub alignment_revision_id: i64,
    pub bbox_json: String,
    pub crop_artifact_id: Option<i64>,
    pub mapping_confidence: Option<f64>,
    pub decision: String,
    pub reason_code: Option<String>,
    pub confirmed_by: Option<String>,
    pub state: String,
}

pub struct NewAnswerRegionRevision<'a> {
    pub page_id: i64,
    pub assessment_item_id: i64,
    pub region_index: i64,
    pub bbox_json: &'a str,
    pub crop_artifact_id: Option<i64>,
    pub mapping_confidence: Option<f64>,
    pub decision: &'a str,
    pub reason_code: Option<&'a str>,
    pub confirmed_by: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegionTrace {
    pub student_id: i64,
    pub assessment_version_id: i64,
    pub attempt_id: i64,
    pub page_no: i64,
    pub source_page_artifact_id: i64,
    pub aligned_page_artifact_id: i64,
    pub assessment_item_id: i64,
    pub question_version_id: i64,
    pub bbox_json: String,
    pub crop_artifact_id: i64,
    pub region_revision: i64,
    pub region_decision: String,
}

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{field}不能为空")))
    } else {
        Ok(())
    }
}

fn validate_schema_object(json: &str, field: &str) -> CoreResult<()> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| CoreError::Invalid(format!("{field} JSON 无效：{error}")))?;
    if !value.is_object()
        || value
            .get("schema_version")
            .and_then(Value::as_i64)
            .is_none()
    {
        return Err(CoreError::Invalid(format!(
            "{field} 必须是带整数 schema_version 的对象"
        )));
    }
    Ok(())
}

fn validate_transform(json: &str) -> CoreResult<()> {
    validate_schema_object(json, "页面配准变换")?;
    let value: Value = serde_json::from_str(json)
        .map_err(|error| CoreError::Invalid(format!("页面配准变换 JSON 无效：{error}")))?;
    let matrix = value
        .get("matrix")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Invalid("页面配准变换必须包含 matrix 数组".into()))?;
    if matrix.len() != 9 || matrix.iter().any(|value| value.as_f64().is_none()) {
        return Err(CoreError::Invalid(
            "页面配准 matrix 必须包含 9 个数值".into(),
        ));
    }
    Ok(())
}

fn validate_bbox(json: &str) -> CoreResult<()> {
    validate_schema_object(json, "答案区域坐标")?;
    let value: Value = serde_json::from_str(json)
        .map_err(|error| CoreError::Invalid(format!("答案区域坐标 JSON 无效：{error}")))?;
    let number = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_f64)
            .filter(|number| number.is_finite())
            .ok_or_else(|| CoreError::Invalid(format!("答案区域缺少数值字段 {key}")))
    };
    let x = number("x")?;
    let y = number("y")?;
    let width = number("width")?;
    let height = number("height")?;
    if x < 0.0
        || y < 0.0
        || width <= 0.0
        || height <= 0.0
        || x + width > 1.0 + f64::EPSILON
        || y + height > 1.0 + f64::EPSILON
    {
        return Err(CoreError::Invalid(
            "答案区域必须是页面内 0~1 的归一化矩形".into(),
        ));
    }
    Ok(())
}

fn valid_unit(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn validate_optional_unit(value: Option<f64>, field: &str) -> CoreResult<()> {
    if value.is_some_and(|value| !valid_unit(value)) {
        return Err(CoreError::Invalid(format!("{field} 必须位于 0~1")));
    }
    Ok(())
}

fn batch_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<IngestBatch> {
    Ok(IngestBatch {
        id: row.get(0)?,
        public_id: row.get(1)?,
        assessment_version_id: row.get(2)?,
        source_kind: row.get(3)?,
        idempotency_key: row.get(4)?,
        state: row.get(5)?,
        created_by: row.get(6)?,
    })
}

fn page_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<IngestPage> {
    Ok(IngestPage {
        id: row.get(0)?,
        public_id: row.get(1)?,
        batch_id: row.get(2)?,
        source_artifact_id: row.get(3)?,
        import_index: row.get(4)?,
        expected_page_no: row.get(5)?,
        state: row.get(6)?,
    })
}

fn quality_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PageQualityRevision> {
    Ok(PageQualityRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        page_id: row.get(2)?,
        revision: row.get(3)?,
        blur_score: row.get(4)?,
        glare_score: row.get(5)?,
        brightness_score: row.get(6)?,
        perspective_score: row.get(7)?,
        rotation_degrees: row.get(8)?,
        crop_complete: row.get::<_, i64>(9)? != 0,
        result: row.get(10)?,
        issue_codes_json: row.get(11)?,
        checked_by_type: row.get(12)?,
        checked_by: row.get(13)?,
        state: row.get(14)?,
    })
}

fn match_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PageMatchRevision> {
    Ok(PageMatchRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        page_id: row.get(2)?,
        revision: row.get(3)?,
        attempt_id: row.get(4)?,
        page_no: row.get(5)?,
        student_confidence: row.get(6)?,
        page_no_confidence: row.get(7)?,
        template_confidence: row.get(8)?,
        decision: row.get(9)?,
        reason_code: row.get(10)?,
        confirmed_by: row.get(11)?,
        state: row.get(12)?,
    })
}

pub fn get_batch(conn: &Connection, id: i64) -> CoreResult<Option<IngestBatch>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, assessment_version_id, source_kind,
                    idempotency_key, state, created_by
             FROM exam_ingest_batches_v2 WHERE id=?1",
            [id],
            batch_row,
        )
        .optional()?)
}

pub fn get_page(conn: &Connection, id: i64) -> CoreResult<Option<IngestPage>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, batch_id, source_artifact_id,
                    import_index, expected_page_no, state
             FROM exam_ingest_pages_v2 WHERE id=?1",
            [id],
            page_row,
        )
        .optional()?)
}

struct AuditInput<'a> {
    key: &'a str,
    actor_type: AuditActorType,
    actor_id: Option<&'a str>,
    action: &'a str,
    object_type: &'a str,
    object_id: &'a str,
    revision: Option<i64>,
    now: &'a str,
}

fn append_audit(conn: &Connection, input: &AuditInput<'_>) -> CoreResult<()> {
    audit::append(
        conn,
        &audit::NewAuditEvent {
            idempotency_key: input.key,
            actor_type: input.actor_type,
            actor_id: input.actor_id,
            action: input.action,
            object_type: input.object_type,
            object_id: input.object_id,
            object_revision: input.revision,
            note: None,
            meta_json: Some(r#"{"schema_version":1}"#),
            occurred_at: input.now,
        },
    )?;
    Ok(())
}

pub fn create_or_get_ingest_batch(
    conn: &Connection,
    input: &NewIngestBatch<'_>,
) -> CoreResult<IngestBatch> {
    required(input.idempotency_key, "导入幂等键")?;
    required(input.created_by, "创建人")?;
    if !matches!(
        input.source_kind,
        "fixed_fixture" | "image_folder" | "scanner" | "camera"
    ) {
        return Err(CoreError::Invalid("纸面导入来源非法".into()));
    }
    let eligible: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_assessment_versions_v2
         WHERE id=?1 AND state='confirmed')",
        [input.assessment_version_id],
        |row| row.get(0),
    )?;
    if !eligible {
        return Err(CoreError::Invalid(
            "只能向已确认作业版本导入纸面证据".into(),
        ));
    }
    let existing = conn
        .query_row(
            "SELECT id, public_id, assessment_version_id, source_kind,
                    idempotency_key, state, created_by
             FROM exam_ingest_batches_v2
             WHERE assessment_version_id=?1 AND idempotency_key=?2",
            (input.assessment_version_id, input.idempotency_key.trim()),
            batch_row,
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing.source_kind != input.source_kind
            || existing.created_by != input.created_by.trim()
        {
            return Err(CoreError::Invalid(
                "导入幂等键已属于不同来源或创建人".into(),
            ));
        }
        return Ok(existing);
    }

    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO exam_ingest_batches_v2
         (public_id, assessment_version_id, source_kind, idempotency_key,
          state, created_by, created_at, updated_at)
         VALUES (?1,?2,?3,?4,'processing',?5,?6,?6)",
        (
            &public_id,
            input.assessment_version_id,
            input.source_kind,
            input.idempotency_key.trim(),
            input.created_by.trim(),
            &now,
        ),
    )?;
    let id = tx.last_insert_rowid();
    append_audit(
        &tx,
        &AuditInput {
            key: &format!("exam:ingest-batch:{public_id}:created"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.created_by.trim()),
            action: "exam.ingest_batch.created",
            object_type: "exam_ingest_batch",
            object_id: &public_id,
            revision: None,
            now: &now,
        },
    )?;
    tx.commit()?;
    get_batch(conn, id)?.ok_or_else(|| CoreError::NotFound("刚创建的导入批次".into()))
}

pub fn register_ingest_page(conn: &Connection, input: &NewIngestPage) -> CoreResult<IngestPage> {
    if input.import_index < 0 || input.expected_page_no.is_some_and(|value| value < 1) {
        return Err(CoreError::Invalid("导入顺序或预期页码非法".into()));
    }
    let batch = get_batch(conn, input.batch_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ingest_batch#{}", input.batch_id)))?;
    if matches!(batch.state.as_str(), "failed" | "voided") {
        return Err(CoreError::Invalid("失败或作废批次不能继续登记页面".into()));
    }
    let artifact = artifacts::get_by_id(conn, input.source_artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{}", input.source_artifact_id)))?;
    if !matches!(artifact.kind, ArtifactKind::Page | ArtifactKind::Image)
        || artifact.privacy_class != PrivacyClass::StudentSensitive
        || artifact.archive_status != ArchiveStatus::Ready
    {
        return Err(CoreError::Invalid(
            "学生页面必须引用已归档、student_sensitive 的 page/image artifact".into(),
        ));
    }
    let existing = conn
        .query_row(
            "SELECT id, public_id, batch_id, source_artifact_id,
                    import_index, expected_page_no, state
             FROM exam_ingest_pages_v2 WHERE batch_id=?1 AND source_artifact_id=?2",
            (input.batch_id, input.source_artifact_id),
            page_row,
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing.import_index != input.import_index
            || existing.expected_page_no != input.expected_page_no
        {
            return Err(CoreError::Invalid(
                "同一页面 artifact 已用不同顺序或页码登记".into(),
            ));
        }
        return Ok(existing);
    }
    let occupied: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_ingest_pages_v2
         WHERE batch_id=?1 AND import_index=?2)",
        (input.batch_id, input.import_index),
        |row| row.get(0),
    )?;
    if occupied {
        return Err(CoreError::Invalid("该批次导入顺序已被另一页面占用".into()));
    }

    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO exam_ingest_pages_v2
         (public_id, batch_id, source_artifact_id, import_index,
          expected_page_no, state, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,'imported',?6,?6)",
        (
            &public_id,
            input.batch_id,
            input.source_artifact_id,
            input.import_index,
            input.expected_page_no,
            &now,
        ),
    )?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "UPDATE exam_ingest_batches_v2 SET state='processing', updated_at=?1 WHERE id=?2",
        (&now, input.batch_id),
    )?;
    append_audit(
        &tx,
        &AuditInput {
            key: &format!("exam:ingest-page:{public_id}:registered"),
            actor_type: AuditActorType::System,
            actor_id: None,
            action: "exam.ingest_page.registered",
            object_type: "exam_ingest_page",
            object_id: &public_id,
            revision: None,
            now: &now,
        },
    )?;
    tx.commit()?;
    get_page(conn, id)?.ok_or_else(|| CoreError::NotFound("刚登记的页面".into()))
}

fn resolve_open_issue(
    conn: &Connection,
    target_type: &str,
    target_public_id: &str,
    issue_code: &str,
    resolved_by: &str,
    now: &str,
) -> CoreResult<()> {
    conn.execute(
        "UPDATE exam_pipeline_issues_v2
         SET state='resolved', resolved_by=?1, resolved_at=?2,
             resolution_note='new active revision resolved this issue', updated_at=?2
         WHERE target_type=?3 AND target_public_id=?4 AND issue_code=?5 AND state='open'",
        (resolved_by, now, target_type, target_public_id, issue_code),
    )?;
    Ok(())
}

fn open_issue(
    conn: &Connection,
    target_type: &str,
    target_public_id: &str,
    issue_code: &str,
    severity: &str,
    details_json: &str,
    now: &str,
) -> CoreResult<()> {
    validate_schema_object(details_json, "异常明细")?;
    conn.execute(
        "INSERT INTO exam_pipeline_issues_v2
         (public_id, target_type, target_public_id, issue_code, severity,
          details_json, state, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,'open',?7,?7)
         ON CONFLICT DO NOTHING",
        (
            ids::new_public_id(),
            target_type,
            target_public_id,
            issue_code,
            severity,
            details_json,
            now,
        ),
    )?;
    Ok(())
}

fn refresh_batch_state(conn: &Connection, batch_id: i64, now: &str) -> CoreResult<()> {
    let (page_count, ready_count, review_count): (i64, i64, i64) = conn.query_row(
        "SELECT COUNT(*),
                SUM(CASE WHEN state IN ('matched','aligned','segmented') THEN 1 ELSE 0 END),
                SUM(CASE WHEN state='needs_review' THEN 1 ELSE 0 END)
         FROM exam_ingest_pages_v2 WHERE batch_id=?1 AND state<>'voided'",
        [batch_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let blocking: i64 = conn.query_row(
        "SELECT COUNT(*) FROM exam_pipeline_issues_v2 i
         JOIN exam_ingest_pages_v2 p ON p.public_id=i.target_public_id
         WHERE p.batch_id=?1 AND i.target_type='page'
           AND i.severity='blocking' AND i.state='open'",
        [batch_id],
        |row| row.get(0),
    )?;
    let state = if blocking > 0 || review_count > 0 {
        "needs_review"
    } else if page_count > 0 && page_count == ready_count {
        "ready"
    } else {
        "processing"
    };
    conn.execute(
        "UPDATE exam_ingest_batches_v2 SET state=?1, updated_at=?2
         WHERE id=?3 AND state<>'voided'",
        (state, now, batch_id),
    )?;
    Ok(())
}

fn get_quality(conn: &Connection, id: i64) -> CoreResult<Option<PageQualityRevision>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, page_id, revision, blur_score, glare_score,
                    brightness_score, perspective_score, rotation_degrees, crop_complete,
                    result, issue_codes_json, checked_by_type, checked_by, state
             FROM exam_page_quality_revisions_v2 WHERE id=?1",
            [id],
            quality_row,
        )
        .optional()?)
}

pub fn record_page_quality(
    conn: &Connection,
    input: &NewPageQualityRevision<'_>,
) -> CoreResult<PageQualityRevision> {
    record_page_quality_impl(conn, input, false)
}

fn record_page_quality_impl(
    conn: &Connection,
    input: &NewPageQualityRevision<'_>,
    fail_after_supersede: bool,
) -> CoreResult<PageQualityRevision> {
    for (value, field) in [
        (input.blur_score, "模糊度"),
        (input.glare_score, "反光度"),
        (input.brightness_score, "亮度"),
        (input.perspective_score, "透视完整度"),
    ] {
        if !valid_unit(value) {
            return Err(CoreError::Invalid(format!("{field}必须位于 0~1")));
        }
    }
    if !input.rotation_degrees.is_finite() || !(-180.0..=180.0).contains(&input.rotation_degrees) {
        return Err(CoreError::Invalid("旋转角度必须位于 -180~180".into()));
    }
    validate_schema_object(input.issue_codes_json, "质量异常代码")?;
    if !matches!(input.result, "pass" | "needs_review" | "reject") {
        return Err(CoreError::Invalid("页面质量结论非法".into()));
    }
    if !matches!(input.checked_by_type, "fixture" | "rule" | "teacher") {
        return Err(CoreError::Invalid("页面质量检查来源非法".into()));
    }
    match (input.checked_by_type, input.checked_by) {
        ("teacher", Some(value)) => required(value, "质量检查人")?,
        ("teacher", None) => return Err(CoreError::Invalid("老师检查必须记录检查人".into())),
        (_, Some(_)) => return Err(CoreError::Invalid("机器检查不能冒充老师身份".into())),
        _ => {}
    }
    let page = get_page(conn, input.page_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ingest_page#{}", input.page_id)))?;
    if page.state == "voided" {
        return Err(CoreError::Invalid("作废页面不能追加质量结论".into()));
    }

    let tx = conn.unchecked_transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_page_quality_revisions_v2
         WHERE page_id=?1",
        [input.page_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE exam_page_quality_revisions_v2 SET state='superseded'
         WHERE page_id=?1 AND state='active'",
        [input.page_id],
    )?;
    if fail_after_supersede {
        return Err(CoreError::Db("injected quality transaction failure".into()));
    }
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    tx.execute(
        "INSERT INTO exam_page_quality_revisions_v2
         (public_id, page_id, revision, blur_score, glare_score, brightness_score,
          perspective_score, rotation_degrees, crop_complete, result, issue_codes_json,
          checked_by_type, checked_by, state, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,'active',?14)",
        (
            &public_id,
            input.page_id,
            revision,
            input.blur_score,
            input.glare_score,
            input.brightness_score,
            input.perspective_score,
            input.rotation_degrees,
            i64::from(input.crop_complete),
            input.result,
            input.issue_codes_json,
            input.checked_by_type,
            input.checked_by.map(str::trim),
            &now,
        ),
    )?;
    let id = tx.last_insert_rowid();
    let resolver = input.checked_by.unwrap_or("system");
    resolve_open_issue(
        &tx,
        "page",
        &page.public_id,
        "PAGE_QUALITY_GATE",
        resolver,
        &now,
    )?;
    let page_state = if input.result == "pass" {
        "quality_checked"
    } else {
        open_issue(
            &tx,
            "page",
            &page.public_id,
            "PAGE_QUALITY_GATE",
            "blocking",
            input.issue_codes_json,
            &now,
        )?;
        "needs_review"
    };
    tx.execute(
        "UPDATE exam_ingest_pages_v2 SET state=?1, updated_at=?2 WHERE id=?3",
        (page_state, &now, input.page_id),
    )?;
    refresh_batch_state(&tx, page.batch_id, &now)?;
    append_audit(
        &tx,
        &AuditInput {
            key: &format!("exam:page-quality:{public_id}:active"),
            actor_type: if input.checked_by_type == "teacher" {
                AuditActorType::Teacher
            } else {
                AuditActorType::System
            },
            actor_id: input.checked_by.map(str::trim),
            action: "exam.page_quality.activated",
            object_type: "exam_page_quality",
            object_id: &public_id,
            revision: Some(revision),
            now: &now,
        },
    )?;
    tx.commit()?;
    get_quality(conn, id)?.ok_or_else(|| CoreError::NotFound("刚创建的质量结论".into()))
}

fn get_match(conn: &Connection, id: i64) -> CoreResult<Option<PageMatchRevision>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, page_id, revision, attempt_id, page_no,
                    student_confidence, page_no_confidence, template_confidence,
                    decision, reason_code, confirmed_by, state
             FROM exam_page_match_revisions_v2 WHERE id=?1",
            [id],
            match_row,
        )
        .optional()?)
}

pub fn decide_page_match(
    conn: &Connection,
    input: &NewPageMatchRevision<'_>,
) -> CoreResult<PageMatchRevision> {
    if !matches!(
        input.decision,
        "suggested" | "teacher_confirmed" | "rejected" | "unmatched"
    ) {
        return Err(CoreError::Invalid("页面匹配结论非法".into()));
    }
    for (value, field) in [
        (input.student_confidence, "学生匹配置信度"),
        (input.page_no_confidence, "页码置信度"),
        (input.template_confidence, "模板置信度"),
    ] {
        validate_optional_unit(value, field)?;
    }
    if input.page_no.is_some_and(|value| value < 1) {
        return Err(CoreError::Invalid("匹配页码必须大于 0".into()));
    }
    if matches!(input.decision, "suggested" | "teacher_confirmed")
        && (input.attempt_id.is_none()
            || input.page_no.is_none()
            || input.student_confidence.is_none()
            || input.page_no_confidence.is_none())
    {
        return Err(CoreError::Invalid(
            "匹配建议必须包含 attempt、页码和独立置信度".into(),
        ));
    }
    match (input.decision, input.confirmed_by) {
        ("teacher_confirmed", Some(value)) => required(value, "匹配确认人")?,
        ("teacher_confirmed", None) => {
            return Err(CoreError::Invalid("老师确认匹配必须记录确认人".into()))
        }
        (_, Some(_)) => return Err(CoreError::Invalid("机器建议不能冒充老师确认".into())),
        _ => {}
    }
    let page_scope: Option<(IngestPage, i64)> = conn
        .query_row(
            "SELECT p.id, p.public_id, p.batch_id, p.source_artifact_id,
                    p.import_index, p.expected_page_no, p.state, b.assessment_version_id
             FROM exam_ingest_pages_v2 p
             JOIN exam_ingest_batches_v2 b ON b.id=p.batch_id
             WHERE p.id=?1",
            [input.page_id],
            |row| Ok((page_row(row)?, row.get(7)?)),
        )
        .optional()?;
    let Some((page, assessment_version_id)) = page_scope else {
        return Err(CoreError::NotFound(format!(
            "ingest_page#{}",
            input.page_id
        )));
    };
    let quality_pass: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_page_quality_revisions_v2
         WHERE page_id=?1 AND state='active' AND result='pass')",
        [input.page_id],
        |row| row.get(0),
    )?;
    if !quality_pass {
        return Err(CoreError::Invalid("页面必须先通过当前质量闸门".into()));
    }
    if let Some(attempt_id) = input.attempt_id {
        let valid: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM exam_attempts_v2
             WHERE id=?1 AND assessment_version_id=?2 AND state<>'voided')",
            (attempt_id, assessment_version_id),
            |row| row.get(0),
        )?;
        if !valid {
            return Err(CoreError::Invalid(
                "候选 attempt 与页面所属作业版本不一致".into(),
            ));
        }
    }

    let tx = conn.unchecked_transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_page_match_revisions_v2
         WHERE page_id=?1",
        [input.page_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE exam_page_match_revisions_v2 SET state='superseded'
         WHERE page_id=?1 AND state='active'",
        [input.page_id],
    )?;
    // 页面身份一旦产生新 revision，旧配准与题区不能继续冒充当前结果。
    tx.execute(
        "UPDATE exam_answer_region_revisions_v2 SET state='superseded'
         WHERE page_id=?1 AND state='active'",
        [input.page_id],
    )?;
    tx.execute(
        "UPDATE exam_page_alignment_revisions_v2 SET state='superseded'
         WHERE page_id=?1 AND state='active'",
        [input.page_id],
    )?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    tx.execute(
        "INSERT INTO exam_page_match_revisions_v2
         (public_id, page_id, revision, attempt_id, page_no, student_confidence,
          page_no_confidence, template_confidence, decision, reason_code,
          confirmed_by, state, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'active',?12)",
        (
            &public_id,
            input.page_id,
            revision,
            input.attempt_id,
            input.page_no,
            input.student_confidence,
            input.page_no_confidence,
            input.template_confidence,
            input.decision,
            input.reason_code.map(str::trim),
            input.confirmed_by.map(str::trim),
            &now,
        ),
    )?;
    let id = tx.last_insert_rowid();
    resolve_open_issue(
        &tx,
        "page",
        &page.public_id,
        "PAGE_MATCH_REVIEW",
        input.confirmed_by.unwrap_or("system"),
        &now,
    )?;
    let low_confidence = input.student_confidence.is_some_and(|value| value < 0.95)
        || input.page_no_confidence.is_some_and(|value| value < 0.95)
        || input.template_confidence.is_some_and(|value| value < 0.90);
    let page_state = match input.decision {
        "teacher_confirmed" => "matched",
        "suggested" if !low_confidence => "quality_checked",
        _ => {
            let details = serde_json::json!({
                "schema_version": 1,
                "decision": input.decision,
                "reason_code": input.reason_code,
                "student_confidence": input.student_confidence,
                "page_no_confidence": input.page_no_confidence,
                "template_confidence": input.template_confidence
            })
            .to_string();
            open_issue(
                &tx,
                "page",
                &page.public_id,
                "PAGE_MATCH_REVIEW",
                "blocking",
                &details,
                &now,
            )?;
            "needs_review"
        }
    };
    tx.execute(
        "UPDATE exam_ingest_pages_v2 SET state=?1, updated_at=?2 WHERE id=?3",
        (page_state, &now, input.page_id),
    )?;
    refresh_batch_state(&tx, page.batch_id, &now)?;
    append_audit(
        &tx,
        &AuditInput {
            key: &format!("exam:page-match:{public_id}:active"),
            actor_type: if input.decision == "teacher_confirmed" {
                AuditActorType::Teacher
            } else {
                AuditActorType::System
            },
            actor_id: input.confirmed_by.map(str::trim),
            action: "exam.page_match.activated",
            object_type: "exam_page_match",
            object_id: &public_id,
            revision: Some(revision),
            now: &now,
        },
    )?;
    tx.commit()?;
    get_match(conn, id)?.ok_or_else(|| CoreError::NotFound("刚创建的匹配结论".into()))
}

fn alignment_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PageAlignmentRevision> {
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
}

fn get_alignment(conn: &Connection, id: i64) -> CoreResult<Option<PageAlignmentRevision>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, page_id, revision, match_revision_id,
                    template_version, transform_json, confidence, aligned_artifact_id,
                    decision, reason_code, confirmed_by, state
             FROM exam_page_alignment_revisions_v2 WHERE id=?1",
            [id],
            alignment_row,
        )
        .optional()?)
}

pub fn record_page_alignment(
    conn: &Connection,
    input: &NewPageAlignmentRevision<'_>,
) -> CoreResult<PageAlignmentRevision> {
    let tx = conn.unchecked_transaction()?;
    let result = record_page_alignment_in(&tx, input)?;
    tx.commit()?;
    Ok(result)
}

pub(crate) fn record_page_alignment_in(
    conn: &Connection,
    input: &NewPageAlignmentRevision<'_>,
) -> CoreResult<PageAlignmentRevision> {
    required(input.template_version, "模板版本")?;
    validate_transform(input.transform_json)?;
    if !valid_unit(input.confidence) {
        return Err(CoreError::Invalid("配准置信度必须位于 0~1".into()));
    }
    if !matches!(
        input.decision,
        "suggested" | "teacher_confirmed" | "rejected"
    ) {
        return Err(CoreError::Invalid("页面配准结论非法".into()));
    }
    match (input.decision, input.confirmed_by) {
        ("teacher_confirmed", Some(value)) => required(value, "配准确认人")?,
        ("teacher_confirmed", None) => {
            return Err(CoreError::Invalid("老师确认配准必须记录确认人".into()))
        }
        (_, Some(_)) => return Err(CoreError::Invalid("机器配准不能冒充老师确认".into())),
        _ => {}
    }
    if matches!(input.decision, "suggested" | "teacher_confirmed")
        && input.aligned_artifact_id.is_none()
    {
        return Err(CoreError::Invalid(
            "有效配准必须引用派生页面 artifact".into(),
        ));
    }
    let scope: Option<(i64, IngestPage)> = conn
        .query_row(
            "SELECT m.id, p.id, p.public_id, p.batch_id, p.source_artifact_id,
                    p.import_index, p.expected_page_no, p.state
             FROM exam_page_match_revisions_v2 m
             JOIN exam_ingest_pages_v2 p ON p.id=m.page_id
             WHERE p.id=?1 AND m.state='active' AND m.decision='teacher_confirmed'",
            [input.page_id],
            |row| {
                Ok((
                    row.get(0)?,
                    IngestPage {
                        id: row.get(1)?,
                        public_id: row.get(2)?,
                        batch_id: row.get(3)?,
                        source_artifact_id: row.get(4)?,
                        import_index: row.get(5)?,
                        expected_page_no: row.get(6)?,
                        state: row.get(7)?,
                    },
                ))
            },
        )
        .optional()?;
    let Some((match_revision_id, page)) = scope else {
        return Err(CoreError::Invalid(
            "页面必须先有当前老师确认的学生/页码匹配".into(),
        ));
    };
    if let Some(artifact_id) = input.aligned_artifact_id {
        let artifact = artifacts::get_by_id(conn, artifact_id)?
            .ok_or_else(|| CoreError::NotFound(format!("artifact#{artifact_id}")))?;
        if !matches!(artifact.kind, ArtifactKind::Page | ArtifactKind::Image)
            || artifact.parent_artifact_id != Some(page.source_artifact_id)
            || artifact.privacy_class != PrivacyClass::StudentSensitive
            || artifact.archive_status != ArchiveStatus::Ready
        {
            return Err(CoreError::Invalid(
                "配准结果必须是原页面派生的已归档 student_sensitive page/image artifact".into(),
            ));
        }
    }

    let revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_page_alignment_revisions_v2
         WHERE page_id=?1",
        [input.page_id],
        |row| row.get(0),
    )?;
    // 新配准会改变坐标系，旧题区必须同时失效。
    conn.execute(
        "UPDATE exam_answer_region_revisions_v2 SET state='superseded'
         WHERE page_id=?1 AND state='active'",
        [input.page_id],
    )?;
    conn.execute(
        "UPDATE exam_page_alignment_revisions_v2 SET state='superseded'
         WHERE page_id=?1 AND state='active'",
        [input.page_id],
    )?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO exam_page_alignment_revisions_v2
         (public_id, page_id, revision, match_revision_id, template_version,
          transform_json, confidence, aligned_artifact_id, decision, reason_code,
          confirmed_by, state, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'active',?12)",
        (
            &public_id,
            input.page_id,
            revision,
            match_revision_id,
            input.template_version.trim(),
            input.transform_json,
            input.confidence,
            input.aligned_artifact_id,
            input.decision,
            input.reason_code.map(str::trim),
            input.confirmed_by.map(str::trim),
            &now,
        ),
    )?;
    let id = conn.last_insert_rowid();
    resolve_open_issue(
        conn,
        "page",
        &page.public_id,
        "PAGE_ALIGNMENT_REVIEW",
        input.confirmed_by.unwrap_or("system"),
        &now,
    )?;
    let page_state = match input.decision {
        "teacher_confirmed" => "aligned",
        "suggested" if input.confidence >= 0.90 => "matched",
        _ => {
            let details = serde_json::json!({
                "schema_version": 1,
                "decision": input.decision,
                "confidence": input.confidence,
                "reason_code": input.reason_code
            })
            .to_string();
            open_issue(
                conn,
                "page",
                &page.public_id,
                "PAGE_ALIGNMENT_REVIEW",
                "blocking",
                &details,
                &now,
            )?;
            "needs_review"
        }
    };
    conn.execute(
        "UPDATE exam_ingest_pages_v2 SET state=?1, updated_at=?2 WHERE id=?3",
        (page_state, &now, input.page_id),
    )?;
    refresh_batch_state(conn, page.batch_id, &now)?;
    append_audit(
        conn,
        &AuditInput {
            key: &format!("exam:page-alignment:{public_id}:active"),
            actor_type: if input.decision == "teacher_confirmed" {
                AuditActorType::Teacher
            } else {
                AuditActorType::System
            },
            actor_id: input.confirmed_by.map(str::trim),
            action: "exam.page_alignment.activated",
            object_type: "exam_page_alignment",
            object_id: &public_id,
            revision: Some(revision),
            now: &now,
        },
    )?;
    get_alignment(conn, id)?.ok_or_else(|| CoreError::NotFound("刚创建的页面配准".into()))
}

fn region_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnswerRegionRevision> {
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
}

fn get_region(conn: &Connection, id: i64) -> CoreResult<Option<AnswerRegionRevision>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, page_id, assessment_item_id, region_index,
                    revision, alignment_revision_id, bbox_json, crop_artifact_id,
                    mapping_confidence, decision, reason_code, confirmed_by, state
             FROM exam_answer_region_revisions_v2 WHERE id=?1",
            [id],
            region_row,
        )
        .optional()?)
}

pub fn record_answer_region(
    conn: &Connection,
    input: &NewAnswerRegionRevision<'_>,
) -> CoreResult<AnswerRegionRevision> {
    let tx = conn.unchecked_transaction()?;
    let result = record_answer_region_in(&tx, input)?;
    tx.commit()?;
    Ok(result)
}

pub(crate) fn record_answer_region_in(
    conn: &Connection,
    input: &NewAnswerRegionRevision<'_>,
) -> CoreResult<AnswerRegionRevision> {
    if input.region_index < 0 {
        return Err(CoreError::Invalid("答案区域顺序不能为负数".into()));
    }
    validate_bbox(input.bbox_json)?;
    validate_optional_unit(input.mapping_confidence, "题区映射置信度")?;
    if !matches!(
        input.decision,
        "suggested" | "teacher_confirmed" | "rejected"
    ) {
        return Err(CoreError::Invalid("答案区域结论非法".into()));
    }
    if matches!(input.decision, "suggested" | "teacher_confirmed")
        && (input.crop_artifact_id.is_none() || input.mapping_confidence.is_none())
    {
        return Err(CoreError::Invalid(
            "有效答案区域必须引用裁剪 artifact 和独立置信度".into(),
        ));
    }
    match (input.decision, input.confirmed_by) {
        ("teacher_confirmed", Some(value)) => required(value, "题区确认人")?,
        ("teacher_confirmed", None) => {
            return Err(CoreError::Invalid("老师确认题区必须记录确认人".into()))
        }
        (_, Some(_)) => return Err(CoreError::Invalid("机器题区不能冒充老师确认".into())),
        _ => {}
    }
    let scope: Option<(i64, i64, i64, String, i64)> = conn
        .query_row(
            "SELECT a.id, a.aligned_artifact_id, p.batch_id, p.public_id,
                    at.assessment_version_id
             FROM exam_page_alignment_revisions_v2 a
             JOIN exam_page_match_revisions_v2 m ON m.id=a.match_revision_id
             JOIN exam_attempts_v2 at ON at.id=m.attempt_id
             JOIN exam_ingest_pages_v2 p ON p.id=a.page_id
             WHERE a.page_id=?1 AND a.state='active' AND a.decision='teacher_confirmed'",
            [input.page_id],
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
        .optional()?;
    let Some((alignment_id, aligned_artifact_id, batch_id, page_public_id, version_id)) = scope
    else {
        return Err(CoreError::Invalid(
            "答案区域必须引用当前老师确认的页面配准".into(),
        ));
    };
    let valid_item: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_assessment_items_v2
         WHERE id=?1 AND assessment_version_id=?2 AND state='active')",
        (input.assessment_item_id, version_id),
        |row| row.get(0),
    )?;
    if !valid_item {
        return Err(CoreError::Invalid("题区与页面所属作业题目不一致".into()));
    }
    if let Some(crop_id) = input.crop_artifact_id {
        let artifact = artifacts::get_by_id(conn, crop_id)?
            .ok_or_else(|| CoreError::NotFound(format!("artifact#{crop_id}")))?;
        if artifact.kind != ArtifactKind::Crop
            || artifact.parent_artifact_id != Some(aligned_artifact_id)
            || artifact.privacy_class != PrivacyClass::StudentSensitive
            || artifact.archive_status != ArchiveStatus::Ready
        {
            return Err(CoreError::Invalid(
                "答案裁剪必须是当前配准页面派生的已归档 student_sensitive crop artifact".into(),
            ));
        }
    }

    let revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_answer_region_revisions_v2
         WHERE page_id=?1 AND assessment_item_id=?2 AND region_index=?3",
        (input.page_id, input.assessment_item_id, input.region_index),
        |row| row.get(0),
    )?;
    conn.execute(
        "UPDATE exam_answer_region_revisions_v2 SET state='superseded'
         WHERE page_id=?1 AND assessment_item_id=?2 AND region_index=?3 AND state='active'",
        (input.page_id, input.assessment_item_id, input.region_index),
    )?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO exam_answer_region_revisions_v2
         (public_id, page_id, assessment_item_id, region_index, revision,
          alignment_revision_id, bbox_json, crop_artifact_id, mapping_confidence,
          decision, reason_code, confirmed_by, state, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'active',?13)",
        (
            &public_id,
            input.page_id,
            input.assessment_item_id,
            input.region_index,
            revision,
            alignment_id,
            input.bbox_json,
            input.crop_artifact_id,
            input.mapping_confidence,
            input.decision,
            input.reason_code.map(str::trim),
            input.confirmed_by.map(str::trim),
            &now,
        ),
    )?;
    let id = conn.last_insert_rowid();
    let issue_code = format!(
        "ANSWER_REGION_REVIEW:{}:{}",
        input.assessment_item_id, input.region_index
    );
    resolve_open_issue(
        conn,
        "page",
        &page_public_id,
        &issue_code,
        input.confirmed_by.unwrap_or("system"),
        &now,
    )?;
    let low_confidence = input.mapping_confidence.is_some_and(|value| value < 0.95);
    let proposed_state = match input.decision {
        "teacher_confirmed" => "segmented",
        "suggested" if !low_confidence => "aligned",
        _ => {
            let details = serde_json::json!({
                "schema_version": 1,
                "decision": input.decision,
                "mapping_confidence": input.mapping_confidence,
                "reason_code": input.reason_code,
                "assessment_item_id": input.assessment_item_id,
                "region_index": input.region_index
            })
            .to_string();
            open_issue(
                conn,
                "page",
                &page_public_id,
                &issue_code,
                "blocking",
                &details,
                &now,
            )?;
            "needs_review"
        }
    };
    let other_blocking: i64 = conn.query_row(
        "SELECT COUNT(*) FROM exam_pipeline_issues_v2
         WHERE target_type='page' AND target_public_id=?1
           AND severity='blocking' AND state='open'",
        [&page_public_id],
        |row| row.get(0),
    )?;
    let page_state = if other_blocking > 0 {
        "needs_review"
    } else {
        proposed_state
    };
    conn.execute(
        "UPDATE exam_ingest_pages_v2 SET state=?1, updated_at=?2 WHERE id=?3",
        (page_state, &now, input.page_id),
    )?;
    refresh_batch_state(conn, batch_id, &now)?;
    append_audit(
        conn,
        &AuditInput {
            key: &format!("exam:answer-region:{public_id}:active"),
            actor_type: if input.decision == "teacher_confirmed" {
                AuditActorType::Teacher
            } else {
                AuditActorType::System
            },
            actor_id: input.confirmed_by.map(str::trim),
            action: "exam.answer_region.activated",
            object_type: "exam_answer_region",
            object_id: &public_id,
            revision: Some(revision),
            now: &now,
        },
    )?;
    get_region(conn, id)?.ok_or_else(|| CoreError::NotFound("刚创建的答案区域".into()))
}

pub fn trace_answer_region(conn: &Connection, region_id: i64) -> CoreResult<Option<RegionTrace>> {
    Ok(conn
        .query_row(
            "SELECT at.student_id, at.assessment_version_id, m.attempt_id, m.page_no,
                    p.source_artifact_id, a.aligned_artifact_id, r.assessment_item_id,
                    i.question_version_id, r.bbox_json, r.crop_artifact_id,
                    r.revision, r.decision
             FROM exam_answer_region_revisions_v2 r
             JOIN exam_page_alignment_revisions_v2 a ON a.id=r.alignment_revision_id
             JOIN exam_page_match_revisions_v2 m ON m.id=a.match_revision_id
             JOIN exam_attempts_v2 at ON at.id=m.attempt_id
             JOIN exam_ingest_pages_v2 p ON p.id=r.page_id
             JOIN exam_assessment_items_v2 i ON i.id=r.assessment_item_id
             WHERE r.id=?1",
            [region_id],
            |row| {
                Ok(RegionTrace {
                    student_id: row.get(0)?,
                    assessment_version_id: row.get(1)?,
                    attempt_id: row.get(2)?,
                    page_no: row.get(3)?,
                    source_page_artifact_id: row.get(4)?,
                    aligned_page_artifact_id: row.get(5)?,
                    assessment_item_id: row.get(6)?,
                    question_version_id: row.get(7)?,
                    bbox_json: row.get(8)?,
                    crop_artifact_id: row.get(9)?,
                    region_revision: row.get(10)?,
                    region_decision: row.get(11)?,
                })
            },
        )
        .optional()?)
}

/// 启动恢复：中断的 processing 批次进入老师可见的 needs_review，页面事实不丢失。
pub fn recover_stale_ingest_batches(conn: &Connection) -> CoreResult<usize> {
    let mut stmt = conn.prepare(
        "SELECT id, public_id FROM exam_ingest_batches_v2
         WHERE state='processing' ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let batches = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    if batches.is_empty() {
        return Ok(0);
    }
    let now = time::utc_now_rfc3339();
    let tx = conn.unchecked_transaction()?;
    for (id, public_id) in &batches {
        tx.execute(
            "UPDATE exam_ingest_batches_v2 SET state='needs_review', updated_at=?1
             WHERE id=?2 AND state='processing'",
            (&now, id),
        )?;
        open_issue(
            &tx,
            "batch",
            public_id,
            "INTERRUPTED_PROCESSING",
            "blocking",
            r#"{"schema_version":1,"reason":"app_restarted_during_processing"}"#,
            &now,
        )?;
        append_audit(
            &tx,
            &AuditInput {
                key: &format!("exam:ingest-batch:{public_id}:recovered"),
                actor_type: AuditActorType::System,
                actor_id: None,
                action: "exam.ingest_batch.recovered",
                object_type: "exam_ingest_batch",
                object_id: public_id,
                revision: None,
                now: &now,
            },
        )?;
    }
    tx.commit()?;
    Ok(batches.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::repo::artifacts::{create_or_get, NewArtifact};
    use suite_core::db::{open, open_in_memory, run_migrations, CORE_MIGRATIONS};

    struct Fixture {
        conn: Connection,
        version_id: i64,
        item_id: i64,
        attempt_id: i64,
        other_attempt_id: i64,
        artifact_id: i64,
    }

    fn seed(conn: Connection) -> Fixture {
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        let hash = "a".repeat(64);
        conn.execute_batch(&format!(
            r#"INSERT INTO subjects(name) VALUES ('历史');
             INSERT INTO classes(name, term) VALUES ('八年级一班','2026秋');
             INSERT INTO students(student_no,name,class_id) VALUES ('S001','小林',1);
             INSERT INTO exam_assessments_v2
               (public_id,title,class_id,assessment_context,evidence_policy,state,
                created_by,created_at,updated_at)
               VALUES ('assessment-a','随堂测',1,'quiz','include','active','teacher',
                       '2026-07-13T08:00:00.000Z','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_assessment_versions_v2
               (public_id,assessment_id,revision,item_set_hash,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('assessment-version-a',1,1,'{hash}','confirmed',
                       '2026-07-13T08:00:00.000Z','teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO k1_textbook_editions
               (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
               VALUES ('edition-a',1,'PEP','2024','中国历史八上','8','upper','active',
                       '2026-07-13T08:00:00.000Z');
             INSERT INTO k1_knowledge_maps
               (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
               VALUES ('map-a',1,1,'confirmed','2026-07-13T08:00:00.000Z',
                       '2026-07-13T08:00:00.000Z');
             INSERT INTO k1_questions
               (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
               VALUES ('question-a','personal','teacher','unknown',0,
                       '2026-07-13T08:00:00.000Z');
             INSERT INTO k1_question_versions
               (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                quality_level,state,created_at)
               VALUES ('question-version-a',1,1,'true_false','鸦片战争爆发于1840年。',1,'{hash}',
                       'L3','published','2026-07-13T08:00:00.000Z');
             INSERT INTO k1_answer_key_versions
               (public_id,question_version_id,revision,answer_json,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('answer-a',1,1,'{{"schema_version":1,"correct":true}}','confirmed',
                       '2026-07-13T08:00:00.000Z','teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO k1_rubric_versions
               (public_id,question_version_id,revision,max_score,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('rubric-a',1,1,1,'confirmed','2026-07-13T08:00:00.000Z',
                       'teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO k1_link_sets
               (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('link-set-a',1,1,1,'confirmed','2026-07-13T08:00:00.000Z',
                       'teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_assessment_items_v2
               (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                state,created_at)
               VALUES ('assessment-item-a',1,1,1,1,1,0,1,
                       '{{"schema_version":1,"question_no":"1"}}','active',
                       '2026-07-13T08:00:00.000Z');
             INSERT INTO exam_attempts_v2
               (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                attempt_kind,state,created_at,updated_at)
               VALUES ('attempt-a',1,1,1,'image','first','ingesting',
                       '2026-07-13T08:00:00.000Z','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_assessments_v2
               (public_id,title,class_id,assessment_context,evidence_policy,state,
                created_by,created_at,updated_at)
               VALUES ('assessment-b','另一作业',1,'quiz','include','active','teacher',
                       '2026-07-13T08:00:00.000Z','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_assessment_versions_v2
               (public_id,assessment_id,revision,item_set_hash,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('assessment-version-b',2,1,'{hash}','confirmed',
                       '2026-07-13T08:00:00.000Z','teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_attempts_v2
               (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                attempt_kind,state,created_at,updated_at)
               VALUES ('attempt-b',2,1,1,'image','first','ingesting',
                       '2026-07-13T08:00:00.000Z','2026-07-13T08:00:00.000Z');"#
        ))
        .unwrap();
        let artifact = create_or_get(
            &conn,
            &NewArtifact {
                kind: ArtifactKind::Page,
                sha256: &"1".repeat(64),
                mime_type: "image/png",
                byte_size: 128,
                original_name: Some("S001-page-1.png"),
                original_path: Some("/fixture/S001-page-1.png"),
                archived_path: "/archive/page-1.png",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: "fixture-v1",
                privacy_class: PrivacyClass::StudentSensitive,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        Fixture {
            conn,
            version_id: 1,
            item_id: 1,
            attempt_id: 1,
            other_attempt_id: 2,
            artifact_id: artifact.id,
        }
    }

    fn setup() -> Fixture {
        seed(open_in_memory().unwrap())
    }

    fn batch(fixture: &Fixture, key: &str) -> IngestBatch {
        create_or_get_ingest_batch(
            &fixture.conn,
            &NewIngestBatch {
                assessment_version_id: fixture.version_id,
                source_kind: "fixed_fixture",
                idempotency_key: key,
                created_by: "teacher",
            },
        )
        .unwrap()
    }

    fn page(fixture: &Fixture, batch_id: i64) -> IngestPage {
        register_ingest_page(
            &fixture.conn,
            &NewIngestPage {
                batch_id,
                source_artifact_id: fixture.artifact_id,
                import_index: 0,
                expected_page_no: Some(1),
            },
        )
        .unwrap()
    }

    fn quality<'a>(page_id: i64, result: &'a str) -> NewPageQualityRevision<'a> {
        NewPageQualityRevision {
            page_id,
            blur_score: 0.02,
            glare_score: 0.01,
            brightness_score: 0.8,
            perspective_score: 0.98,
            rotation_degrees: 0.0,
            crop_complete: true,
            result,
            issue_codes_json: r#"{"schema_version":1,"codes":[]}"#,
            checked_by_type: "fixture",
            checked_by: None,
        }
    }

    fn matched_page(fixture: &Fixture, key: &str) -> IngestPage {
        let batch = batch(fixture, key);
        let page = page(fixture, batch.id);
        record_page_quality(&fixture.conn, &quality(page.id, "pass")).unwrap();
        decide_page_match(
            &fixture.conn,
            &NewPageMatchRevision {
                page_id: page.id,
                attempt_id: Some(fixture.attempt_id),
                page_no: Some(1),
                student_confidence: Some(0.99),
                page_no_confidence: Some(0.99),
                template_confidence: Some(0.99),
                decision: "teacher_confirmed",
                reason_code: None,
                confirmed_by: Some("teacher"),
            },
        )
        .unwrap();
        page
    }

    fn derived_artifact(
        fixture: &Fixture,
        kind: ArtifactKind,
        hash_digit: char,
        parent_id: i64,
        derivative_type: &str,
    ) -> i64 {
        create_or_get(
            &fixture.conn,
            &NewArtifact {
                kind,
                sha256: &hash_digit.to_string().repeat(64),
                mime_type: "image/png",
                byte_size: 96,
                original_name: None,
                original_path: None,
                archived_path: &format!("/archive/{derivative_type}-{hash_digit}.png"),
                parent_artifact_id: Some(parent_id),
                derivative_type: Some(derivative_type),
                processing_version: "fixture-v1",
                privacy_class: PrivacyClass::StudentSensitive,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn duplicate_batch_and_page_registration_are_idempotent() {
        let fixture = setup();
        let first_batch = batch(&fixture, "fixture-batch-1");
        let repeated_batch = batch(&fixture, "fixture-batch-1");
        assert_eq!(first_batch.id, repeated_batch.id);
        let first_page = page(&fixture, first_batch.id);
        let repeated_page = page(&fixture, first_batch.id);
        assert_eq!(first_page.id, repeated_page.id);
        assert!(register_ingest_page(
            &fixture.conn,
            &NewIngestPage {
                batch_id: first_batch.id,
                source_artifact_id: fixture.artifact_id,
                import_index: 1,
                expected_page_no: Some(1),
            }
        )
        .is_err());
        let counts: (i64, i64) = fixture
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM exam_ingest_batches_v2),
                        (SELECT COUNT(*) FROM exam_ingest_pages_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(counts, (1, 1));
    }

    #[test]
    fn quality_revisions_are_atomic_and_recover_from_rejection() {
        let fixture = setup();
        let page = page(&fixture, batch(&fixture, "quality-batch").id);
        let first = record_page_quality(&fixture.conn, &quality(page.id, "pass")).unwrap();
        assert_eq!(first.revision, 1);
        assert!(
            record_page_quality_impl(&fixture.conn, &quality(page.id, "reject"), true).is_err()
        );
        let active: (i64, String) = fixture
            .conn
            .query_row(
                "SELECT revision,state FROM exam_page_quality_revisions_v2
                 WHERE page_id=?1 AND state='active'",
                [page.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(active, (1, "active".into()));

        let rejected = record_page_quality(&fixture.conn, &quality(page.id, "reject")).unwrap();
        assert_eq!(rejected.revision, 2);
        assert_eq!(
            get_page(&fixture.conn, page.id).unwrap().unwrap().state,
            "needs_review"
        );
        let recovered = record_page_quality(&fixture.conn, &quality(page.id, "pass")).unwrap();
        assert_eq!(recovered.revision, 3);
        assert_eq!(
            get_page(&fixture.conn, page.id).unwrap().unwrap().state,
            "quality_checked"
        );
        let open_issues: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_pipeline_issues_v2 WHERE state='open'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(open_issues, 0);
    }

    #[test]
    fn low_confidence_never_silently_binds_and_wrong_assessment_is_rejected() {
        let fixture = setup();
        let batch = batch(&fixture, "match-batch");
        let page = page(&fixture, batch.id);
        record_page_quality(&fixture.conn, &quality(page.id, "pass")).unwrap();
        assert!(decide_page_match(
            &fixture.conn,
            &NewPageMatchRevision {
                page_id: page.id,
                attempt_id: Some(fixture.other_attempt_id),
                page_no: Some(1),
                student_confidence: Some(0.99),
                page_no_confidence: Some(0.99),
                template_confidence: Some(0.99),
                decision: "suggested",
                reason_code: None,
                confirmed_by: None,
            }
        )
        .is_err());
        let suggestion = decide_page_match(
            &fixture.conn,
            &NewPageMatchRevision {
                page_id: page.id,
                attempt_id: Some(fixture.attempt_id),
                page_no: Some(1),
                student_confidence: Some(0.72),
                page_no_confidence: Some(0.99),
                template_confidence: Some(0.99),
                decision: "suggested",
                reason_code: Some("LOW_STUDENT_CONFIDENCE"),
                confirmed_by: None,
            },
        )
        .unwrap();
        assert_eq!(suggestion.revision, 1);
        assert_eq!(
            get_page(&fixture.conn, page.id).unwrap().unwrap().state,
            "needs_review"
        );
        assert_eq!(
            get_batch(&fixture.conn, batch.id).unwrap().unwrap().state,
            "needs_review"
        );

        let confirmed = decide_page_match(
            &fixture.conn,
            &NewPageMatchRevision {
                page_id: page.id,
                attempt_id: Some(fixture.attempt_id),
                page_no: Some(1),
                student_confidence: Some(0.72),
                page_no_confidence: Some(0.99),
                template_confidence: Some(0.99),
                decision: "teacher_confirmed",
                reason_code: Some("TEACHER_CHECKED_NAME"),
                confirmed_by: Some("teacher"),
            },
        )
        .unwrap();
        assert_eq!(confirmed.revision, 2);
        assert_eq!(
            get_page(&fixture.conn, page.id).unwrap().unwrap().state,
            "matched"
        );
        assert_eq!(
            get_batch(&fixture.conn, batch.id).unwrap().unwrap().state,
            "ready"
        );
    }

    #[test]
    fn restart_recovery_preserves_pages_and_surfaces_interrupted_batches() {
        let path = std::env::temp_dir().join(format!(
            "jiaofu-paper-restart-{}-{}.db",
            std::process::id(),
            time::utc_now_rfc3339().replace([':', '.'], "-")
        ));
        let fixture = seed(open(&path).unwrap());
        let batch = batch(&fixture, "restart-batch");
        let page = page(&fixture, batch.id);
        drop(fixture);

        let conn = open(&path).unwrap();
        assert_eq!(recover_stale_ingest_batches(&conn).unwrap(), 1);
        assert_eq!(recover_stale_ingest_batches(&conn).unwrap(), 0);
        assert_eq!(
            get_batch(&conn, batch.id).unwrap().unwrap().state,
            "needs_review"
        );
        assert!(get_page(&conn, page.id).unwrap().is_some());
        let issue: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM exam_pipeline_issues_v2
                 WHERE target_type='batch' AND target_public_id=?1
                   AND issue_code='INTERRUPTED_PROCESSING' AND state='open'",
                [&batch.public_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(issue, 1);
        drop(conn);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn alignment_and_region_require_teacher_confirmation_and_keep_full_trace() {
        let fixture = setup();
        let page = matched_page(&fixture, "region-batch");
        let aligned_id = derived_artifact(
            &fixture,
            ArtifactKind::Page,
            '2',
            fixture.artifact_id,
            "page_alignment",
        );
        let transform = r#"{"schema_version":1,"matrix":[1,0,0,0,1,0,0,0,1]}"#;
        let suggested = record_page_alignment(
            &fixture.conn,
            &NewPageAlignmentRevision {
                page_id: page.id,
                template_version: "template-v1",
                transform_json: transform,
                confidence: 0.71,
                aligned_artifact_id: Some(aligned_id),
                decision: "suggested",
                reason_code: Some("LOW_ALIGNMENT_CONFIDENCE"),
                confirmed_by: None,
            },
        )
        .unwrap();
        assert_eq!(suggested.revision, 1);
        assert_eq!(
            get_page(&fixture.conn, page.id).unwrap().unwrap().state,
            "needs_review"
        );

        let alignment = record_page_alignment(
            &fixture.conn,
            &NewPageAlignmentRevision {
                page_id: page.id,
                template_version: "template-v1",
                transform_json: transform,
                confidence: 0.71,
                aligned_artifact_id: Some(aligned_id),
                decision: "teacher_confirmed",
                reason_code: Some("TEACHER_CHECKED_CORNERS"),
                confirmed_by: Some("teacher"),
            },
        )
        .unwrap();
        assert_eq!(alignment.revision, 2);
        assert_eq!(
            get_page(&fixture.conn, page.id).unwrap().unwrap().state,
            "aligned"
        );

        let crop_id = derived_artifact(
            &fixture,
            ArtifactKind::Crop,
            '3',
            aligned_id,
            "answer_region",
        );
        let bbox = r#"{"schema_version":1,"x":0.1,"y":0.2,"width":0.7,"height":0.2}"#;
        let region_suggestion = record_answer_region(
            &fixture.conn,
            &NewAnswerRegionRevision {
                page_id: page.id,
                assessment_item_id: fixture.item_id,
                region_index: 0,
                bbox_json: bbox,
                crop_artifact_id: Some(crop_id),
                mapping_confidence: Some(0.74),
                decision: "suggested",
                reason_code: Some("LOW_REGION_CONFIDENCE"),
                confirmed_by: None,
            },
        )
        .unwrap();
        assert_eq!(region_suggestion.revision, 1);
        assert_eq!(
            get_page(&fixture.conn, page.id).unwrap().unwrap().state,
            "needs_review"
        );

        let region = record_answer_region(
            &fixture.conn,
            &NewAnswerRegionRevision {
                page_id: page.id,
                assessment_item_id: fixture.item_id,
                region_index: 0,
                bbox_json: bbox,
                crop_artifact_id: Some(crop_id),
                mapping_confidence: Some(0.74),
                decision: "teacher_confirmed",
                reason_code: Some("TEACHER_CHECKED_REGION"),
                confirmed_by: Some("teacher"),
            },
        )
        .unwrap();
        assert_eq!(region.revision, 2);
        assert_eq!(
            get_page(&fixture.conn, page.id).unwrap().unwrap().state,
            "segmented"
        );
        let trace = trace_answer_region(&fixture.conn, region.id)
            .unwrap()
            .unwrap();
        assert_eq!(trace.student_id, 1);
        assert_eq!(trace.assessment_version_id, fixture.version_id);
        assert_eq!(trace.attempt_id, fixture.attempt_id);
        assert_eq!(trace.page_no, 1);
        assert_eq!(trace.source_page_artifact_id, fixture.artifact_id);
        assert_eq!(trace.aligned_page_artifact_id, aligned_id);
        assert_eq!(trace.assessment_item_id, fixture.item_id);
        assert_eq!(trace.question_version_id, 1);
        assert_eq!(trace.crop_artifact_id, crop_id);
    }

    #[test]
    fn new_alignment_invalidates_old_regions_and_crop_lineage_is_enforced() {
        let fixture = setup();
        let page = matched_page(&fixture, "realign-batch");
        let aligned_id = derived_artifact(
            &fixture,
            ArtifactKind::Page,
            '4',
            fixture.artifact_id,
            "page_alignment",
        );
        let transform = r#"{"schema_version":1,"matrix":[1,0,0,0,1,0,0,0,1]}"#;
        record_page_alignment(
            &fixture.conn,
            &NewPageAlignmentRevision {
                page_id: page.id,
                template_version: "template-v1",
                transform_json: transform,
                confidence: 0.99,
                aligned_artifact_id: Some(aligned_id),
                decision: "teacher_confirmed",
                reason_code: None,
                confirmed_by: Some("teacher"),
            },
        )
        .unwrap();
        let wrong_parent_crop = derived_artifact(
            &fixture,
            ArtifactKind::Crop,
            '5',
            fixture.artifact_id,
            "answer_region_wrong_parent",
        );
        let bbox = r#"{"schema_version":1,"x":0.1,"y":0.2,"width":0.7,"height":0.2}"#;
        assert!(record_answer_region(
            &fixture.conn,
            &NewAnswerRegionRevision {
                page_id: page.id,
                assessment_item_id: fixture.item_id,
                region_index: 0,
                bbox_json: bbox,
                crop_artifact_id: Some(wrong_parent_crop),
                mapping_confidence: Some(0.99),
                decision: "teacher_confirmed",
                reason_code: None,
                confirmed_by: Some("teacher"),
            }
        )
        .is_err());
        let crop_id = derived_artifact(
            &fixture,
            ArtifactKind::Crop,
            '6',
            aligned_id,
            "answer_region",
        );
        let region = record_answer_region(
            &fixture.conn,
            &NewAnswerRegionRevision {
                page_id: page.id,
                assessment_item_id: fixture.item_id,
                region_index: 0,
                bbox_json: bbox,
                crop_artifact_id: Some(crop_id),
                mapping_confidence: Some(0.99),
                decision: "teacher_confirmed",
                reason_code: None,
                confirmed_by: Some("teacher"),
            },
        )
        .unwrap();

        let realigned_id = derived_artifact(
            &fixture,
            ArtifactKind::Page,
            '7',
            fixture.artifact_id,
            "page_alignment_v2",
        );
        record_page_alignment(
            &fixture.conn,
            &NewPageAlignmentRevision {
                page_id: page.id,
                template_version: "template-v2",
                transform_json: transform,
                confidence: 0.98,
                aligned_artifact_id: Some(realigned_id),
                decision: "teacher_confirmed",
                reason_code: Some("TEMPLATE_CHANGED"),
                confirmed_by: Some("teacher"),
            },
        )
        .unwrap();
        assert_eq!(
            get_region(&fixture.conn, region.id).unwrap().unwrap().state,
            "superseded"
        );
        assert_eq!(
            get_page(&fixture.conn, page.id).unwrap().unwrap().state,
            "aligned"
        );
        assert!(trace_answer_region(&fixture.conn, region.id)
            .unwrap()
            .is_some());
    }
}
