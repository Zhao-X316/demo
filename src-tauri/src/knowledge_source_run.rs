//! 独立题目来源的一站式导入与 AI run 编排。
//!
//! 文件读取、Office 解析、PDF 渲染和外部调用均在 SQLite 锁外完成。原文件先按
//! teaching_content 归档；AI 成功后只落入 K1 来源草稿箱，老师确认前不创建题目。

use std::path::{Path, PathBuf};

use module_knowledge::db::source_documents::{
    self, RegisterSourceDocumentRequest, SourceDocument, SourceExtractionRecord,
};
use module_knowledge::source_import::{
    validate_input, validate_output, SourceExtractionInput, SourceExtractionOutput,
    SourceQuestionRecognizer, SourceVisualPage, MAX_VISUAL_PAGES, SOURCE_EXTRACTION_SCHEMA_VERSION,
    SOURCE_EXTRACTION_VERSION,
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::ai_runs;
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRun, AiRunStatus, ArtifactKind, PrivacyClass};

use crate::exam_intake;
use crate::pdf_pages;

const SOURCE_ARCHIVE_VERSION: &str = "k1-source-original-v1";
const SOURCE_INPUT_HASH_VERSION: &str = "k1-source-input-hash-v1";
const MAX_SOURCE_BYTES: usize = 25 * 1024 * 1024;
const MAX_SOURCE_TEXT_CHARS: usize = 200_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSourceRequest {
    pub path: String,
    pub source_type: String,
    pub request_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRunFailure {
    #[serde(alias = "schema_version")]
    pub schema_version: i64,
    pub code: String,
    #[serde(alias = "safe_message")]
    pub safe_message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceImportAnalysisResult {
    pub document: SourceDocument,
    pub ai_run_id: i64,
    pub status: String,
    pub extraction: Option<SourceExtractionRecord>,
    pub failure: Option<SourceRunFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceFormat {
    Jpeg,
    Pdf,
    Text,
    Docx,
    Xlsx,
}

impl SourceFormat {
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
                "题目来源只支持 JPG、PDF、TXT、Word(.docx) 或 Excel(.xlsx)".into(),
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

pub struct PreparedSource {
    path: PathBuf,
    original_name: String,
    format: SourceFormat,
    bytes: Vec<u8>,
    source_hash: String,
    page_count: i64,
    extracted_text: Option<String>,
    visual_pages: Vec<SourceVisualPage>,
}

impl PreparedSource {
    pub fn extraction_input(&self, document: &SourceDocument) -> SourceExtractionInput {
        SourceExtractionInput {
            schema_version: SOURCE_EXTRACTION_SCHEMA_VERSION,
            source_document_public_id: document.public_id.clone(),
            source_type: document.source_type.clone(),
            source_format: document.source_format.clone(),
            source_hash: document.source_hash.clone(),
            page_count: document.page_count,
            extracted_text: self.extracted_text.clone(),
            visual_pages: self.visual_pages.clone(),
        }
    }
}

fn ensure_source_type(value: &str) -> CoreResult<()> {
    if matches!(value, "blank_paper" | "source_document") {
        Ok(())
    } else {
        Err(CoreError::Invalid("题目来源类型非法".into()))
    }
}

fn capped_text(text: String) -> CoreResult<String> {
    if text.trim().is_empty() {
        return Err(CoreError::Invalid("题目来源没有可提取文字".into()));
    }
    if text.chars().count() > MAX_SOURCE_TEXT_CHARS {
        return Err(CoreError::Invalid(
            "题目来源文字过长，请拆成较小文件后再导入".into(),
        ));
    }
    Ok(text)
}

pub fn prepare_source(path: &str, source_type: &str) -> CoreResult<PreparedSource> {
    ensure_source_type(source_type)?;
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(CoreError::Io(format!("文件不存在：{}", path.display())));
    }
    let format = SourceFormat::from_path(&path)?;
    let bytes = std::fs::read(&path)
        .map_err(|error| CoreError::Io(format!("读取题目来源失败：{error}")))?;
    if bytes.is_empty() {
        return Err(CoreError::Invalid("题目来源文件不能为空".into()));
    }
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(CoreError::Invalid("题目来源文件不能超过 25 MB".into()));
    }
    let source_hash = hashing::sha256_hex(&bytes);
    let (page_count, extracted_text, visual_pages) = match format {
        SourceFormat::Jpeg => {
            image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg)
                .map_err(|_| CoreError::Invalid("题目来源图片损坏或不是有效 JPEG".into()))?;
            (
                1,
                None,
                vec![SourceVisualPage {
                    page_no: 1,
                    mime_type: "image/jpeg".into(),
                    bytes: bytes.clone(),
                }],
            )
        }
        SourceFormat::Pdf => {
            let rendered = pdf_pages::render_to_jpegs(&path)?;
            if rendered.is_empty() {
                return Err(CoreError::Invalid("PDF 没有可读取页面".into()));
            }
            if rendered.len() > MAX_VISUAL_PAGES {
                return Err(CoreError::Invalid(format!(
                    "单次最多导入 {MAX_VISUAL_PAGES} 页 PDF，请拆分后重试"
                )));
            }
            let pages = rendered
                .into_iter()
                .enumerate()
                .map(|(index, bytes)| SourceVisualPage {
                    page_no: (index + 1) as i64,
                    mime_type: "image/jpeg".into(),
                    bytes,
                })
                .collect::<Vec<_>>();
            (pages.len() as i64, None, pages)
        }
        SourceFormat::Text => (
            1,
            Some(capped_text(String::from_utf8(bytes.clone()).map_err(
                |_| CoreError::Invalid("TXT 题目来源必须是 UTF-8".into()),
            )?)?),
            Vec::new(),
        ),
        SourceFormat::Docx => (
            1,
            Some(capped_text(crate::office_answers::extract_docx(&bytes)?)?),
            Vec::new(),
        ),
        SourceFormat::Xlsx => (
            1,
            Some(capped_text(crate::office_answers::extract_xlsx(&bytes)?)?),
            Vec::new(),
        ),
    };
    let original_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("题目来源")
        .to_owned();
    Ok(PreparedSource {
        path,
        original_name,
        format,
        bytes,
        source_hash,
        page_count,
        extracted_text,
        visual_pages,
    })
}

pub fn persist_source(
    conn: &mut Connection,
    data_dir: &Path,
    prepared: &PreparedSource,
    source_type: &str,
    request_key: &str,
) -> CoreResult<SourceDocument> {
    ensure_source_type(source_type)?;
    if request_key.trim().is_empty() {
        return Err(CoreError::Invalid("题目来源请求键不能为空".into()));
    }
    let archive_dir = data_dir.join("archive").join("k1").join("sources");
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
        SOURCE_ARCHIVE_VERSION,
        PrivacyClass::TeachingContent,
    )?;
    source_documents::register_source_document(
        conn,
        &RegisterSourceDocumentRequest {
            request_key: format!("{}:register", request_key.trim()),
            owner_id: "local_teacher".into(),
            source_artifact_public_id: artifact.public_id,
            source_type: source_type.into(),
            source_format: prepared.format.as_str().into(),
            source_hash: prepared.source_hash.clone(),
            page_count: prepared.page_count,
            extraction_version: SOURCE_EXTRACTION_VERSION.into(),
            created_by: "local_teacher".into(),
        },
    )
}

#[derive(Serialize)]
struct InputHash<'a> {
    schema_version: i64,
    hash_version: &'a str,
    source_document_public_id: &'a str,
    source_type: &'a str,
    source_format: &'a str,
    source_hash: &'a str,
    page_count: i64,
    extracted_text_hash: Option<String>,
    visual_page_hashes: Vec<(i64, String, String)>,
}

fn input_hash(input: &SourceExtractionInput) -> CoreResult<String> {
    validate_input(input)?;
    let value = InputHash {
        schema_version: SOURCE_EXTRACTION_SCHEMA_VERSION,
        hash_version: SOURCE_INPUT_HASH_VERSION,
        source_document_public_id: &input.source_document_public_id,
        source_type: &input.source_type,
        source_format: &input.source_format,
        source_hash: &input.source_hash,
        page_count: input.page_count,
        extracted_text_hash: input
            .extracted_text
            .as_deref()
            .map(|text| hashing::sha256_hex(text.as_bytes())),
        visual_page_hashes: input
            .visual_pages
            .iter()
            .map(|page| {
                (
                    page.page_no,
                    page.mime_type.clone(),
                    hashing::sha256_hex(&page.bytes),
                )
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&value)
        .map_err(|error| CoreError::Parse(format!("题目来源输入序列化失败：{error}")))?;
    Ok(hashing::sha256_hex(&bytes))
}

pub enum BeginSourceRun {
    Execute { ai_run_id: i64 },
    Completed { ai_run_id: i64 },
}

pub fn begin(
    conn: &Connection,
    input: &SourceExtractionInput,
    recognizer: &dyn SourceQuestionRecognizer,
    request_key: &str,
    input_artifact_id: i64,
) -> CoreResult<BeginSourceRun> {
    if request_key.trim().is_empty() {
        return Err(CoreError::Invalid("题目提取请求键不能为空".into()));
    }
    let hash = input_hash(input)?;
    let cached = conn
        .query_row(
            "SELECT id FROM ai_runs
             WHERE run_type='question_source_extract' AND source_module='knowledge'
               AND business_ref_type='k1_source_document' AND business_ref_id=?1
               AND input_artifact_id=?2 AND provider=?3 AND model_name=?4
               AND model_version=?5 AND config_version=?6 AND prompt_or_rule_version=?7
               AND input_hash=?8 AND status='succeeded'
             ORDER BY id DESC LIMIT 1",
            rusqlite::params![
                &input.source_document_public_id,
                input_artifact_id,
                recognizer.provider(),
                recognizer.model_name(),
                recognizer.model_version(),
                recognizer.config_version(),
                recognizer.prompt_or_rule_version(),
                &hash,
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(ai_run_id) = cached {
        return Ok(BeginSourceRun::Completed { ai_run_id });
    }
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: &format!("{}:analyze", request_key.trim()),
            run_type: "question_source_extract",
            source_module: "knowledge",
            business_ref_type: "k1_source_document",
            business_ref_id: &input.source_document_public_id,
            input_artifact_id: Some(input_artifact_id),
            provider: recognizer.provider(),
            model_name: recognizer.model_name(),
            model_version: recognizer.model_version(),
            config_version: recognizer.config_version(),
            prompt_or_rule_version: recognizer.prompt_or_rule_version(),
            input_hash: &hash,
            retry_of_ai_run_id: None,
        },
    )?;
    match run.status {
        AiRunStatus::Pending => {
            ai_runs::start(conn, run.id, &time::utc_now_rfc3339(), None)?;
            Ok(BeginSourceRun::Execute { ai_run_id: run.id })
        }
        AiRunStatus::Succeeded | AiRunStatus::Failed => {
            Ok(BeginSourceRun::Completed { ai_run_id: run.id })
        }
        AiRunStatus::Processing => Err(CoreError::Invalid(
            "该题目来源正在识别，请勿重复提交".into(),
        )),
        AiRunStatus::Voided => Err(CoreError::Invalid(
            "该题目提取运行已作废，请重新选择文件".into(),
        )),
    }
}

fn failure_from_error(error: CoreError) -> SourceRunFailure {
    let message = error.to_string();
    let (code, retryable) = if message.contains("未配置") {
        ("configuration_missing", true)
    } else if message.contains("超时") {
        ("timeout", true)
    } else if message.contains("服务") || message.contains("请求过多") {
        ("provider_unavailable", true)
    } else {
        ("invalid_output", false)
    };
    SourceRunFailure {
        schema_version: 1,
        code: code.into(),
        safe_message: message,
        retryable,
    }
}

fn failure_json(failure: &SourceRunFailure) -> CoreResult<String> {
    serde_json::to_string(&serde_json::json!({
        "schema_version": failure.schema_version,
        "code": failure.code,
        "safe_message": failure.safe_message,
        "retryable": failure.retryable,
    }))
    .map_err(|error| CoreError::Parse(format!("题目提取错误序列化失败：{error}")))
}

pub fn finish(
    conn: &Connection,
    input: &SourceExtractionInput,
    ai_run_id: i64,
    result: CoreResult<SourceExtractionOutput>,
) -> CoreResult<()> {
    let finished_at = time::utc_now_rfc3339();
    match result {
        Ok(output) => match validate_output(input, &output).and_then(|_| {
            serde_json::to_string(&output)
                .map_err(|error| CoreError::Parse(format!("题目提取输出序列化失败：{error}")))
        }) {
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
            Err(error) => {
                let failure = failure_from_error(error);
                ai_runs::finalize_failed(conn, ai_run_id, &failure_json(&failure)?, &finished_at)?;
            }
        },
        Err(error) => {
            let failure = failure_from_error(error);
            ai_runs::finalize_failed(conn, ai_run_id, &failure_json(&failure)?, &finished_at)?;
        }
    }
    Ok(())
}

fn run_failure(run: &AiRun) -> CoreResult<SourceRunFailure> {
    serde_json::from_str(
        run.error_meta_json
            .as_deref()
            .ok_or_else(|| CoreError::Invalid("失败题目提取 run 缺少错误".into()))?,
    )
    .map_err(|error| CoreError::Parse(format!("题目提取错误记录损坏：{error}")))
}

pub fn materialize_result(
    conn: &mut Connection,
    document: SourceDocument,
    ai_run_id: i64,
) -> CoreResult<SourceImportAnalysisResult> {
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    match run.status {
        AiRunStatus::Succeeded => {
            let extraction = source_documents::materialize_source_extraction(
                conn,
                "local_teacher",
                &document.public_id,
                ai_run_id,
            )?;
            Ok(SourceImportAnalysisResult {
                document,
                ai_run_id,
                status: "succeeded".into(),
                extraction: Some(extraction),
                failure: None,
            })
        }
        AiRunStatus::Failed => Ok(SourceImportAnalysisResult {
            document,
            ai_run_id,
            status: "failed".into(),
            extraction: None,
            failure: Some(run_failure(&run)?),
        }),
        _ => Err(CoreError::Invalid("题目提取尚未形成结果".into())),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use module_knowledge::source_import::{
        SourceAnchorDraft, SourcePrivacyResult, SourceQuestionDraft,
    };
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    use super::*;

    struct FakeRecognizer;

    impl SourceQuestionRecognizer for FakeRecognizer {
        fn provider(&self) -> &str {
            "fake"
        }
        fn model_name(&self) -> &str {
            "fake-model"
        }
        fn model_version(&self) -> &str {
            "v1"
        }
        fn config_version(&self) -> &str {
            "v1"
        }
        fn prompt_or_rule_version(&self) -> &str {
            "v1"
        }
        fn recognize(&self, _input: &SourceExtractionInput) -> CoreResult<SourceExtractionOutput> {
            unreachable!()
        }
    }

    fn output() -> SourceExtractionOutput {
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
                stem: "洋务运动前期的口号是什么？".into(),
                material_text: None,
                max_score: 1.0,
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

    #[test]
    fn text_source_is_archived_and_only_materializes_source_draft() {
        let root = std::env::temp_dir().join(format!(
            "jiaofu-k1-source-{}-{}",
            std::process::id(),
            time::utc_now_rfc3339().replace([':', '.'], "-")
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("questions.txt");
        fs::write(&path, "1. 洋务运动前期的口号是什么？").unwrap();
        let prepared = prepare_source(path.to_str().unwrap(), "source_document").unwrap();
        let mut connection = open_in_memory().unwrap();
        run_migrations(&connection, CORE_MIGRATIONS).unwrap();
        run_migrations(&connection, module_knowledge::knowledge_migrations()).unwrap();
        let document = persist_source(
            &mut connection,
            &root,
            &prepared,
            "source_document",
            "source-test-1",
        )
        .unwrap();
        let input = prepared.extraction_input(&document);
        let begin = begin(
            &connection,
            &input,
            &FakeRecognizer,
            "source-test-1",
            document.source_artifact_id,
        )
        .unwrap();
        let BeginSourceRun::Execute { ai_run_id } = begin else {
            panic!("expected execute")
        };
        finish(&connection, &input, ai_run_id, Ok(output())).unwrap();
        let result = materialize_result(&mut connection, document, ai_run_id).unwrap();
        assert_eq!(result.status, "succeeded");
        assert_eq!(result.extraction.unwrap().drafts.len(), 1);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM k1_questions", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn same_successful_input_uses_cached_run() {
        let mut connection = open_in_memory().unwrap();
        run_migrations(&connection, CORE_MIGRATIONS).unwrap();
        run_migrations(&connection, module_knowledge::knowledge_migrations()).unwrap();
        let root = std::env::temp_dir().join(format!("jiaofu-k1-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("questions.txt");
        fs::write(&path, "1. 洋务运动前期的口号是什么？").unwrap();
        let prepared = prepare_source(path.to_str().unwrap(), "source_document").unwrap();
        let document = persist_source(
            &mut connection,
            &root,
            &prepared,
            "source_document",
            "source-cache-1",
        )
        .unwrap();
        let input = prepared.extraction_input(&document);
        let BeginSourceRun::Execute { ai_run_id } = begin(
            &connection,
            &input,
            &FakeRecognizer,
            "source-cache-1",
            document.source_artifact_id,
        )
        .unwrap() else {
            panic!("expected execute")
        };
        finish(&connection, &input, ai_run_id, Ok(output())).unwrap();
        assert!(matches!(
            begin(
                &connection,
                &input,
                &FakeRecognizer,
                "source-cache-2",
                document.source_artifact_id,
            )
            .unwrap(),
            BeginSourceRun::Completed {
                ai_run_id: cached
            } if cached == ai_run_id
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn failed_run_remains_visible_in_source_inbox() {
        let mut connection = open_in_memory().unwrap();
        run_migrations(&connection, CORE_MIGRATIONS).unwrap();
        run_migrations(&connection, module_knowledge::knowledge_migrations()).unwrap();
        let root = std::env::temp_dir().join(format!("jiaofu-k1-failure-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("questions.txt");
        fs::write(&path, "1. 洋务运动前期的口号是什么？").unwrap();
        let prepared = prepare_source(path.to_str().unwrap(), "source_document").unwrap();
        let document = persist_source(
            &mut connection,
            &root,
            &prepared,
            "source_document",
            "source-failure-1",
        )
        .unwrap();
        let input = prepared.extraction_input(&document);
        let BeginSourceRun::Execute { ai_run_id } = begin(
            &connection,
            &input,
            &FakeRecognizer,
            "source-failure-1",
            document.source_artifact_id,
        )
        .unwrap() else {
            panic!("expected execute")
        };
        finish(
            &connection,
            &input,
            ai_run_id,
            Err(CoreError::Invalid("题目提取超时，可稍后重试".into())),
        )
        .unwrap();
        let result = materialize_result(&mut connection, document, ai_run_id).unwrap();
        assert_eq!(result.status, "failed");
        let inbox = source_documents::list_source_inbox(&connection, "local_teacher", 10).unwrap();
        assert_eq!(inbox[0].latest_ai_status.as_deref(), Some("failed"));
        assert!(inbox[0]
            .latest_ai_error_meta_json
            .as_deref()
            .is_some_and(|value| value.contains("题目提取超时")));
        assert!(inbox[0]
            .latest_ai_error_meta_json
            .as_deref()
            .is_some_and(|value| value.contains("\"schema_version\":1")));
        assert!(inbox[0].latest_extraction.is_none());
        let _ = fs::remove_dir_all(&root);
    }
}
