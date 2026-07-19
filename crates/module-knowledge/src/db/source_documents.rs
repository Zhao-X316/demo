//! K1 独立空白卷 / 电子题目文件来源服务。
//!
//! 来源登记、提取落库和老师处理均有独立幂等边界。老师确认题目结构后，
//! 最多创建 L0/review_pending；答案、rubric、知识链接和作业必须由后续显式流程补齐。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::ai_runs;
use suite_core::db::repo::artifacts;
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRunStatus, ArchiveStatus, ArtifactKind, AuditActorType, PrivacyClass};

use crate::db::content::{
    self, NewQuestion, NewQuestionOption, NewQuestionVersion, QuestionVersion,
};
use crate::source_import::{
    validate_output, SourceExtractionInput, SourceExtractionOutput, SourceOptionDraft,
    SOURCE_EXTRACTION_SCHEMA_VERSION,
};

const REVIEW_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDocument {
    pub public_id: String,
    pub owner_scope: String,
    pub owner_id: String,
    pub source_artifact_id: i64,
    pub source_artifact_public_id: String,
    pub source_type: String,
    pub source_format: String,
    pub source_hash: String,
    pub page_count: i64,
    pub extraction_version: String,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterSourceDocumentRequest {
    pub request_key: String,
    pub owner_id: String,
    pub source_artifact_public_id: String,
    pub source_type: String,
    pub source_format: String,
    pub source_hash: String,
    pub page_count: i64,
    pub extraction_version: String,
    pub created_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDraft {
    pub public_id: String,
    pub source_document_public_id: String,
    pub extraction_run_public_id: String,
    pub order_index: i64,
    pub question_no: Option<String>,
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub max_score: f64,
    pub options: Vec<SourceOptionDraft>,
    pub source_anchor_json: String,
    pub confidence: f64,
    pub content_hash: String,
    pub review_action: Option<String>,
    pub result_kind: Option<String>,
    pub result_question_version_public_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceExtractionRecord {
    pub public_id: String,
    pub source_document_public_id: String,
    pub ai_run_public_id: String,
    pub extraction_state: String,
    pub confidence: f64,
    pub issue_codes: Vec<String>,
    pub drafts: Vec<SourceDraft>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInboxItem {
    pub document: SourceDocument,
    pub latest_extraction: Option<SourceExtractionRecord>,
    pub latest_ai_run_public_id: Option<String>,
    pub latest_ai_status: Option<String>,
    pub latest_ai_error_meta_json: Option<String>,
    pub total_drafts: i64,
    pub pending_drafts: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectedSourceQuestion {
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub max_score: f64,
    pub options: Vec<SourceOptionDraft>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewSourceDraftRequest {
    pub request_key: String,
    pub draft_public_id: String,
    pub expected_content_hash: String,
    pub corrected: Option<CorrectedSourceQuestion>,
    pub reviewed_by: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardSourceDraftRequest {
    pub request_key: String,
    pub draft_public_id: String,
    pub expected_content_hash: String,
    pub reviewed_by: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDraftReview {
    pub public_id: String,
    pub source_draft_public_id: String,
    pub action: String,
    pub result_kind: String,
    pub result_question_version_public_id: Option<String>,
    pub reviewed_by: String,
    pub note: Option<String>,
    pub reviewed_at: String,
}

#[derive(Serialize)]
struct RegisterHashInput<'a> {
    schema_version: i64,
    owner_id: &'a str,
    source_artifact_public_id: &'a str,
    source_type: &'a str,
    source_format: &'a str,
    source_hash: &'a str,
    page_count: i64,
    extraction_version: &'a str,
    created_by: &'a str,
}

#[derive(Serialize)]
struct AcceptHashInput<'a> {
    schema_version: i64,
    draft_public_id: &'a str,
    expected_content_hash: &'a str,
    corrected: &'a Option<CorrectedSourceQuestion>,
    reviewed_by: &'a str,
    note: Option<&'a str>,
}

#[derive(Serialize)]
struct DiscardHashInput<'a> {
    schema_version: i64,
    draft_public_id: &'a str,
    expected_content_hash: &'a str,
    reviewed_by: &'a str,
    note: Option<&'a str>,
}

#[derive(Debug)]
struct DraftRow {
    id: i64,
    public_id: String,
    source_document_public_id: String,
    source_artifact_id: i64,
    owner_id: String,
    question_type: String,
    stem: String,
    material_text: Option<String>,
    max_score: f64,
    options_json: String,
    source_anchor_json: String,
    content_hash: String,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn normalized_hash(value: &str, label: &str) -> CoreResult<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(format!(
            "{label}必须是 64 位十六进制 hash"
        )));
    }
    Ok(value)
}

fn validate_source_type(value: &str) -> CoreResult<()> {
    if matches!(value, "blank_paper" | "source_document") {
        Ok(())
    } else {
        Err(CoreError::Invalid("题目来源类型非法".into()))
    }
}

fn validate_source_format(value: &str) -> CoreResult<()> {
    if matches!(value, "jpeg" | "pdf" | "text" | "docx" | "xlsx") {
        Ok(())
    } else {
        Err(CoreError::Invalid("题目来源格式非法".into()))
    }
}

fn source_document_by_id(conn: &Connection, id: i64) -> CoreResult<SourceDocument> {
    conn.query_row(
        "SELECT d.public_id,d.owner_scope,d.owner_id,d.source_artifact_id,a.public_id,
                d.source_type,d.source_format,d.source_hash,d.page_count,d.extraction_version,
                d.created_by,d.created_at
         FROM k1_source_documents d
         JOIN artifacts a ON a.id=d.source_artifact_id
         WHERE d.id=?1",
        [id],
        |row| {
            Ok(SourceDocument {
                public_id: row.get(0)?,
                owner_scope: row.get(1)?,
                owner_id: row.get(2)?,
                source_artifact_id: row.get(3)?,
                source_artifact_public_id: row.get(4)?,
                source_type: row.get(5)?,
                source_format: row.get(6)?,
                source_hash: row.get(7)?,
                page_count: row.get(8)?,
                extraction_version: row.get(9)?,
                created_by: row.get(10)?,
                created_at: row.get(11)?,
            })
        },
    )
    .map_err(Into::into)
}

pub fn get_source_document(
    conn: &Connection,
    owner_id: &str,
    public_id: &str,
) -> CoreResult<SourceDocument> {
    let id = conn
        .query_row(
            "SELECT id FROM k1_source_documents
             WHERE public_id=?1 AND owner_scope='personal' AND owner_id=?2",
            params![public_id.trim(), owner_id.trim()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("题目来源文档".into()))?;
    source_document_by_id(conn, id)
}

fn validate_artifact_for_source(
    conn: &Connection,
    artifact_public_id: &str,
    source_format: &str,
    expected_hash: &str,
) -> CoreResult<suite_core::models::Artifact> {
    let artifact = artifacts::get_by_public_id(conn, artifact_public_id.trim())?
        .ok_or_else(|| CoreError::NotFound("题目来源归档资产".into()))?;
    if artifact.archive_status != ArchiveStatus::Ready {
        return Err(CoreError::Invalid("题目来源归档尚不可用".into()));
    }
    if !matches!(
        artifact.privacy_class,
        PrivacyClass::TeachingContent | PrivacyClass::PublicSafe
    ) {
        return Err(CoreError::Invalid(
            "学生敏感资产不能作为独立题目来源；请改用空白卷或脱敏文件".into(),
        ));
    }
    let expected_kind = if source_format == "jpeg" {
        ArtifactKind::Image
    } else {
        ArtifactKind::Document
    };
    if artifact.kind != expected_kind {
        return Err(CoreError::Invalid(
            "题目来源格式与归档资产类型不一致".into(),
        ));
    }
    if artifact.sha256 != expected_hash {
        return Err(CoreError::Invalid("题目来源 hash 与归档资产不一致".into()));
    }
    Ok(artifact)
}

pub fn register_source_document(
    conn: &mut Connection,
    request: &RegisterSourceDocumentRequest,
) -> CoreResult<SourceDocument> {
    for (value, label) in [
        (&request.request_key, "请求键"),
        (&request.owner_id, "题库老师"),
        (&request.source_artifact_public_id, "来源资产"),
        (&request.extraction_version, "提取规则版本"),
        (&request.created_by, "创建老师"),
    ] {
        required(value, label)?;
    }
    if request.owner_id.trim() != request.created_by.trim() {
        return Err(CoreError::Invalid("只能以当前题库老师身份登记来源".into()));
    }
    validate_source_type(request.source_type.as_str())?;
    validate_source_format(request.source_format.as_str())?;
    if !(1..=200).contains(&request.page_count) {
        return Err(CoreError::Invalid(
            "题目来源页数必须在 1 到 200 之间".into(),
        ));
    }
    let source_hash = normalized_hash(&request.source_hash, "题目来源 hash")?;
    let request_hash = hashing::sha256_hex(
        &serde_json::to_vec(&RegisterHashInput {
            schema_version: REVIEW_SCHEMA_VERSION,
            owner_id: request.owner_id.trim(),
            source_artifact_public_id: request.source_artifact_public_id.trim(),
            source_type: request.source_type.as_str(),
            source_format: request.source_format.as_str(),
            source_hash: &source_hash,
            page_count: request.page_count,
            extraction_version: request.extraction_version.trim(),
            created_by: request.created_by.trim(),
        })
        .map_err(|error| CoreError::Parse(format!("题目来源登记请求序列化失败：{error}")))?,
    );
    if let Some((id, existing_hash)) = conn
        .query_row(
            "SELECT id,request_hash FROM k1_source_documents WHERE idempotency_key=?1",
            [request.request_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid("同一请求键对应不同题目来源".into()));
        }
        return source_document_by_id(conn, id);
    }
    let artifact = validate_artifact_for_source(
        conn,
        &request.source_artifact_public_id,
        &request.source_format,
        &source_hash,
    )?;
    if let Some(existing_id) = conn
        .query_row(
            "SELECT id FROM k1_source_documents
             WHERE owner_scope='personal' AND owner_id=?1 AND source_artifact_id=?2
               AND source_type=?3 AND extraction_version=?4",
            params![
                request.owner_id.trim(),
                artifact.id,
                request.source_type,
                request.extraction_version.trim()
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        return source_document_by_id(conn, existing_id);
    }
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO k1_source_documents
         (public_id,idempotency_key,request_hash,owner_scope,owner_id,source_artifact_id,
          source_type,source_format,source_hash,page_count,extraction_version,created_by,created_at)
         VALUES (?1,?2,?3,'personal',?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            &public_id,
            request.request_key.trim(),
            &request_hash,
            request.owner_id.trim(),
            artifact.id,
            request.source_type,
            request.source_format,
            &source_hash,
            request.page_count,
            request.extraction_version.trim(),
            request.created_by.trim(),
            &now,
        ],
    )?;
    let id = tx.last_insert_rowid();
    let event_payload = serde_json::json!({
        "schema_version": REVIEW_SCHEMA_VERSION,
        "source_document_public_id": &public_id,
        "source_type": &request.source_type,
        "source_format": &request.source_format,
        "page_count": request.page_count,
        "creates_assessment": false,
        "creates_question": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("{}:outbox", request.request_key.trim()),
            event_type: "k1.source_document.registered",
            event_version: 1,
            aggregate_type: "k1_source_document",
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
            action: "k1.source_document.registered",
            object_type: "k1_source_document",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("仅登记干净题目来源；未创建题目、答案或作业"),
            meta_json: Some(&event_payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    source_document_by_id(conn, id)
}

fn load_drafts_for_run(
    conn: &Connection,
    run_id: i64,
    run_public_id: &str,
    document_public_id: &str,
) -> CoreResult<Vec<SourceDraft>> {
    let mut statement = conn.prepare(
        "SELECT d.public_id,d.order_index,d.question_no,d.question_type,d.stem,d.material_text,
                d.max_score,d.options_json,d.source_anchor_json,d.confidence,d.content_hash,
                r.action,r.result_kind,v.public_id
         FROM k1_source_question_drafts d
         LEFT JOIN k1_source_question_reviews r ON r.source_draft_id=d.id
         LEFT JOIN k1_question_versions v ON v.id=r.result_question_version_id
         WHERE d.extraction_run_id=?1
         ORDER BY d.order_index,d.id",
    )?;
    let rows = statement.query_map([run_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, f64>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, f64>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, Option<String>>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, Option<String>>(13)?,
        ))
    })?;
    let mut drafts = Vec::new();
    for row in rows {
        let (
            public_id,
            order_index,
            question_no,
            question_type,
            stem,
            material_text,
            max_score,
            options_json,
            source_anchor_json,
            confidence,
            content_hash,
            review_action,
            result_kind,
            result_question_version_public_id,
        ) = row?;
        drafts.push(SourceDraft {
            public_id,
            source_document_public_id: document_public_id.to_owned(),
            extraction_run_public_id: run_public_id.to_owned(),
            order_index,
            question_no,
            question_type,
            stem,
            material_text,
            max_score,
            options: serde_json::from_str(&options_json)
                .map_err(|error| CoreError::Parse(format!("来源题目选项损坏：{error}")))?,
            source_anchor_json,
            confidence,
            content_hash,
            review_action,
            result_kind,
            result_question_version_public_id,
        });
    }
    Ok(drafts)
}

fn extraction_by_id(conn: &Connection, id: i64) -> CoreResult<SourceExtractionRecord> {
    let (
        public_id,
        document_public_id,
        ai_run_public_id,
        extraction_state,
        confidence,
        issue_codes_json,
        created_at,
    ) = conn.query_row(
        "SELECT r.public_id,d.public_id,a.public_id,r.extraction_state,r.confidence,
                r.issue_codes_json,r.created_at
         FROM k1_source_extraction_runs r
         JOIN k1_source_documents d ON d.id=r.source_document_id
         JOIN ai_runs a ON a.id=r.ai_run_id
         WHERE r.id=?1",
        [id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        },
    )?;
    let issue_codes = serde_json::from_str(&issue_codes_json)
        .map_err(|error| CoreError::Parse(format!("来源提取问题码损坏：{error}")))?;
    let drafts = load_drafts_for_run(conn, id, &public_id, &document_public_id)?;
    Ok(SourceExtractionRecord {
        public_id,
        source_document_public_id: document_public_id,
        ai_run_public_id,
        extraction_state,
        confidence,
        issue_codes,
        drafts,
        created_at,
    })
}

pub fn materialize_source_extraction(
    conn: &mut Connection,
    owner_id: &str,
    source_document_public_id: &str,
    ai_run_id: i64,
) -> CoreResult<SourceExtractionRecord> {
    let document = get_source_document(conn, owner_id, source_document_public_id)?;
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM k1_source_extraction_runs WHERE ai_run_id=?1",
            [ai_run_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        return extraction_by_id(conn, id);
    }
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound("题目来源 AI 运行".into()))?;
    if run.status != AiRunStatus::Succeeded
        || run.run_type != "question_source_extract"
        || run.source_module != "knowledge"
        || run.business_ref_type != "k1_source_document"
        || run.business_ref_id != document.public_id
        || run.input_artifact_id != Some(document.source_artifact_id)
    {
        return Err(CoreError::Invalid(
            "AI 运行不属于当前题目来源或尚未成功".into(),
        ));
    }
    let output_json = run
        .output_json
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("成功 AI 运行缺少输出".into()))?;
    let output_hash = hashing::sha256_hex(output_json.as_bytes());
    if run.output_hash.as_deref() != Some(output_hash.as_str()) {
        return Err(CoreError::Invalid("AI 运行输出 hash 不一致".into()));
    }
    let output: SourceExtractionOutput = serde_json::from_str(output_json)
        .map_err(|error| CoreError::Parse(format!("题目来源 AI 输出无效：{error}")))?;
    let validation_input = SourceExtractionInput {
        schema_version: SOURCE_EXTRACTION_SCHEMA_VERSION,
        source_document_public_id: document.public_id.clone(),
        source_type: document.source_type.clone(),
        source_format: document.source_format.clone(),
        source_hash: document.source_hash.clone(),
        page_count: document.page_count,
        extracted_text: Some("archived_source_validated_by_adapter".into()),
        visual_pages: Vec::new(),
    };
    validate_output(&validation_input, &output)?;
    let privacy_json = serde_json::to_string(&output.privacy)
        .map_err(|error| CoreError::Parse(format!("题目来源隐私结果序列化失败：{error}")))?;
    let issue_codes_json = serde_json::to_string(&output.issue_codes)
        .map_err(|error| CoreError::Parse(format!("题目来源问题码序列化失败：{error}")))?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    let document_id = conn.query_row(
        "SELECT id FROM k1_source_documents WHERE public_id=?1",
        [document.public_id.as_str()],
        |row| row.get::<_, i64>(0),
    )?;
    if let Some(existing_id) = conn
        .query_row(
            "SELECT id FROM k1_source_extraction_runs
             WHERE source_document_id=?1 AND output_hash=?2",
            params![document_id, &output_hash],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        return extraction_by_id(conn, existing_id);
    }
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO k1_source_extraction_runs
         (public_id,source_document_id,ai_run_id,output_hash,extraction_state,
          confidence,privacy_json,issue_codes_json,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            &public_id,
            document_id,
            ai_run_id,
            &output_hash,
            &output.state,
            output.confidence,
            &privacy_json,
            &issue_codes_json,
            &now,
        ],
    )?;
    let extraction_id = tx.last_insert_rowid();
    for draft in &output.drafts {
        let options_json = serde_json::to_string(&draft.options)
            .map_err(|error| CoreError::Parse(format!("来源题目选项序列化失败：{error}")))?;
        let anchor_json = serde_json::to_string(&draft.source_anchor)
            .map_err(|error| CoreError::Parse(format!("来源锚点序列化失败：{error}")))?;
        let options = new_options(&draft.options);
        let content_hash = content::content_hash(&NewQuestionVersion {
            question_id: 0,
            revision: 1,
            question_type: &draft.question_type,
            stem: &draft.stem,
            material_text: draft.material_text.as_deref(),
            max_score: draft.max_score,
            source_artifact_id: Some(document.source_artifact_id),
            source_anchor_json: Some(&anchor_json),
            supersedes_version_id: None,
            quality_level: "L0",
            state: "review_pending",
            options: &options,
        })?;
        tx.execute(
            "INSERT INTO k1_source_question_drafts
             (public_id,extraction_run_id,order_index,question_no,question_type,stem,
              material_text,max_score,options_json,source_anchor_json,confidence,
              content_hash,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                ids::new_public_id(),
                extraction_id,
                draft.order_index,
                draft
                    .question_no
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
                &draft.question_type,
                draft.stem.trim(),
                draft
                    .material_text
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty()),
                draft.max_score,
                &options_json,
                &anchor_json,
                draft.confidence,
                &content_hash,
                &now,
            ],
        )?;
    }
    let event_payload = serde_json::json!({
        "schema_version": REVIEW_SCHEMA_VERSION,
        "source_document_public_id": &document.public_id,
        "extraction_run_public_id": &public_id,
        "state": &output.state,
        "draft_count": output.drafts.len(),
        "creates_assessment": false,
        "creates_question": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("k1:outbox:source-extraction:{public_id}"),
            event_type: "k1.source_document.extracted",
            event_version: 1,
            aggregate_type: "k1_source_extraction",
            aggregate_id: &public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &now,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("k1:audit:source-extraction:{public_id}"),
            actor_type: AuditActorType::System,
            actor_id: None,
            action: "k1.source_document.extracted",
            object_type: "k1_source_extraction",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("机器结果仅进入来源草稿箱；未创建正式题目"),
            meta_json: Some(&event_payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    extraction_by_id(conn, extraction_id)
}

fn new_options(options: &[SourceOptionDraft]) -> Vec<NewQuestionOption<'_>> {
    options
        .iter()
        .map(|option| NewQuestionOption {
            label: &option.label,
            content: &option.content,
            order_index: option.order_index,
        })
        .collect()
}

fn load_draft(conn: &Connection, owner_id: &str, public_id: &str) -> CoreResult<DraftRow> {
    conn.query_row(
        "SELECT d.id,d.public_id,doc.public_id,doc.source_artifact_id,
                doc.owner_id,d.question_type,d.stem,d.material_text,
                d.max_score,d.options_json,d.source_anchor_json,d.content_hash
         FROM k1_source_question_drafts d
         JOIN k1_source_extraction_runs run ON run.id=d.extraction_run_id
         JOIN k1_source_documents doc ON doc.id=run.source_document_id
         WHERE d.public_id=?1 AND doc.owner_scope='personal' AND doc.owner_id=?2",
        params![public_id.trim(), owner_id.trim()],
        |row| {
            Ok(DraftRow {
                id: row.get(0)?,
                public_id: row.get(1)?,
                source_document_public_id: row.get(2)?,
                source_artifact_id: row.get(3)?,
                owner_id: row.get(4)?,
                question_type: row.get(5)?,
                stem: row.get(6)?,
                material_text: row.get(7)?,
                max_score: row.get(8)?,
                options_json: row.get(9)?,
                source_anchor_json: row.get(10)?,
                content_hash: row.get(11)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound("来源题目草稿".into()))
}

fn review_by_id(conn: &Connection, id: i64) -> CoreResult<SourceDraftReview> {
    conn.query_row(
        "SELECT r.public_id,d.public_id,r.action,r.result_kind,v.public_id,
                r.reviewed_by,r.note,r.reviewed_at
         FROM k1_source_question_reviews r
         JOIN k1_source_question_drafts d ON d.id=r.source_draft_id
         LEFT JOIN k1_question_versions v ON v.id=r.result_question_version_id
         WHERE r.id=?1",
        [id],
        |row| {
            Ok(SourceDraftReview {
                public_id: row.get(0)?,
                source_draft_public_id: row.get(1)?,
                action: row.get(2)?,
                result_kind: row.get(3)?,
                result_question_version_public_id: row.get(4)?,
                reviewed_by: row.get(5)?,
                note: row.get(6)?,
                reviewed_at: row.get(7)?,
            })
        },
    )
    .map_err(Into::into)
}

fn existing_review_by_request(
    conn: &Connection,
    request_key: &str,
) -> CoreResult<Option<(i64, String)>> {
    Ok(conn
        .query_row(
            "SELECT id,request_hash FROM k1_source_question_reviews WHERE request_key=?1",
            [request_key.trim()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

fn ensure_not_reviewed(conn: &Connection, draft_id: i64) -> CoreResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM k1_source_question_reviews WHERE source_draft_id=?1)",
        [draft_id],
        |row| row.get(0),
    )?;
    if exists {
        Err(CoreError::Invalid("该来源题目草稿已经处理".into()))
    } else {
        Ok(())
    }
}

fn normalized_note(note: Option<&str>) -> Option<&str> {
    note.map(str::trim).filter(|value| !value.is_empty())
}

fn final_question(
    draft: &DraftRow,
    corrected: Option<&CorrectedSourceQuestion>,
) -> CoreResult<CorrectedSourceQuestion> {
    let original_options: Vec<SourceOptionDraft> = serde_json::from_str(&draft.options_json)
        .map_err(|error| CoreError::Parse(format!("来源题目选项损坏：{error}")))?;
    let question = corrected.cloned().unwrap_or(CorrectedSourceQuestion {
        question_type: draft.question_type.clone(),
        stem: draft.stem.clone(),
        material_text: draft.material_text.clone(),
        max_score: draft.max_score,
        options: original_options,
    });
    required(&question.stem, "确认题干")?;
    if !matches!(
        question.question_type.as_str(),
        "single" | "multiple" | "true_false" | "fill_blank" | "short_answer"
    ) {
        return Err(CoreError::Invalid("确认题型非法".into()));
    }
    if !question.max_score.is_finite() || question.max_score <= 0.0 {
        return Err(CoreError::Invalid("确认分值必须大于 0".into()));
    }
    if matches!(question.question_type.as_str(), "single" | "multiple") {
        if question.options.len() < 2 {
            return Err(CoreError::Invalid("选择题至少需要两个选项".into()));
        }
    } else if !question.options.is_empty() {
        return Err(CoreError::Invalid("非选择题不得携带选项".into()));
    }
    Ok(question)
}

fn choose_or_create_l0_version(
    conn: &Connection,
    draft: &DraftRow,
    question: &CorrectedSourceQuestion,
) -> CoreResult<(QuestionVersion, &'static str)> {
    let options = new_options(&question.options);
    let version_input = NewQuestionVersion {
        question_id: 0,
        revision: 1,
        question_type: &question.question_type,
        stem: &question.stem,
        material_text: question.material_text.as_deref(),
        max_score: question.max_score,
        source_artifact_id: Some(draft.source_artifact_id),
        source_anchor_json: Some(&draft.source_anchor_json),
        supersedes_version_id: None,
        quality_level: "L0",
        state: "review_pending",
        options: &options,
    };
    let hash = content::content_hash(&version_input)?;
    let exact = content::find_exact_versions_for_owner(
        conn,
        &hash,
        &question.question_type,
        "personal",
        &draft.owner_id,
    )?;
    let mut question_ids = exact
        .iter()
        .map(|version| version.question_id)
        .collect::<std::collections::HashSet<_>>();
    if question_ids.len() > 1 {
        return Err(CoreError::Invalid(
            "当前个人题库已有多个完全重复身份，请先处理重复候选".into(),
        ));
    }
    if let Some(existing) = exact
        .iter()
        .filter(|version| version.quality_level != "C0")
        .max_by_key(|version| (version.revision, version.id))
    {
        return Ok((existing.clone(), "exact_reused"));
    }
    let (question_id, revision, supersedes_version_id) = if let Some(existing) = exact
        .iter()
        .max_by_key(|version| (version.revision, version.id))
    {
        (
            existing.question_id,
            existing.revision + 1,
            Some(existing.id),
        )
    } else {
        let identity = content::create_question(
            conn,
            &NewQuestion {
                owner_scope: "personal",
                owner_id: &draft.owner_id,
                question_family_id: None,
                rights_status: "unknown",
                sharing_allowed: false,
            },
        )?;
        question_ids.insert(identity.id);
        (identity.id, 1, None)
    };
    let version = content::create_question_version_in_transaction(
        conn,
        &NewQuestionVersion {
            question_id,
            revision,
            supersedes_version_id,
            ..version_input
        },
    )?;
    Ok((version, "draft_created"))
}

pub fn accept_source_draft(
    conn: &mut Connection,
    owner_id: &str,
    request: &ReviewSourceDraftRequest,
) -> CoreResult<SourceDraftReview> {
    for (value, label) in [
        (&request.request_key, "请求键"),
        (&request.draft_public_id, "来源题目草稿"),
        (&request.reviewed_by, "确认老师"),
    ] {
        required(value, label)?;
    }
    if owner_id.trim() != request.reviewed_by.trim() {
        return Err(CoreError::Invalid("只能由当前题库老师确认来源题目".into()));
    }
    let expected_hash = normalized_hash(&request.expected_content_hash, "题目草稿 hash")?;
    let request_hash = hashing::sha256_hex(
        &serde_json::to_vec(&AcceptHashInput {
            schema_version: REVIEW_SCHEMA_VERSION,
            draft_public_id: request.draft_public_id.trim(),
            expected_content_hash: &expected_hash,
            corrected: &request.corrected,
            reviewed_by: request.reviewed_by.trim(),
            note: normalized_note(request.note.as_deref()),
        })
        .map_err(|error| CoreError::Parse(format!("来源题目确认请求序列化失败：{error}")))?,
    );
    if let Some((id, existing_hash)) = existing_review_by_request(conn, request.request_key.trim())?
    {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid("同一请求键对应不同来源题目确认".into()));
        }
        return review_by_id(conn, id);
    }
    let draft = load_draft(conn, owner_id, request.draft_public_id.trim())?;
    if draft.content_hash != expected_hash {
        return Err(CoreError::Invalid(
            "来源题目内容已变化，请刷新后重试".into(),
        ));
    }
    ensure_not_reviewed(conn, draft.id)?;
    let question = final_question(&draft, request.corrected.as_ref())?;
    let corrected_json = request
        .corrected
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| CoreError::Parse(format!("题目修正序列化失败：{error}")))?;
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let tx = conn.transaction()?;
    let (version, result_kind) = choose_or_create_l0_version(&tx, &draft, &question)?;
    tx.execute(
        "INSERT INTO k1_source_question_reviews
         (public_id,request_key,request_hash,source_draft_id,action,corrected_payload_json,
          result_kind,result_question_version_id,reviewed_by,note,reviewed_at)
         VALUES (?1,?2,?3,?4,'accept',?5,?6,?7,?8,?9,?10)",
        params![
            &public_id,
            request.request_key.trim(),
            &request_hash,
            draft.id,
            corrected_json,
            result_kind,
            version.id,
            request.reviewed_by.trim(),
            normalized_note(request.note.as_deref()),
            &now,
        ],
    )?;
    let review_id = tx.last_insert_rowid();
    let event_payload = serde_json::json!({
        "schema_version": REVIEW_SCHEMA_VERSION,
        "source_draft_public_id": &draft.public_id,
        "source_document_public_id": &draft.source_document_public_id,
        "result_kind": result_kind,
        "result_question_version_public_id": &version.public_id,
        "quality_level": &version.quality_level,
        "state": &version.state,
        "creates_answer": false,
        "creates_rubric": false,
        "creates_assessment": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("{}:outbox", request.request_key.trim()),
            event_type: "k1.source_question.reviewed",
            event_version: 1,
            aggregate_type: "k1_source_question_review",
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
            action: "k1.source_question.accepted",
            object_type: "k1_source_question_review",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("老师仅确认题目结构；结果仍为 L0，未创建答案、评分规则或作业"),
            meta_json: Some(&event_payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    review_by_id(conn, review_id)
}

pub fn discard_source_draft(
    conn: &mut Connection,
    owner_id: &str,
    request: &DiscardSourceDraftRequest,
) -> CoreResult<SourceDraftReview> {
    for (value, label) in [
        (&request.request_key, "请求键"),
        (&request.draft_public_id, "来源题目草稿"),
        (&request.reviewed_by, "确认老师"),
    ] {
        required(value, label)?;
    }
    if owner_id.trim() != request.reviewed_by.trim() {
        return Err(CoreError::Invalid("只能由当前题库老师丢弃来源题目".into()));
    }
    let expected_hash = normalized_hash(&request.expected_content_hash, "题目草稿 hash")?;
    let request_hash = hashing::sha256_hex(
        &serde_json::to_vec(&DiscardHashInput {
            schema_version: REVIEW_SCHEMA_VERSION,
            draft_public_id: request.draft_public_id.trim(),
            expected_content_hash: &expected_hash,
            reviewed_by: request.reviewed_by.trim(),
            note: normalized_note(request.note.as_deref()),
        })
        .map_err(|error| CoreError::Parse(format!("来源题目丢弃请求序列化失败：{error}")))?,
    );
    if let Some((id, existing_hash)) = existing_review_by_request(conn, request.request_key.trim())?
    {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid("同一请求键对应不同来源题目丢弃".into()));
        }
        return review_by_id(conn, id);
    }
    let draft = load_draft(conn, owner_id, request.draft_public_id.trim())?;
    if draft.content_hash != expected_hash {
        return Err(CoreError::Invalid(
            "来源题目内容已变化，请刷新后重试".into(),
        ));
    }
    ensure_not_reviewed(conn, draft.id)?;
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO k1_source_question_reviews
         (public_id,request_key,request_hash,source_draft_id,action,result_kind,
          reviewed_by,note,reviewed_at)
         VALUES (?1,?2,?3,?4,'discard','discarded',?5,?6,?7)",
        params![
            &public_id,
            request.request_key.trim(),
            &request_hash,
            draft.id,
            request.reviewed_by.trim(),
            normalized_note(request.note.as_deref()),
            &now,
        ],
    )?;
    let review_id = tx.last_insert_rowid();
    let event_payload = serde_json::json!({
        "schema_version": REVIEW_SCHEMA_VERSION,
        "source_draft_public_id": &draft.public_id,
        "source_document_public_id": &draft.source_document_public_id,
        "result_kind": "discarded"
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("{}:outbox", request.request_key.trim()),
            event_type: "k1.source_question.reviewed",
            event_version: 1,
            aggregate_type: "k1_source_question_review",
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
            action: "k1.source_question.discarded",
            object_type: "k1_source_question_review",
            object_id: &public_id,
            object_revision: Some(1),
            note: normalized_note(request.note.as_deref()),
            meta_json: Some(&event_payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    review_by_id(conn, review_id)
}

pub fn list_source_inbox(
    conn: &Connection,
    owner_id: &str,
    limit: i64,
) -> CoreResult<Vec<SourceInboxItem>> {
    required(owner_id, "题库老师")?;
    if !(1..=100).contains(&limit) {
        return Err(CoreError::Invalid(
            "来源收件箱 limit 必须在 1 到 100 之间".into(),
        ));
    }
    let mut statement = conn.prepare(
        "SELECT id FROM k1_source_documents
         WHERE owner_scope='personal' AND owner_id=?1
         ORDER BY created_at DESC,id DESC LIMIT ?2",
    )?;
    let rows = statement.query_map(params![owner_id.trim(), limit], |row| row.get::<_, i64>(0))?;
    let mut items = Vec::new();
    for row in rows {
        let document_id = row?;
        let document = source_document_by_id(conn, document_id)?;
        let latest_run_id = conn
            .query_row(
                "SELECT id FROM k1_source_extraction_runs
                 WHERE source_document_id=?1 ORDER BY id DESC LIMIT 1",
                [document_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        let latest_extraction = latest_run_id
            .map(|id| extraction_by_id(conn, id))
            .transpose()?;
        let latest_ai = conn
            .query_row(
                "SELECT public_id,status,error_meta_json
                 FROM ai_runs
                 WHERE run_type='question_source_extract'
                   AND source_module='knowledge'
                   AND business_ref_type='k1_source_document'
                   AND business_ref_id=?1
                   AND input_artifact_id=?2
                 ORDER BY id DESC LIMIT 1",
                params![&document.public_id, document.source_artifact_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        let (total_drafts, pending_drafts) = conn.query_row(
            "SELECT COUNT(*),
                    SUM(CASE WHEN review.id IS NULL THEN 1 ELSE 0 END)
             FROM k1_source_question_drafts draft
             JOIN k1_source_extraction_runs run ON run.id=draft.extraction_run_id
             LEFT JOIN k1_source_question_reviews review ON review.source_draft_id=draft.id
             WHERE run.source_document_id=?1",
            [document_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                ))
            },
        )?;
        items.push(SourceInboxItem {
            document,
            latest_extraction,
            latest_ai_run_public_id: latest_ai.as_ref().map(|value| value.0.clone()),
            latest_ai_status: latest_ai.as_ref().map(|value| value.1.clone()),
            latest_ai_error_meta_json: latest_ai.and_then(|value| value.2),
            total_drafts,
            pending_drafts,
        });
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge_migrations;
    use crate::source_import::{SourceAnchorDraft, SourcePrivacyResult, SourceQuestionDraft};
    use suite_core::db::repo::ai_runs::{self, NewAiRun};
    use suite_core::db::repo::artifacts::{self, NewArtifact};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

    fn setup() -> Connection {
        let connection = open_in_memory().unwrap();
        run_migrations(&connection, CORE_MIGRATIONS).unwrap();
        run_migrations(&connection, knowledge_migrations()).unwrap();
        connection
    }

    fn artifact(
        connection: &Connection,
        privacy_class: PrivacyClass,
    ) -> suite_core::models::Artifact {
        artifacts::create_or_get(
            connection,
            &NewArtifact {
                kind: ArtifactKind::Document,
                sha256: &"a".repeat(64),
                mime_type: "text/plain",
                byte_size: 12,
                original_name: Some("questions.txt"),
                original_path: None,
                archived_path: "/tmp/archive/k1/questions.txt",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: "k1-source-v1",
                privacy_class,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap()
    }

    fn register(
        connection: &mut Connection,
        artifact: &suite_core::models::Artifact,
    ) -> SourceDocument {
        register_source_document(
            connection,
            &RegisterSourceDocumentRequest {
                request_key: "source-register-1".into(),
                owner_id: "teacher-1".into(),
                source_artifact_public_id: artifact.public_id.clone(),
                source_type: "source_document".into(),
                source_format: "text".into(),
                source_hash: artifact.sha256.clone(),
                page_count: 1,
                extraction_version: "extract-v1".into(),
                created_by: "teacher-1".into(),
            },
        )
        .unwrap()
    }

    fn output(stem: &str) -> SourceExtractionOutput {
        SourceExtractionOutput {
            schema_version: 1,
            state: "ready".into(),
            confidence: 0.99,
            privacy: SourcePrivacyResult {
                schema_version: 1,
                sanitized: true,
                contains_student_identity: false,
                contains_student_answer: false,
                contains_teacher_mark: false,
                contains_score: false,
            },
            issue_codes: Vec::new(),
            drafts: vec![SourceQuestionDraft {
                order_index: 1,
                question_no: Some("1".into()),
                question_type: "short_answer".into(),
                stem: stem.into(),
                material_text: None,
                max_score: 2.0,
                options: Vec::new(),
                source_anchor: SourceAnchorDraft {
                    schema_version: 1,
                    page_no: 1,
                    region: None,
                    line_start: Some(1),
                    line_end: Some(1),
                },
                confidence: 0.99,
            }],
        }
    }

    fn succeeded_run(
        connection: &Connection,
        document: &SourceDocument,
        output: &SourceExtractionOutput,
        key: &str,
    ) -> i64 {
        let json = serde_json::to_string(output).unwrap();
        let run = ai_runs::create_or_get(
            connection,
            &NewAiRun {
                idempotency_key: key,
                run_type: "question_source_extract",
                source_module: "knowledge",
                business_ref_type: "k1_source_document",
                business_ref_id: &document.public_id,
                input_artifact_id: Some(document.source_artifact_id),
                provider: "fake",
                model_name: "fake-model",
                model_version: "v1",
                config_version: "v1",
                prompt_or_rule_version: "v1",
                input_hash: &document.source_hash,
                retry_of_ai_run_id: None,
            },
        )
        .unwrap();
        ai_runs::start(connection, run.id, "2026-07-19T00:00:00Z", None).unwrap();
        ai_runs::finalize_succeeded(
            connection,
            run.id,
            &hashing::sha256_hex(json.as_bytes()),
            Some(output.confidence),
            &json,
            "2026-07-19T00:00:01Z",
        )
        .unwrap();
        run.id
    }

    fn extracted(connection: &mut Connection, stem: &str) -> SourceExtractionRecord {
        let artifact = artifact(connection, PrivacyClass::TeachingContent);
        let document = register(connection, &artifact);
        let output = output(stem);
        let run_id = succeeded_run(connection, &document, &output, "source-ai-1");
        materialize_source_extraction(connection, "teacher-1", &document.public_id, run_id).unwrap()
    }

    #[test]
    fn student_sensitive_artifact_is_rejected_before_source_registration() {
        let mut connection = setup();
        let artifact = artifact(&connection, PrivacyClass::StudentSensitive);
        let error = register_source_document(
            &mut connection,
            &RegisterSourceDocumentRequest {
                request_key: "source-register-1".into(),
                owner_id: "teacher-1".into(),
                source_artifact_public_id: artifact.public_id,
                source_type: "source_document".into(),
                source_format: "text".into(),
                source_hash: artifact.sha256,
                page_count: 1,
                extraction_version: "extract-v1".into(),
                created_by: "teacher-1".into(),
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("学生敏感资产"));
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM k1_source_documents", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn extraction_only_materializes_inbox_drafts() {
        let mut connection = setup();
        let extraction = extracted(&mut connection, "洋务运动前期的口号是什么？");
        assert_eq!(extraction.drafts.len(), 1);
        for table in [
            "k1_questions",
            "k1_question_versions",
            "k1_answer_key_versions",
            "k1_rubric_versions",
        ] {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "{table}");
        }
        let ai_run_id = connection
            .query_row(
                "SELECT ai_run_id FROM k1_source_extraction_runs",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            materialize_source_extraction(
                &mut connection,
                "teacher-1",
                &extraction.source_document_public_id,
                ai_run_id,
            )
            .unwrap(),
            extraction
        );
    }

    #[test]
    fn teacher_accept_creates_only_l0_and_is_idempotent() {
        let mut connection = setup();
        let extraction = extracted(&mut connection, "洋务运动前期的口号是什么？");
        let draft = &extraction.drafts[0];
        let request = ReviewSourceDraftRequest {
            request_key: "source-review-1".into(),
            draft_public_id: draft.public_id.clone(),
            expected_content_hash: draft.content_hash.clone(),
            corrected: None,
            reviewed_by: "teacher-1".into(),
            note: None,
        };
        let first = accept_source_draft(&mut connection, "teacher-1", &request).unwrap();
        let repeated = accept_source_draft(&mut connection, "teacher-1", &request).unwrap();
        assert_eq!(first, repeated);
        assert_eq!(first.result_kind, "draft_created");
        let (quality, state): (String, String) = connection
            .query_row(
                "SELECT quality_level,state FROM k1_question_versions",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((quality.as_str(), state.as_str()), ("L0", "review_pending"));
        for table in [
            "k1_answer_key_versions",
            "k1_rubric_versions",
            "exam_assessments_v2",
        ] {
            let count: i64 = connection
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap_or(0);
            assert_eq!(count, 0, "{table}");
        }
    }

    #[test]
    fn exact_l0_is_reused_without_duplicate_identity() {
        let mut connection = setup();
        let first_extraction = extracted(&mut connection, "洋务运动前期的口号是什么？");
        let first_draft = &first_extraction.drafts[0];
        accept_source_draft(
            &mut connection,
            "teacher-1",
            &ReviewSourceDraftRequest {
                request_key: "source-review-1".into(),
                draft_public_id: first_draft.public_id.clone(),
                expected_content_hash: first_draft.content_hash.clone(),
                corrected: None,
                reviewed_by: "teacher-1".into(),
                note: None,
            },
        )
        .unwrap();

        let document_id: i64 = connection
            .query_row("SELECT id FROM k1_source_documents", [], |row| row.get(0))
            .unwrap();
        let document = source_document_by_id(&connection, document_id).unwrap();
        let mut second_output = output("洋务运动前期的口号是什么？");
        second_output.drafts[0].question_no = Some("01".into());
        let second_run = succeeded_run(&connection, &document, &second_output, "source-ai-2");
        let second_extraction = materialize_source_extraction(
            &mut connection,
            "teacher-1",
            &document.public_id,
            second_run,
        )
        .unwrap();
        let second_draft = &second_extraction.drafts[0];
        let review = accept_source_draft(
            &mut connection,
            "teacher-1",
            &ReviewSourceDraftRequest {
                request_key: "source-review-2".into(),
                draft_public_id: second_draft.public_id.clone(),
                expected_content_hash: second_draft.content_hash.clone(),
                corrected: None,
                reviewed_by: "teacher-1".into(),
                note: None,
            },
        )
        .unwrap();
        assert_eq!(review.result_kind, "exact_reused");
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM k1_questions", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM k1_question_versions", [], |row| row
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            1
        );
    }

    #[test]
    fn discard_writes_review_without_question() {
        let mut connection = setup();
        let extraction = extracted(&mut connection, "无法确认的残缺题干");
        let draft = &extraction.drafts[0];
        let review = discard_source_draft(
            &mut connection,
            "teacher-1",
            &DiscardSourceDraftRequest {
                request_key: "source-discard-1".into(),
                draft_public_id: draft.public_id.clone(),
                expected_content_hash: draft.content_hash.clone(),
                reviewed_by: "teacher-1".into(),
                note: Some("题干残缺".into()),
            },
        )
        .unwrap();
        assert_eq!(review.result_kind, "discarded");
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM k1_questions", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
