//! K1 独立答案资料、逐题匹配草稿与老师确认服务。
//!
//! 这里不依赖 M2 作业身份。答案资料绑定一个 K1 题目来源文档，AI 输出只落不可变
//! 匹配草稿；每个未匹配题也落 `missing`。老师确认时才在同一事务创建答案/rubric、
//! 写质量事件并晋级 L1/L2，绝不创建 assessment、成绩或学习证据。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::ai_runs;
use suite_core::db::repo::artifacts;
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRunStatus, ArchiveStatus, AuditActorType, PrivacyClass};

use crate::db::content::{
    self, NewAnswerKeyVersion, NewAnswerSlot, NewRubricPoint, NewRubricVersion,
};

pub const ANSWER_REVIEW_SCHEMA_VERSION: i64 = 1;
pub const ANSWER_EXTRACTION_VERSION: &str = "k1-answer-source-extraction-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterAnswerSourceRequest {
    pub request_key: String,
    pub owner_id: String,
    pub question_source_document_public_id: String,
    pub source_artifact_public_id: String,
    pub source_format: String,
    pub source_hash: String,
    pub page_count: i64,
    pub extraction_version: String,
    pub created_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSourceDocument {
    #[serde(skip_serializing)]
    pub id: i64,
    pub public_id: String,
    pub owner_scope: String,
    pub owner_id: String,
    pub question_source_document_public_id: String,
    pub source_artifact_id: i64,
    pub source_artifact_public_id: String,
    pub source_format: String,
    pub source_hash: String,
    pub page_count: i64,
    pub extraction_version: String,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerTargetSet {
    pub question_source_document_public_id: String,
    pub source_type: String,
    pub source_format: String,
    pub created_at: String,
    pub target_count: i64,
    pub completed_count: i64,
    pub pending_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerTargetSpec {
    #[serde(skip_serializing)]
    pub question_version_id: i64,
    #[serde(skip_serializing)]
    pub source_question_draft_id: i64,
    pub question_version_public_id: String,
    pub order_index: i64,
    pub question_no: String,
    pub question_type: String,
    pub stem: String,
    pub max_score: f64,
    pub quality_level: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerMatchDraft {
    pub public_id: String,
    pub answer_source_document_public_id: String,
    pub extraction_run_public_id: String,
    pub question_version_public_id: String,
    pub order_index: i64,
    pub question_no: String,
    pub question_type: String,
    pub stem: String,
    pub max_score: f64,
    pub quality_level: String,
    pub candidate_state: String,
    pub answer_json: Option<Value>,
    pub source_anchor: Option<Value>,
    pub confidence: f64,
    pub content_hash: String,
    pub reviewed: bool,
    pub result_answer_key_version_public_id: Option<String>,
    pub result_rubric_version_public_id: Option<String>,
    pub result_quality: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerExtractionRecord {
    pub public_id: String,
    pub answer_source_document_public_id: String,
    pub ai_run_public_id: String,
    pub extraction_state: String,
    pub confidence: f64,
    pub issue_codes: Vec<String>,
    pub target_count: i64,
    pub matched_count: i64,
    pub drafts: Vec<AnswerMatchDraft>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSourceInboxItem {
    pub document: AnswerSourceDocument,
    pub latest_extraction: Option<AnswerExtractionRecord>,
    pub latest_ai_run_public_id: Option<String>,
    pub latest_ai_status: Option<String>,
    pub latest_ai_error_meta_json: Option<String>,
    pub target_count: i64,
    pub pending_matches: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmAnswerMatchRequest {
    pub request_key: String,
    pub match_draft_public_id: String,
    pub expected_content_hash: String,
    pub corrected_answer_json: Value,
    pub reviewed_by: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerMatchReview {
    pub public_id: String,
    pub match_draft_public_id: String,
    pub result_answer_key_version_public_id: String,
    pub result_rubric_version_public_id: Option<String>,
    pub result_quality: String,
    pub reviewed_by: String,
    pub note: Option<String>,
    pub reviewed_at: String,
}

#[derive(Serialize)]
struct RegisterHashInput<'a> {
    schema_version: i64,
    owner_id: &'a str,
    question_source_document_public_id: &'a str,
    source_artifact_public_id: &'a str,
    source_format: &'a str,
    source_hash: &'a str,
    page_count: i64,
    extraction_version: &'a str,
    created_by: &'a str,
}

#[derive(Serialize)]
struct ReviewHashInput<'a> {
    schema_version: i64,
    match_draft_public_id: &'a str,
    expected_content_hash: &'a str,
    corrected_answer_json: &'a Value,
    reviewed_by: &'a str,
    note: Option<&'a str>,
}

#[derive(Debug)]
struct MatchRow {
    id: i64,
    public_id: String,
    content_hash: String,
    extraction_run_id: i64,
    answer_source_document_id: i64,
    question_version_id: i64,
    question_version_public_id: String,
    question_type: String,
    max_score: f64,
    quality_level: String,
}

#[derive(Debug)]
struct OwnedAnswerSlot {
    order_index: i64,
    canonical_answers_json: String,
    normalization_rules_json: String,
    max_score: f64,
}

#[derive(Debug)]
struct OwnedRubricPoint {
    order_index: i64,
    canonical_text: String,
    allowed_paraphrases_json: Option<String>,
    required_concepts_json: Option<String>,
    max_score: f64,
}

#[derive(Debug)]
struct ConfirmedPayload {
    answer_json: String,
    answer_slots: Vec<OwnedAnswerSlot>,
    rubric_points: Vec<OwnedRubricPoint>,
    target_quality: &'static str,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn valid_hash(value: &str, label: &str) -> CoreResult<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(format!(
            "{label}必须是 64 位十六进制 hash"
        )));
    }
    Ok(normalized)
}

fn normalized_note(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|note| !note.is_empty())
}

fn answer_source_document_by_id(conn: &Connection, id: i64) -> CoreResult<AnswerSourceDocument> {
    conn.query_row(
        "SELECT d.id,d.public_id,d.owner_scope,d.owner_id,q.public_id,
                d.source_artifact_id,a.public_id,d.source_format,d.source_hash,d.page_count,
                d.extraction_version,d.created_by,d.created_at
         FROM k1_answer_source_documents d
         JOIN k1_source_documents q ON q.id=d.question_source_document_id
         JOIN artifacts a ON a.id=d.source_artifact_id
         WHERE d.id=?1",
        [id],
        |row| {
            Ok(AnswerSourceDocument {
                id: row.get(0)?,
                public_id: row.get(1)?,
                owner_scope: row.get(2)?,
                owner_id: row.get(3)?,
                question_source_document_public_id: row.get(4)?,
                source_artifact_id: row.get(5)?,
                source_artifact_public_id: row.get(6)?,
                source_format: row.get(7)?,
                source_hash: row.get(8)?,
                page_count: row.get(9)?,
                extraction_version: row.get(10)?,
                created_by: row.get(11)?,
                created_at: row.get(12)?,
            })
        },
    )
    .map_err(Into::into)
}

pub fn get_answer_source_document(
    conn: &Connection,
    owner_id: &str,
    public_id: &str,
) -> CoreResult<AnswerSourceDocument> {
    required(owner_id, "题库老师")?;
    required(public_id, "答案来源文档")?;
    let id = conn
        .query_row(
            "SELECT id FROM k1_answer_source_documents
             WHERE public_id=?1 AND owner_scope='personal' AND owner_id=?2",
            params![public_id.trim(), owner_id.trim()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("答案来源文档".into()))?;
    answer_source_document_by_id(conn, id)
}

pub fn list_answer_target_sets(
    conn: &Connection,
    owner_id: &str,
    limit: i64,
) -> CoreResult<Vec<AnswerTargetSet>> {
    required(owner_id, "题库老师")?;
    if !(1..=200).contains(&limit) {
        return Err(CoreError::Invalid(
            "答案目标批次数量必须在 1 到 200 之间".into(),
        ));
    }
    let mut statement = conn.prepare(
        "SELECT doc.public_id,doc.source_type,doc.source_format,doc.created_at,
                COUNT(DISTINCT v.id) AS target_count,
                COUNT(DISTINCT CASE WHEN v.quality_level IN ('L2','L3','L4') THEN v.id END)
                  AS completed_count,
                COUNT(DISTINCT CASE WHEN v.quality_level IN ('L0','L1') THEN v.id END)
                  AS pending_count
         FROM k1_source_documents doc
         JOIN k1_source_extraction_runs run ON run.source_document_id=doc.id
         JOIN k1_source_question_drafts draft ON draft.extraction_run_id=run.id
         JOIN k1_source_question_reviews review
           ON review.source_draft_id=draft.id AND review.action='accept'
         JOIN k1_question_versions v ON v.id=review.result_question_version_id
         JOIN k1_questions q ON q.id=v.question_id
         WHERE doc.owner_scope='personal' AND doc.owner_id=?1
           AND q.owner_scope='personal' AND q.owner_id=?1
           AND v.quality_level<>'C0' AND v.state NOT IN ('deprecated','archived')
         GROUP BY doc.id
         ORDER BY doc.created_at DESC,doc.id DESC
         LIMIT ?2",
    )?;
    let rows = statement.query_map(params![owner_id.trim(), limit], |row| {
        Ok(AnswerTargetSet {
            question_source_document_public_id: row.get(0)?,
            source_type: row.get(1)?,
            source_format: row.get(2)?,
            created_at: row.get(3)?,
            target_count: row.get(4)?,
            completed_count: row.get(5)?,
            pending_count: row.get(6)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn answer_target_specs(
    conn: &Connection,
    owner_id: &str,
    question_source_document_public_id: &str,
) -> CoreResult<Vec<AnswerTargetSpec>> {
    required(owner_id, "题库老师")?;
    required(question_source_document_public_id, "题目来源文档")?;
    let mut statement = conn.prepare(
        "SELECT v.id,MIN(draft.id),v.public_id,MIN(draft.order_index),
                COALESCE(MIN(NULLIF(TRIM(draft.question_no),'')),CAST(MIN(draft.order_index) AS TEXT)),
                v.question_type,v.stem,v.max_score,v.quality_level
         FROM k1_source_documents doc
         JOIN k1_source_extraction_runs run ON run.source_document_id=doc.id
         JOIN k1_source_question_drafts draft ON draft.extraction_run_id=run.id
         JOIN k1_source_question_reviews review
           ON review.source_draft_id=draft.id AND review.action='accept'
         JOIN k1_question_versions v ON v.id=review.result_question_version_id
         JOIN k1_questions q ON q.id=v.question_id
         WHERE doc.public_id=?1 AND doc.owner_scope='personal' AND doc.owner_id=?2
           AND q.owner_scope='personal' AND q.owner_id=?2
           AND v.quality_level IN ('L0','L1')
           AND v.state NOT IN ('deprecated','archived')
         GROUP BY v.id
         ORDER BY MIN(draft.order_index),v.id",
    )?;
    let rows = statement.query_map(
        params![question_source_document_public_id.trim(), owner_id.trim()],
        |row| {
            Ok(AnswerTargetSpec {
                question_version_id: row.get(0)?,
                source_question_draft_id: row.get(1)?,
                question_version_public_id: row.get(2)?,
                order_index: row.get(3)?,
                question_no: row.get(4)?,
                question_type: row.get(5)?,
                stem: row.get(6)?,
                max_score: row.get(7)?,
                quality_level: row.get(8)?,
            })
        },
    )?;
    let targets = rows.collect::<Result<Vec<_>, _>>()?;
    if targets.is_empty() {
        return Err(CoreError::Invalid(
            "该题目来源没有等待补答案或评分规则的 L0/L1 题目".into(),
        ));
    }
    Ok(targets)
}

pub fn register_answer_source_document(
    conn: &mut Connection,
    request: &RegisterAnswerSourceRequest,
) -> CoreResult<AnswerSourceDocument> {
    for (value, label) in [
        (&request.request_key, "请求键"),
        (&request.owner_id, "题库老师"),
        (&request.question_source_document_public_id, "题目来源文档"),
        (&request.source_artifact_public_id, "答案资料 artifact"),
        (&request.extraction_version, "答案提取版本"),
        (&request.created_by, "创建人"),
    ] {
        required(value, label)?;
    }
    if request.owner_id.trim() != request.created_by.trim() {
        return Err(CoreError::Invalid("只能为当前老师登记答案资料".into()));
    }
    if !matches!(
        request.source_format.as_str(),
        "jpeg" | "pdf" | "text" | "docx" | "xlsx"
    ) {
        return Err(CoreError::Invalid("答案资料格式非法".into()));
    }
    if !(1..=200).contains(&request.page_count) {
        return Err(CoreError::Invalid(
            "答案资料页数必须在 1 到 200 之间".into(),
        ));
    }
    let source_hash = valid_hash(&request.source_hash, "答案资料 hash")?;
    let request_hash = hashing::sha256_hex(
        &serde_json::to_vec(&RegisterHashInput {
            schema_version: ANSWER_REVIEW_SCHEMA_VERSION,
            owner_id: request.owner_id.trim(),
            question_source_document_public_id: request.question_source_document_public_id.trim(),
            source_artifact_public_id: request.source_artifact_public_id.trim(),
            source_format: &request.source_format,
            source_hash: &source_hash,
            page_count: request.page_count,
            extraction_version: request.extraction_version.trim(),
            created_by: request.created_by.trim(),
        })
        .map_err(|error| CoreError::Parse(format!("答案来源登记序列化失败：{error}")))?,
    );
    if let Some((id, existing_hash)) = conn
        .query_row(
            "SELECT id,request_hash FROM k1_answer_source_documents WHERE idempotency_key=?1",
            [request.request_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid("同一请求键对应不同答案来源".into()));
        }
        return answer_source_document_by_id(conn, id);
    }
    let question_source_document_id = conn
        .query_row(
            "SELECT id FROM k1_source_documents
             WHERE public_id=?1 AND owner_scope='personal' AND owner_id=?2",
            params![
                request.question_source_document_public_id.trim(),
                request.owner_id.trim()
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("题目来源文档".into()))?;
    let artifact = artifacts::get_by_public_id(conn, request.source_artifact_public_id.trim())?
        .ok_or_else(|| CoreError::NotFound("答案资料 artifact".into()))?;
    if artifact.archive_status != ArchiveStatus::Ready
        || artifact.privacy_class != PrivacyClass::TeachingContent
        || artifact.sha256 != source_hash
    {
        return Err(CoreError::Invalid(
            "答案资料必须以 teaching_content 完整归档且 hash 一致".into(),
        ));
    }
    if let Some(existing_id) = conn
        .query_row(
            "SELECT id FROM k1_answer_source_documents
             WHERE owner_scope='personal' AND owner_id=?1
               AND question_source_document_id=?2 AND source_artifact_id=?3
               AND extraction_version=?4",
            params![
                request.owner_id.trim(),
                question_source_document_id,
                artifact.id,
                request.extraction_version.trim()
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        return answer_source_document_by_id(conn, existing_id);
    }
    answer_target_specs(
        conn,
        request.owner_id.trim(),
        request.question_source_document_public_id.trim(),
    )?;
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO k1_answer_source_documents
         (public_id,idempotency_key,request_hash,owner_scope,owner_id,
          question_source_document_id,source_artifact_id,source_format,source_hash,page_count,
          extraction_version,created_by,created_at)
         VALUES (?1,?2,?3,'personal',?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            &public_id,
            request.request_key.trim(),
            &request_hash,
            request.owner_id.trim(),
            question_source_document_id,
            artifact.id,
            &request.source_format,
            &source_hash,
            request.page_count,
            request.extraction_version.trim(),
            request.created_by.trim(),
            &now,
        ],
    )?;
    let id = tx.last_insert_rowid();
    let event_payload = serde_json::json!({
        "schema_version": ANSWER_REVIEW_SCHEMA_VERSION,
        "answer_source_document_public_id": &public_id,
        "question_source_document_public_id": request.question_source_document_public_id.trim(),
        "source_artifact_public_id": &artifact.public_id,
        "creates_answer": false,
        "creates_assessment": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("{}:outbox", request.request_key.trim()),
            event_type: "k1.answer_source.registered",
            event_version: 1,
            aggregate_type: "k1_answer_source_document",
            aggregate_id: &public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &now,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("{}:audit", request.request_key.trim()),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(request.created_by.trim()),
            action: "k1.answer_source.registered",
            object_type: "k1_answer_source_document",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("仅登记老师答案资料；未创建标准答案或作业"),
            meta_json: Some(&event_payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    answer_source_document_by_id(conn, id)
}

fn parse_schema_object(raw: &str, label: &str) -> CoreResult<Value> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| CoreError::Parse(format!("{label}损坏：{error}")))?;
    if !value.is_object()
        || value
            .get("schema_version")
            .and_then(Value::as_i64)
            .is_none()
    {
        return Err(CoreError::Invalid(format!(
            "{label}必须是带整数 schema_version 的对象"
        )));
    }
    Ok(value)
}

fn extraction_by_id(conn: &Connection, id: i64) -> CoreResult<AnswerExtractionRecord> {
    let (
        public_id,
        answer_source_public_id,
        ai_run_public_id,
        extraction_state,
        confidence,
        issue_codes_json,
        target_count,
        matched_count,
        created_at,
    ) = conn.query_row(
        "SELECT run.public_id,doc.public_id,ai.public_id,run.extraction_state,
                run.confidence,run.issue_codes_json,run.target_count,run.matched_count,run.created_at
         FROM k1_answer_source_extraction_runs run
         JOIN k1_answer_source_documents doc ON doc.id=run.answer_source_document_id
         JOIN ai_runs ai ON ai.id=run.ai_run_id
         WHERE run.id=?1",
        [id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
            ))
        },
    )?;
    let issue_codes = serde_json::from_str(&issue_codes_json)
        .map_err(|error| CoreError::Parse(format!("答案问题码损坏：{error}")))?;
    let mut statement = conn.prepare(
        "SELECT draft.public_id,v.public_id,draft.order_index,
                COALESCE(draft.question_no,CAST(draft.order_index AS TEXT)),
                v.question_type,v.stem,v.max_score,v.quality_level,draft.candidate_state,
                draft.answer_json,draft.source_anchor_json,draft.confidence,draft.content_hash,
                review.id IS NOT NULL,answer.public_id,rubric.public_id,review.result_quality
         FROM k1_answer_match_drafts draft
         JOIN k1_question_versions v ON v.id=draft.question_version_id
         LEFT JOIN k1_answer_match_reviews review ON review.answer_match_draft_id=draft.id
         LEFT JOIN k1_answer_key_versions answer ON answer.id=review.result_answer_key_version_id
         LEFT JOIN k1_rubric_versions rubric ON rubric.id=review.result_rubric_version_id
         WHERE draft.extraction_run_id=?1
         ORDER BY draft.order_index,draft.id",
    )?;
    let rows = statement.query_map([id], |row| {
        let answer_json = row
            .get::<_, Option<String>>(9)?
            .map(|raw| parse_schema_object(&raw, "候选答案"))
            .transpose()
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
        let source_anchor = row
            .get::<_, Option<String>>(10)?
            .map(|raw| parse_schema_object(&raw, "答案来源锚点"))
            .transpose()
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    10,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
        Ok(AnswerMatchDraft {
            public_id: row.get(0)?,
            answer_source_document_public_id: answer_source_public_id.clone(),
            extraction_run_public_id: public_id.clone(),
            question_version_public_id: row.get(1)?,
            order_index: row.get(2)?,
            question_no: row.get(3)?,
            question_type: row.get(4)?,
            stem: row.get(5)?,
            max_score: row.get(6)?,
            quality_level: row.get(7)?,
            candidate_state: row.get(8)?,
            answer_json,
            source_anchor,
            confidence: row.get(11)?,
            content_hash: row.get(12)?,
            reviewed: row.get(13)?,
            result_answer_key_version_public_id: row.get(14)?,
            result_rubric_version_public_id: row.get(15)?,
            result_quality: row.get(16)?,
        })
    })?;
    let drafts = rows.collect::<Result<Vec<_>, _>>()?;
    Ok(AnswerExtractionRecord {
        public_id,
        answer_source_document_public_id: answer_source_public_id,
        ai_run_public_id,
        extraction_state,
        confidence,
        issue_codes,
        target_count,
        matched_count,
        drafts,
        created_at,
    })
}

fn output_state(value: &Value) -> Option<&str> {
    value
        .get("state")
        .and_then(Value::as_str)
        .and_then(|state| matches!(state, "ready" | "needs_review" | "blocked").then_some(state))
}

pub fn materialize_answer_extraction(
    conn: &mut Connection,
    owner_id: &str,
    answer_source_document_public_id: &str,
    ai_run_id: i64,
) -> CoreResult<AnswerExtractionRecord> {
    let document = get_answer_source_document(conn, owner_id, answer_source_document_public_id)?;
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM k1_answer_source_extraction_runs WHERE ai_run_id=?1",
            [ai_run_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        return extraction_by_id(conn, id);
    }
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound("答案来源 AI 运行".into()))?;
    if run.status != AiRunStatus::Succeeded
        || run.run_type != "answer_source_structure"
        || run.source_module != "knowledge"
        || run.business_ref_type != "k1_answer_source_document"
        || run.business_ref_id != document.public_id
        || run.input_artifact_id != Some(document.source_artifact_id)
    {
        return Err(CoreError::Invalid(
            "AI 运行不属于当前 K1 答案来源或尚未成功".into(),
        ));
    }
    let output_json = run
        .output_json
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("成功答案 AI 运行缺少输出".into()))?;
    let output_hash = hashing::sha256_hex(output_json.as_bytes());
    if run.output_hash.as_deref() != Some(output_hash.as_str()) {
        return Err(CoreError::Invalid("答案 AI 运行输出 hash 不一致".into()));
    }
    if let Some(existing_id) = conn
        .query_row(
            "SELECT id FROM k1_answer_source_extraction_runs
             WHERE answer_source_document_id=?1 AND output_hash=?2",
            params![document.id, &output_hash],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        return extraction_by_id(conn, existing_id);
    }
    let output: Value = serde_json::from_str(output_json)
        .map_err(|error| CoreError::Parse(format!("答案 AI 输出无效：{error}")))?;
    if output.get("schema_version").and_then(Value::as_i64) != Some(1)
        || output.get("ingest_batch_id").and_then(Value::as_i64) != Some(document.id)
        || output.get("source_artifact_id").and_then(Value::as_i64)
            != Some(document.source_artifact_id)
        || output.get("source_artifact_sha256").and_then(Value::as_str)
            != Some(document.source_hash.as_str())
    {
        return Err(CoreError::Invalid(
            "答案 AI 输出身份链不属于当前来源".into(),
        ));
    }
    let state =
        output_state(&output).ok_or_else(|| CoreError::Invalid("答案 AI 状态非法".into()))?;
    let confidence = output
        .get("confidence")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
        .ok_or_else(|| CoreError::Invalid("答案 AI 总置信度非法".into()))?;
    let issue_codes = output
        .get("issue_codes")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Invalid("答案 AI 问题码缺失".into()))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|code| !code.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| CoreError::Invalid("答案 AI 问题码非法".into()))
        })
        .collect::<CoreResult<Vec<_>>>()?;
    if state != "ready" && issue_codes.is_empty() {
        return Err(CoreError::Invalid("非 ready 答案结果必须说明问题".into()));
    }
    let targets =
        answer_target_specs(conn, owner_id, &document.question_source_document_public_id)?;
    let target_ids = targets
        .iter()
        .map(|target| target.question_version_id)
        .collect::<BTreeSet<_>>();
    let mut candidates = BTreeMap::<i64, (String, String, f64)>::new();
    for entry in output
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Invalid("答案 AI entries 缺失".into()))?
    {
        let target_id = entry
            .get("assessment_item_id")
            .and_then(Value::as_i64)
            .ok_or_else(|| CoreError::Invalid("答案 AI 题目身份缺失".into()))?;
        if !target_ids.contains(&target_id) || candidates.contains_key(&target_id) {
            return Err(CoreError::Invalid("答案 AI 包含越界或重复题目".into()));
        }
        let answer = entry
            .get("answer_json")
            .ok_or_else(|| CoreError::Invalid("答案 AI 候选缺失".into()))?;
        let anchor = entry
            .get("source_anchor")
            .ok_or_else(|| CoreError::Invalid("答案 AI 来源锚点缺失".into()))?;
        if answer
            .get("schema_version")
            .and_then(Value::as_i64)
            .is_none()
            || anchor
                .get("schema_version")
                .and_then(Value::as_i64)
                .is_none()
        {
            return Err(CoreError::Invalid(
                "答案候选与来源锚点必须带 schema_version".into(),
            ));
        }
        let entry_confidence = entry
            .get("confidence")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            .ok_or_else(|| CoreError::Invalid("答案 AI 逐题置信度非法".into()))?;
        candidates.insert(
            target_id,
            (
                serde_json::to_string(answer)
                    .map_err(|error| CoreError::Parse(format!("候选答案序列化失败：{error}")))?,
                serde_json::to_string(anchor)
                    .map_err(|error| CoreError::Parse(format!("答案锚点序列化失败：{error}")))?,
                entry_confidence,
            ),
        );
    }
    if state == "ready"
        && (candidates.len() != targets.len()
            || confidence < 0.95
            || candidates.values().any(|candidate| candidate.2 < 0.95))
    {
        return Err(CoreError::Invalid(
            "ready 答案结果必须完整覆盖且置信度不低于 0.95".into(),
        ));
    }
    let issue_codes_json = serde_json::to_string(&issue_codes)
        .map_err(|error| CoreError::Parse(format!("答案问题码序列化失败：{error}")))?;
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO k1_answer_source_extraction_runs
         (public_id,answer_source_document_id,ai_run_id,output_hash,extraction_state,
          confidence,issue_codes_json,target_count,matched_count,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            &public_id,
            document.id,
            ai_run_id,
            &output_hash,
            state,
            confidence,
            &issue_codes_json,
            targets.len() as i64,
            candidates.len() as i64,
            &now,
        ],
    )?;
    let extraction_id = tx.last_insert_rowid();
    for target in &targets {
        let candidate = candidates.get(&target.question_version_id);
        let candidate_state = match candidate {
            None => "missing",
            Some((_, _, entry_confidence)) if state == "ready" && *entry_confidence >= 0.95 => {
                "ready"
            }
            Some(_) => "needs_review",
        };
        let answer_json = candidate.map(|candidate| candidate.0.as_str());
        let source_anchor_json = candidate.map(|candidate| candidate.1.as_str());
        let entry_confidence = candidate.map_or(0.0, |candidate| candidate.2);
        let content_hash = hashing::sha256_hex(
            &serde_json::to_vec(&serde_json::json!({
                "schema_version": ANSWER_REVIEW_SCHEMA_VERSION,
                "question_version_public_id": &target.question_version_public_id,
                "candidate_state": candidate_state,
                "answer_json": answer_json,
                "source_anchor_json": source_anchor_json,
                "confidence": entry_confidence
            }))
            .map_err(|error| CoreError::Parse(format!("答案草稿 hash 序列化失败：{error}")))?,
        );
        tx.execute(
            "INSERT INTO k1_answer_match_drafts
             (public_id,extraction_run_id,source_question_draft_id,question_version_id,
              order_index,question_no,candidate_state,answer_json,source_anchor_json,
              confidence,content_hash,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                ids::new_public_id(),
                extraction_id,
                target.source_question_draft_id,
                target.question_version_id,
                target.order_index,
                &target.question_no,
                candidate_state,
                answer_json,
                source_anchor_json,
                entry_confidence,
                &content_hash,
                &now,
            ],
        )?;
    }
    let event_payload = serde_json::json!({
        "schema_version": ANSWER_REVIEW_SCHEMA_VERSION,
        "answer_source_document_public_id": &document.public_id,
        "extraction_run_public_id": &public_id,
        "state": state,
        "target_count": targets.len(),
        "matched_count": candidates.len(),
        "creates_answer": false,
        "creates_assessment": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("k1:outbox:answer-extraction:{public_id}"),
            event_type: "k1.answer_source.extracted",
            event_version: 1,
            aggregate_type: "k1_answer_source_extraction",
            aggregate_id: &public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &now,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("k1:audit:answer-extraction:{public_id}"),
            actor_type: AuditActorType::System,
            actor_id: None,
            action: "k1.answer_source.extracted",
            object_type: "k1_answer_source_extraction",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("机器答案只进入匹配草稿；缺题显式落 missing，未创建答案版本"),
            meta_json: Some(&event_payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    extraction_by_id(conn, extraction_id)
}

pub fn list_answer_source_inbox(
    conn: &Connection,
    owner_id: &str,
    limit: i64,
) -> CoreResult<Vec<AnswerSourceInboxItem>> {
    required(owner_id, "题库老师")?;
    if !(1..=200).contains(&limit) {
        return Err(CoreError::Invalid(
            "答案来源列表数量必须在 1 到 200 之间".into(),
        ));
    }
    let mut statement = conn.prepare(
        "SELECT doc.id,
                (SELECT run.id FROM k1_answer_source_extraction_runs run
                 WHERE run.answer_source_document_id=doc.id ORDER BY run.id DESC LIMIT 1),
                ai.public_id,ai.status,ai.error_meta_json
         FROM k1_answer_source_documents doc
         LEFT JOIN ai_runs ai ON ai.id=(
           SELECT candidate.id FROM ai_runs candidate
           WHERE candidate.source_module='knowledge'
             AND candidate.business_ref_type='k1_answer_source_document'
             AND candidate.business_ref_id=doc.public_id
           ORDER BY candidate.id DESC LIMIT 1
         )
         WHERE doc.owner_scope='personal' AND doc.owner_id=?1
         ORDER BY doc.created_at DESC,doc.id DESC
         LIMIT ?2",
    )?;
    let rows = statement.query_map(params![owner_id.trim(), limit], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (document_id, extraction_id, run_public_id, run_status, error_meta_json) = row?;
        let document = answer_source_document_by_id(conn, document_id)?;
        let extraction = extraction_id
            .map(|id| extraction_by_id(conn, id))
            .transpose()?;
        let target_count = extraction.as_ref().map_or(0, |record| record.target_count);
        let pending_matches = extraction.as_ref().map_or(0, |record| {
            record.drafts.iter().filter(|draft| !draft.reviewed).count() as i64
        });
        result.push(AnswerSourceInboxItem {
            document,
            latest_extraction: extraction,
            latest_ai_run_public_id: run_public_id,
            latest_ai_status: run_status,
            latest_ai_error_meta_json: error_meta_json,
            target_count,
            pending_matches,
        });
    }
    Ok(result)
}

fn load_match(conn: &Connection, owner_id: &str, public_id: &str) -> CoreResult<MatchRow> {
    conn.query_row(
        "SELECT draft.id,draft.public_id,draft.content_hash,draft.extraction_run_id,
                run.answer_source_document_id,v.id,v.public_id,
                v.question_type,v.max_score,v.quality_level
         FROM k1_answer_match_drafts draft
         JOIN k1_answer_source_extraction_runs run ON run.id=draft.extraction_run_id
         JOIN k1_answer_source_documents doc ON doc.id=run.answer_source_document_id
         JOIN k1_question_versions v ON v.id=draft.question_version_id
         JOIN k1_questions q ON q.id=v.question_id
         WHERE draft.public_id=?1
           AND doc.owner_scope='personal' AND doc.owner_id=?2
           AND q.owner_scope='personal' AND q.owner_id=?2",
        params![public_id.trim(), owner_id.trim()],
        |row| {
            Ok(MatchRow {
                id: row.get(0)?,
                public_id: row.get(1)?,
                content_hash: row.get(2)?,
                extraction_run_id: row.get(3)?,
                answer_source_document_id: row.get(4)?,
                question_version_id: row.get(5)?,
                question_version_public_id: row.get(6)?,
                question_type: row.get(7)?,
                max_score: row.get(8)?,
                quality_level: row.get(9)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound("答案匹配草稿".into()))
}

fn string_list(
    value: Option<&Value>,
    label: &str,
    required_nonempty: bool,
) -> CoreResult<Vec<String>> {
    let values = value
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Invalid(format!("{label}必须是字符串数组")))?;
    let values = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| CoreError::Invalid(format!("{label}含空值或非字符串")))
        })
        .collect::<CoreResult<Vec<_>>>()?;
    if required_nonempty && values.is_empty() {
        return Err(CoreError::Invalid(format!("{label}不能为空")));
    }
    Ok(values)
}

fn validate_object_schema(value: &Value) -> CoreResult<()> {
    if !value.is_object() || value.get("schema_version").and_then(Value::as_i64) != Some(1) {
        return Err(CoreError::Invalid(
            "确认答案必须是 schema_version=1 的对象".into(),
        ));
    }
    Ok(())
}

fn validate_confirmed_payload(
    conn: &Connection,
    row: &MatchRow,
    value: &Value,
) -> CoreResult<ConfirmedPayload> {
    validate_object_schema(value)?;
    let answer_json = serde_json::to_string(value)
        .map_err(|error| CoreError::Parse(format!("确认答案序列化失败：{error}")))?;
    match row.question_type.as_str() {
        "single" | "multiple" => {
            let labels = string_list(value.get("correct_labels"), "正确选项", true)?;
            let unique = labels.iter().collect::<BTreeSet<_>>();
            if unique.len() != labels.len() || (row.question_type == "single" && labels.len() != 1)
            {
                return Err(CoreError::Invalid(
                    "单选必须恰有一个正确项，多选项不得重复".into(),
                ));
            }
            let known = {
                let mut statement = conn.prepare(
                    "SELECT label FROM k1_question_options WHERE question_version_id=?1",
                )?;
                let labels = statement
                    .query_map([row.question_version_id], |option| {
                        option.get::<_, String>(0)
                    })?
                    .collect::<Result<BTreeSet<_>, _>>()?;
                labels
            };
            if labels.iter().any(|label| !known.contains(label)) {
                return Err(CoreError::Invalid("正确选项不在当前题目选项中".into()));
            }
            Ok(ConfirmedPayload {
                answer_json,
                answer_slots: Vec::new(),
                rubric_points: Vec::new(),
                target_quality: "L2",
            })
        }
        "true_false" => {
            if value.get("correct").and_then(Value::as_bool).is_none() {
                return Err(CoreError::Invalid("判断题必须明确正确或错误".into()));
            }
            Ok(ConfirmedPayload {
                answer_json,
                answer_slots: Vec::new(),
                rubric_points: Vec::new(),
                target_quality: "L2",
            })
        }
        "fill_blank" => {
            let raw_slots = value
                .get("slots")
                .and_then(Value::as_array)
                .filter(|slots| !slots.is_empty())
                .ok_or_else(|| CoreError::Invalid("填空题至少需要一个答案槽位".into()))?;
            let mut orders = BTreeSet::new();
            let mut total = 0.0;
            let mut slots = Vec::new();
            for raw in raw_slots {
                let order_index = raw
                    .get("order_index")
                    .and_then(Value::as_i64)
                    .filter(|order| *order >= 0 && orders.insert(*order))
                    .ok_or_else(|| CoreError::Invalid("填空槽位顺序必须是不重复非负整数".into()))?;
                let canonical = string_list(raw.get("canonical_answers"), "填空标准答案", true)?;
                let accepted = match raw.get("accepted_variants") {
                    Some(values) => string_list(Some(values), "填空可接受写法", false)?,
                    None => Vec::new(),
                };
                let max_score = raw
                    .get("max_score")
                    .and_then(Value::as_f64)
                    .filter(|score| score.is_finite() && *score > 0.0)
                    .ok_or_else(|| CoreError::Invalid("每个填空槽位都必须明确分值".into()))?;
                total += max_score;
                slots.push(OwnedAnswerSlot {
                    order_index,
                    canonical_answers_json: serde_json::json!({
                        "schema_version": 1,
                        "canonical_answers": canonical,
                        "accepted_variants": accepted
                    })
                    .to_string(),
                    normalization_rules_json: serde_json::json!({
                        "schema_version": 1,
                        "trim": true,
                        "normalize_whitespace": true
                    })
                    .to_string(),
                    max_score,
                });
            }
            if (total - row.max_score).abs() > 0.000_001 {
                return Err(CoreError::Invalid(
                    "填空槽位分值之和必须等于题目总分".into(),
                ));
            }
            Ok(ConfirmedPayload {
                answer_json,
                answer_slots: slots,
                rubric_points: Vec::new(),
                target_quality: "L2",
            })
        }
        "short_answer" => {
            let reference = value
                .get("reference_answer")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .ok_or_else(|| CoreError::Invalid("简答题必须有参考答案".into()))?;
            let _ = reference;
            let Some(raw_points) = value.get("rubric_points") else {
                return Ok(ConfirmedPayload {
                    answer_json,
                    answer_slots: Vec::new(),
                    rubric_points: Vec::new(),
                    target_quality: "L1",
                });
            };
            let raw_points = raw_points
                .as_array()
                .filter(|points| !points.is_empty())
                .ok_or_else(|| CoreError::Invalid("评分点必须是非空数组".into()))?;
            let mut orders = BTreeSet::new();
            let mut total = 0.0;
            let mut points = Vec::new();
            for raw in raw_points {
                let order_index = raw
                    .get("order_index")
                    .and_then(Value::as_i64)
                    .filter(|order| *order >= 0 && orders.insert(*order))
                    .ok_or_else(|| CoreError::Invalid("评分点顺序必须是不重复非负整数".into()))?;
                let canonical_text = raw
                    .get("canonical_text")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| CoreError::Invalid("评分点表述不能为空".into()))?;
                let max_score = raw
                    .get("max_score")
                    .and_then(Value::as_f64)
                    .filter(|score| score.is_finite() && *score > 0.0)
                    .ok_or_else(|| CoreError::Invalid("评分点必须明确正分值".into()))?;
                let allowed = match raw.get("allowed_paraphrases") {
                    Some(values) => Some(
                        serde_json::to_string(&string_list(Some(values), "允许改述", false)?)
                            .map_err(|error| {
                                CoreError::Parse(format!("允许改述序列化失败：{error}"))
                            })?,
                    ),
                    None => None,
                };
                let concepts = match raw.get("required_concepts") {
                    Some(values) => Some(
                        serde_json::to_string(&string_list(Some(values), "必需概念", false)?)
                            .map_err(|error| {
                                CoreError::Parse(format!("必需概念序列化失败：{error}"))
                            })?,
                    ),
                    None => None,
                };
                total += max_score;
                points.push(OwnedRubricPoint {
                    order_index,
                    canonical_text,
                    allowed_paraphrases_json: allowed,
                    required_concepts_json: concepts,
                    max_score,
                });
            }
            if (total - row.max_score).abs() > 0.000_001 {
                return Err(CoreError::Invalid(
                    "简答评分点分值之和必须等于题目总分".into(),
                ));
            }
            Ok(ConfirmedPayload {
                answer_json,
                answer_slots: Vec::new(),
                rubric_points: points,
                target_quality: "L2",
            })
        }
        _ => Err(CoreError::Invalid("当前题型不支持建立答案".into())),
    }
}

fn review_by_id(conn: &Connection, id: i64) -> CoreResult<AnswerMatchReview> {
    conn.query_row(
        "SELECT review.public_id,draft.public_id,answer.public_id,rubric.public_id,
                review.result_quality,review.reviewed_by,review.note,review.reviewed_at
         FROM k1_answer_match_reviews review
         JOIN k1_answer_match_drafts draft ON draft.id=review.answer_match_draft_id
         JOIN k1_answer_key_versions answer ON answer.id=review.result_answer_key_version_id
         LEFT JOIN k1_rubric_versions rubric ON rubric.id=review.result_rubric_version_id
         WHERE review.id=?1",
        [id],
        |row| {
            Ok(AnswerMatchReview {
                public_id: row.get(0)?,
                match_draft_public_id: row.get(1)?,
                result_answer_key_version_public_id: row.get(2)?,
                result_rubric_version_public_id: row.get(3)?,
                result_quality: row.get(4)?,
                reviewed_by: row.get(5)?,
                note: row.get(6)?,
                reviewed_at: row.get(7)?,
            })
        },
    )
    .map_err(Into::into)
}

pub fn confirm_answer_match(
    conn: &mut Connection,
    owner_id: &str,
    request: &ConfirmAnswerMatchRequest,
) -> CoreResult<AnswerMatchReview> {
    for (value, label) in [
        (&request.request_key, "请求键"),
        (&request.match_draft_public_id, "答案匹配草稿"),
        (&request.reviewed_by, "确认老师"),
    ] {
        required(value, label)?;
    }
    if owner_id.trim() != request.reviewed_by.trim() {
        return Err(CoreError::Invalid("只能由当前题库老师确认答案".into()));
    }
    let expected_hash = valid_hash(&request.expected_content_hash, "答案草稿 hash")?;
    validate_object_schema(&request.corrected_answer_json)?;
    let request_hash = hashing::sha256_hex(
        &serde_json::to_vec(&ReviewHashInput {
            schema_version: ANSWER_REVIEW_SCHEMA_VERSION,
            match_draft_public_id: request.match_draft_public_id.trim(),
            expected_content_hash: &expected_hash,
            corrected_answer_json: &request.corrected_answer_json,
            reviewed_by: request.reviewed_by.trim(),
            note: normalized_note(request.note.as_deref()),
        })
        .map_err(|error| CoreError::Parse(format!("答案确认请求序列化失败：{error}")))?,
    );
    if let Some((id, existing_hash)) = conn
        .query_row(
            "SELECT id,request_hash FROM k1_answer_match_reviews WHERE request_key=?1",
            [request.request_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid("同一请求键对应不同答案确认".into()));
        }
        return review_by_id(conn, id);
    }
    let row = load_match(conn, owner_id, request.match_draft_public_id.trim())?;
    if row.content_hash != expected_hash {
        return Err(CoreError::Invalid(
            "答案匹配内容已变化，请刷新后重试".into(),
        ));
    }
    let latest_extraction_run_id: i64 = conn.query_row(
        "SELECT id FROM k1_answer_source_extraction_runs
         WHERE answer_source_document_id=?1 ORDER BY id DESC LIMIT 1",
        [row.answer_source_document_id],
        |query| query.get(0),
    )?;
    if row.extraction_run_id != latest_extraction_run_id {
        return Err(CoreError::Invalid(
            "该答案草稿已被更新的识别结果替代，请刷新后确认最新结果".into(),
        ));
    }
    let already_reviewed: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM k1_answer_match_reviews WHERE answer_match_draft_id=?1)",
        [row.id],
        |query| query.get(0),
    )?;
    if already_reviewed {
        return Err(CoreError::Invalid("该答案匹配草稿已经确认".into()));
    }
    if !matches!(row.quality_level.as_str(), "L0" | "L1") {
        return Err(CoreError::Invalid(
            "当前题目已达到可批改等级，无需重复确认".into(),
        ));
    }
    let payload = validate_confirmed_payload(conn, &row, &request.corrected_answer_json)?;
    if row.quality_level == "L1" && payload.target_quality == "L1" {
        return Err(CoreError::Invalid(
            "该题已可练习；继续补充完整评分点后才能升级为可批改".into(),
        ));
    }
    let answer_revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_answer_key_versions
         WHERE question_version_id=?1",
        [row.question_version_id],
        |query| query.get(0),
    )?;
    let supersedes_answer_key_id = conn
        .query_row(
            "SELECT id FROM k1_answer_key_versions
             WHERE question_version_id=?1 ORDER BY revision DESC,id DESC LIMIT 1",
            [row.question_version_id],
            |query| query.get::<_, i64>(0),
        )
        .optional()?;
    let rubric_revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_rubric_versions
         WHERE question_version_id=?1",
        [row.question_version_id],
        |query| query.get(0),
    )?;
    let supersedes_rubric_id = conn
        .query_row(
            "SELECT id FROM k1_rubric_versions
             WHERE question_version_id=?1 ORDER BY revision DESC,id DESC LIMIT 1",
            [row.question_version_id],
            |query| query.get::<_, i64>(0),
        )
        .optional()?;
    let answer_slots = payload
        .answer_slots
        .iter()
        .map(|slot| NewAnswerSlot {
            stable_id: None,
            order_index: slot.order_index,
            canonical_answers_json: slot.canonical_answers_json.as_str(),
            normalization_rules_json: Some(slot.normalization_rules_json.as_str()),
            max_score: slot.max_score,
        })
        .collect::<Vec<_>>();
    let rubric_points = payload
        .rubric_points
        .iter()
        .map(|point| NewRubricPoint {
            stable_id: None,
            order_index: point.order_index,
            canonical_text: point.canonical_text.as_str(),
            allowed_paraphrases_json: point.allowed_paraphrases_json.as_deref(),
            required_concepts_json: point.required_concepts_json.as_deref(),
            max_score: point.max_score,
        })
        .collect::<Vec<_>>();
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let corrected_json = serde_json::to_string(&request.corrected_answer_json)
        .map_err(|error| CoreError::Parse(format!("老师答案序列化失败：{error}")))?;
    let tx = conn.transaction()?;
    let answer = content::create_answer_key_version_in_transaction(
        &tx,
        &NewAnswerKeyVersion {
            question_version_id: row.question_version_id,
            revision: answer_revision,
            answer_json: &payload.answer_json,
            state: "confirmed",
            confirmed_by: Some(request.reviewed_by.trim()),
            supersedes_answer_key_id,
            slots: &answer_slots,
        },
    )?;
    let rubric = if rubric_points.is_empty() {
        None
    } else {
        Some(content::create_rubric_version_in_transaction(
            &tx,
            &NewRubricVersion {
                question_version_id: row.question_version_id,
                revision: rubric_revision,
                max_score: row.max_score,
                state: "confirmed",
                confirmed_by: Some(request.reviewed_by.trim()),
                supersedes_rubric_id,
                points: &rubric_points,
            },
        )?)
    };
    content::promote_question_version_in_transaction(
        &tx,
        row.question_version_id,
        payload.target_quality,
        request.reviewed_by.trim(),
        Some("老师确认答案资料与题目匹配"),
    )?;
    tx.execute(
        "INSERT INTO k1_answer_match_reviews
         (public_id,request_key,request_hash,answer_match_draft_id,corrected_answer_json,
          result_answer_key_version_id,result_rubric_version_id,result_quality,
          reviewed_by,note,reviewed_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            &public_id,
            request.request_key.trim(),
            &request_hash,
            row.id,
            &corrected_json,
            answer.id,
            rubric.as_ref().map(|value| value.id),
            payload.target_quality,
            request.reviewed_by.trim(),
            normalized_note(request.note.as_deref()),
            &now,
        ],
    )?;
    let review_id = tx.last_insert_rowid();
    let event_payload = serde_json::json!({
        "schema_version": ANSWER_REVIEW_SCHEMA_VERSION,
        "answer_match_draft_public_id": &row.public_id,
        "question_version_public_id": &row.question_version_public_id,
        "answer_key_version_public_id": &answer.public_id,
        "rubric_version_public_id": rubric.as_ref().map(|value| &value.public_id),
        "result_quality": payload.target_quality,
        "creates_assessment": false,
        "creates_grade": false,
        "creates_learning_evidence": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("{}:outbox", request.request_key.trim()),
            event_type: "k1.answer_match.confirmed",
            event_version: 1,
            aggregate_type: "k1_answer_match_review",
            aggregate_id: &public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &now,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("{}:audit", request.request_key.trim()),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(request.reviewed_by.trim()),
            action: "k1.answer_match.confirmed",
            object_type: "k1_answer_match_review",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("老师确认后才创建答案/rubric 并晋级；未创建作业或成绩"),
            meta_json: Some(&event_payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    review_by_id(conn, review_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

    fn setup() -> (Connection, String, String, i64, i64) {
        let mut conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, crate::knowledge_migrations()).unwrap();
        let now = time::utc_now_rfc3339();
        let question_artifact = artifacts::create_or_get(
            &conn,
            &artifacts::NewArtifact {
                kind: ArtifactKind::Document,
                sha256: &hashing::sha256_hex(b"questions"),
                mime_type: "text/plain",
                byte_size: 9,
                original_name: Some("questions.txt"),
                original_path: None,
                archived_path: "/tmp/questions.txt",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: "test-question-source-v1",
                privacy_class: PrivacyClass::TeachingContent,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        let question_artifact_id = question_artifact.id;
        conn.execute(
            "INSERT INTO k1_source_documents
             (public_id,idempotency_key,request_hash,owner_scope,owner_id,source_artifact_id,
              source_type,source_format,source_hash,page_count,extraction_version,created_by,created_at)
             VALUES ('source-questions','source-register',?1,'personal','local_teacher',?2,
                     'source_document','text',?3,1,'test','local_teacher',?4)",
            params![
                hashing::sha256_hex(b"source-register"),
                question_artifact_id,
                hashing::sha256_hex(b"questions"),
                &now
            ],
        )
        .unwrap();
        let question_source_id = conn.last_insert_rowid();
        let question_output = "{\"schema_version\":1}";
        conn.execute(
            "INSERT INTO ai_runs
             (public_id,idempotency_key,run_type,source_module,business_ref_type,business_ref_id,
              input_artifact_id,provider,model_name,model_version,config_version,
              prompt_or_rule_version,input_hash,status,created_at,started_at,finished_at,
             output_hash,output_json)
             VALUES (?1,'ai-question','question_source_extract','knowledge',
                     'k1_source_document','source-questions',?2,'fake','fake','v1','v1','v1',
                     ?3,'succeeded',?4,?4,?4,?5,?6)",
            params![
                ids::new_public_id(),
                question_artifact_id,
                hashing::sha256_hex(b"input"),
                &now,
                hashing::sha256_hex(question_output.as_bytes()),
                question_output
            ],
        )
        .unwrap();
        let question_ai_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO k1_source_extraction_runs
             (public_id,source_document_id,ai_run_id,output_hash,extraction_state,confidence,
              privacy_json,issue_codes_json,created_at)
             VALUES ('extract-questions',?1,?2,?3,'ready',0.99,
                     '{\"schema_version\":1,\"sanitized\":true,\"contains_student_identity\":false,
                       \"contains_student_answer\":false,\"contains_teacher_mark\":false,
                       \"contains_score\":false}','[]',?4)",
            params![
                question_source_id,
                question_ai_id,
                hashing::sha256_hex(question_output.as_bytes()),
                &now
            ],
        )
        .unwrap();
        let question_extract_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO k1_questions
             (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
             VALUES ('question-one','personal','local_teacher','unknown',0,?1)",
            [&now],
        )
        .unwrap();
        let question_id = conn.last_insert_rowid();
        let stem = "鸦片战争爆发于哪一年？";
        let content_hash = hashing::sha256_hex(stem.as_bytes());
        conn.execute(
            "INSERT INTO k1_question_versions
             (public_id,question_id,revision,question_type,stem,max_score,content_hash,
              source_artifact_id,source_anchor_json,quality_level,state,created_at)
             VALUES ('question-version-one',?1,1,'single',?2,1,?3,?4,
                     '{\"schema_version\":1,\"page_no\":1}','L0','review_pending',?5)",
            params![question_id, stem, &content_hash, question_artifact_id, &now],
        )
        .unwrap();
        let question_version_id = conn.last_insert_rowid();
        for (index, label) in ["A", "B", "C", "D"].iter().enumerate() {
            conn.execute(
                "INSERT INTO k1_question_options
                 (public_id,question_version_id,label,content,order_index,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    format!("option-{label}"),
                    question_version_id,
                    label,
                    format!("选项{label}"),
                    index as i64 + 1,
                    &now
                ],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO k1_source_question_drafts
             (public_id,extraction_run_id,order_index,question_no,question_type,stem,max_score,
              options_json,source_anchor_json,confidence,content_hash,created_at)
             VALUES ('draft-one',?1,1,'1','single',?2,1,'[]',
                     '{\"schema_version\":1,\"page_no\":1}',0.99,?3,?4)",
            params![question_extract_id, stem, &content_hash, &now],
        )
        .unwrap();
        let source_draft_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO k1_source_question_reviews
             (public_id,request_key,request_hash,source_draft_id,action,result_kind,
              result_question_version_id,reviewed_by,reviewed_at)
             VALUES ('review-one','review-one',?1,?2,'accept','draft_created',?3,
                     'local_teacher',?4)",
            params![
                hashing::sha256_hex(b"review-one"),
                source_draft_id,
                question_version_id,
                &now
            ],
        )
        .unwrap();
        let answer_artifact = artifacts::create_or_get(
            &conn,
            &artifacts::NewArtifact {
                kind: ArtifactKind::Document,
                sha256: &hashing::sha256_hex(b"1.A"),
                mime_type: "text/plain",
                byte_size: 3,
                original_name: Some("answers.txt"),
                original_path: None,
                archived_path: "/tmp/answers.txt",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: "test-answer-source-v1",
                privacy_class: PrivacyClass::TeachingContent,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        let answer_artifact_id = answer_artifact.id;
        let document = register_answer_source_document(
            &mut conn,
            &RegisterAnswerSourceRequest {
                request_key: "answer-source-register".into(),
                owner_id: "local_teacher".into(),
                question_source_document_public_id: "source-questions".into(),
                source_artifact_public_id: answer_artifact.public_id,
                source_format: "text".into(),
                source_hash: hashing::sha256_hex(b"1.A"),
                page_count: 1,
                extraction_version: ANSWER_EXTRACTION_VERSION.into(),
                created_by: "local_teacher".into(),
            },
        )
        .unwrap();
        (
            conn,
            document.public_id,
            document.source_hash,
            answer_artifact_id,
            question_version_id,
        )
    }

    fn materialize(
        conn: &mut Connection,
        document_public_id: &str,
        source_hash: &str,
        answer_artifact_id: i64,
        question_version_id: i64,
        with_entry: bool,
    ) -> AnswerExtractionRecord {
        let document =
            get_answer_source_document(conn, "local_teacher", document_public_id).unwrap();
        let entries = if with_entry {
            serde_json::json!([{
                "assessment_item_id": question_version_id,
                "answer_json":{"schema_version":1,"correct_labels":["A"]},
                "source_anchor":{"schema_version":1,"line":1,"quote":"1.A"},
                "confidence":0.99
            }])
        } else {
            serde_json::json!([])
        };
        let (state, issue_codes) = if with_entry {
            ("ready", serde_json::json!([]))
        } else {
            ("needs_review", serde_json::json!(["missing_answer"]))
        };
        let output = serde_json::json!({
            "schema_version":1,
            "ingest_batch_id":document.id,
            "source_artifact_id":answer_artifact_id,
            "source_artifact_sha256":source_hash,
            "input_hash":hashing::sha256_hex(b"answer-input"),
            "descriptor":{"provider":"fake","model_name":"fake","model_version":"v1",
                          "config_version":"v1","rule_version":"v1"},
            "state":state,
            "entries":entries,
            "confidence":if with_entry { 0.99 } else { 0.5 },
            "issue_codes":issue_codes
        })
        .to_string();
        let now = time::utc_now_rfc3339();
        conn.execute(
            "INSERT INTO ai_runs
             (public_id,idempotency_key,run_type,source_module,business_ref_type,business_ref_id,
              input_artifact_id,provider,model_name,model_version,config_version,
              prompt_or_rule_version,input_hash,status,created_at,started_at,finished_at,
              output_hash,output_json)
             VALUES (?1,?2,'answer_source_structure','knowledge','k1_answer_source_document',
                     ?3,?4,'fake','fake','v1','v1','v1',?5,'succeeded',?6,?6,?6,?7,?8)",
            params![
                ids::new_public_id(),
                format!(
                    "ai-answer-key-{}",
                    if with_entry { "ready" } else { "missing" }
                ),
                document_public_id,
                answer_artifact_id,
                hashing::sha256_hex(b"answer-input"),
                &now,
                hashing::sha256_hex(output.as_bytes()),
                &output,
            ],
        )
        .unwrap();
        let ai_run_id = conn.last_insert_rowid();
        materialize_answer_extraction(conn, "local_teacher", document_public_id, ai_run_id).unwrap()
    }

    fn add_short_answer_target(
        conn: &Connection,
        suffix: &str,
        order_index: i64,
        max_score: f64,
    ) -> i64 {
        let now = time::utc_now_rfc3339();
        let question_extract_id: i64 = conn
            .query_row(
                "SELECT id FROM k1_source_extraction_runs WHERE public_id='extract-questions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let question_artifact_id: i64 = conn
            .query_row(
                "SELECT source_artifact_id FROM k1_source_documents
                 WHERE public_id='source-questions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO k1_questions
             (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
             VALUES (?1,'personal','local_teacher','unknown',0,?2)",
            params![format!("question-{suffix}"), &now],
        )
        .unwrap();
        let question_id = conn.last_insert_rowid();
        let stem = format!("简答题 {suffix}");
        let content_hash = hashing::sha256_hex(stem.as_bytes());
        conn.execute(
            "INSERT INTO k1_question_versions
             (public_id,question_id,revision,question_type,stem,max_score,content_hash,
              source_artifact_id,source_anchor_json,quality_level,state,created_at)
             VALUES (?1,?2,1,'short_answer',?3,?4,?5,?6,
                     '{\"schema_version\":1,\"page_no\":1}','L0','review_pending',?7)",
            params![
                format!("question-version-{suffix}"),
                question_id,
                &stem,
                max_score,
                &content_hash,
                question_artifact_id,
                &now
            ],
        )
        .unwrap();
        let version_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO k1_source_question_drafts
             (public_id,extraction_run_id,order_index,question_no,question_type,stem,max_score,
              options_json,source_anchor_json,confidence,content_hash,created_at)
             VALUES (?1,?2,?3,?4,'short_answer',?5,?6,'[]',
                     '{\"schema_version\":1,\"page_no\":1}',0.99,?7,?8)",
            params![
                format!("draft-{suffix}"),
                question_extract_id,
                order_index,
                order_index.to_string(),
                &stem,
                max_score,
                &content_hash,
                &now
            ],
        )
        .unwrap();
        let draft_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO k1_source_question_reviews
             (public_id,request_key,request_hash,source_draft_id,action,result_kind,
              result_question_version_id,reviewed_by,reviewed_at)
             VALUES (?1,?2,?3,?4,'accept','draft_created',?5,'local_teacher',?6)",
            params![
                format!("review-{suffix}"),
                format!("review-key-{suffix}"),
                hashing::sha256_hex(format!("review-{suffix}").as_bytes()),
                draft_id,
                version_id,
                &now
            ],
        )
        .unwrap();
        version_id
    }

    #[test]
    fn same_answer_file_with_new_request_key_reuses_document_for_retry() {
        let (mut conn, document_public_id, source_hash, artifact_id, _) = setup();
        let artifact_public_id: String = conn
            .query_row(
                "SELECT public_id FROM artifacts WHERE id=?1",
                [artifact_id],
                |row| row.get(0),
            )
            .unwrap();
        let retried = register_answer_source_document(
            &mut conn,
            &RegisterAnswerSourceRequest {
                request_key: "answer-source-retry".into(),
                owner_id: "local_teacher".into(),
                question_source_document_public_id: "source-questions".into(),
                source_artifact_public_id: artifact_public_id,
                source_format: "text".into(),
                source_hash,
                page_count: 1,
                extraction_version: ANSWER_EXTRACTION_VERSION.into(),
                created_by: "local_teacher".into(),
            },
        )
        .unwrap();
        assert_eq!(retried.public_id, document_public_id);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM k1_answer_source_documents",
                [],
                |row| { row.get::<_, i64>(0) }
            )
            .unwrap(),
            1
        );
    }

    #[test]
    fn newer_extraction_blocks_confirmation_of_stale_answer_draft() {
        let (mut conn, document, hash, artifact_id, question_version_id) = setup();
        let stale = materialize(
            &mut conn,
            &document,
            &hash,
            artifact_id,
            question_version_id,
            false,
        );
        materialize(
            &mut conn,
            &document,
            &hash,
            artifact_id,
            question_version_id,
            true,
        );
        let stale_draft = &stale.drafts[0];
        let error = confirm_answer_match(
            &mut conn,
            "local_teacher",
            &ConfirmAnswerMatchRequest {
                request_key: "confirm-stale-answer".into(),
                match_draft_public_id: stale_draft.public_id.clone(),
                expected_content_hash: stale_draft.content_hash.clone(),
                corrected_answer_json: serde_json::json!({
                    "schema_version":1,
                    "correct_labels":["A"]
                }),
                reviewed_by: "local_teacher".into(),
                note: None,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("更新的识别结果替代"));
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM k1_answer_key_versions", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            0
        );
    }

    #[test]
    fn missing_answer_is_materialized_without_creating_answer_or_assessment() {
        let (mut conn, document, hash, artifact_id, question_version_id) = setup();
        let extraction = materialize(
            &mut conn,
            &document,
            &hash,
            artifact_id,
            question_version_id,
            false,
        );
        assert_eq!(extraction.drafts[0].candidate_state, "missing");
        assert!(extraction.drafts[0].answer_json.is_none());
        for table in ["k1_answer_key_versions", "k1_rubric_versions"] {
            assert_eq!(
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
                0,
                "{table}"
            );
        }
    }

    #[test]
    fn teacher_confirmation_creates_answer_and_promotes_to_l2_atomically() {
        let (mut conn, document, hash, artifact_id, question_version_id) = setup();
        let extraction = materialize(
            &mut conn,
            &document,
            &hash,
            artifact_id,
            question_version_id,
            true,
        );
        let draft = &extraction.drafts[0];
        let review = confirm_answer_match(
            &mut conn,
            "local_teacher",
            &ConfirmAnswerMatchRequest {
                request_key: "confirm-answer-one".into(),
                match_draft_public_id: draft.public_id.clone(),
                expected_content_hash: draft.content_hash.clone(),
                corrected_answer_json: draft.answer_json.clone().unwrap(),
                reviewed_by: "local_teacher".into(),
                note: Some("答案资料与题号一致".into()),
            },
        )
        .unwrap();
        assert_eq!(review.result_quality, "L2");
        assert_eq!(
            conn.query_row(
                "SELECT quality_level FROM k1_question_versions WHERE id=?1",
                [question_version_id],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "L2"
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM k1_answer_key_versions", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            1
        );
    }

    #[test]
    fn invalid_objective_answer_leaves_all_business_tables_unchanged() {
        let (mut conn, document, hash, artifact_id, question_version_id) = setup();
        let extraction = materialize(
            &mut conn,
            &document,
            &hash,
            artifact_id,
            question_version_id,
            true,
        );
        let draft = &extraction.drafts[0];
        let error = confirm_answer_match(
            &mut conn,
            "local_teacher",
            &ConfirmAnswerMatchRequest {
                request_key: "confirm-invalid".into(),
                match_draft_public_id: draft.public_id.clone(),
                expected_content_hash: draft.content_hash.clone(),
                corrected_answer_json: serde_json::json!({
                    "schema_version":1,
                    "correct_labels":["Z"]
                }),
                reviewed_by: "local_teacher".into(),
                note: None,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("不在当前题目选项"));
        for table in [
            "k1_answer_key_versions",
            "k1_question_quality_events",
            "k1_answer_match_reviews",
        ] {
            assert_eq!(
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
                0,
                "{table}"
            );
        }
    }

    #[test]
    fn short_answer_reference_becomes_l1_and_complete_rubric_becomes_l2() {
        let (mut conn, document, hash, artifact_id, objective_id) = setup();
        let l1_version_id = add_short_answer_target(&conn, "short-l1", 2, 4.0);
        let l2_version_id = add_short_answer_target(&conn, "short-l2", 3, 4.0);
        let extraction = materialize(
            &mut conn,
            &document,
            &hash,
            artifact_id,
            objective_id,
            false,
        );
        let l1_draft = extraction
            .drafts
            .iter()
            .find(|draft| draft.question_version_public_id == "question-version-short-l1")
            .unwrap();
        let l2_draft = extraction
            .drafts
            .iter()
            .find(|draft| draft.question_version_public_id == "question-version-short-l2")
            .unwrap();
        let l1_review = confirm_answer_match(
            &mut conn,
            "local_teacher",
            &ConfirmAnswerMatchRequest {
                request_key: "confirm-short-l1".into(),
                match_draft_public_id: l1_draft.public_id.clone(),
                expected_content_hash: l1_draft.content_hash.clone(),
                corrected_answer_json: serde_json::json!({
                    "schema_version":1,
                    "reference_answer":"参考答案，但暂未拆评分点"
                }),
                reviewed_by: "local_teacher".into(),
                note: None,
            },
        )
        .unwrap();
        assert_eq!(l1_review.result_quality, "L1");
        assert!(l1_review.result_rubric_version_public_id.is_none());
        let l2_review = confirm_answer_match(
            &mut conn,
            "local_teacher",
            &ConfirmAnswerMatchRequest {
                request_key: "confirm-short-l2".into(),
                match_draft_public_id: l2_draft.public_id.clone(),
                expected_content_hash: l2_draft.content_hash.clone(),
                corrected_answer_json: serde_json::json!({
                    "schema_version":1,
                    "reference_answer":"包含两个评分点",
                    "rubric_points":[
                        {"order_index":0,"canonical_text":"评分点一","max_score":2.0},
                        {"order_index":1,"canonical_text":"评分点二","max_score":2.0}
                    ]
                }),
                reviewed_by: "local_teacher".into(),
                note: None,
            },
        )
        .unwrap();
        assert_eq!(l2_review.result_quality, "L2");
        assert!(l2_review.result_rubric_version_public_id.is_some());
        for (version_id, quality) in [(l1_version_id, "L1"), (l2_version_id, "L2")] {
            assert_eq!(
                conn.query_row(
                    "SELECT quality_level FROM k1_question_versions WHERE id=?1",
                    [version_id],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
                quality
            );
        }
    }
}
