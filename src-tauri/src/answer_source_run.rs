//! 答案图片/文本结构化 run 编排。
//!
//! 文件读取与外部模型调用在 SQLite 锁外；成功输出先进入不可变 ai_run，再幂等落为
//! `ai_draft` 逐题候选。老师确认由 module-exam 的答案资料服务单独处理。

use std::path::PathBuf;

use module_exam::answer_source_recognition::{
    AnswerSourceFailure, AnswerSourceItemSpec, AnswerSourceQuestionType,
    AnswerSourceRecognitionOutput, AnswerSourceRecognitionRequest,
    AnswerSourceRecognizerDescriptor, AnswerSourceVisualPage,
};
use module_exam::service::answer_source::{self, AnswerSourceReviewSummary};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use suite_core::db::repo::{ai_runs, artifacts};
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRun, AiRunStatus, ArchiveStatus, PrivacyClass};

use crate::pdf_pages;

const SOURCE_IMAGE_VERSION: &str = "answer-source-original-jpeg-v1";
const PDF_VISUALIZATION_VERSION: &str = "macos-coregraphics-gray-jpeg-1800-v1";

pub struct AnswerSourceRunMetadata {
    ingest_batch_id: i64,
    source_artifact_id: i64,
    source_artifact_sha256: String,
    source_format: String,
    mime_type: String,
    archived_path: PathBuf,
    items: Vec<AnswerSourceItemSpec>,
}

pub struct AnswerSourceRunInput {
    ingest_batch_id: i64,
    source_artifact_id: i64,
    source_artifact_sha256: String,
    source_format: String,
    mime_type: String,
    source_bytes: Vec<u8>,
    source_text: Option<String>,
    text_extraction_version: Option<String>,
    visualization_version: Option<String>,
    visual_pages: Vec<AnswerSourceVisualPage>,
    items: Vec<AnswerSourceItemSpec>,
    input_hash: String,
}

impl AnswerSourceRunInput {
    pub fn request(&self) -> AnswerSourceRecognitionRequest<'_> {
        AnswerSourceRecognitionRequest {
            ingest_batch_id: self.ingest_batch_id,
            source_artifact_id: self.source_artifact_id,
            source_artifact_sha256: &self.source_artifact_sha256,
            source_format: &self.source_format,
            mime_type: &self.mime_type,
            source_bytes: &self.source_bytes,
            source_text: self.source_text.as_deref(),
            text_extraction_version: self.text_extraction_version.as_deref(),
            visualization_version: self.visualization_version.as_deref(),
            visual_pages: &self.visual_pages,
            items: &self.items,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AnswerSourceRunResult {
    pub ai_run_id: i64,
    pub status: String,
    pub output: Option<AnswerSourceRecognitionOutput>,
    pub failure: Option<AnswerSourceFailure>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSourceAnalysisResult {
    pub run: AnswerSourceRunResult,
    pub review: Option<AnswerSourceReviewSummary>,
}

pub enum BeginAnswerSourceRun {
    Execute { ai_run_id: i64 },
    Completed(Box<AnswerSourceRunResult>),
}

pub fn load_metadata(conn: &Connection, batch_id: i64) -> CoreResult<AnswerSourceRunMetadata> {
    if batch_id <= 0 {
        return Err(CoreError::Invalid("答案资料批次 id 必须为正数".into()));
    }
    let (artifact_id, registered_format, assessment_version_id): (i64, String, i64) = conn
        .query_row(
            "SELECT d.source_artifact_id,d.source_format,b.assessment_version_id
             FROM exam_fixed_input_documents_v2 d
             JOIN exam_ingest_batches_v2 b ON b.id=d.ingest_batch_id
             WHERE d.ingest_batch_id=?1 AND d.document_role='answer_source'
               AND d.state<>'voided'",
            [batch_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("本批没有答案资料".into()))?;
    let artifact = artifacts::get_by_id(conn, artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{artifact_id}")))?;
    if artifact.archive_status != ArchiveStatus::Ready
        || artifact.privacy_class != PrivacyClass::TeachingContent
    {
        return Err(CoreError::Invalid(
            "答案资料必须已归档为 teaching_content".into(),
        ));
    }
    if !matches!(registered_format.as_str(), "jpeg" | "pdf" | "text") {
        return Err(CoreError::Invalid(
            "答案资料登记格式只支持 JPG、PDF 或文本型文档".into(),
        ));
    }
    let source_format = match (registered_format.as_str(), artifact.mime_type.as_str()) {
        ("text", "application/vnd.openxmlformats-officedocument.wordprocessingml.document") => {
            "docx".to_string()
        }
        ("text", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet") => {
            "xlsx".to_string()
        }
        _ => registered_format,
    };
    let mut stmt = conn.prepare(
        "SELECT i.id,i.order_index,
                COALESCE(json_extract(i.presentation_snapshot_json,'$.question_no'),CAST(i.order_index+1 AS TEXT)),
                q.question_type,q.stem,i.score
         FROM exam_assessment_items_v2 i
         JOIN k1_question_versions q ON q.id=i.question_version_id
         WHERE i.assessment_version_id=?1 AND i.state='active'
         ORDER BY i.order_index,i.id",
    )?;
    let rows = stmt.query_map([assessment_version_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, f64>(5)?,
        ))
    })?;
    let mut items = Vec::new();
    for row in rows {
        let (assessment_item_id, order_index, question_no, question_type, stem, max_score) = row?;
        let question_type = AnswerSourceQuestionType::from_db(&question_type)
            .ok_or_else(|| CoreError::Invalid("答案资料包含当前不支持的题型".into()))?;
        items.push(AnswerSourceItemSpec {
            assessment_item_id,
            order_index,
            question_no,
            question_type,
            stem,
            max_score,
        });
    }
    if items.is_empty() {
        return Err(CoreError::Invalid("当前作业没有可匹配题目".into()));
    }
    Ok(AnswerSourceRunMetadata {
        ingest_batch_id: batch_id,
        source_artifact_id: artifact.id,
        source_artifact_sha256: artifact.sha256,
        source_format,
        mime_type: artifact.mime_type,
        archived_path: artifact.archived_path.into(),
        items,
    })
}

pub fn load_input(metadata: AnswerSourceRunMetadata) -> CoreResult<AnswerSourceRunInput> {
    let source_bytes = std::fs::read(&metadata.archived_path)
        .map_err(|_| CoreError::Invalid("答案资料归档缺失或不可读".into()))?;
    if hashing::sha256_hex(&source_bytes) != metadata.source_artifact_sha256 {
        return Err(CoreError::Invalid("答案资料归档与登记 hash 不一致".into()));
    }
    let (source_text, text_extraction_version, visualization_version, visual_pages) =
        match metadata.source_format.as_str() {
            "text" => (
                Some(
                    String::from_utf8(source_bytes.clone())
                        .map_err(|_| CoreError::Invalid("文本答案资料必须是 UTF-8".into()))?,
                ),
                None,
                None,
                vec![],
            ),
            "docx" => (
                Some(crate::office_answers::extract_docx(&source_bytes)?),
                Some(crate::office_answers::DOCX_EXTRACTION_VERSION.into()),
                None,
                vec![],
            ),
            "xlsx" => (
                Some(crate::office_answers::extract_xlsx(&source_bytes)?),
                Some(crate::office_answers::XLSX_EXTRACTION_VERSION.into()),
                None,
                vec![],
            ),
            "jpeg" => (
                None,
                None,
                Some(SOURCE_IMAGE_VERSION.into()),
                vec![AnswerSourceVisualPage {
                    page_no: 1,
                    mime_type: "image/jpeg".into(),
                    sha256: metadata.source_artifact_sha256.clone(),
                    bytes: source_bytes.clone(),
                }],
            ),
            "pdf" => {
                let rendered = pdf_pages::render_to_jpegs(&metadata.archived_path)?;
                let pages = rendered
                    .into_iter()
                    .enumerate()
                    .map(|(index, bytes)| AnswerSourceVisualPage {
                        page_no: (index + 1) as i64,
                        mime_type: "image/jpeg".into(),
                        sha256: hashing::sha256_hex(&bytes),
                        bytes,
                    })
                    .collect();
                (None, None, Some(PDF_VISUALIZATION_VERSION.into()), pages)
            }
            _ => return Err(CoreError::Invalid("答案资料格式不受支持".into())),
        };
    let mut input = AnswerSourceRunInput {
        ingest_batch_id: metadata.ingest_batch_id,
        source_artifact_id: metadata.source_artifact_id,
        source_artifact_sha256: metadata.source_artifact_sha256,
        source_format: metadata.source_format,
        mime_type: metadata.mime_type,
        source_bytes,
        source_text,
        text_extraction_version,
        visualization_version,
        visual_pages,
        items: metadata.items,
        input_hash: String::new(),
    };
    input.input_hash = input.request().input_hash()?;
    Ok(input)
}

pub fn begin(
    conn: &Connection,
    input: &AnswerSourceRunInput,
    descriptor: &AnswerSourceRecognizerDescriptor,
    idempotency_key: &str,
) -> CoreResult<BeginAnswerSourceRun> {
    descriptor.validate()?;
    if idempotency_key.trim().is_empty() {
        return Err(CoreError::Invalid("答案结构化幂等键不能为空".into()));
    }
    let business_ref_id = input.ingest_batch_id.to_string();
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: idempotency_key.trim(),
            run_type: "answer_source_structure",
            source_module: "exam",
            business_ref_type: "fixed_answer_source",
            business_ref_id: &business_ref_id,
            input_artifact_id: Some(input.source_artifact_id),
            provider: &descriptor.provider,
            model_name: &descriptor.model_name,
            model_version: &descriptor.model_version,
            config_version: &descriptor.config_version,
            prompt_or_rule_version: &descriptor.rule_version,
            input_hash: &input.input_hash,
            retry_of_ai_run_id: None,
        },
    )?;
    match run.status {
        AiRunStatus::Pending => {
            ai_runs::start(conn, run.id, &time::utc_now_rfc3339(), None)?;
            Ok(BeginAnswerSourceRun::Execute { ai_run_id: run.id })
        }
        AiRunStatus::Succeeded | AiRunStatus::Failed => Ok(BeginAnswerSourceRun::Completed(
            Box::new(result_from_run(input, &run)?),
        )),
        AiRunStatus::Processing => Err(CoreError::Invalid("答案资料正在整理，请勿重复提交".into())),
        AiRunStatus::Voided => Err(CoreError::Invalid(
            "答案结构化 run 已作废，请使用新幂等键".into(),
        )),
    }
}

pub fn finish(
    conn: &Connection,
    input: &AnswerSourceRunInput,
    ai_run_id: i64,
    result: Result<AnswerSourceRecognitionOutput, AnswerSourceFailure>,
) -> CoreResult<AnswerSourceRunResult> {
    let finished_at = time::utc_now_rfc3339();
    match result {
        Ok(output) => match output.to_json_against(&input.request()) {
            Ok(output_json) => {
                ai_runs::finalize_succeeded(
                    conn,
                    ai_run_id,
                    &hashing::sha256_hex(output_json.as_bytes()),
                    Some(output.confidence),
                    &output_json,
                    &finished_at,
                )?;
            }
            Err(_) => {
                let failure = AnswerSourceFailure {
                    schema_version: 1,
                    code:
                        module_exam::answer_source_recognition::AnswerSourceErrorCode::InvalidOutput,
                    safe_message: "答案结构化结果未通过安全校验，已阻断等待重试".into(),
                    retryable: false,
                };
                ai_runs::finalize_failed(conn, ai_run_id, &failure.to_json()?, &finished_at)?;
            }
        },
        Err(failure) => {
            ai_runs::finalize_failed(conn, ai_run_id, &failure.to_json()?, &finished_at)?;
        }
    }
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    result_from_run(input, &run)
}

fn result_from_run(input: &AnswerSourceRunInput, run: &AiRun) -> CoreResult<AnswerSourceRunResult> {
    match run.status {
        AiRunStatus::Succeeded => {
            let output: AnswerSourceRecognitionOutput = serde_json::from_str(
                run.output_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("答案结构化成功 run 缺少输出".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("答案结构化 run 输出无法解析：{error}")))?;
            output.validate_against(&input.request())?;
            Ok(AnswerSourceRunResult {
                ai_run_id: run.id,
                status: "succeeded".into(),
                output: Some(output),
                failure: None,
            })
        }
        AiRunStatus::Failed => {
            let failure: AnswerSourceFailure = serde_json::from_str(
                run.error_meta_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("答案结构化失败 run 缺少错误".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("答案结构化错误无法解析：{error}")))?;
            failure.validate()?;
            Ok(AnswerSourceRunResult {
                ai_run_id: run.id,
                status: "failed".into(),
                output: None,
                failure: Some(failure),
            })
        }
        _ => Err(CoreError::Invalid("答案结构化 run 尚未形成结果".into())),
    }
}

pub fn materialize_and_review(
    conn: &Connection,
    result: AnswerSourceRunResult,
) -> CoreResult<AnswerSourceAnalysisResult> {
    let review = if let Some(output) = result.output.as_ref() {
        answer_source::materialize_ai_drafts(conn, output, result.ai_run_id)?;
        Some(answer_source::review_summary(
            conn,
            output.ingest_batch_id,
            result.ai_run_id,
        )?)
    } else {
        None
    };
    Ok(AnswerSourceAnalysisResult {
        run: result,
        review,
    })
}
