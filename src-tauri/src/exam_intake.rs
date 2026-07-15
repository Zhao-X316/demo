//! T6.1b 固定试卷一站式入站：本地归档、PDF 单页拆分、B1 页面登记与 B3a 预检。
//!
//! 本模块只把老师选择的资料变成不可变 artifact 和可恢复入站事实；不调用 OCR/OMR，
//! 不创建机器评分、老师判定或成绩发布。上传答案在本批仅登记为 teaching_content 资料，
//! 不能伪装成老师已经确认的正式答案。

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use suite_core::db::repo::artifacts::{self, NewArtifact};
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{ArchiveStatus, Artifact, ArtifactKind, PrivacyClass};

use module_exam::service::fixed_paper::{
    self, FixedPaperPreflightInput, FixedPaperPreflightRevision, NewFixedInputDocument,
};
use module_exam::service::papers::{self, NewIngestBatch, NewIngestPage};

use crate::pdf_pages;

const ACTOR: &str = "teacher";
const ORIGINAL_STUDENT_VERSION: &str = "exam-intake-original-student-v1";
const ORIGINAL_ANSWER_VERSION: &str = "exam-intake-original-answer-v1";
const PDF_PAGE_VERSION: &str = "exam-intake-pdf-page-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedIntakeOption {
    pub class_id: i64,
    pub class_name: String,
    pub assessment_id: i64,
    pub assessment_version_id: i64,
    pub assessment_title: String,
    pub revision: i64,
    pub template_version: Option<String>,
    pub item_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedIntakeRequest {
    pub assessment_version_id: i64,
    pub student_paths: Vec<String>,
    pub answer_path: Option<String>,
    pub answer_text: Option<String>,
    pub expected_pages_per_attempt: i64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedIntakeDocumentSummary {
    pub role: String,
    pub format: String,
    pub original_name: String,
    pub page_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedIntakeResult {
    pub batch_id: i64,
    pub batch_public_id: String,
    pub documents: Vec<FixedIntakeDocumentSummary>,
    pub student_document_count: i64,
    pub student_page_count: i64,
    pub answer_document_count: i64,
    pub route: String,
    pub target_count: i64,
    pub ready_count: i64,
    pub review_count: i64,
    pub blocked_count: i64,
    pub completed_count: i64,
    pub reason_codes: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceFormat {
    Jpeg,
    Pdf,
    Text,
}

impl SourceFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Pdf => "pdf",
            Self::Text => "text",
        }
    }

    fn mime_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Pdf => "application/pdf",
            Self::Text => "text/plain",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Pdf => "pdf",
            Self::Text => "txt",
        }
    }

    fn artifact_kind(self) -> ArtifactKind {
        match self {
            Self::Jpeg => ArtifactKind::Image,
            Self::Pdf | Self::Text => ArtifactKind::Document,
        }
    }
}

struct PreparedSource {
    path: Option<PathBuf>,
    original_name: String,
    format: SourceFormat,
    bytes: Option<Vec<u8>>,
    expected_hash: String,
    pdf_pages: Vec<Vec<u8>>,
}

pub(crate) struct PreparedFixedIntake {
    student_sources: Vec<PreparedSource>,
    answer_source: Option<PreparedSource>,
}

struct ArchivedFile {
    path: PathBuf,
    created: bool,
}

impl ArchivedFile {
    fn rollback_new_file(&self) {
        if self.created {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn io_error(context: &str, error: std::io::Error) -> CoreError {
    CoreError::Io(format!("{context}: {error}"))
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn source_format(path: &Path, allow_text: bool) -> CoreResult<SourceFormat> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("jpg" | "jpeg") => Ok(SourceFormat::Jpeg),
        Some("pdf") => Ok(SourceFormat::Pdf),
        Some("txt") if allow_text => Ok(SourceFormat::Text),
        _ if allow_text => Err(CoreError::Invalid(
            "答案资料只支持 JPG、JPEG、PDF 或 TXT".into(),
        )),
        _ => Err(CoreError::Invalid("学生试卷只支持 JPG、JPEG 或 PDF".into())),
    }
}

fn split_pdf_pages(path: &Path) -> CoreResult<Vec<Vec<u8>>> {
    pdf_pages::split_to_single_page_pdfs(path)
}

fn prepare_path(path: &str, allow_text: bool) -> CoreResult<PreparedSource> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(CoreError::Io(format!("文件不存在：{}", path.display())));
    }
    let format = source_format(&path, allow_text)?;
    let original_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("未命名资料")
        .to_string();
    let expected_hash = hashing::sha256_file(&path)?;
    let pdf_pages = if format == SourceFormat::Pdf {
        split_pdf_pages(&path)?
    } else {
        Vec::new()
    };
    Ok(PreparedSource {
        path: Some(path),
        original_name,
        format,
        bytes: None,
        expected_hash,
        pdf_pages,
    })
}

fn prepare_text(text: &str) -> CoreResult<PreparedSource> {
    required(text, "粘贴答案")?;
    let bytes = text.trim().as_bytes().to_vec();
    Ok(PreparedSource {
        path: None,
        original_name: "粘贴答案.txt".into(),
        format: SourceFormat::Text,
        expected_hash: hashing::sha256_hex(&bytes),
        bytes: Some(bytes),
        pdf_pages: Vec::new(),
    })
}

fn archive_bytes(
    bytes: &[u8],
    hash: &str,
    extension: &str,
    dir: &Path,
) -> CoreResult<ArchivedFile> {
    std::fs::create_dir_all(dir).map_err(|error| io_error("创建试卷归档目录失败", error))?;
    let destination = dir.join(format!("{hash}.{extension}"));
    if destination.is_file() {
        let existing_hash = hashing::sha256_file(&destination)?;
        if existing_hash != hash {
            return Err(CoreError::Invalid("同名归档内容不一致，拒绝覆盖".into()));
        }
        return Ok(ArchivedFile {
            path: destination,
            created: false,
        });
    }
    let temp = dir.join(format!(".{hash}.{}.tmp", std::process::id()));
    let result = (|| -> CoreResult<()> {
        std::fs::write(&temp, bytes).map_err(|error| io_error("写入试卷归档失败", error))?;
        let copied_hash = hashing::sha256_file(&temp)?;
        if copied_hash != hash {
            return Err(CoreError::Invalid("试卷归档 hash 校验失败".into()));
        }
        std::fs::rename(&temp, &destination)
            .map_err(|error| io_error("提交试卷归档失败", error))?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    Ok(ArchivedFile {
        path: destination,
        created: true,
    })
}

fn archive_source(source: &PreparedSource, dir: &Path) -> CoreResult<(ArchivedFile, String, i64)> {
    let bytes = match (&source.path, &source.bytes) {
        (Some(path), None) => {
            std::fs::read(path).map_err(|error| io_error("读取待归档试卷失败", error))?
        }
        (None, Some(bytes)) => bytes.clone(),
        _ => return Err(CoreError::Invalid("试卷资料来源状态非法".into())),
    };
    let hash = hashing::sha256_hex(&bytes);
    if hash != source.expected_hash {
        return Err(CoreError::Invalid(
            "文件在导入期间发生变化，请重新选择后再试".into(),
        ));
    }
    let archived = archive_bytes(&bytes, &hash, source.format.extension(), dir)?;
    Ok((archived, hash, bytes.len() as i64))
}

#[allow(clippy::too_many_arguments)]
fn register_artifact(
    conn: &Connection,
    archived: &ArchivedFile,
    hash: &str,
    byte_size: i64,
    kind: ArtifactKind,
    mime_type: &str,
    original_name: Option<&str>,
    original_path: Option<&Path>,
    parent_artifact_id: Option<i64>,
    derivative_type: Option<&str>,
    processing_version: &str,
    privacy_class: PrivacyClass,
) -> CoreResult<Artifact> {
    let archived_path = archived.path.to_string_lossy();
    let original_path = original_path.map(|path| path.to_string_lossy().into_owned());
    let result = artifacts::create_or_get(
        conn,
        &NewArtifact {
            kind,
            sha256: hash,
            mime_type,
            byte_size,
            original_name,
            original_path: original_path.as_deref(),
            archived_path: &archived_path,
            parent_artifact_id,
            derivative_type,
            processing_version,
            privacy_class,
            archive_status: ArchiveStatus::Ready,
        },
    );
    if result.is_err() {
        archived.rollback_new_file();
    }
    result
}

pub fn list_options(conn: &Connection) -> CoreResult<Vec<FixedIntakeOption>> {
    let mut stmt = conn.prepare(
        "SELECT c.id,c.name,a.id,v.id,a.title,v.revision,v.template_version,COUNT(i.id)
         FROM exam_assessment_versions_v2 v
         JOIN exam_assessments_v2 a ON a.id=v.assessment_id AND a.state='active'
         JOIN classes c ON c.id=a.class_id
         JOIN exam_assessment_items_v2 i ON i.assessment_version_id=v.id AND i.state='active'
         JOIN k1_question_versions q ON q.id=i.question_version_id
         WHERE v.state='confirmed' AND q.question_type IN ('single','multiple','true_false')
         GROUP BY c.id,c.name,a.id,v.id,a.title,v.revision,v.template_version
         HAVING COUNT(i.id)>0
         ORDER BY c.id,a.id,v.revision DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(FixedIntakeOption {
            class_id: row.get(0)?,
            class_name: row.get(1)?,
            assessment_id: row.get(2)?,
            assessment_version_id: row.get(3)?,
            assessment_title: row.get(4)?,
            revision: row.get(5)?,
            template_version: row.get(6)?,
            item_count: row.get(7)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

fn parse_reason_codes(preflight: &FixedPaperPreflightRevision) -> CoreResult<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(&preflight.reason_codes_json)
        .map_err(|error| CoreError::Parse(format!("预检原因读取失败：{error}")))?;
    Ok(value
        .get("codes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_string)
        .collect())
}

fn next_action(route: &str) -> &'static str {
    match route {
        "ready_for_batch_confirm" => "前往标准卷终审，确认后再显式发布成绩",
        "review_required" => "查看需复核项目，老师确认分歧或模糊项",
        _ => "资料已安全导入，等待页面识别、学生匹配和题区确认",
    }
}

pub(crate) fn prepare_fixed_intake_files(
    request: &FixedIntakeRequest,
) -> CoreResult<PreparedFixedIntake> {
    required(&request.idempotency_key, "上传请求编号")?;
    if request.expected_pages_per_attempt < 1 {
        return Err(CoreError::Invalid("每名学生的试卷页数必须大于 0".into()));
    }
    if request.student_paths.is_empty() {
        return Err(CoreError::Invalid("请至少选择一份学生试卷".into()));
    }
    if request
        .answer_path
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        && request
            .answer_text
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(CoreError::Invalid("答案文件和粘贴答案只能选择一种".into()));
    }

    // 所有外部文件先完成格式和 PDF 结构校验，避免发现坏文件前已写入部分批次事实。
    let student_sources = request
        .student_paths
        .iter()
        .map(|path| prepare_path(path, false))
        .collect::<CoreResult<Vec<_>>>()?;
    let unique_student_hashes = student_sources
        .iter()
        .map(|source| source.expected_hash.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if unique_student_hashes.len() != student_sources.len() {
        return Err(CoreError::Invalid(
            "检测到内容完全相同的重复试卷，请移除重复文件".into(),
        ));
    }
    let answer_source = match (
        request
            .answer_path
            .as_deref()
            .filter(|value| !value.trim().is_empty()),
        request
            .answer_text
            .as_deref()
            .filter(|value| !value.trim().is_empty()),
    ) {
        (Some(path), None) => Some(prepare_path(path, true)?),
        (None, Some(text)) => Some(prepare_text(text)?),
        (None, None) => None,
        (Some(_), Some(_)) => unreachable!("validated"),
    };
    Ok(PreparedFixedIntake {
        student_sources,
        answer_source,
    })
}

pub(crate) fn persist_fixed_intake(
    conn: &Connection,
    data_dir: &Path,
    request: &FixedIntakeRequest,
    prepared: &PreparedFixedIntake,
) -> CoreResult<FixedIntakeResult> {
    let student_sources = &prepared.student_sources;
    let answer_source = prepared.answer_source.as_ref();

    let batch = papers::create_or_get_ingest_batch(
        conn,
        &NewIngestBatch {
            assessment_version_id: request.assessment_version_id,
            source_kind: "image_folder",
            idempotency_key: request.idempotency_key.trim(),
            created_by: ACTOR,
        },
    )?;
    let originals_dir = data_dir.join("archive/exam/originals");
    let pages_dir = data_dir.join("archive/exam/pages");
    let mut summaries = Vec::new();
    let mut import_index = 0_i64;

    for (source_index, source) in student_sources.iter().enumerate() {
        let (archived, hash, byte_size) = archive_source(source, &originals_dir)?;
        let artifact = register_artifact(
            conn,
            &archived,
            &hash,
            byte_size,
            source.format.artifact_kind(),
            source.format.mime_type(),
            Some(&source.original_name),
            source.path.as_deref(),
            None,
            None,
            ORIGINAL_STUDENT_VERSION,
            PrivacyClass::StudentSensitive,
        )?;
        let page_count = if source.format == SourceFormat::Pdf {
            source.pdf_pages.len() as i64
        } else {
            1
        };
        fixed_paper::register_fixed_input_document(
            conn,
            &NewFixedInputDocument {
                ingest_batch_id: batch.id,
                source_artifact_id: artifact.id,
                document_role: "student_work",
                source_format: source.format.as_str(),
                import_index: source_index as i64,
                page_count,
                idempotency_key: &format!(
                    "{}:student:{source_index}",
                    request.idempotency_key.trim()
                ),
                created_by: ACTOR,
            },
        )?;

        if source.format == SourceFormat::Pdf {
            for (page_index, bytes) in source.pdf_pages.iter().enumerate() {
                let processing_version =
                    format!("{PDF_PAGE_VERSION}:{}:p{}", artifact.sha256, page_index + 1);
                let existing_page = artifacts::list_children(conn, artifact.id)?
                    .into_iter()
                    .find(|candidate| {
                        candidate.derivative_type.as_deref() == Some("pdf_page")
                            && candidate.processing_version == processing_version
                    });
                let page_artifact = if let Some(existing) = existing_page {
                    let archived_path = Path::new(&existing.archived_path);
                    if !archived_path.is_file()
                        || hashing::sha256_file(archived_path)? != existing.sha256
                    {
                        return Err(CoreError::Invalid(
                            "既有 PDF 单页归档缺失或损坏，请从备份恢复".into(),
                        ));
                    }
                    existing
                } else {
                    let page_hash = hashing::sha256_hex(bytes);
                    let page_archived = archive_bytes(bytes, &page_hash, "pdf", &pages_dir)?;
                    register_artifact(
                        conn,
                        &page_archived,
                        &page_hash,
                        bytes.len() as i64,
                        ArtifactKind::Page,
                        "application/pdf",
                        Some(&format!(
                            "{}-第{}页.pdf",
                            source.original_name,
                            page_index + 1
                        )),
                        source.path.as_deref(),
                        Some(artifact.id),
                        Some("pdf_page"),
                        &processing_version,
                        PrivacyClass::StudentSensitive,
                    )?
                };
                papers::register_ingest_page(
                    conn,
                    &NewIngestPage {
                        batch_id: batch.id,
                        source_artifact_id: page_artifact.id,
                        import_index,
                        expected_page_no: Some(
                            import_index % request.expected_pages_per_attempt + 1,
                        ),
                    },
                )?;
                import_index += 1;
            }
        } else {
            papers::register_ingest_page(
                conn,
                &NewIngestPage {
                    batch_id: batch.id,
                    source_artifact_id: artifact.id,
                    import_index,
                    expected_page_no: Some(import_index % request.expected_pages_per_attempt + 1),
                },
            )?;
            import_index += 1;
        }
        summaries.push(FixedIntakeDocumentSummary {
            role: "student_work".into(),
            format: source.format.as_str().into(),
            original_name: source.original_name.clone(),
            page_count,
        });
    }

    let mut answer_document_count = 0_i64;
    if let Some(source) = answer_source {
        let (archived, hash, byte_size) = archive_source(source, &originals_dir)?;
        let artifact = register_artifact(
            conn,
            &archived,
            &hash,
            byte_size,
            source.format.artifact_kind(),
            source.format.mime_type(),
            Some(&source.original_name),
            source.path.as_deref(),
            None,
            None,
            ORIGINAL_ANSWER_VERSION,
            PrivacyClass::TeachingContent,
        )?;
        let page_count = if source.format == SourceFormat::Pdf {
            source.pdf_pages.len() as i64
        } else {
            1
        };
        fixed_paper::register_fixed_input_document(
            conn,
            &NewFixedInputDocument {
                ingest_batch_id: batch.id,
                source_artifact_id: artifact.id,
                document_role: "answer_source",
                source_format: source.format.as_str(),
                import_index: 0,
                page_count,
                idempotency_key: &format!("{}:answer:0", request.idempotency_key.trim()),
                created_by: ACTOR,
            },
        )?;
        summaries.push(FixedIntakeDocumentSummary {
            role: "answer_source".into(),
            format: source.format.as_str().into(),
            original_name: source.original_name.clone(),
            page_count,
        });
        answer_document_count = 1;
    }

    let preflight = fixed_paper::preflight_fixed_paper_batch(
        conn,
        &FixedPaperPreflightInput {
            ingest_batch_id: batch.id,
            expected_pages_per_attempt: request.expected_pages_per_attempt,
            created_by_type: "teacher",
            created_by: Some(ACTOR),
        },
    )?;
    let reason_codes = parse_reason_codes(&preflight)?;
    Ok(FixedIntakeResult {
        batch_id: batch.id,
        batch_public_id: batch.public_id,
        student_document_count: student_sources.len() as i64,
        student_page_count: import_index,
        answer_document_count,
        documents: summaries,
        route: preflight.route.clone(),
        target_count: preflight.target_count,
        ready_count: preflight.ready_count,
        review_count: preflight.review_count,
        blocked_count: preflight.blocked_count,
        completed_count: preflight.completed_count,
        reason_codes,
        next_action: next_action(&preflight.route).into(),
    })
}

#[cfg(test)]
fn prepare_fixed_intake(
    conn: &Connection,
    data_dir: &Path,
    request: &FixedIntakeRequest,
) -> CoreResult<FixedIntakeResult> {
    let prepared = prepare_fixed_intake_files(request)?;
    persist_fixed_intake(conn, data_dir, request, &prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn test_root(label: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("jiaofu-exam-intake-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn seed() -> Connection {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, module_exam::exam_migrations()).unwrap();
        let hash = "a".repeat(64);
        conn.execute_batch(&format!(
            r#"INSERT INTO subjects(name) VALUES ('历史');
               INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
               INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('edition',1,'PEP','2024','中国历史八上','8','upper','active','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('map',1,1,'confirmed','2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('question','personal','teacher','unknown',0,'2026-07-14T08:00:00.000Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,quality_level,state,created_at)
                 VALUES ('question-v1',1,1,'true_false','鸦片战争爆发于1840年。',1,'{hash}','L3','published','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('answer-v1',1,1,'{{"schema_version":1,"correct":true}}','confirmed','2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('rubric-v1',1,1,1,'confirmed','2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('link-v1',1,1,1,'confirmed','2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,created_by,created_at,updated_at)
                 VALUES ('assessment','固定试卷',1,'quiz','include','active','teacher','2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('assessment-v1',1,1,'{hash}','fixed-template-v1','confirmed','2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,state,created_at)
                 VALUES ('item',1,1,1,1,1,0,1,'{{"schema_version":1}}','active','2026-07-14T08:00:00.000Z');"#
        ))
        .unwrap();
        conn
    }

    fn write_jpeg(path: &Path, value: &[u8]) {
        std::fs::write(path, value).unwrap();
    }

    #[test]
    fn options_only_include_confirmed_objective_assessments() {
        let conn = seed();
        let options = list_options(&conn).unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].class_name, "八年级一班");
        assert_eq!(options[0].item_count, 1);
    }

    #[test]
    fn jpeg_and_pasted_answer_are_archived_without_creating_scores() {
        let root = test_root("jpeg");
        let student = root.join("student.jpg");
        write_jpeg(&student, b"jpeg-student-paper");
        let conn = seed();
        let request = FixedIntakeRequest {
            assessment_version_id: 1,
            student_paths: vec![student.to_string_lossy().into_owned()],
            answer_path: None,
            answer_text: Some("1. 正确".into()),
            expected_pages_per_attempt: 1,
            idempotency_key: "jpeg-intake".into(),
        };
        let result = prepare_fixed_intake(&conn, &root, &request).unwrap();
        assert_eq!(result.student_document_count, 1);
        assert_eq!(result.student_page_count, 1);
        assert_eq!(result.answer_document_count, 1);
        assert_eq!(result.route, "review_required");
        assert!(result
            .reason_codes
            .iter()
            .any(|code| code == "PAGE_QUALITY_REVIEW_REQUIRED"));
        let privacy: Vec<String> = conn
            .prepare("SELECT privacy_class FROM artifacts ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(privacy, vec!["student_sensitive", "teaching_content"]);
        let grade_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM exam_grade_decisions_v2", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(grade_count, 0);
    }

    #[test]
    fn retry_with_same_key_is_idempotent() {
        let root = test_root("retry");
        let student = root.join("student.jpeg");
        write_jpeg(&student, b"same-student-paper");
        let conn = seed();
        let request = FixedIntakeRequest {
            assessment_version_id: 1,
            student_paths: vec![student.to_string_lossy().into_owned()],
            answer_path: None,
            answer_text: None,
            expected_pages_per_attempt: 1,
            idempotency_key: "same-intake".into(),
        };
        let first = prepare_fixed_intake(&conn, &root, &request).unwrap();
        let second = prepare_fixed_intake(&conn, &root, &request).unwrap();
        assert_eq!(first.batch_id, second.batch_id);
        for table in [
            "exam_ingest_batches_v2",
            "exam_fixed_input_documents_v2",
            "exam_ingest_pages_v2",
            "exam_fixed_preflight_revisions_v2",
        ] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 1, "{table}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn pdf_registers_real_page_derivatives_and_retry_reuses_them() {
        let root = test_root("pdf");
        let student = root.join("student.pdf");
        std::fs::write(&student, crate::pdf_pages::two_page_pdf_fixture()).unwrap();
        let conn = seed();
        let request = FixedIntakeRequest {
            assessment_version_id: 1,
            student_paths: vec![student.to_string_lossy().into_owned()],
            answer_path: None,
            answer_text: None,
            expected_pages_per_attempt: 2,
            idempotency_key: "pdf-intake".into(),
        };
        let first = prepare_fixed_intake(&conn, &root, &request).unwrap();
        let second = prepare_fixed_intake(&conn, &root, &request).unwrap();
        assert_eq!(first.batch_id, second.batch_id);
        assert_eq!(first.student_page_count, 2);
        let kinds: Vec<String> = conn
            .prepare("SELECT kind FROM artifacts ORDER BY id")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(kinds, vec!["document", "page", "page"]);
        let page_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM exam_ingest_pages_v2", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(page_count, 2);
    }

    #[test]
    fn duplicate_student_content_is_rejected_before_batch_creation() {
        let root = test_root("duplicate");
        let first = root.join("first.jpg");
        let second = root.join("second.jpg");
        write_jpeg(&first, b"same-paper");
        write_jpeg(&second, b"same-paper");
        let conn = seed();
        let request = FixedIntakeRequest {
            assessment_version_id: 1,
            student_paths: vec![
                first.to_string_lossy().into_owned(),
                second.to_string_lossy().into_owned(),
            ],
            answer_path: None,
            answer_text: None,
            expected_pages_per_attempt: 1,
            idempotency_key: "duplicate-intake".into(),
        };
        let error = prepare_fixed_intake(&conn, &root, &request)
            .unwrap_err()
            .to_string();
        assert!(error.contains("重复试卷"));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM exam_ingest_batches_v2", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn invalid_student_format_fails_before_batch_creation() {
        let root = test_root("invalid");
        let student = root.join("student.png");
        std::fs::write(&student, b"png").unwrap();
        let conn = seed();
        let request = FixedIntakeRequest {
            assessment_version_id: 1,
            student_paths: vec![student.to_string_lossy().into_owned()],
            answer_path: None,
            answer_text: None,
            expected_pages_per_attempt: 1,
            idempotency_key: "invalid-intake".into(),
        };
        assert!(prepare_fixed_intake(&conn, &root, &request).is_err());
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM exam_ingest_batches_v2", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }
}
