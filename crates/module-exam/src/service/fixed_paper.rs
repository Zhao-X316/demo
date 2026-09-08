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

/// Current read-only preflight projection; no revision is created.
#[derive(Debug)]
pub struct FixedPaperInspection {
    pub route: String,
    pub target_count: i64,
    pub ready_count: i64,
    pub review_count: i64,
    pub blocked_count: i64,
    pub completed_count: i64,
    pub reason_codes_json: String,
    pub authority_summary: String,
    pub grouping_json: String,
    pub snapshot_hash: String,
}

pub fn inspect_fixed_paper_batch(
    conn: &Connection,
    input: &FixedPaperPreflightInput<'_>,
) -> CoreResult<FixedPaperInspection> {
    if input.expected_pages_per_attempt < 1 {
        return Err(CoreError::Invalid("固定试卷每份页数必须大于 0".into()));
    }
    audit_actor(input.created_by_type, input.created_by)?;
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
    Ok(FixedPaperInspection {
        route: route.to_string(),
        target_count,
        ready_count,
        review_count,
        blocked_count,
        completed_count,
        reason_codes_json: reason_codes_json.to_string(),
        authority_summary: authority_summary.to_string(),
        grouping_json: grouping_json.to_string(),
        snapshot_hash,
    })
}

fn preflight_fixed_paper_batch_impl(
    conn: &Connection,
    input: &FixedPaperPreflightInput<'_>,
    fail_after_supersede: bool,
) -> CoreResult<FixedPaperPreflightRevision> {
    let actor_type = audit_actor(input.created_by_type, input.created_by)?;
    let FixedPaperInspection {
        route,
        target_count,
        ready_count,
        review_count,
        blocked_count,
        completed_count,
        reason_codes_json,
        authority_summary,
        grouping_json,
        snapshot_hash,
    } = inspect_fixed_paper_batch(conn, input)?;
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
#[path = "fixed_paper_tests.rs"]
mod tests;
