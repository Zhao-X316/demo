//! M2-B3a 固定试卷入站清单、答案权威与三路预检。
//!
//! 本服务只编排已有 B1/B2 事实：原始资料必须先归档为 core artifact，页面身份仍以
//! teacher-confirmed page match 为准，题区/客观题 observation/老师 grade decision 继续由
//! 既有服务维护。这里不确认评分、不发布成绩，也不生成正式学习证据。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::{artifacts, audit};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{ArchiveStatus, ArtifactKind, AuditActorType, PrivacyClass};

use super::answer_source::{self, AnswerSourcePreflightGate};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixedInputDocument {
    pub id: i64,
    pub public_id: String,
    pub ingest_batch_id: i64,
    pub source_artifact_id: i64,
    pub document_role: String,
    pub source_format: String,
    pub import_index: i64,
    pub page_count: i64,
    pub idempotency_key: String,
    pub state: String,
    pub created_by: String,
}

pub struct NewFixedInputDocument<'a> {
    pub ingest_batch_id: i64,
    pub source_artifact_id: i64,
    pub document_role: &'a str,
    pub source_format: &'a str,
    pub import_index: i64,
    pub page_count: i64,
    pub idempotency_key: &'a str,
    pub created_by: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerAuthorityCandidate {
    pub id: i64,
    pub public_id: String,
    pub ingest_batch_id: i64,
    pub assessment_item_id: i64,
    pub source_kind: String,
    pub source_priority: i64,
    pub answer_key_version_id: Option<i64>,
    pub source_artifact_id: Option<i64>,
    pub candidate_answer_json: String,
    pub answer_fingerprint: String,
    pub source_anchor_json: String,
    pub teacher_confirmed: bool,
    pub idempotency_key: String,
    pub state: String,
    pub created_by_type: String,
    pub created_by: Option<String>,
}

pub struct NewAnswerAuthorityCandidate<'a> {
    pub ingest_batch_id: i64,
    pub assessment_item_id: i64,
    pub source_kind: &'a str,
    pub answer_key_version_id: Option<i64>,
    pub source_artifact_id: Option<i64>,
    pub candidate_answer_json: &'a str,
    pub source_anchor_json: &'a str,
    pub teacher_confirmed: bool,
    pub idempotency_key: &'a str,
    pub created_by_type: &'a str,
    pub created_by: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixedPaperPreflightRevision {
    pub id: i64,
    pub public_id: String,
    pub ingest_batch_id: i64,
    pub revision: i64,
    pub snapshot_hash: String,
    pub expected_pages_per_attempt: i64,
    pub route: String,
    pub target_count: i64,
    pub ready_count: i64,
    pub review_count: i64,
    pub blocked_count: i64,
    pub completed_count: i64,
    pub reason_codes_json: String,
    pub answer_authority_json: String,
    pub grouping_json: String,
    pub state: String,
    pub created_by_type: String,
    pub created_by: Option<String>,
}

pub struct FixedPaperPreflightInput<'a> {
    pub ingest_batch_id: i64,
    pub expected_pages_per_attempt: i64,
    pub created_by_type: &'a str,
    pub created_by: Option<&'a str>,
}

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{field}不能为空")))
    } else {
        Ok(())
    }
}

fn canonical_schema_object(json: &str, field: &str) -> CoreResult<(Value, String)> {
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
    let canonical = serde_json::to_string(&value)
        .map_err(|error| CoreError::Parse(format!("{field} JSON 规范化失败：{error}")))?;
    Ok((value, canonical))
}

fn audit_actor(actor_type: &str, actor_id: Option<&str>) -> CoreResult<AuditActorType> {
    match (actor_type, actor_id) {
        ("system", None) => Ok(AuditActorType::System),
        ("teacher", Some(value)) if !value.trim().is_empty() => Ok(AuditActorType::Teacher),
        ("teacher", _) => Err(CoreError::Invalid("老师操作必须记录操作人".into())),
        ("system", Some(_)) => Err(CoreError::Invalid("系统操作不能冒充老师身份".into())),
        _ => Err(CoreError::Invalid("操作人类型非法".into())),
    }
}

struct AuditInput<'a> {
    idempotency_key: &'a str,
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
            idempotency_key: input.idempotency_key,
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

fn input_document_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FixedInputDocument> {
    Ok(FixedInputDocument {
        id: row.get(0)?,
        public_id: row.get(1)?,
        ingest_batch_id: row.get(2)?,
        source_artifact_id: row.get(3)?,
        document_role: row.get(4)?,
        source_format: row.get(5)?,
        import_index: row.get(6)?,
        page_count: row.get(7)?,
        idempotency_key: row.get(8)?,
        state: row.get(9)?,
        created_by: row.get(10)?,
    })
}

fn candidate_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnswerAuthorityCandidate> {
    Ok(AnswerAuthorityCandidate {
        id: row.get(0)?,
        public_id: row.get(1)?,
        ingest_batch_id: row.get(2)?,
        assessment_item_id: row.get(3)?,
        source_kind: row.get(4)?,
        source_priority: row.get(5)?,
        answer_key_version_id: row.get(6)?,
        source_artifact_id: row.get(7)?,
        candidate_answer_json: row.get(8)?,
        answer_fingerprint: row.get(9)?,
        source_anchor_json: row.get(10)?,
        teacher_confirmed: row.get::<_, i64>(11)? != 0,
        idempotency_key: row.get(12)?,
        state: row.get(13)?,
        created_by_type: row.get(14)?,
        created_by: row.get(15)?,
    })
}

fn preflight_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FixedPaperPreflightRevision> {
    Ok(FixedPaperPreflightRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        ingest_batch_id: row.get(2)?,
        revision: row.get(3)?,
        snapshot_hash: row.get(4)?,
        expected_pages_per_attempt: row.get(5)?,
        route: row.get(6)?,
        target_count: row.get(7)?,
        ready_count: row.get(8)?,
        review_count: row.get(9)?,
        blocked_count: row.get(10)?,
        completed_count: row.get(11)?,
        reason_codes_json: row.get(12)?,
        answer_authority_json: row.get(13)?,
        grouping_json: row.get(14)?,
        state: row.get(15)?,
        created_by_type: row.get(16)?,
        created_by: row.get(17)?,
    })
}

fn get_input_document(conn: &Connection, id: i64) -> CoreResult<Option<FixedInputDocument>> {
    Ok(conn
        .query_row(
            "SELECT id,public_id,ingest_batch_id,source_artifact_id,document_role,
                    source_format,import_index,page_count,idempotency_key,state,created_by
             FROM exam_fixed_input_documents_v2 WHERE id=?1",
            [id],
            input_document_row,
        )
        .optional()?)
}

fn get_candidate(conn: &Connection, id: i64) -> CoreResult<Option<AnswerAuthorityCandidate>> {
    Ok(conn
        .query_row(
            "SELECT id,public_id,ingest_batch_id,assessment_item_id,source_kind,
                    source_priority,answer_key_version_id,source_artifact_id,
                    candidate_answer_json,answer_fingerprint,source_anchor_json,
                    teacher_confirmed,idempotency_key,state,created_by_type,created_by
             FROM exam_answer_authority_candidates_v2 WHERE id=?1",
            [id],
            candidate_row,
        )
        .optional()?)
}

fn get_preflight(conn: &Connection, id: i64) -> CoreResult<Option<FixedPaperPreflightRevision>> {
    Ok(conn
        .query_row(
            "SELECT id,public_id,ingest_batch_id,revision,snapshot_hash,
                    expected_pages_per_attempt,route,target_count,ready_count,
                    review_count,blocked_count,completed_count,reason_codes_json,
                    answer_authority_json,grouping_json,state,created_by_type,created_by
             FROM exam_fixed_preflight_revisions_v2 WHERE id=?1",
            [id],
            preflight_row,
        )
        .optional()?)
}

fn batch_assessment_version(conn: &Connection, batch_id: i64) -> CoreResult<i64> {
    conn.query_row(
        "SELECT b.assessment_version_id
         FROM exam_ingest_batches_v2 b
         JOIN exam_assessment_versions_v2 v ON v.id=b.assessment_version_id
         WHERE b.id=?1 AND b.state<>'voided' AND v.state='confirmed'",
        [batch_id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| CoreError::Invalid("固定试卷批次必须属于已确认作业版本".into()))
}

pub fn register_fixed_input_document(
    conn: &Connection,
    input: &NewFixedInputDocument<'_>,
) -> CoreResult<FixedInputDocument> {
    required(input.idempotency_key, "资料幂等键")?;
    required(input.created_by, "资料登记人")?;
    if input.import_index < 0 || input.page_count < 1 {
        return Err(CoreError::Invalid("资料顺序或页数非法".into()));
    }
    if !matches!(input.document_role, "student_work" | "answer_source")
        || !matches!(input.source_format, "jpeg" | "pdf" | "text")
        || (input.document_role == "student_work" && input.source_format == "text")
        || (input.source_format == "jpeg" && input.page_count != 1)
    {
        return Err(CoreError::Invalid(
            "固定试卷资料角色、格式或页数非法".into(),
        ));
    }
    batch_assessment_version(conn, input.ingest_batch_id)?;
    let artifact = artifacts::get_by_id(conn, input.source_artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{}", input.source_artifact_id)))?;
    if artifact.archive_status != ArchiveStatus::Ready {
        return Err(CoreError::Invalid("固定试卷资料必须先完成归档".into()));
    }
    let expected_privacy = if input.document_role == "student_work" {
        PrivacyClass::StudentSensitive
    } else {
        PrivacyClass::TeachingContent
    };
    if artifact.privacy_class != expected_privacy {
        return Err(CoreError::Invalid("资料角色与隐私分类不一致".into()));
    }
    let format_matches = match input.source_format {
        "jpeg" => artifact.kind == ArtifactKind::Image && artifact.mime_type == "image/jpeg",
        "pdf" => artifact.kind == ArtifactKind::Document && artifact.mime_type == "application/pdf",
        "text" => {
            artifact.kind == ArtifactKind::Document
                && matches!(
                    artifact.mime_type.as_str(),
                    "text/plain"
                        | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                        | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                )
        }
        _ => false,
    };
    if !format_matches {
        return Err(CoreError::Invalid("资料声明格式与 artifact 不一致".into()));
    }

    let existing = conn
        .query_row(
            "SELECT id,public_id,ingest_batch_id,source_artifact_id,document_role,
                    source_format,import_index,page_count,idempotency_key,state,created_by
             FROM exam_fixed_input_documents_v2
             WHERE ingest_batch_id=?1 AND idempotency_key=?2",
            (input.ingest_batch_id, input.idempotency_key.trim()),
            input_document_row,
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing.source_artifact_id != input.source_artifact_id
            || existing.document_role != input.document_role
            || existing.source_format != input.source_format
            || existing.import_index != input.import_index
            || existing.page_count != input.page_count
            || existing.created_by != input.created_by.trim()
        {
            return Err(CoreError::Invalid("资料幂等键已属于不同输入".into()));
        }
        return Ok(existing);
    }

    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO exam_fixed_input_documents_v2
         (public_id,ingest_batch_id,source_artifact_id,document_role,source_format,
          import_index,page_count,idempotency_key,state,created_by,created_at,updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'registered',?9,?10,?10)",
        (
            &public_id,
            input.ingest_batch_id,
            input.source_artifact_id,
            input.document_role,
            input.source_format,
            input.import_index,
            input.page_count,
            input.idempotency_key.trim(),
            input.created_by.trim(),
            &now,
        ),
    )?;
    let id = tx.last_insert_rowid();
    append_audit(
        &tx,
        &AuditInput {
            idempotency_key: &format!("exam:fixed-input:{public_id}:registered"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.created_by.trim()),
            action: "exam.fixed_input.registered",
            object_type: "exam_fixed_input_document",
            object_id: &public_id,
            revision: None,
            now: &now,
        },
    )?;
    tx.commit()?;
    get_input_document(conn, id)?.ok_or_else(|| CoreError::NotFound("刚登记的固定资料".into()))
}

fn source_priority(source_kind: &str) -> CoreResult<i64> {
    match source_kind {
        "selected_k1" => Ok(2),
        "uploaded_official" => Ok(3),
        "manual_confirmed" => Ok(4),
        "ai_draft" => Ok(5),
        _ => Err(CoreError::Invalid("答案权威来源非法".into())),
    }
}

pub fn record_answer_authority_candidate(
    conn: &Connection,
    input: &NewAnswerAuthorityCandidate<'_>,
) -> CoreResult<AnswerAuthorityCandidate> {
    required(input.idempotency_key, "答案候选幂等键")?;
    let actor_type = audit_actor(input.created_by_type, input.created_by)?;
    let (_, candidate_json) = canonical_schema_object(input.candidate_answer_json, "候选答案")?;
    let (_, source_anchor_json) =
        canonical_schema_object(input.source_anchor_json, "答案来源锚点")?;
    let priority = source_priority(input.source_kind)?;
    if (input.source_kind == "ai_draft" && input.created_by_type != "system")
        || (input.source_kind != "ai_draft" && input.created_by_type != "teacher")
    {
        return Err(CoreError::Invalid(
            "AI 草稿必须由系统记录，其他答案候选必须由老师记录".into(),
        ));
    }
    let assessment_version_id = batch_assessment_version(conn, input.ingest_batch_id)?;
    let question_version_id: i64 = conn
        .query_row(
            "SELECT question_version_id FROM exam_assessment_items_v2
             WHERE id=?1 AND assessment_version_id=?2 AND state='active'",
            (input.assessment_item_id, assessment_version_id),
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("答案候选题目不属于该固定试卷".into()))?;

    match input.source_kind {
        "selected_k1" | "manual_confirmed"
            if !input.teacher_confirmed || input.answer_key_version_id.is_none() =>
        {
            return Err(CoreError::Invalid(
                "已选/人工答案必须经老师确认并引用 K1 版本".into(),
            ))
        }
        "uploaded_official" if input.source_artifact_id.is_none() => {
            return Err(CoreError::Invalid("上传答案必须引用已归档答案资料".into()))
        }
        "uploaded_official" if input.teacher_confirmed && input.answer_key_version_id.is_none() => {
            return Err(CoreError::Invalid(
                "确认后的上传答案必须引用 K1 答案版本".into(),
            ))
        }
        "ai_draft" if input.teacher_confirmed || input.answer_key_version_id.is_some() => {
            return Err(CoreError::Invalid("AI 草稿不能伪装成老师确认答案".into()))
        }
        _ => {}
    }
    if let Some(artifact_id) = input.source_artifact_id {
        let artifact = artifacts::get_by_id(conn, artifact_id)?
            .ok_or_else(|| CoreError::NotFound(format!("artifact#{artifact_id}")))?;
        if artifact.archive_status != ArchiveStatus::Ready
            || artifact.privacy_class != PrivacyClass::TeachingContent
        {
            return Err(CoreError::Invalid(
                "答案资料必须已归档且归类为 teaching_content".into(),
            ));
        }
        if input.source_kind == "uploaded_official" {
            let registered: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM exam_fixed_input_documents_v2
                 WHERE ingest_batch_id=?1 AND source_artifact_id=?2
                   AND document_role='answer_source' AND state<>'voided')",
                (input.ingest_batch_id, artifact_id),
                |row| row.get(0),
            )?;
            if !registered {
                return Err(CoreError::Invalid(
                    "上传答案 artifact 未登记到本批答案资料".into(),
                ));
            }
        }
    }
    if let Some(answer_key_version_id) = input.answer_key_version_id {
        let answer_scope: Option<(i64, String, String)> = conn
            .query_row(
                "SELECT question_version_id,answer_json,state
                 FROM k1_answer_key_versions WHERE id=?1",
                [answer_key_version_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some((answer_question_id, answer_json, answer_state)) = answer_scope else {
            return Err(CoreError::NotFound(format!(
                "answer_key_version#{answer_key_version_id}"
            )));
        };
        if answer_question_id != question_version_id || answer_state != "confirmed" {
            return Err(CoreError::Invalid(
                "正式答案候选必须引用同题且已确认的 K1 版本".into(),
            ));
        }
        let (_, answer_json) = canonical_schema_object(&answer_json, "K1 答案")?;
        if answer_json != candidate_json {
            return Err(CoreError::Invalid(
                "候选答案内容与引用的 K1 版本不一致".into(),
            ));
        }
    }
    let fingerprint = hashing::sha256_hex(candidate_json.as_bytes());

    let existing = conn
        .query_row(
            "SELECT id,public_id,ingest_batch_id,assessment_item_id,source_kind,
                    source_priority,answer_key_version_id,source_artifact_id,
                    candidate_answer_json,answer_fingerprint,source_anchor_json,
                    teacher_confirmed,idempotency_key,state,created_by_type,created_by
             FROM exam_answer_authority_candidates_v2
             WHERE ingest_batch_id=?1 AND assessment_item_id=?2 AND idempotency_key=?3",
            (
                input.ingest_batch_id,
                input.assessment_item_id,
                input.idempotency_key.trim(),
            ),
            candidate_row,
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing.source_kind != input.source_kind
            || existing.answer_key_version_id != input.answer_key_version_id
            || existing.source_artifact_id != input.source_artifact_id
            || existing.candidate_answer_json != candidate_json
            || existing.source_anchor_json != source_anchor_json
            || existing.teacher_confirmed != input.teacher_confirmed
            || existing.created_by_type != input.created_by_type
            || existing.created_by.as_deref() != input.created_by.map(str::trim)
        {
            return Err(CoreError::Invalid("答案候选幂等键已属于不同内容".into()));
        }
        return Ok(existing);
    }

    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO exam_answer_authority_candidates_v2
         (public_id,ingest_batch_id,assessment_item_id,source_kind,source_priority,
          answer_key_version_id,source_artifact_id,candidate_answer_json,
          answer_fingerprint,source_anchor_json,teacher_confirmed,idempotency_key,
          state,created_by_type,created_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'active',?13,?14,?15)",
        (
            &public_id,
            input.ingest_batch_id,
            input.assessment_item_id,
            input.source_kind,
            priority,
            input.answer_key_version_id,
            input.source_artifact_id,
            &candidate_json,
            &fingerprint,
            &source_anchor_json,
            i64::from(input.teacher_confirmed),
            input.idempotency_key.trim(),
            input.created_by_type,
            input.created_by.map(str::trim),
            &now,
        ),
    )?;
    let id = tx.last_insert_rowid();
    append_audit(
        &tx,
        &AuditInput {
            idempotency_key: &format!("exam:answer-authority:{public_id}:recorded"),
            actor_type,
            actor_id: input.created_by.map(str::trim),
            action: "exam.answer_authority.recorded",
            object_type: "exam_answer_authority_candidate",
            object_id: &public_id,
            revision: None,
            now: &now,
        },
    )?;
    tx.commit()?;
    get_candidate(conn, id)?.ok_or_else(|| CoreError::NotFound("刚创建的答案候选".into()))
}

#[derive(Debug)]
struct PageEvidence {
    import_index: i64,
    expected_page_no: Option<i64>,
    quality_result: Option<String>,
    match_decision: Option<String>,
    attempt_id: Option<i64>,
    page_no: Option<i64>,
    source_registered: bool,
}

#[derive(Debug, Clone, Serialize)]
struct GroupSummary {
    attempt_id: i64,
    start_import_index: i64,
    end_import_index: i64,
    page_count: i64,
    page_numbers: Vec<i64>,
    valid: bool,
}

#[derive(Debug, Clone)]
struct SelectedAuthority {
    source_kind: String,
    source_priority: i64,
    answer_key_version_id: i64,
    answer_fingerprint: String,
}

#[derive(Debug)]
struct ItemScope {
    item_id: i64,
    question_version_id: i64,
    bound_answer_key_version_id: i64,
    bound_answer_json: String,
    bound_answer_state: String,
}

fn collect_items(conn: &Connection, assessment_version_id: i64) -> CoreResult<Vec<ItemScope>> {
    let mut stmt = conn.prepare(
        "SELECT i.id,i.question_version_id,i.answer_key_version_id,a.answer_json,a.state
         FROM exam_assessment_items_v2 i
         JOIN k1_answer_key_versions a ON a.id=i.answer_key_version_id
         WHERE i.assessment_version_id=?1 AND i.state='active'
         ORDER BY i.order_index,i.id",
    )?;
    let rows = stmt.query_map([assessment_version_id], |row| {
        Ok(ItemScope {
            item_id: row.get(0)?,
            question_version_id: row.get(1)?,
            bound_answer_key_version_id: row.get(2)?,
            bound_answer_json: row.get(3)?,
            bound_answer_state: row.get(4)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

fn collect_pages(conn: &Connection, batch_id: i64) -> CoreResult<Vec<PageEvidence>> {
    let mut stmt = conn.prepare(
        "SELECT p.import_index,p.expected_page_no,q.result,m.decision,
                m.attempt_id,m.page_no,
                EXISTS(
                  SELECT 1
                  FROM exam_fixed_input_documents_v2 d
                  JOIN artifacts source ON source.id=p.source_artifact_id
                  WHERE d.ingest_batch_id=p.batch_id
                    AND d.document_role='student_work' AND d.state<>'voided'
                    AND (d.source_artifact_id=p.source_artifact_id
                         OR d.source_artifact_id=source.parent_artifact_id)
                )
         FROM exam_ingest_pages_v2 p
         LEFT JOIN exam_page_quality_revisions_v2 q
           ON q.page_id=p.id AND q.state='active'
         LEFT JOIN exam_page_match_revisions_v2 m
           ON m.page_id=p.id AND m.state='active'
         WHERE p.batch_id=?1 AND p.state<>'voided'
         ORDER BY p.import_index,p.id",
    )?;
    let rows = stmt.query_map([batch_id], |row| {
        Ok(PageEvidence {
            import_index: row.get(0)?,
            expected_page_no: row.get(1)?,
            quality_result: row.get(2)?,
            match_decision: row.get(3)?,
            attempt_id: row.get(4)?,
            page_no: row.get(5)?,
            source_registered: row.get::<_, i64>(6)? != 0,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

fn select_authorities(
    conn: &Connection,
    batch_id: i64,
    items: &[ItemScope],
    reasons: &mut BTreeSet<String>,
) -> CoreResult<(BTreeMap<i64, SelectedAuthority>, Value)> {
    let mut selected = BTreeMap::new();
    let mut summaries = Vec::new();
    for item in items {
        let (_, bound_json) = canonical_schema_object(&item.bound_answer_json, "作业绑定答案")?;
        let bound_fingerprint = hashing::sha256_hex(bound_json.as_bytes());
        let candidate_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM exam_answer_authority_candidates_v2
             WHERE ingest_batch_id=?1 AND assessment_item_id=?2 AND state='active'",
            (batch_id, item.item_id),
            |row| row.get(0),
        )?;
        if item.bound_answer_state == "confirmed" {
            let authority = SelectedAuthority {
                source_kind: "assessment_bound".into(),
                source_priority: 1,
                answer_key_version_id: item.bound_answer_key_version_id,
                answer_fingerprint: bound_fingerprint,
            };
            summaries.push(serde_json::json!({
                "assessment_item_id": item.item_id,
                "source_kind": authority.source_kind,
                "source_priority": authority.source_priority,
                "answer_key_version_id": authority.answer_key_version_id,
                "answer_fingerprint": authority.answer_fingerprint,
                "ignored_lower_candidate_count": candidate_count,
                "status": "selected"
            }));
            selected.insert(item.item_id, authority);
            continue;
        }

        let mut stmt = conn.prepare(
            "SELECT c.source_kind,c.source_priority,c.answer_key_version_id,
                    c.answer_fingerprint,c.candidate_answer_json,
                    a.answer_json,a.state,a.question_version_id
             FROM exam_answer_authority_candidates_v2 c
             LEFT JOIN k1_answer_key_versions a ON a.id=c.answer_key_version_id
             WHERE c.ingest_batch_id=?1 AND c.assessment_item_id=?2
               AND c.state='active' AND c.teacher_confirmed=1
             ORDER BY c.source_priority,c.id",
        )?;
        let rows = stmt.query_map((batch_id, item.item_id), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<i64>>(7)?,
            ))
        })?;
        let mut eligible = Vec::new();
        for row in rows {
            let (
                source_kind,
                priority,
                answer_key_version_id,
                stored_fingerprint,
                candidate_json,
                k1_json,
                k1_state,
                answer_question_id,
            ) = row?;
            let Some(answer_key_version_id) = answer_key_version_id else {
                reasons.insert("ANSWER_CANDIDATE_INVALID".into());
                continue;
            };
            let Some(k1_json) = k1_json else {
                reasons.insert("ANSWER_CANDIDATE_INVALID".into());
                continue;
            };
            let (_, candidate_json) = canonical_schema_object(&candidate_json, "候选答案")?;
            let (_, k1_json) = canonical_schema_object(&k1_json, "K1 候选答案")?;
            let actual_fingerprint = hashing::sha256_hex(candidate_json.as_bytes());
            if k1_state.as_deref() != Some("confirmed")
                || answer_question_id != Some(item.question_version_id)
                || candidate_json != k1_json
                || stored_fingerprint != actual_fingerprint
            {
                reasons.insert("ANSWER_CANDIDATE_DRIFT".into());
                continue;
            }
            eligible.push(SelectedAuthority {
                source_kind,
                source_priority: priority,
                answer_key_version_id,
                answer_fingerprint: actual_fingerprint,
            });
        }
        let Some(min_priority) = eligible.iter().map(|value| value.source_priority).min() else {
            reasons.insert("ANSWER_AUTHORITY_MISSING".into());
            summaries.push(serde_json::json!({
                "assessment_item_id": item.item_id,
                "status": "missing",
                "candidate_count": candidate_count
            }));
            continue;
        };
        let winners = eligible
            .into_iter()
            .filter(|value| value.source_priority == min_priority)
            .collect::<Vec<_>>();
        let fingerprints = winners
            .iter()
            .map(|value| value.answer_fingerprint.as_str())
            .collect::<BTreeSet<_>>();
        if fingerprints.len() != 1 {
            reasons.insert("ANSWER_SAME_LEVEL_CONFLICT".into());
            summaries.push(serde_json::json!({
                "assessment_item_id": item.item_id,
                "source_priority": min_priority,
                "status": "same_level_conflict",
                "fingerprint_count": fingerprints.len()
            }));
            continue;
        }
        let authority = winners.into_iter().next().expect("winner exists");
        summaries.push(serde_json::json!({
            "assessment_item_id": item.item_id,
            "source_kind": authority.source_kind,
            "source_priority": authority.source_priority,
            "answer_key_version_id": authority.answer_key_version_id,
            "answer_fingerprint": authority.answer_fingerprint,
            "status": "selected"
        }));
        selected.insert(item.item_id, authority);
    }
    Ok((
        selected,
        serde_json::json!({
            "schema_version": 1,
            "policy": "assessment_bound>selected_k1>uploaded_official>manual_confirmed>ai_draft",
            "items": summaries
        }),
    ))
}

fn build_groups(
    pages: &[PageEvidence],
    expected_pages: i64,
    reasons: &mut BTreeSet<String>,
) -> (Vec<GroupSummary>, BTreeSet<i64>, BTreeSet<i64>, bool) {
    let mut groups = Vec::<GroupSummary>::new();
    let mut attempts = BTreeSet::new();
    let mut blocked_attempts = BTreeSet::new();
    let mut seen_closed = BTreeSet::new();
    let mut global_blocked = false;
    let mut global_review = false;
    if pages.is_empty() {
        reasons.insert("BATCH_EMPTY".into());
        return (groups, attempts, blocked_attempts, true);
    }
    for (position, page) in pages.iter().enumerate() {
        if page.import_index != position as i64 {
            reasons.insert("IMPORT_SEQUENCE_GAP".into());
            global_blocked = true;
        }
        if !page.source_registered {
            reasons.insert("SOURCE_DOCUMENT_UNREGISTERED".into());
            global_blocked = true;
        }
        if page.quality_result.as_deref() != Some("pass") {
            reasons.insert("PAGE_QUALITY_REVIEW_REQUIRED".into());
            global_review = true;
            continue;
        }
        if page.match_decision.as_deref() != Some("teacher_confirmed")
            || page.attempt_id.is_none()
            || page.page_no.is_none()
        {
            reasons.insert("PAGE_IDENTITY_UNCONFIRMED".into());
            global_blocked = true;
            continue;
        }
        let attempt_id = page.attempt_id.expect("checked");
        let page_no = page.page_no.expect("checked");
        attempts.insert(attempt_id);
        if page.expected_page_no.is_some_and(|value| value != page_no) {
            reasons.insert("PAGE_NUMBER_CONFLICT".into());
            blocked_attempts.insert(attempt_id);
        }
        match groups.last_mut() {
            Some(group) if group.attempt_id == attempt_id => {
                let expected_next = group.page_numbers.last().copied().unwrap_or(0) + 1;
                if page_no != expected_next {
                    reasons.insert("PAGE_SEQUENCE_INVALID".into());
                    group.valid = false;
                    blocked_attempts.insert(attempt_id);
                }
                group.end_import_index = page.import_index;
                group.page_count += 1;
                group.page_numbers.push(page_no);
            }
            Some(previous) => {
                seen_closed.insert(previous.attempt_id);
                if seen_closed.contains(&attempt_id) {
                    reasons.insert("STUDENT_PAGES_NONCONTIGUOUS".into());
                    blocked_attempts.insert(attempt_id);
                    global_blocked = true;
                }
                groups.push(GroupSummary {
                    attempt_id,
                    start_import_index: page.import_index,
                    end_import_index: page.import_index,
                    page_count: 1,
                    page_numbers: vec![page_no],
                    valid: page_no == 1,
                });
                if page_no != 1 {
                    reasons.insert("GROUP_FIRST_PAGE_INVALID".into());
                    blocked_attempts.insert(attempt_id);
                }
            }
            None => {
                groups.push(GroupSummary {
                    attempt_id,
                    start_import_index: page.import_index,
                    end_import_index: page.import_index,
                    page_count: 1,
                    page_numbers: vec![page_no],
                    valid: page_no == 1,
                });
                if page_no != 1 {
                    reasons.insert("GROUP_FIRST_PAGE_INVALID".into());
                    blocked_attempts.insert(attempt_id);
                }
            }
        }
    }
    for group in &mut groups {
        if group.page_count != expected_pages
            || group.page_numbers != (1..=expected_pages).collect::<Vec<_>>()
        {
            group.valid = false;
            blocked_attempts.insert(group.attempt_id);
            reasons.insert("GROUP_PAGE_COUNT_MISMATCH".into());
        }
    }
    if global_review {
        reasons.insert("REVIEW_INPUT_PRESENT".into());
    }
    (groups, attempts, blocked_attempts, global_blocked)
}

fn current_region_count(
    conn: &Connection,
    batch_id: i64,
    attempt_id: i64,
    item_id: i64,
) -> CoreResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*)
         FROM exam_answer_region_revisions_v2 r
         JOIN exam_ingest_pages_v2 p ON p.id=r.page_id AND p.batch_id=?1
         JOIN exam_page_match_revisions_v2 m
           ON m.page_id=p.id AND m.state='active'
          AND m.decision='teacher_confirmed' AND m.attempt_id=?2
         JOIN exam_page_alignment_revisions_v2 a
           ON a.id=r.alignment_revision_id AND a.state='active'
          AND a.decision='teacher_confirmed'
         WHERE r.assessment_item_id=?3 AND r.state='active'
           AND r.decision='teacher_confirmed'",
        (batch_id, attempt_id, item_id),
        |row| row.get(0),
    )?)
}

fn preflight_fixed_paper_batch_impl(
    conn: &Connection,
    input: &FixedPaperPreflightInput<'_>,
    fail_after_supersede: bool,
) -> CoreResult<FixedPaperPreflightRevision> {
    if input.expected_pages_per_attempt < 1 {
        return Err(CoreError::Invalid("固定试卷每份页数必须大于 0".into()));
    }
    let actor_type = audit_actor(input.created_by_type, input.created_by)?;
    let assessment_version_id = batch_assessment_version(conn, input.ingest_batch_id)?;
    let items = collect_items(conn, assessment_version_id)?;
    if items.is_empty() {
        return Err(CoreError::Invalid(
            "固定试卷必须至少包含一道有效题目".into(),
        ));
    }
    let pages = collect_pages(conn, input.ingest_batch_id)?;
    let mut reasons = BTreeSet::new();
    let (groups, attempts, blocked_attempts, global_group_blocked) =
        build_groups(&pages, input.expected_pages_per_attempt, &mut reasons);
    let (authorities, authority_summary) =
        select_authorities(conn, input.ingest_batch_id, &items, &mut reasons)?;
    let (answer_source_gate, answer_source_reason) =
        answer_source::latest_preflight_gate(conn, input.ingest_batch_id)?;
    if let Some(reason) = answer_source_reason {
        reasons.insert(reason.into());
    }
    let missing_authority = authorities.len() != items.len();
    let global_review = reasons.contains("PAGE_QUALITY_REVIEW_REQUIRED");

    let target_count = attempts.len() as i64 * items.len() as i64;
    let mut ready_count = 0_i64;
    let mut review_count = 0_i64;
    let mut blocked_count = 0_i64;
    let mut completed_count = 0_i64;
    for attempt_id in &attempts {
        for item in &items {
            if answer_source_gate == AnswerSourcePreflightGate::Blocked
                || global_group_blocked
                || blocked_attempts.contains(attempt_id)
                || !authorities.contains_key(&item.item_id)
            {
                blocked_count += 1;
                continue;
            }
            if answer_source_gate == AnswerSourcePreflightGate::ReviewRequired {
                review_count += 1;
                continue;
            }
            let region_count =
                current_region_count(conn, input.ingest_batch_id, *attempt_id, item.item_id)?;
            if region_count != 1 {
                reasons.insert(if region_count == 0 {
                    "ANSWER_REGION_MISSING".into()
                } else {
                    "ANSWER_REGION_AMBIGUOUS".into()
                });
                blocked_count += 1;
                continue;
            }
            let already_confirmed: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM exam_grade_decisions_v2
                 WHERE attempt_id=?1 AND assessment_item_id=?2 AND state='active')",
                (*attempt_id, item.item_id),
                |row| row.get(0),
            )?;
            if already_confirmed {
                completed_count += 1;
                continue;
            }
            let suggestion: Option<(bool, i64, String)> = conn
                .query_row(
                    "SELECT s.batch_eligible,s.answer_key_version_id,o.result_state
                     FROM exam_objective_grade_suggestions_v2 s
                     JOIN exam_objective_observation_revisions_v2 o
                       ON o.id=s.observation_revision_id AND o.state='active'
                     WHERE s.attempt_id=?1 AND s.assessment_item_id=?2 AND s.state='active'",
                    (*attempt_id, item.item_id),
                    |row| Ok((row.get::<_, i64>(0)? != 0, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((batch_eligible, suggestion_answer_id, observation_state)) = suggestion else {
                reasons.insert("OBJECTIVE_RESULT_REVIEW_REQUIRED".into());
                review_count += 1;
                continue;
            };
            let authority = authorities.get(&item.item_id).expect("checked");
            if suggestion_answer_id != authority.answer_key_version_id {
                reasons.insert("ANSWER_VERSION_DRIFT".into());
                blocked_count += 1;
            } else if batch_eligible && observation_state == "recognized" {
                ready_count += 1;
            } else {
                reasons.insert("OBJECTIVE_RESULT_REVIEW_REQUIRED".into());
                review_count += 1;
            }
        }
    }
    debug_assert_eq!(
        target_count,
        ready_count + review_count + blocked_count + completed_count
    );
    let route = if answer_source_gate == AnswerSourcePreflightGate::Blocked
        || global_group_blocked
        || missing_authority
        || blocked_count > 0
    {
        "blocked"
    } else if answer_source_gate == AnswerSourcePreflightGate::ReviewRequired
        || global_review
        || review_count > 0
    {
        "review_required"
    } else {
        "ready_for_batch_confirm"
    };
    let reason_codes_json = serde_json::json!({
        "schema_version": 1,
        "codes": reasons.iter().collect::<Vec<_>>()
    });
    let grouping_json = serde_json::json!({
        "schema_version": 1,
        "policy": "continuous_group_first_page_identity_page_sequence_v1",
        "page_count": pages.len(),
        "group_count": groups.len(),
        "groups": groups
    });
    let snapshot = serde_json::json!({
        "schema_version": 1,
        "ingest_batch_id": input.ingest_batch_id,
        "assessment_version_id": assessment_version_id,
        "expected_pages_per_attempt": input.expected_pages_per_attempt,
        "route": route,
        "target_count": target_count,
        "ready_count": ready_count,
        "review_count": review_count,
        "blocked_count": blocked_count,
        "completed_count": completed_count,
        "reason_codes": reason_codes_json,
        "answer_authority": authority_summary,
        "grouping": grouping_json
    });
    let snapshot_bytes = serde_json::to_vec(&snapshot)
        .map_err(|error| CoreError::Parse(format!("固定试卷预检快照失败：{error}")))?;
    let snapshot_hash = hashing::sha256_hex(&snapshot_bytes);
    let existing = conn
        .query_row(
            "SELECT id,public_id,ingest_batch_id,revision,snapshot_hash,
                    expected_pages_per_attempt,route,target_count,ready_count,
                    review_count,blocked_count,completed_count,reason_codes_json,
                    answer_authority_json,grouping_json,state,created_by_type,created_by
             FROM exam_fixed_preflight_revisions_v2
             WHERE ingest_batch_id=?1 AND snapshot_hash=?2 AND state='active'",
            (input.ingest_batch_id, &snapshot_hash),
            preflight_row,
        )
        .optional()?;
    if let Some(existing) = existing {
        return Ok(existing);
    }

    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let reason_codes_json = reason_codes_json.to_string();
    let authority_summary = authority_summary.to_string();
    let grouping_json = grouping_json.to_string();
    let tx = conn.unchecked_transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_fixed_preflight_revisions_v2
         WHERE ingest_batch_id=?1",
        [input.ingest_batch_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE exam_fixed_preflight_revisions_v2 SET state='superseded'
         WHERE ingest_batch_id=?1 AND state='active'",
        [input.ingest_batch_id],
    )?;
    if fail_after_supersede {
        return Err(CoreError::Db(
            "injected fixed preflight transaction failure".into(),
        ));
    }
    tx.execute(
        "INSERT INTO exam_fixed_preflight_revisions_v2
         (public_id,ingest_batch_id,revision,snapshot_hash,expected_pages_per_attempt,
          route,target_count,ready_count,review_count,blocked_count,completed_count,
          reason_codes_json,answer_authority_json,grouping_json,state,
          created_by_type,created_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,
                 'active',?15,?16,?17)",
        params![
            &public_id,
            input.ingest_batch_id,
            revision,
            &snapshot_hash,
            input.expected_pages_per_attempt,
            route,
            target_count,
            ready_count,
            review_count,
            blocked_count,
            completed_count,
            &reason_codes_json,
            &authority_summary,
            &grouping_json,
            input.created_by_type,
            input.created_by.map(str::trim),
            &now,
        ],
    )?;
    let id = tx.last_insert_rowid();
    append_audit(
        &tx,
        &AuditInput {
            idempotency_key: &format!("exam:fixed-preflight:{public_id}:active"),
            actor_type,
            actor_id: input.created_by.map(str::trim),
            action: "exam.fixed_preflight.activated",
            object_type: "exam_fixed_preflight",
            object_id: &public_id,
            revision: Some(revision),
            now: &now,
        },
    )?;
    tx.commit()?;
    get_preflight(conn, id)?.ok_or_else(|| CoreError::NotFound("刚创建的固定试卷预检".into()))
}

pub fn preflight_fixed_paper_batch(
    conn: &Connection,
    input: &FixedPaperPreflightInput<'_>,
) -> CoreResult<FixedPaperPreflightRevision> {
    preflight_fixed_paper_batch_impl(conn, input, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::objective::{record_objective_observation, NewObjectiveObservation};
    use crate::service::papers::{
        create_or_get_ingest_batch, decide_page_match, record_answer_region, record_page_alignment,
        record_page_quality, register_ingest_page, NewAnswerRegionRevision, NewIngestBatch,
        NewIngestPage, NewPageAlignmentRevision, NewPageMatchRevision, NewPageQualityRevision,
    };
    use suite_core::db::repo::artifacts::{create_or_get, NewArtifact};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    struct Fixture {
        conn: Connection,
        batch_id: i64,
        item_id: i64,
        bound_answer_id: i64,
        attempt_one: i64,
        attempt_two: i64,
        first_document_id: i64,
    }

    #[allow(clippy::too_many_arguments)]
    fn create_artifact(
        conn: &Connection,
        kind: ArtifactKind,
        hash_char: char,
        mime_type: &str,
        parent_artifact_id: Option<i64>,
        derivative_type: Option<&str>,
        privacy_class: PrivacyClass,
        name: &str,
    ) -> i64 {
        create_or_get(
            conn,
            &NewArtifact {
                kind,
                sha256: &hash_char.to_string().repeat(64),
                mime_type,
                byte_size: 128,
                original_name: Some(name),
                original_path: Some(&format!("/fixture/{name}")),
                archived_path: &format!("/archive/{name}"),
                parent_artifact_id,
                derivative_type,
                processing_version: "fixed-paper-fixture-v1",
                privacy_class,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap()
        .id
    }

    fn seed_base() -> Fixture {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        let hash = "a".repeat(64);
        conn.execute_batch(&format!(
            r#"INSERT INTO subjects(name) VALUES ('历史');
               INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
               INSERT INTO students(student_no,name,class_id) VALUES ('S001','学生一',1);
               INSERT INTO students(student_no,name,class_id) VALUES ('S002','学生二',1);
               INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('fixed-edition',1,'PEP','2024','中国历史八上','8','upper','active',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('fixed-map',1,1,'confirmed','2026-07-14T08:00:00.000Z',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('fixed-question','personal','teacher','unknown',0,
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES ('fixed-question-v1',1,1,'true_false','鸦片战争爆发于1840年。',1,
                         '{hash}','L3','published','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('fixed-answer-v1',1,1,'{{"schema_version":1,"correct":true}}',
                         'confirmed','2026-07-14T08:00:00.000Z','teacher',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('fixed-rubric-v1',1,1,1,'confirmed','2026-07-14T08:00:00.000Z',
                         'teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('fixed-link-v1',1,1,1,'confirmed','2026-07-14T08:00:00.000Z',
                         'teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,
                  created_by,created_at,updated_at)
                 VALUES ('fixed-assessment','固定试卷',1,'quiz','include','active','teacher',
                         '2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,
                  created_at,confirmed_by,confirmed_at)
                 VALUES ('fixed-assessment-v1',1,1,'{hash}','fixed-template-v1','confirmed',
                         '2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                  state,created_at)
                 VALUES ('fixed-item',1,1,1,1,1,0,1,
                         '{{"schema_version":1,"question_no":"1"}}','active',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES ('fixed-attempt-1',1,1,1,'image','first','ingesting',
                         '2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES ('fixed-attempt-2',1,2,1,'image','first','ingesting',
                         '2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');"#
        ))
        .unwrap();
        let batch = create_or_get_ingest_batch(
            &conn,
            &NewIngestBatch {
                assessment_version_id: 1,
                source_kind: "image_folder",
                idempotency_key: "fixed-real-batch",
                created_by: "teacher",
            },
        )
        .unwrap();
        let first_document_id = add_ready_page(&conn, batch.id, 0, 1, '1', '3', '5', true);
        add_ready_page(&conn, batch.id, 1, 2, '2', '4', '6', true);
        Fixture {
            conn,
            batch_id: batch.id,
            item_id: 1,
            bound_answer_id: 1,
            attempt_one: 1,
            attempt_two: 2,
            first_document_id,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_ready_page(
        conn: &Connection,
        batch_id: i64,
        import_index: i64,
        attempt_id: i64,
        source_hash: char,
        aligned_hash: char,
        crop_hash: char,
        record_observation: bool,
    ) -> i64 {
        let source_name = format!("student-{attempt_id}-page-{import_index}.jpg");
        let source_id = create_artifact(
            conn,
            ArtifactKind::Image,
            source_hash,
            "image/jpeg",
            None,
            None,
            PrivacyClass::StudentSensitive,
            &source_name,
        );
        let document = register_fixed_input_document(
            conn,
            &NewFixedInputDocument {
                ingest_batch_id: batch_id,
                source_artifact_id: source_id,
                document_role: "student_work",
                source_format: "jpeg",
                import_index,
                page_count: 1,
                idempotency_key: &format!("input-{import_index}"),
                created_by: "teacher",
            },
        )
        .unwrap();
        let page = register_ingest_page(
            conn,
            &NewIngestPage {
                batch_id,
                source_artifact_id: source_id,
                import_index,
                expected_page_no: Some(1),
            },
        )
        .unwrap();
        record_page_quality(
            conn,
            &NewPageQualityRevision {
                page_id: page.id,
                blur_score: 0.01,
                glare_score: 0.01,
                brightness_score: 0.8,
                perspective_score: 0.99,
                rotation_degrees: 0.0,
                crop_complete: true,
                result: "pass",
                issue_codes_json: r#"{"schema_version":1,"codes":[]}"#,
                checked_by_type: "rule",
                checked_by: None,
            },
        )
        .unwrap();
        decide_page_match(
            conn,
            &NewPageMatchRevision {
                page_id: page.id,
                attempt_id: Some(attempt_id),
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
        let aligned_id = create_artifact(
            conn,
            ArtifactKind::Page,
            aligned_hash,
            "image/jpeg",
            Some(source_id),
            Some("aligned_page"),
            PrivacyClass::StudentSensitive,
            &format!("aligned-{attempt_id}.jpg"),
        );
        record_page_alignment(
            conn,
            &NewPageAlignmentRevision {
                page_id: page.id,
                template_version: "fixed-template-v1",
                transform_json: r#"{"schema_version":1,"matrix":[1,0,0,0,1,0,0,0,1]}"#,
                confidence: 0.99,
                aligned_artifact_id: Some(aligned_id),
                decision: "teacher_confirmed",
                reason_code: None,
                confirmed_by: Some("teacher"),
            },
        )
        .unwrap();
        let crop_id = create_artifact(
            conn,
            ArtifactKind::Crop,
            crop_hash,
            "image/jpeg",
            Some(aligned_id),
            Some("answer_region"),
            PrivacyClass::StudentSensitive,
            &format!("crop-{attempt_id}.jpg"),
        );
        let region = record_answer_region(
            conn,
            &NewAnswerRegionRevision {
                page_id: page.id,
                assessment_item_id: 1,
                region_index: 0,
                bbox_json: r#"{"schema_version":1,"x":0.1,"y":0.1,"width":0.3,"height":0.2}"#,
                crop_artifact_id: Some(crop_id),
                mapping_confidence: Some(0.99),
                decision: "teacher_confirmed",
                reason_code: None,
                confirmed_by: Some("teacher"),
            },
        )
        .unwrap();
        if record_observation {
            record_objective_observation(
                conn,
                &NewObjectiveObservation {
                    answer_region_revision_id: region.id,
                    source_kind: "fixed_fixture",
                    result_state: "recognized",
                    observed_answer_json: Some(r#"{"schema_version":1,"selected":true}"#),
                    confidence: Some(0.99),
                    ai_run_id: None,
                    failure_meta_json: None,
                    idempotency_key: &format!("observation-{attempt_id}-{import_index}"),
                },
            )
            .unwrap();
        }
        document.id
    }

    fn preflight(fixture: &Fixture) -> FixedPaperPreflightRevision {
        preflight_fixed_paper_batch(
            &fixture.conn,
            &FixedPaperPreflightInput {
                ingest_batch_id: fixture.batch_id,
                expected_pages_per_attempt: 1,
                created_by_type: "system",
                created_by: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn jpeg_and_pdf_input_registration_is_idempotent_and_role_checked() {
        let fixture = seed_base();
        let existing = get_input_document(&fixture.conn, fixture.first_document_id)
            .unwrap()
            .unwrap();
        let repeated = register_fixed_input_document(
            &fixture.conn,
            &NewFixedInputDocument {
                ingest_batch_id: fixture.batch_id,
                source_artifact_id: existing.source_artifact_id,
                document_role: "student_work",
                source_format: "jpeg",
                import_index: 0,
                page_count: 1,
                idempotency_key: "input-0",
                created_by: "teacher",
            },
        )
        .unwrap();
        assert_eq!(existing.id, repeated.id);

        let pdf_id = create_artifact(
            &fixture.conn,
            ArtifactKind::Document,
            '7',
            "application/pdf",
            None,
            None,
            PrivacyClass::StudentSensitive,
            "student-batch.pdf",
        );
        let pdf = register_fixed_input_document(
            &fixture.conn,
            &NewFixedInputDocument {
                ingest_batch_id: fixture.batch_id,
                source_artifact_id: pdf_id,
                document_role: "student_work",
                source_format: "pdf",
                import_index: 2,
                page_count: 4,
                idempotency_key: "input-pdf",
                created_by: "teacher",
            },
        )
        .unwrap();
        assert_eq!(pdf.page_count, 4);
        assert!(register_fixed_input_document(
            &fixture.conn,
            &NewFixedInputDocument {
                ingest_batch_id: fixture.batch_id,
                source_artifact_id: pdf_id,
                document_role: "answer_source",
                source_format: "pdf",
                import_index: 0,
                page_count: 4,
                idempotency_key: "wrong-privacy-role",
                created_by: "teacher",
            }
        )
        .is_err());
    }

    #[test]
    fn office_documents_are_answer_sources_not_student_work() {
        let fixture = seed_base();
        for (index, (format, mime_type, hash_char)) in [
            (
                "docx",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                '8',
            ),
            (
                "xlsx",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                '9',
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let artifact_id = create_artifact(
                &fixture.conn,
                ArtifactKind::Document,
                hash_char,
                mime_type,
                None,
                None,
                PrivacyClass::TeachingContent,
                &format!("answer.{format}"),
            );
            let answer = register_fixed_input_document(
                &fixture.conn,
                &NewFixedInputDocument {
                    ingest_batch_id: fixture.batch_id,
                    source_artifact_id: artifact_id,
                    document_role: "answer_source",
                    source_format: "text",
                    import_index: index as i64,
                    page_count: 1,
                    idempotency_key: &format!("answer-{format}"),
                    created_by: "teacher",
                },
            )
            .unwrap();
            assert_eq!(answer.source_format, "text");
            assert!(register_fixed_input_document(
                &fixture.conn,
                &NewFixedInputDocument {
                    ingest_batch_id: fixture.batch_id,
                    source_artifact_id: artifact_id,
                    document_role: "student_work",
                    source_format: "text",
                    import_index: index as i64,
                    page_count: 1,
                    idempotency_key: &format!("student-{format}"),
                    created_by: "teacher",
                }
            )
            .is_err());
        }
    }

    #[test]
    fn confirmed_contiguous_groups_and_current_answers_route_to_batch_confirm() {
        let fixture = seed_base();
        let first = preflight(&fixture);
        let repeated = preflight(&fixture);
        assert_eq!(first.id, repeated.id);
        assert_eq!(first.route, "ready_for_batch_confirm");
        assert_eq!((first.target_count, first.ready_count), (2, 2));
        assert_eq!((first.review_count, first.blocked_count), (0, 0));
        let groups: Value = serde_json::from_str(&first.grouping_json).unwrap();
        assert_eq!(groups["group_count"], 2);
        let publications: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_grade_publications_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(publications, 0, "预检不得确认或发布成绩");
    }

    #[test]
    fn quality_uncertainty_routes_to_review_without_overriding_identity() {
        let fixture = seed_base();
        let second_page_id: i64 = fixture
            .conn
            .query_row(
                "SELECT id FROM exam_ingest_pages_v2 WHERE batch_id=?1 AND import_index=1",
                [fixture.batch_id],
                |row| row.get(0),
            )
            .unwrap();
        record_page_quality(
            &fixture.conn,
            &NewPageQualityRevision {
                page_id: second_page_id,
                blur_score: 0.8,
                glare_score: 0.1,
                brightness_score: 0.5,
                perspective_score: 0.8,
                rotation_degrees: 0.0,
                crop_complete: true,
                result: "needs_review",
                issue_codes_json: r#"{"schema_version":1,"codes":["BLUR"]}"#,
                checked_by_type: "rule",
                checked_by: None,
            },
        )
        .unwrap();
        let result = preflight(&fixture);
        assert_eq!(result.route, "review_required");
        assert!(result
            .reason_codes_json
            .contains("PAGE_QUALITY_REVIEW_REQUIRED"));
        assert_eq!(result.blocked_count, 0);
    }

    #[test]
    fn noncontiguous_student_pages_are_blocked() {
        let fixture = seed_base();
        add_ready_page(
            &fixture.conn,
            fixture.batch_id,
            2,
            fixture.attempt_one,
            '8',
            '9',
            'b',
            false,
        );
        let result = preflight_fixed_paper_batch(
            &fixture.conn,
            &FixedPaperPreflightInput {
                ingest_batch_id: fixture.batch_id,
                expected_pages_per_attempt: 2,
                created_by_type: "teacher",
                created_by: Some("teacher"),
            },
        )
        .unwrap();
        assert_eq!(result.route, "blocked");
        assert!(result
            .reason_codes_json
            .contains("STUDENT_PAGES_NONCONTIGUOUS"));
    }

    fn add_answer_version(
        conn: &Connection,
        revision: i64,
        selected: bool,
        public_id: &str,
    ) -> i64 {
        conn.execute(
            "INSERT INTO k1_answer_key_versions
             (public_id,question_version_id,revision,answer_json,state,created_at,
              confirmed_by,confirmed_at)
             VALUES (?1,1,?2,?3,'confirmed','2026-07-14T08:01:00.000Z',
                     'teacher','2026-07-14T08:01:00.000Z')",
            (
                public_id,
                revision,
                serde_json::json!({"schema_version":1,"correct":selected}).to_string(),
            ),
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn higher_answer_authority_wins_and_same_level_conflict_blocks_when_needed() {
        let fixture = seed_base();
        let false_answer = add_answer_version(&fixture.conn, 2, false, "fixed-answer-v2");
        let true_answer = add_answer_version(&fixture.conn, 3, true, "fixed-answer-v3");
        let answer_artifact_id = create_artifact(
            &fixture.conn,
            ArtifactKind::Image,
            'c',
            "image/jpeg",
            None,
            None,
            PrivacyClass::TeachingContent,
            "official-answer.jpg",
        );
        register_fixed_input_document(
            &fixture.conn,
            &NewFixedInputDocument {
                ingest_batch_id: fixture.batch_id,
                source_artifact_id: answer_artifact_id,
                document_role: "answer_source",
                source_format: "jpeg",
                import_index: 0,
                page_count: 1,
                idempotency_key: "answer-source",
                created_by: "teacher",
            },
        )
        .unwrap();
        for (key, answer_id, answer_json) in [
            (
                "official-false",
                false_answer,
                r#"{"schema_version":1,"correct":false}"#,
            ),
            (
                "official-true",
                true_answer,
                r#"{"schema_version":1,"correct":true}"#,
            ),
        ] {
            record_answer_authority_candidate(
                &fixture.conn,
                &NewAnswerAuthorityCandidate {
                    ingest_batch_id: fixture.batch_id,
                    assessment_item_id: fixture.item_id,
                    source_kind: "uploaded_official",
                    answer_key_version_id: Some(answer_id),
                    source_artifact_id: Some(answer_artifact_id),
                    candidate_answer_json: answer_json,
                    source_anchor_json: r#"{"schema_version":1,"page":1,"question_no":"1"}"#,
                    teacher_confirmed: true,
                    idempotency_key: key,
                    created_by_type: "teacher",
                    created_by: Some("teacher"),
                },
            )
            .unwrap();
        }
        let bound_wins = preflight(&fixture);
        assert_eq!(bound_wins.route, "ready_for_batch_confirm");
        assert!(bound_wins
            .answer_authority_json
            .contains("assessment_bound"));

        fixture
            .conn
            .execute(
                "UPDATE k1_answer_key_versions SET state='retired' WHERE id=?1",
                [fixture.bound_answer_id],
            )
            .unwrap();
        let conflict = preflight(&fixture);
        assert_eq!(conflict.route, "blocked");
        assert!(conflict
            .reason_codes_json
            .contains("ANSWER_SAME_LEVEL_CONFLICT"));
    }

    #[test]
    fn ai_draft_never_becomes_formal_answer_and_snapshot_update_is_atomic() {
        let fixture = seed_base();
        let initial = preflight(&fixture);
        assert_eq!(initial.revision, 1);
        record_answer_authority_candidate(
            &fixture.conn,
            &NewAnswerAuthorityCandidate {
                ingest_batch_id: fixture.batch_id,
                assessment_item_id: fixture.item_id,
                source_kind: "ai_draft",
                answer_key_version_id: None,
                source_artifact_id: None,
                candidate_answer_json: r#"{"schema_version":1,"selected":false}"#,
                source_anchor_json: r#"{"schema_version":1,"generator":"fixture"}"#,
                teacher_confirmed: false,
                idempotency_key: "ai-draft",
                created_by_type: "system",
                created_by: None,
            },
        )
        .unwrap();
        fixture
            .conn
            .execute(
                "UPDATE k1_answer_key_versions SET state='retired' WHERE id=?1",
                [fixture.bound_answer_id],
            )
            .unwrap();
        let failed = preflight_fixed_paper_batch_impl(
            &fixture.conn,
            &FixedPaperPreflightInput {
                ingest_batch_id: fixture.batch_id,
                expected_pages_per_attempt: 1,
                created_by_type: "system",
                created_by: None,
            },
            true,
        );
        assert!(failed.is_err());
        let active: (i64, String) = fixture
            .conn
            .query_row(
                "SELECT revision,route FROM exam_fixed_preflight_revisions_v2
                 WHERE ingest_batch_id=?1 AND state='active'",
                [fixture.batch_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(active, (1, "ready_for_batch_confirm".into()));

        let blocked = preflight(&fixture);
        assert_eq!(blocked.route, "blocked");
        assert!(blocked
            .reason_codes_json
            .contains("ANSWER_AUTHORITY_MISSING"));
    }

    #[test]
    fn preflight_never_touches_teacher_confirmation_or_publication() {
        let fixture = seed_base();
        let result = preflight(&fixture);
        assert_eq!(result.ready_count, 2);
        let counts: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_publications_v2),
                        (SELECT COUNT(*) FROM learning_evidence)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(counts, (0, 0, 0));
        assert_ne!(fixture.attempt_one, fixture.attempt_two);
    }
}
