//! K1 独立答案资料的一站式归档、AI run 与匹配草稿编排。
//!
//! 文件读取、Office 提取、PDF 渲染和外部调用都在 SQLite 锁外；结果先落 K1
//! 答案匹配草稿，老师确认前不创建答案版本、作业或成绩。

use std::path::{Path, PathBuf};

use module_exam::answer_source_recognition::{
    AnswerSourceFailure, AnswerSourceItemSpec, AnswerSourceQuestionType,
    AnswerSourceRecognitionOutput, AnswerSourceRecognitionRequest, AnswerSourceRecognizer,
    AnswerSourceRecognizerDescriptor, AnswerSourceVisualPage,
};
use module_knowledge::db::answer_sources::{
    self, AnswerExtractionRecord, AnswerSourceDocument, AnswerTargetSpec,
    RegisterAnswerSourceRequest, ANSWER_EXTRACTION_VERSION,
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::ai_runs;
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRun, AiRunStatus, ArtifactKind, PrivacyClass};

use crate::{exam_intake, pdf_pages};

const ANSWER_ARCHIVE_VERSION: &str = "k1-answer-source-original-v1";
const SOURCE_IMAGE_VERSION: &str = "k1-answer-source-jpeg-v1";
const PDF_VISUALIZATION_VERSION: &str = "macos-coregraphics-gray-jpeg-1800-v1";
const MAX_SOURCE_BYTES: usize = 25 * 1024 * 1024;
const MAX_SOURCE_TEXT_CHARS: usize = 200_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportKnowledgeAnswerRequest {
    pub path: String,
    pub question_source_document_public_id: String,
    pub request_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeAnswerFailure {
    pub schema_version: i64,
    pub code: String,
    pub safe_message: String,
    pub retryable: bool,
}

impl From<AnswerSourceFailure> for KnowledgeAnswerFailure {
    fn from(value: AnswerSourceFailure) -> Self {
        Self {
            schema_version: value.schema_version,
            code: serde_json::to_value(value.code)
                .ok()
                .and_then(|code| code.as_str().map(str::to_owned))
                .unwrap_or_else(|| "internal".into()),
            safe_message: value.safe_message,
            retryable: value.retryable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeAnswerAnalysisResult {
    pub document: AnswerSourceDocument,
    pub ai_run_id: i64,
    pub status: String,
    pub extraction: Option<AnswerExtractionRecord>,
    pub failure: Option<KnowledgeAnswerFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnswerFormat {
    Jpeg,
    Pdf,
    Text,
    Docx,
    Xlsx,
}

impl AnswerFormat {
    fn from_path(path: &Path) -> CoreResult<Self> {
        match path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            "pdf" => Ok(Self::Pdf),
            "txt" => Ok(Self::Text),
            "docx" => Ok(Self::Docx),
            "xlsx" => Ok(Self::Xlsx),
            _ => Err(CoreError::Invalid(
                "答案资料只支持 JPG、PDF、TXT、Word(.docx) 或 Excel(.xlsx)".into(),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Pdf => "pdf",
            Self::Text => "text",
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Pdf => "pdf",
            Self::Text => "txt",
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
        }
    }

    fn mime_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Pdf => "application/pdf",
            Self::Text => "text/plain",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }
    }

    fn artifact_kind(self) -> ArtifactKind {
        if self == Self::Jpeg {
            ArtifactKind::Image
        } else {
            ArtifactKind::Document
        }
    }
}

#[derive(Debug)]
pub struct PreparedKnowledgeAnswer {
    path: PathBuf,
    original_name: String,
    format: AnswerFormat,
    bytes: Vec<u8>,
    source_hash: String,
    page_count: i64,
    source_text: Option<String>,
    text_extraction_version: Option<String>,
    visualization_version: Option<String>,
    visual_pages: Vec<AnswerSourceVisualPage>,
}

fn capped_text(text: String) -> CoreResult<String> {
    if text.trim().is_empty() {
        return Err(CoreError::Invalid("答案资料没有可提取文字".into()));
    }
    if text.chars().count() > MAX_SOURCE_TEXT_CHARS {
        return Err(CoreError::Invalid(
            "答案资料文字过长，请拆成较小文件后再导入".into(),
        ));
    }
    Ok(text)
}

pub fn prepare(path: &str) -> CoreResult<PreparedKnowledgeAnswer> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(CoreError::Io(format!("文件不存在：{}", path.display())));
    }
    let format = AnswerFormat::from_path(&path)?;
    let bytes = std::fs::read(&path)
        .map_err(|error| CoreError::Io(format!("读取答案资料失败：{error}")))?;
    if bytes.is_empty() {
        return Err(CoreError::Invalid("答案资料文件不能为空".into()));
    }
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(CoreError::Invalid("答案资料文件不能超过 25 MB".into()));
    }
    let source_hash = hashing::sha256_hex(&bytes);
    let (page_count, source_text, text_extraction_version, visualization_version, visual_pages) =
        match format {
            AnswerFormat::Jpeg => {
                image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg)
                    .map_err(|_| CoreError::Invalid("答案图片损坏或不是有效 JPEG".into()))?;
                (
                    1,
                    None,
                    None,
                    Some(SOURCE_IMAGE_VERSION.into()),
                    vec![AnswerSourceVisualPage {
                        page_no: 1,
                        mime_type: "image/jpeg".into(),
                        sha256: source_hash.clone(),
                        bytes: bytes.clone(),
                    }],
                )
            }
            AnswerFormat::Pdf => {
                let rendered = pdf_pages::render_to_jpegs(&path)?;
                if rendered.is_empty() {
                    return Err(CoreError::Invalid("答案 PDF 没有可读取页面".into()));
                }
                if rendered.len()
                    > module_exam::answer_source_recognition::ANSWER_SOURCE_MAX_VISUAL_PAGES
                {
                    return Err(CoreError::Invalid(format!(
                        "答案 PDF 单次最多 {} 页，请拆分后重试",
                        module_exam::answer_source_recognition::ANSWER_SOURCE_MAX_VISUAL_PAGES
                    )));
                }
                let pages = rendered
                    .into_iter()
                    .enumerate()
                    .map(|(index, bytes)| AnswerSourceVisualPage {
                        page_no: index as i64 + 1,
                        mime_type: "image/jpeg".into(),
                        sha256: hashing::sha256_hex(&bytes),
                        bytes,
                    })
                    .collect::<Vec<_>>();
                (
                    pages.len() as i64,
                    None,
                    None,
                    Some(PDF_VISUALIZATION_VERSION.into()),
                    pages,
                )
            }
            AnswerFormat::Text => (
                1,
                Some(capped_text(String::from_utf8(bytes.clone()).map_err(
                    |_| CoreError::Invalid("TXT 答案资料必须是 UTF-8".into()),
                )?)?),
                None,
                None,
                Vec::new(),
            ),
            AnswerFormat::Docx => (
                1,
                Some(capped_text(crate::office_answers::extract_docx(&bytes)?)?),
                Some(crate::office_answers::DOCX_EXTRACTION_VERSION.into()),
                None,
                Vec::new(),
            ),
            AnswerFormat::Xlsx => (
                1,
                Some(capped_text(crate::office_answers::extract_xlsx(&bytes)?)?),
                Some(crate::office_answers::XLSX_EXTRACTION_VERSION.into()),
                None,
                Vec::new(),
            ),
        };
    let original_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("答案资料")
        .to_owned();
    Ok(PreparedKnowledgeAnswer {
        path,
        original_name,
        format,
        bytes,
        source_hash,
        page_count,
        source_text,
        text_extraction_version,
        visualization_version,
        visual_pages,
    })
}

pub fn persist(
    conn: &mut Connection,
    data_dir: &Path,
    prepared: &PreparedKnowledgeAnswer,
    question_source_document_public_id: &str,
    request_key: &str,
) -> CoreResult<AnswerSourceDocument> {
    if request_key.trim().is_empty() {
        return Err(CoreError::Invalid("答案资料请求键不能为空".into()));
    }
    let archive_dir = data_dir.join("archive").join("k1").join("answers");
    let archived = exam_intake::archive_bytes(
        &prepared.bytes,
        &prepared.source_hash,
        prepared.format.extension(),
        &archive_dir,
    )?;
    let artifact = exam_intake::register_artifact(
        conn,
        &archived,
        &prepared.source_hash,
        prepared.bytes.len() as i64,
        prepared.format.artifact_kind(),
        prepared.format.mime_type(),
        Some(&prepared.original_name),
        Some(&prepared.path),
        None,
        None,
        ANSWER_ARCHIVE_VERSION,
        PrivacyClass::TeachingContent,
    )?;
    answer_sources::register_answer_source_document(
        conn,
        &RegisterAnswerSourceRequest {
            request_key: format!("{}:register", request_key.trim()),
            owner_id: "local_teacher".into(),
            question_source_document_public_id: question_source_document_public_id.into(),
            source_artifact_public_id: artifact.public_id,
            source_format: prepared.format.as_str().into(),
            source_hash: prepared.source_hash.clone(),
            page_count: prepared.page_count,
            extraction_version: ANSWER_EXTRACTION_VERSION.into(),
            created_by: "local_teacher".into(),
        },
    )
}

pub struct KnowledgeAnswerRunInput {
    document: AnswerSourceDocument,
    source_bytes: Vec<u8>,
    source_text: Option<String>,
    text_extraction_version: Option<String>,
    visualization_version: Option<String>,
    visual_pages: Vec<AnswerSourceVisualPage>,
    items: Vec<AnswerSourceItemSpec>,
    input_hash: String,
}

impl KnowledgeAnswerRunInput {
    pub fn request(&self) -> AnswerSourceRecognitionRequest<'_> {
        AnswerSourceRecognitionRequest {
            ingest_batch_id: self.document.id,
            source_artifact_id: self.document.source_artifact_id,
            source_artifact_sha256: &self.document.source_hash,
            source_format: &self.document.source_format,
            mime_type: match self.document.source_format.as_str() {
                "jpeg" => "image/jpeg",
                "pdf" => "application/pdf",
                "text" => "text/plain",
                "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                _ => "application/octet-stream",
            },
            source_bytes: &self.source_bytes,
            source_text: self.source_text.as_deref(),
            text_extraction_version: self.text_extraction_version.as_deref(),
            visualization_version: self.visualization_version.as_deref(),
            visual_pages: &self.visual_pages,
            items: &self.items,
        }
    }
}

fn item_spec(target: AnswerTargetSpec) -> CoreResult<AnswerSourceItemSpec> {
    let question_type = AnswerSourceQuestionType::from_db(&target.question_type)
        .ok_or_else(|| CoreError::Invalid("答案资料包含当前不支持的题型".into()))?;
    Ok(AnswerSourceItemSpec {
        assessment_item_id: target.question_version_id,
        order_index: target.order_index - 1,
        question_no: target.question_no,
        question_type,
        stem: target.stem,
        max_score: target.max_score,
    })
}

pub fn build_input(
    conn: &Connection,
    prepared: &PreparedKnowledgeAnswer,
    document: AnswerSourceDocument,
) -> CoreResult<KnowledgeAnswerRunInput> {
    if document.source_hash != prepared.source_hash
        || document.source_format != prepared.format.as_str()
    {
        return Err(CoreError::Invalid(
            "答案资料归档身份与当前文件不一致".into(),
        ));
    }
    let items = answer_sources::answer_target_specs(
        conn,
        "local_teacher",
        &document.question_source_document_public_id,
    )?
    .into_iter()
    .map(item_spec)
    .collect::<CoreResult<Vec<_>>>()?;
    let mut input = KnowledgeAnswerRunInput {
        document,
        source_bytes: prepared.bytes.clone(),
        source_text: prepared.source_text.clone(),
        text_extraction_version: prepared.text_extraction_version.clone(),
        visualization_version: prepared.visualization_version.clone(),
        visual_pages: prepared.visual_pages.clone(),
        items,
        input_hash: String::new(),
    };
    input.input_hash = input.request().input_hash()?;
    Ok(input)
}

pub enum BeginKnowledgeAnswerRun {
    Execute { ai_run_id: i64 },
    Completed { ai_run_id: i64 },
}

pub fn begin(
    conn: &Connection,
    input: &KnowledgeAnswerRunInput,
    descriptor: &AnswerSourceRecognizerDescriptor,
    request_key: &str,
) -> CoreResult<BeginKnowledgeAnswerRun> {
    descriptor.validate()?;
    if request_key.trim().is_empty() {
        return Err(CoreError::Invalid("答案结构化幂等键不能为空".into()));
    }
    if let Some(ai_run_id) = conn
        .query_row(
            "SELECT id FROM ai_runs
             WHERE run_type='answer_source_structure' AND source_module='knowledge'
               AND business_ref_type='k1_answer_source_document' AND business_ref_id=?1
               AND input_artifact_id=?2 AND provider=?3 AND model_name=?4
               AND model_version=?5 AND config_version=?6 AND prompt_or_rule_version=?7
               AND input_hash=?8 AND status='succeeded'
             ORDER BY id DESC LIMIT 1",
            rusqlite::params![
                &input.document.public_id,
                input.document.source_artifact_id,
                &descriptor.provider,
                &descriptor.model_name,
                &descriptor.model_version,
                &descriptor.config_version,
                &descriptor.rule_version,
                &input.input_hash,
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        return Ok(BeginKnowledgeAnswerRun::Completed { ai_run_id });
    }
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: &format!("{}:analyze", request_key.trim()),
            run_type: "answer_source_structure",
            source_module: "knowledge",
            business_ref_type: "k1_answer_source_document",
            business_ref_id: &input.document.public_id,
            input_artifact_id: Some(input.document.source_artifact_id),
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
            Ok(BeginKnowledgeAnswerRun::Execute { ai_run_id: run.id })
        }
        AiRunStatus::Succeeded | AiRunStatus::Failed => {
            Ok(BeginKnowledgeAnswerRun::Completed { ai_run_id: run.id })
        }
        AiRunStatus::Processing => Err(CoreError::Invalid(
            "该答案资料正在识别，请勿重复提交".into(),
        )),
        AiRunStatus::Voided => Err(CoreError::Invalid(
            "该答案识别运行已作废，请重新选择文件".into(),
        )),
    }
}

pub fn finish(
    conn: &Connection,
    input: &KnowledgeAnswerRunInput,
    ai_run_id: i64,
    result: Result<AnswerSourceRecognitionOutput, AnswerSourceFailure>,
) -> CoreResult<()> {
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
    Ok(())
}

fn run_failure(run: &AiRun) -> CoreResult<KnowledgeAnswerFailure> {
    let failure: AnswerSourceFailure = serde_json::from_str(
        run.error_meta_json
            .as_deref()
            .ok_or_else(|| CoreError::Invalid("失败答案 run 缺少错误".into()))?,
    )
    .map_err(|error| CoreError::Parse(format!("答案识别错误记录损坏：{error}")))?;
    failure.validate()?;
    Ok(failure.into())
}

pub fn materialize_result(
    conn: &mut Connection,
    document: AnswerSourceDocument,
    ai_run_id: i64,
) -> CoreResult<KnowledgeAnswerAnalysisResult> {
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    match run.status {
        AiRunStatus::Succeeded => {
            let extraction = answer_sources::materialize_answer_extraction(
                conn,
                "local_teacher",
                &document.public_id,
                ai_run_id,
            )?;
            Ok(KnowledgeAnswerAnalysisResult {
                document,
                ai_run_id,
                status: "succeeded".into(),
                extraction: Some(extraction),
                failure: None,
            })
        }
        AiRunStatus::Failed => Ok(KnowledgeAnswerAnalysisResult {
            document,
            ai_run_id,
            status: "failed".into(),
            extraction: None,
            failure: Some(run_failure(&run)?),
        }),
        _ => Err(CoreError::Invalid("答案结构化尚未形成结果".into())),
    }
}

pub fn recognize(
    recognizer: &dyn AnswerSourceRecognizer,
    input: &KnowledgeAnswerRunInput,
) -> Result<AnswerSourceRecognitionOutput, AnswerSourceFailure> {
    recognizer.recognize(&input.request())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn rejects_unsupported_and_invalid_image_before_archiving() {
        let root =
            std::env::temp_dir().join(format!("jiaofu-k1-answer-prepare-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let unsupported = root.join("answers.png");
        fs::write(&unsupported, b"png").unwrap();
        assert!(prepare(unsupported.to_str().unwrap())
            .unwrap_err()
            .to_string()
            .contains("只支持"));
        let broken = root.join("answers.jpg");
        fs::write(&broken, b"broken").unwrap();
        assert!(prepare(broken.to_str().unwrap())
            .unwrap_err()
            .to_string()
            .contains("损坏"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn text_answer_is_loaded_without_student_or_bound_answer_context() {
        let root =
            std::env::temp_dir().join(format!("jiaofu-k1-answer-text-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("answers.txt");
        fs::write(&path, "1.A\n2.正确").unwrap();
        let prepared = prepare(path.to_str().unwrap()).unwrap();
        assert_eq!(prepared.format, AnswerFormat::Text);
        assert_eq!(prepared.page_count, 1);
        assert_eq!(prepared.source_text.as_deref(), Some("1.A\n2.正确"));
        assert!(prepared.visual_pages.is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
