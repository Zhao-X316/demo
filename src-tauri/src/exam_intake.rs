//! T6.1b 固定试卷一站式入站：本地归档、PDF 单页拆分、B1 页面登记与 B3a 预检。
//!
//! 本模块只把老师选择的资料变成不可变 artifact 和可恢复入站事实；不调用 OCR/OMR，
//! 不创建机器评分、老师判定或成绩发布。上传答案在本批仅登记为 teaching_content 资料，
//! 不能伪装成老师已经确认的正式答案。

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::artifacts::{self, NewArtifact};
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{ArchiveStatus, Artifact, ArtifactKind, PrivacyClass};

use module_exam::service::fixed_paper::{
    self, FixedPaperPreflightInput, FixedPaperPreflightRevision, NewFixedInputDocument,
};
use module_exam::service::ordered_activation::{
    self, ConfirmGroupingQualityInput, GroupedPageEvidence,
};
use module_exam::service::ordered_intake::{
    self, ConfirmOrderedGroupingInput, ImportOrderEntry, NewImportOrderRevision,
    NewMaterialTypeRevision, NewPageTypeRevision, OrderedGroupingInput,
};
use module_exam::service::ordered_retake::{self, ReplaceRejectedPageInput, RetakeArtifactInput};
use module_exam::service::page_cycle;
use module_exam::service::papers::{self, NewIngestBatch, NewIngestPage};

use crate::pdf_pages;

const ACTOR: &str = "teacher";
const ORIGINAL_STUDENT_VERSION: &str = "exam-intake-original-student-v1";
const ORIGINAL_ANSWER_VERSION: &str = "exam-intake-original-answer-v1";
const PDF_PAGE_VERSION: &str = "exam-intake-pdf-page-v1";
const RETAKE_STUDENT_VERSION: &str = "exam-intake-retake-student-v1";

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
    pub is_default: bool,
    pub default_selection_public_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedIntakeRequest {
    pub assessment_version_id: i64,
    pub student_paths: Vec<String>,
    pub answer_path: Option<String>,
    pub answer_text: Option<String>,
    pub expected_pages_per_attempt: i64,
    #[serde(default)]
    pub material_type: Option<String>,
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
pub struct GroupingRosterStudent {
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    pub order_policy: String,
    pub order_confidence: f64,
    pub order_conflict_codes: Vec<String>,
    pub material_type: String,
    pub material_type_decision: String,
    pub material_type_confidence: f64,
    pub material_type_needs_confirmation: bool,
    pub grouping_route: String,
    pub student_group_count: i64,
    pub grouping_issue_codes: Vec<String>,
    pub expected_pages_per_attempt: i64,
    pub page_cycle_source: String,
    pub page_cycle_confidence: f64,
    pub page_cycle_needs_teacher_input: bool,
    pub grouping_roster: Vec<GroupingRosterStudent>,
    pub grouping_confirmed: bool,
    pub grouping_first_student_no: Option<String>,
    pub grouping_last_student_no: Option<String>,
    pub quality_review_completed: bool,
    pub mapped_group_count: i64,
    pub rejected_group_count: i64,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageCycleSuggestion {
    pub expected_pages_per_attempt: i64,
    pub confidence: f64,
    pub source: String,
    pub issue_codes: Vec<String>,
    pub needs_teacher_input: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialTypeConfirmationResult {
    pub material_type: String,
    pub material_type_decision: String,
    pub material_type_confidence: f64,
    pub grouping_route: String,
    pub student_group_count: i64,
    pub grouping_issue_codes: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupingConfirmationResult {
    pub grouping_route: String,
    pub student_group_count: i64,
    pub grouping_issue_codes: Vec<String>,
    pub grouping_confirmed: bool,
    pub grouping_first_student_no: String,
    pub grouping_last_student_no: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupingQualityConfirmationResult {
    pub quality_review_completed: bool,
    pub mapped_group_count: i64,
    pub rejected_group_count: i64,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupingRetakeResult {
    pub replacement_page_id: i64,
    pub activated_student: bool,
    pub mapped_group_count: i64,
    pub rejected_group_count: i64,
    pub next_action: String,
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
    fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Pdf => "pdf",
            Self::Text => "text",
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

    fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Pdf => "pdf",
            Self::Text => "txt",
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
        }
    }

    fn artifact_kind(self) -> ArtifactKind {
        match self {
            Self::Jpeg => ArtifactKind::Image,
            Self::Pdf | Self::Text | Self::Docx | Self::Xlsx => ArtifactKind::Document,
        }
    }

    fn registration_format(self) -> &'static str {
        match self {
            Self::Docx | Self::Xlsx => "text",
            _ => self.as_str(),
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
    original_request_index: i64,
    capture_time: Option<String>,
    file_created_ms: Option<i64>,
    file_modified_ms: Option<i64>,
}

pub(crate) struct PreparedFixedIntake {
    student_sources: Vec<PreparedSource>,
    answer_source: Option<PreparedSource>,
    order_confidence: f64,
    order_conflict_codes: Vec<String>,
    page_cycle: PageCycleSuggestion,
}

pub(crate) struct ArchivedFile {
    pub(crate) path: PathBuf,
    created: bool,
}

impl ArchivedFile {
    pub(crate) fn rollback_new_file(&self) {
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
        Some("docx") if allow_text => Ok(SourceFormat::Docx),
        Some("xlsx") if allow_text => Ok(SourceFormat::Xlsx),
        _ if allow_text => Err(CoreError::Invalid(
            "答案资料只支持 JPG、JPEG、PDF、DOCX、XLSX 或 TXT".into(),
        )),
        _ => Err(CoreError::Invalid("学生试卷只支持 JPG、JPEG 或 PDF".into())),
    }
}

fn split_pdf_pages(path: &Path) -> CoreResult<Vec<Vec<u8>>> {
    pdf_pages::split_to_single_page_pdfs(path)
}

fn system_time_ms(value: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    value
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

#[derive(Clone, Copy)]
enum TiffEndian {
    Little,
    Big,
}

fn tiff_u16(bytes: &[u8], offset: usize, endian: TiffEndian) -> Option<u16> {
    let raw = bytes.get(offset..offset + 2)?;
    Some(match endian {
        TiffEndian::Little => u16::from_le_bytes([raw[0], raw[1]]),
        TiffEndian::Big => u16::from_be_bytes([raw[0], raw[1]]),
    })
}

fn tiff_u32(bytes: &[u8], offset: usize, endian: TiffEndian) -> Option<u32> {
    let raw = bytes.get(offset..offset + 4)?;
    Some(match endian {
        TiffEndian::Little => u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
        TiffEndian::Big => u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]),
    })
}

fn tiff_ifd_entry(
    bytes: &[u8],
    ifd_offset: usize,
    wanted_tag: u16,
    endian: TiffEndian,
) -> Option<(u16, u32, usize)> {
    let count = usize::from(tiff_u16(bytes, ifd_offset, endian)?);
    for index in 0..count {
        let offset = ifd_offset.checked_add(2 + index * 12)?;
        if tiff_u16(bytes, offset, endian)? == wanted_tag {
            return Some((
                tiff_u16(bytes, offset + 2, endian)?,
                tiff_u32(bytes, offset + 4, endian)?,
                offset + 8,
            ));
        }
    }
    None
}

fn tiff_ascii_tag(bytes: &[u8], ifd_offset: usize, tag: u16, endian: TiffEndian) -> Option<String> {
    let (value_type, count, value_offset) = tiff_ifd_entry(bytes, ifd_offset, tag, endian)?;
    if value_type != 2 || count == 0 {
        return None;
    }
    let count = usize::try_from(count).ok()?;
    let start = if count <= 4 {
        value_offset
    } else {
        usize::try_from(tiff_u32(bytes, value_offset, endian)?).ok()?
    };
    let value = std::str::from_utf8(bytes.get(start..start.checked_add(count)?)?)
        .ok()?
        .trim_matches(char::from(0))
        .trim()
        .to_string();
    let raw = value.as_bytes();
    let plausible = raw.len() >= 19
        && [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|index| raw.get(*index).is_some_and(u8::is_ascii_digit))
        && raw.get(4) == Some(&b':')
        && raw.get(7) == Some(&b':')
        && raw.get(10) == Some(&b' ')
        && raw.get(13) == Some(&b':')
        && raw.get(16) == Some(&b':');
    plausible.then_some(value)
}

fn exif_capture_time_from_tiff(bytes: &[u8]) -> Option<String> {
    let endian = match bytes.get(0..2)? {
        b"II" => TiffEndian::Little,
        b"MM" => TiffEndian::Big,
        _ => return None,
    };
    if tiff_u16(bytes, 2, endian)? != 42 {
        return None;
    }
    let ifd0 = usize::try_from(tiff_u32(bytes, 4, endian)?).ok()?;
    let original = tiff_ifd_entry(bytes, ifd0, 0x8769, endian)
        .and_then(|(value_type, count, offset)| {
            (value_type == 4 && count == 1)
                .then(|| usize::try_from(tiff_u32(bytes, offset, endian)?).ok())?
        })
        .and_then(|exif_ifd| tiff_ascii_tag(bytes, exif_ifd, 0x9003, endian));
    original.or_else(|| tiff_ascii_tag(bytes, ifd0, 0x0132, endian))
}

fn jpeg_capture_time(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.get(0..2)? != [0xff, 0xd8] {
        return None;
    }
    let mut offset = 2_usize;
    while offset + 4 <= bytes.len() {
        if bytes[offset] != 0xff {
            offset += 1;
            continue;
        }
        let marker = bytes[offset + 1];
        offset += 2;
        if matches!(marker, 0xd8 | 0xd9) || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let length = usize::from(u16::from_be_bytes([
            *bytes.get(offset)?,
            *bytes.get(offset + 1)?,
        ]));
        if length < 2 || offset + length > bytes.len() {
            return None;
        }
        let payload = &bytes[offset + 2..offset + length];
        if marker == 0xe1 && payload.starts_with(b"Exif\0\0") {
            return exif_capture_time_from_tiff(&payload[6..]);
        }
        if marker == 0xda {
            break;
        }
        offset += length;
    }
    None
}

fn prepare_path(
    path: &str,
    allow_text: bool,
    original_request_index: i64,
) -> CoreResult<PreparedSource> {
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
    let metadata = std::fs::metadata(&path).map_err(|error| io_error("读取文件时间失败", error))?;
    let capture_time = (format == SourceFormat::Jpeg)
        .then(|| jpeg_capture_time(&path))
        .flatten();
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
        original_request_index,
        capture_time,
        file_created_ms: system_time_ms(metadata.created()),
        file_modified_ms: system_time_ms(metadata.modified()),
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
        original_request_index: 0,
        capture_time: None,
        file_created_ms: None,
        file_modified_ms: None,
    })
}

fn manual_page_cycle(code: &str) -> PageCycleSuggestion {
    PageCycleSuggestion {
        expected_pages_per_attempt: 1,
        confidence: 0.0,
        source: "teacher_input_required".into(),
        issue_codes: vec![code.into()],
        needs_teacher_input: true,
    }
}

fn infer_prepared_page_cycle(sources: &[PreparedSource]) -> PageCycleSuggestion {
    if sources.is_empty() {
        return manual_page_cycle("NO_STUDENT_PAGE");
    }
    if sources
        .iter()
        .all(|source| source.format == SourceFormat::Pdf)
    {
        let page_counts = sources
            .iter()
            .map(|source| source.pdf_pages.len() as i64)
            .collect::<Vec<_>>();
        let first = page_counts[0];
        if first > 0 && page_counts.iter().all(|count| *count == first) {
            return PageCycleSuggestion {
                expected_pages_per_attempt: first,
                confidence: if sources.len() >= 2 { 0.99 } else { 0.80 },
                source: "pdf_document_page_count".into(),
                issue_codes: Vec::new(),
                needs_teacher_input: sources.len() < 2,
            };
        }
        return manual_page_cycle("PDF_PAGE_COUNT_MISMATCH");
    }
    if !sources
        .iter()
        .all(|source| source.format == SourceFormat::Jpeg)
    {
        return manual_page_cycle("MIXED_STUDENT_FORMAT_PAGE_CYCLE");
    }
    if sources.len() == 1 {
        return PageCycleSuggestion {
            expected_pages_per_attempt: 1,
            confidence: 0.60,
            source: "single_photo_fallback".into(),
            issue_codes: vec!["PAGE_CYCLE_NEEDS_MORE_PHOTOS".into()],
            needs_teacher_input: true,
        };
    }
    let signatures = sources
        .iter()
        .map(|source| {
            let path = source
                .path
                .as_ref()
                .ok_or_else(|| CoreError::Invalid("学生照片缺少本地路径".into()))?;
            let bytes = std::fs::read(path).map_err(|error| io_error("读取页面版式失败", error))?;
            page_cycle::signature_from_jpeg(&bytes)
        })
        .collect::<CoreResult<Vec<_>>>();
    let Ok(signatures) = signatures else {
        return manual_page_cycle("PAGE_LAYOUT_DECODE_FAILED");
    };
    let Some(inference) = page_cycle::infer_repeating_cycle(&signatures, 12) else {
        return manual_page_cycle("PAGE_CYCLE_NOT_CONFIDENT");
    };
    PageCycleSuggestion {
        expected_pages_per_attempt: inference.pages_per_attempt as i64,
        confidence: inference.confidence,
        source: "visual_repeating_layout_v1".into(),
        issue_codes: Vec::new(),
        needs_teacher_input: inference.confidence < 0.80,
    }
}

/// 数据库写入前的轻量版式预判；失败时只要求老师补一个页数，不产生任何业务事实。
pub(crate) fn infer_page_cycle_paths(paths: &[String]) -> CoreResult<PageCycleSuggestion> {
    if paths.is_empty() {
        return Err(CoreError::Invalid("请至少选择一份学生试卷".into()));
    }
    let mut sources = paths
        .iter()
        .enumerate()
        .map(|(index, path)| prepare_path(path, false, index as i64))
        .collect::<CoreResult<Vec<_>>>()?;
    sources.sort_by(|left, right| {
        ordered_intake::natural_name_cmp(&left.original_name, &right.original_name).then_with(
            || {
                left.original_request_index
                    .cmp(&right.original_request_index)
            },
        )
    });
    Ok(infer_prepared_page_cycle(&sources))
}

pub(crate) fn archive_bytes(
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
pub(crate) fn register_artifact(
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
        "WITH selected_defaults AS (
           SELECT selection.assessment_id,selection.selected_assessment_version_id,
                  selection.public_id
           FROM exam_assessment_default_version_selections_v2 selection
           WHERE selection.revision=(
             SELECT MAX(latest.revision)
             FROM exam_assessment_default_version_selections_v2 latest
             WHERE latest.assessment_id=selection.assessment_id
           )
         )
         SELECT c.id,c.name,a.id,v.id,a.title,v.revision,v.template_version,COUNT(i.id),
                v.id=COALESCE(
                  selected_defaults.selected_assessment_version_id,
                  (SELECT latest_version.id
                   FROM exam_assessment_versions_v2 latest_version
                   WHERE latest_version.assessment_id=a.id
                     AND latest_version.state='confirmed'
                   ORDER BY latest_version.revision DESC,latest_version.id DESC
                   LIMIT 1)
                ),
                selected_defaults.public_id
         FROM exam_assessment_versions_v2 v
         JOIN exam_assessments_v2 a ON a.id=v.assessment_id AND a.state='active'
         JOIN classes c ON c.id=a.class_id
         JOIN exam_assessment_items_v2 i ON i.assessment_version_id=v.id AND i.state='active'
         JOIN k1_question_versions q ON q.id=i.question_version_id
         LEFT JOIN selected_defaults ON selected_defaults.assessment_id=a.id
         WHERE v.state='confirmed'
           AND q.question_type IN ('single','multiple','true_false','fill_blank','short_answer')
           AND NOT EXISTS (
             SELECT 1 FROM exam_assessment_items_v2 unsupported
             JOIN k1_question_versions uq ON uq.id=unsupported.question_version_id
             WHERE unsupported.assessment_version_id=v.id AND unsupported.state='active'
               AND uq.question_type NOT IN (
                 'single','multiple','true_false','fill_blank','short_answer'
               )
           )
         GROUP BY c.id,c.name,a.id,v.id,a.title,v.revision,v.template_version,
                  selected_defaults.selected_assessment_version_id,
                  selected_defaults.public_id
         HAVING COUNT(i.id)>0
         ORDER BY c.id,a.id,9 DESC,v.revision DESC",
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
            is_default: row.get(8)?,
            default_selection_public_id: row.get(9)?,
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

fn grouping_scope_summary(
    decision: Option<&ordered_intake::OrderedGroupingDecision>,
) -> CoreResult<(bool, Option<String>, Option<String>)> {
    let Some(decision) = decision else {
        return Ok((false, None, None));
    };
    let scope: serde_json::Value = serde_json::from_str(&decision.roster_scope_json)
        .map_err(|error| CoreError::Parse(format!("学生范围确认读取失败：{error}")))?;
    Ok((
        true,
        scope
            .get("first_student_no")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        scope
            .get("last_student_no")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    ))
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
    if request.material_type.as_deref().is_some_and(|value| {
        !matches!(
            value,
            "auto" | "ordinary_paper" | "answer_sheet" | "dictation"
        )
    }) {
        return Err(CoreError::Invalid("材料类型非法".into()));
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
    let mut student_sources = request
        .student_paths
        .iter()
        .enumerate()
        .map(|(index, path)| prepare_path(path, false, index as i64))
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
    student_sources.sort_by(|left, right| {
        ordered_intake::natural_name_cmp(&left.original_name, &right.original_name).then_with(
            || {
                left.original_request_index
                    .cmp(&right.original_request_index)
            },
        )
    });
    let mut order_conflict_codes = Vec::new();
    if student_sources.windows(2).any(|pair| {
        ordered_intake::natural_name_cmp(&pair[0].original_name, &pair[1].original_name)
            == std::cmp::Ordering::Equal
    }) {
        order_conflict_codes.push("FILENAME_NATURAL_TIE".into());
    }
    let capture_times = student_sources
        .iter()
        .filter_map(|source| source.capture_time.as_deref())
        .collect::<Vec<_>>();
    let capture_conflict =
        capture_times.len() >= 2 && capture_times.windows(2).any(|pair| pair[0] > pair[1]);
    if capture_conflict {
        order_conflict_codes.push("CAPTURE_TIME_ORDER_CONFLICT".into());
    }
    let file_times = student_sources
        .iter()
        .filter_map(|source| source.file_created_ms.or(source.file_modified_ms))
        .collect::<Vec<_>>();
    let file_time_conflict =
        file_times.len() >= 2 && file_times.windows(2).any(|pair| pair[0] > pair[1]);
    if file_time_conflict {
        order_conflict_codes.push("FILE_TIME_ORDER_CONFLICT".into());
    }
    let order_confidence = if !order_conflict_codes.is_empty() {
        0.6
    } else if capture_times.len() >= 2 {
        0.98
    } else if file_times.len() >= 2 {
        0.9
    } else {
        0.8
    };
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
        (Some(path), None) => Some(prepare_path(path, true, 0)?),
        (None, Some(text)) => Some(prepare_text(text)?),
        (None, None) => None,
        (Some(_), Some(_)) => unreachable!("validated"),
    };
    let page_cycle = infer_prepared_page_cycle(&student_sources);
    Ok(PreparedFixedIntake {
        student_sources,
        answer_source,
        order_confidence,
        order_conflict_codes,
        page_cycle,
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
    let mut order_entries = Vec::new();
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
                source_format: source.format.registration_format(),
                import_index: source_index as i64,
                page_count,
                idempotency_key: &format!(
                    "{}:student:{source_index}",
                    request.idempotency_key.trim()
                ),
                created_by: ACTOR,
            },
        )?;
        order_entries.push(ImportOrderEntry {
            source_artifact_id: artifact.id,
            original_name: source.original_name.clone(),
            original_request_index: source.original_request_index,
            sorted_index: source_index as i64,
            page_count,
            capture_time: source.capture_time.clone(),
            file_created_ms: source.file_created_ms,
            file_modified_ms: source.file_modified_ms,
        });

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
                let page = papers::register_ingest_page(
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
                ordered_intake::record_page_type(
                    conn,
                    &NewPageTypeRevision {
                        page_id: page.id,
                        page_type_key: &format!("page_{}", page_index + 1),
                        confidence: 1.0,
                        evidence_json: r#"{"schema_version":1,"source":"pdf_page_index"}"#,
                        decision: "suggested",
                        created_by_type: "system",
                        created_by: None,
                        confirmed_by: None,
                    },
                )?;
                import_index += 1;
            }
        } else {
            let page = papers::register_ingest_page(
                conn,
                &NewIngestPage {
                    batch_id: batch.id,
                    source_artifact_id: artifact.id,
                    import_index,
                    expected_page_no: Some(import_index % request.expected_pages_per_attempt + 1),
                },
            )?;
            let cycle_matches = prepared.page_cycle.expected_pages_per_attempt
                == request.expected_pages_per_attempt
                && prepared.page_cycle.source == "visual_repeating_layout_v1";
            let page_no = import_index % request.expected_pages_per_attempt + 1;
            let evidence = serde_json::json!({
                "schema_version": 1,
                "source": if cycle_matches {
                    "visual_repeating_layout_v1"
                } else {
                    "teacher_fixed_page_count_sequence"
                },
                "cycle_confidence": if cycle_matches {
                    prepared.page_cycle.confidence
                } else {
                    0.80
                }
            })
            .to_string();
            ordered_intake::record_page_type(
                conn,
                &NewPageTypeRevision {
                    page_id: page.id,
                    page_type_key: &format!("page_{page_no}"),
                    confidence: if cycle_matches {
                        prepared.page_cycle.confidence
                    } else {
                        0.80
                    },
                    evidence_json: &evidence,
                    decision: "suggested",
                    created_by_type: "system",
                    created_by: None,
                    confirmed_by: None,
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
                source_format: source.format.registration_format(),
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

    let order_revision = ordered_intake::record_import_order(
        conn,
        &NewImportOrderRevision {
            ingest_batch_id: batch.id,
            sort_policy: "filename_natural_exif_filetime_crosscheck_v1",
            order_confidence: prepared.order_confidence,
            entries: &order_entries,
            conflict_codes: &prepared.order_conflict_codes,
            created_by_type: "system",
            created_by: None,
        },
    )?;
    let source_names = student_sources
        .iter()
        .map(|source| source.original_name.clone())
        .collect::<Vec<_>>();
    let explicit_material = request
        .material_type
        .as_deref()
        .filter(|value| *value != "auto");
    let material_revision = if let Some(material_type) = explicit_material {
        ordered_intake::record_material_type(
            conn,
            &NewMaterialTypeRevision {
                ingest_batch_id: batch.id,
                material_type,
                confidence: 1.0,
                evidence_json: r#"{"schema_version":1,"source":"teacher_upload_choice"}"#,
                decision: "teacher_confirmed",
                created_by_type: "teacher",
                created_by: Some(ACTOR),
                confirmed_by: Some(ACTOR),
            },
        )?
    } else if let Some(current) = ordered_intake::current_material(conn, batch.id)? {
        current
    } else {
        let suggestion = ordered_intake::suggest_material_type(&source_names);
        ordered_intake::record_material_type(
            conn,
            &NewMaterialTypeRevision {
                ingest_batch_id: batch.id,
                material_type: suggestion.material_type,
                confidence: suggestion.confidence,
                evidence_json: &serde_json::json!({
                    "schema_version": 1,
                    "source": "filename_heuristic",
                    "rule": suggestion.rule
                })
                .to_string(),
                decision: "suggested",
                created_by_type: "system",
                created_by: None,
                confirmed_by: None,
            },
        )?
    };
    let grouping = ordered_intake::preview_ordered_grouping(
        conn,
        &OrderedGroupingInput {
            ingest_batch_id: batch.id,
            expected_pages_per_attempt: request.expected_pages_per_attempt,
            created_by_type: "system",
            created_by: None,
        },
    )?;
    let order_conflict_codes =
        ordered_intake::parse_codes(&order_revision.conflict_codes_json, "导入顺序冲突")?;
    let grouping_issue_codes =
        ordered_intake::parse_codes(&grouping.issue_codes_json, "连续拍摄分组原因")?;
    let grouping_roster = ordered_intake::roster(conn, batch.id)?
        .into_iter()
        .map(|student| GroupingRosterStudent {
            student_id: student.id,
            student_no: student.student_no,
            student_name: student.name,
        })
        .collect::<Vec<_>>();
    let grouping_decision = ordered_intake::current_grouping_decision(conn, batch.id)?;
    let (grouping_confirmed, grouping_first_student_no, grouping_last_student_no) =
        grouping_scope_summary(grouping_decision.as_ref())?;
    let grouping_activation = ordered_activation::current_activation(conn, batch.id)?;

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
    let material_type_needs_confirmation =
        material_revision.decision != "teacher_confirmed" && material_revision.confidence < 0.85;
    let next_action = if material_type_needs_confirmation {
        "确认一次资料类型：普通试卷、答题卡或默写"
    } else if grouping.route == "blocked" {
        "先处理缺页、重复页或页型周期异常，后续学生不能自动顺移"
    } else if !grouping_confirmed {
        "确认本批从哪位学生开始；如有人缺交，只勾选缺交学生"
    } else if grouping_activation.is_none() {
        "查看按学生归组的照片；清楚的页面一次确认，模糊页只标记需重拍"
    } else {
        next_action(&preflight.route)
    };
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
        order_policy: order_revision.sort_policy,
        order_confidence: order_revision.order_confidence,
        order_conflict_codes,
        material_type: material_revision.material_type,
        material_type_decision: material_revision.decision,
        material_type_confidence: material_revision.confidence,
        material_type_needs_confirmation,
        grouping_route: grouping.route,
        student_group_count: grouping.student_group_count,
        grouping_issue_codes,
        expected_pages_per_attempt: request.expected_pages_per_attempt,
        page_cycle_source: prepared.page_cycle.source.clone(),
        page_cycle_confidence: prepared.page_cycle.confidence,
        page_cycle_needs_teacher_input: prepared.page_cycle.needs_teacher_input
            || prepared.page_cycle.expected_pages_per_attempt != request.expected_pages_per_attempt,
        grouping_roster,
        grouping_confirmed,
        grouping_first_student_no,
        grouping_last_student_no,
        quality_review_completed: grouping_activation.is_some(),
        mapped_group_count: grouping_activation
            .as_ref()
            .map(|value| value.mapped_group_count)
            .unwrap_or_default(),
        rejected_group_count: grouping_activation
            .as_ref()
            .map(|value| value.rejected_group_count)
            .unwrap_or_default(),
        next_action: next_action.into(),
    })
}

pub(crate) fn confirm_intake_material_type(
    conn: &Connection,
    batch_id: i64,
    material_type: &str,
) -> CoreResult<MaterialTypeConfirmationResult> {
    if !matches!(
        material_type,
        "ordinary_paper" | "answer_sheet" | "dictation"
    ) {
        return Err(CoreError::Invalid("请选择普通试卷、答题卡或默写".into()));
    }
    let expected_pages: i64 = conn
        .query_row(
            "SELECT expected_pages_per_attempt
             FROM exam_ordered_grouping_revisions_v2
             WHERE ingest_batch_id=?1 AND state='active'",
            [batch_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("本批连续拍摄分组快照".into()))?;
    let material = ordered_intake::confirm_material_type(conn, batch_id, material_type, ACTOR)?;
    let grouping = ordered_intake::preview_ordered_grouping(
        conn,
        &OrderedGroupingInput {
            ingest_batch_id: batch_id,
            expected_pages_per_attempt: expected_pages,
            created_by_type: "teacher",
            created_by: Some(ACTOR),
        },
    )?;
    let grouping_issue_codes =
        ordered_intake::parse_codes(&grouping.issue_codes_json, "连续拍摄分组原因")?;
    let grouping_confirmed = ordered_intake::current_grouping_decision(conn, batch_id)?.is_some();
    let next_action = if grouping.route == "blocked" {
        "先处理缺页、重复页或页型周期异常，后续学生不能自动顺移"
    } else if !grouping_confirmed {
        "资料类型已确认；继续确认本批从哪位学生开始，以及谁缺交"
    } else {
        match material_type {
            "answer_sheet" => "已进入答题卡识别路线，等待定位客观题涂写区",
            "dictation" => "已进入默写识别路线，等待按空位对照答案并复核专名",
            _ => "已进入普通试卷路线，等待识别题目与学生答案区域",
        }
    };
    Ok(MaterialTypeConfirmationResult {
        material_type: material.material_type,
        material_type_decision: material.decision,
        material_type_confidence: material.confidence,
        grouping_route: grouping.route,
        student_group_count: grouping.student_group_count,
        grouping_issue_codes,
        next_action: next_action.into(),
    })
}

pub(crate) fn confirm_intake_grouping(
    conn: &Connection,
    batch_id: i64,
    first_student_no: &str,
    absent_student_nos: &[String],
) -> CoreResult<GroupingConfirmationResult> {
    let expected_pages: i64 = conn
        .query_row(
            "SELECT expected_pages_per_attempt
             FROM exam_ordered_grouping_revisions_v2
             WHERE ingest_batch_id=?1 AND state='active'",
            [batch_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("本批连续拍摄分组快照".into()))?;
    let decision = ordered_intake::confirm_ordered_grouping(
        conn,
        &ConfirmOrderedGroupingInput {
            ingest_batch_id: batch_id,
            expected_pages_per_attempt: expected_pages,
            first_student_no,
            absent_student_nos,
            confirmed_by: ACTOR,
        },
    )?;
    let scope: serde_json::Value = serde_json::from_str(&decision.roster_scope_json)
        .map_err(|error| CoreError::Parse(format!("学生范围确认读取失败：{error}")))?;
    let assignments: serde_json::Value = serde_json::from_str(&decision.assignments_json)
        .map_err(|error| CoreError::Parse(format!("学生页组确认读取失败：{error}")))?;
    let group_count = assignments
        .get("groups")
        .and_then(serde_json::Value::as_array)
        .map(|groups| groups.len() as i64)
        .unwrap_or_default();
    let first = scope
        .get("first_student_no")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| CoreError::Parse("学生范围缺少起始学号".into()))?;
    let last = scope
        .get("last_student_no")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| CoreError::Parse("学生范围缺少结束学号".into()))?;
    Ok(GroupingConfirmationResult {
        grouping_route: "preview_ready".into(),
        student_group_count: group_count,
        grouping_issue_codes: Vec::new(),
        grouping_confirmed: true,
        grouping_first_student_no: first.into(),
        grouping_last_student_no: last.into(),
        next_action: "照片与学生顺序已确认；请查看缩略图，清楚的页面一次确认，模糊页点选需重拍"
            .into(),
    })
}

pub(crate) fn intake_grouping_evidence(
    conn: &Connection,
    batch_id: i64,
) -> CoreResult<Vec<GroupedPageEvidence>> {
    ordered_activation::grouping_evidence(conn, batch_id)
}

pub(crate) fn confirm_intake_grouping_quality(
    conn: &Connection,
    batch_id: i64,
    rejected_page_ids: &[i64],
) -> CoreResult<GroupingQualityConfirmationResult> {
    let activation = ordered_activation::confirm_grouping_quality(
        conn,
        &ConfirmGroupingQualityInput {
            ingest_batch_id: batch_id,
            rejected_page_ids,
            confirmed_by: ACTOR,
        },
    )?;
    Ok(GroupingQualityConfirmationResult {
        quality_review_completed: true,
        mapped_group_count: activation.mapped_group_count,
        rejected_group_count: activation.rejected_group_count,
        next_action: if activation.rejected_group_count > 0 {
            format!(
                "已建立 {} 名学生的正式页面归属；{} 名学生需重拍，只扣住对应页组",
                activation.mapped_group_count, activation.rejected_group_count
            )
        } else {
            format!(
                "{} 名学生的页面质量和归属已确认；下一步按资料类型识别题区或答案位置",
                activation.mapped_group_count
            )
        },
    })
}

/// 老师为一个当前待重拍页选择新的 JPEG。新文件按 hash 归档，数据库在单一事务中
/// 追加 artifact/page/replacement/quality；该学生最后一张待重拍页补齐时才建立正式归属。
pub(crate) fn replace_intake_rejected_page(
    conn: &Connection,
    data_dir: &Path,
    batch_id: i64,
    rejected_page_id: i64,
    replacement_path: &str,
) -> CoreResult<GroupingRetakeResult> {
    let source = prepare_path(replacement_path, false, 0)?;
    if source.format != SourceFormat::Jpeg {
        return Err(CoreError::Invalid("单页重拍只支持 JPG 或 JPEG".into()));
    }
    let existing = artifacts::get_by_identity(
        conn,
        &source.expected_hash,
        ArtifactKind::Image,
        None,
        RETAKE_STUDENT_VERSION,
    )?;
    let originals_dir = data_dir.join("archive/exam/originals");
    let (archived, hash, byte_size) = archive_source(&source, &originals_dir)?;
    let original_path = source
        .path
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("重拍照片缺少本地路径".into()))?
        .to_string_lossy()
        .into_owned();
    let archived_path = archived.path.to_string_lossy().into_owned();
    let result = ordered_retake::replace_rejected_page(
        conn,
        &ReplaceRejectedPageInput {
            ingest_batch_id: batch_id,
            rejected_page_id,
            artifact: RetakeArtifactInput {
                sha256: &hash,
                byte_size,
                original_name: &source.original_name,
                original_path: &original_path,
                archived_path: &archived_path,
                processing_version: RETAKE_STUDENT_VERSION,
            },
            confirmed_by: ACTOR,
        },
    );
    if result.is_err() && existing.is_none() {
        archived.rollback_new_file();
    }
    let result = result?;
    Ok(GroupingRetakeResult {
        replacement_page_id: result.replacement.replacement_page_id,
        activated_student: result.activated_student,
        mapped_group_count: result.activation.mapped_group_count,
        rejected_group_count: result.activation.rejected_group_count,
        next_action: if result.activation.rejected_group_count == 0 {
            "重拍页已替换，全部学生页面归属现已完成；下一步按资料类型识别题区或答案位置".into()
        } else if result.activated_student {
            format!(
                "重拍页已替换并恢复该学生；仍有 {} 名学生需要补拍",
                result.activation.rejected_group_count
            )
        } else {
            "重拍页已替换；该学生还有其他页面需要补拍".into()
        },
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
               INSERT INTO students(student_no,name,class_id,enabled)
                 VALUES ('1','学生一',1,1),('2','学生二',1,1),('10','学生十',1,1);
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
    fn office_files_are_accepted_only_as_answer_sources() {
        assert_eq!(
            source_format(Path::new("答案.docx"), true).unwrap(),
            SourceFormat::Docx
        );
        assert_eq!(
            source_format(Path::new("答案.xlsx"), true).unwrap(),
            SourceFormat::Xlsx
        );
        assert!(source_format(Path::new("学生作业.docx"), false).is_err());
        assert!(source_format(Path::new("学生作业.xlsx"), false).is_err());
    }

    #[test]
    fn exif_datetime_original_is_read_without_trusting_filename() {
        let mut tiff = Vec::new();
        tiff.extend_from_slice(b"II");
        tiff.extend_from_slice(&42_u16.to_le_bytes());
        tiff.extend_from_slice(&8_u32.to_le_bytes());
        tiff.extend_from_slice(&1_u16.to_le_bytes());
        tiff.extend_from_slice(&0x8769_u16.to_le_bytes());
        tiff.extend_from_slice(&4_u16.to_le_bytes());
        tiff.extend_from_slice(&1_u32.to_le_bytes());
        tiff.extend_from_slice(&26_u32.to_le_bytes());
        tiff.extend_from_slice(&0_u32.to_le_bytes());
        tiff.extend_from_slice(&1_u16.to_le_bytes());
        tiff.extend_from_slice(&0x9003_u16.to_le_bytes());
        tiff.extend_from_slice(&2_u16.to_le_bytes());
        tiff.extend_from_slice(&20_u32.to_le_bytes());
        tiff.extend_from_slice(&44_u32.to_le_bytes());
        tiff.extend_from_slice(&0_u32.to_le_bytes());
        tiff.extend_from_slice(b"2026:07:15 02:25:00\0");
        assert_eq!(
            exif_capture_time_from_tiff(&tiff).as_deref(),
            Some("2026:07:15 02:25:00")
        );
    }

    #[test]
    fn options_only_include_confirmed_objective_assessments() {
        let conn = seed();
        let options = list_options(&conn).unwrap();
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].class_name, "八年级一班");
        assert_eq!(options[0].item_count, 1);
        assert!(options[0].is_default);
        assert_eq!(options[0].default_selection_public_id, None);
    }

    #[test]
    fn explicit_future_default_is_listed_before_later_non_default_branch() {
        let conn = seed();
        let hash = "b".repeat(64);
        conn.execute_batch(&format!(
            r#"INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,confirmed_by,confirmed_at)
               VALUES ('answer-v2',1,2,'{{"schema_version":1,"correct":false}}','confirmed',
                       '2026-07-19T08:00:00.000Z','teacher','2026-07-19T08:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,confirmed_by,confirmed_at)
               VALUES ('rubric-v2',1,2,1,'confirmed','2026-07-19T08:00:00.000Z',
                       'teacher','2026-07-19T08:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,confirmed_by,confirmed_at)
               VALUES ('link-v2',1,1,2,'confirmed','2026-07-19T08:00:00.000Z',
                       'teacher','2026-07-19T08:00:00.000Z');
               INSERT INTO exam_question_version_impact_plans_v2
                 (public_id,request_key,request_hash,question_version_id,
                  target_answer_key_version_id,target_rubric_version_id,target_link_set_id,
                  expected_preview_hash,action,impact_json,planned_by,planned_at)
               VALUES ('plan','plan-key','{hash}',1,2,2,2,'{hash}','future_only',
                       '{{"schemaVersion":1}}','teacher','2026-07-19T08:00:00.000Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,
                  supersedes_version_id,created_at,confirmed_by,confirmed_at)
               VALUES ('assessment-v2',1,2,'{hash}','fixed-template-v1','confirmed',1,
                       '2026-07-19T08:00:00.000Z','teacher','2026-07-19T08:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                  state,created_at)
               VALUES ('item-v2',2,1,2,2,2,0,1,'{{"schema_version":1}}','active',
                       '2026-07-19T08:00:00.000Z');
               INSERT INTO exam_assessment_default_version_selections_v2
                 (public_id,request_key,request_hash,assessment_id,revision,
                  previous_assessment_version_id,selected_assessment_version_id,
                  source_impact_plan_id,selected_by,selected_at)
               VALUES ('selection-v1','selection-key','{hash}',1,1,1,2,1,'teacher',
                       '2026-07-19T08:00:00.000Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,
                  supersedes_version_id,created_at,confirmed_by,confirmed_at)
               VALUES ('assessment-v3',1,3,'{hash}','fixed-template-v1','confirmed',2,
                       '2026-07-19T09:00:00.000Z','teacher','2026-07-19T09:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                  state,created_at)
               VALUES ('item-v3',3,1,2,2,2,0,1,'{{"schema_version":1}}','active',
                       '2026-07-19T09:00:00.000Z');"#
        ))
        .unwrap();
        let options = list_options(&conn).unwrap();
        assert_eq!(options.len(), 3);
        assert_eq!(options[0].assessment_version_id, 2);
        assert!(options[0].is_default);
        assert_eq!(
            options[0].default_selection_public_id.as_deref(),
            Some("selection-v1")
        );
        assert!(!options[1].is_default);
        assert_eq!(options[1].assessment_version_id, 3);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn pdf_page_count_is_suggested_before_any_database_write() {
        let root = test_root("pdf-cycle-suggestion");
        let student = root.join("student.pdf");
        std::fs::write(&student, crate::pdf_pages::two_page_pdf_fixture()).unwrap();
        let suggestion = infer_page_cycle_paths(&[student.to_string_lossy().into_owned()]).unwrap();
        assert_eq!(suggestion.expected_pages_per_attempt, 2);
        assert_eq!(suggestion.source, "pdf_document_page_count");
        assert!(suggestion.needs_teacher_input);
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
            material_type: None,
            idempotency_key: "jpeg-intake".into(),
        };
        let result = prepare_fixed_intake(&conn, &root, &request).unwrap();
        assert_eq!(result.student_document_count, 1);
        assert_eq!(result.student_page_count, 1);
        assert_eq!(result.answer_document_count, 1);
        assert_eq!(result.route, "blocked");
        assert!(result
            .reason_codes
            .iter()
            .any(|code| code == "PAGE_QUALITY_REVIEW_REQUIRED"));
        assert!(result
            .reason_codes
            .iter()
            .any(|code| code == "ANSWER_SOURCE_STRUCTURE_PENDING"));
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
            material_type: None,
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
            "exam_import_order_revisions_v2",
            "exam_material_type_revisions_v2",
            "exam_page_type_revisions_v2",
            "exam_ordered_grouping_revisions_v2",
        ] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 1, "{table}");
        }
        let decision_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM exam_ordered_grouping_decisions_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(decision_count, 0);
    }

    #[test]
    fn photo_files_are_persisted_in_natural_filename_order_with_order_snapshot() {
        let root = test_root("natural-order");
        let ten = root.join("IMG_10.jpg");
        let two = root.join("IMG_2.jpg");
        let one = root.join("IMG_1.jpg");
        write_jpeg(&ten, b"paper-ten");
        write_jpeg(&two, b"paper-two");
        write_jpeg(&one, b"paper-one");
        let conn = seed();
        let request = FixedIntakeRequest {
            assessment_version_id: 1,
            student_paths: vec![
                ten.to_string_lossy().into_owned(),
                two.to_string_lossy().into_owned(),
                one.to_string_lossy().into_owned(),
            ],
            answer_path: None,
            answer_text: None,
            expected_pages_per_attempt: 1,
            material_type: Some("ordinary_paper".into()),
            idempotency_key: "natural-order-intake".into(),
        };
        let result = prepare_fixed_intake(&conn, &root, &request).unwrap();
        let names = result
            .documents
            .iter()
            .filter(|document| document.role == "student_work")
            .map(|document| document.original_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["IMG_1.jpg", "IMG_2.jpg", "IMG_10.jpg"]);
        assert_eq!(
            result.order_policy,
            "filename_natural_exif_filetime_crosscheck_v1"
        );
        assert_eq!(result.material_type_decision, "teacher_confirmed");
        let snapshot: String = conn
            .query_row(
                "SELECT ordered_sources_json FROM exam_import_order_revisions_v2
                 WHERE state='active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
        assert_eq!(value["entries"][0]["original_request_index"], 2);
        assert_eq!(value["entries"][2]["original_request_index"], 0);
    }

    #[test]
    fn fixed_page_count_records_a_repeating_page_cycle_instead_of_unknown_types() {
        let root = test_root("fixed-page-cycle");
        let second = root.join("IMG_2.jpg");
        let first = root.join("IMG_1.jpg");
        write_jpeg(&second, b"paper-page-two");
        write_jpeg(&first, b"paper-page-one");
        let conn = seed();
        let request = FixedIntakeRequest {
            assessment_version_id: 1,
            student_paths: vec![
                second.to_string_lossy().into_owned(),
                first.to_string_lossy().into_owned(),
            ],
            answer_path: None,
            answer_text: None,
            expected_pages_per_attempt: 2,
            material_type: Some("ordinary_paper".into()),
            idempotency_key: "fixed-page-cycle-intake".into(),
        };
        let result = prepare_fixed_intake(&conn, &root, &request).unwrap();
        assert_eq!(result.grouping_route, "review_required");
        let page_types = conn
            .prepare(
                "SELECT page_type_key,decision FROM exam_page_type_revisions_v2 ORDER BY page_id",
            )
            .unwrap()
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            page_types,
            vec![
                ("page_1".into(), "suggested".into()),
                ("page_2".into(), "suggested".into())
            ]
        );
        assert!(!result
            .grouping_issue_codes
            .iter()
            .any(|code| code == "PAGE_TYPE_CYCLE_UNVERIFIED"));
    }

    #[test]
    fn low_confidence_material_type_is_confirmed_once_with_new_revision() {
        let root = test_root("material-confirm");
        let student = root.join("IMG_1.jpg");
        write_jpeg(&student, b"paper-material-confirm");
        let conn = seed();
        let request = FixedIntakeRequest {
            assessment_version_id: 1,
            student_paths: vec![student.to_string_lossy().into_owned()],
            answer_path: None,
            answer_text: None,
            expected_pages_per_attempt: 1,
            material_type: Some("auto".into()),
            idempotency_key: "material-confirm-intake".into(),
        };
        let result = prepare_fixed_intake(&conn, &root, &request).unwrap();
        assert!(result.material_type_needs_confirmation);
        let confirmed = confirm_intake_material_type(&conn, result.batch_id, "dictation").unwrap();
        assert_eq!(confirmed.material_type, "dictation");
        assert_eq!(confirmed.material_type_decision, "teacher_confirmed");
        let rows: Vec<(i64, String)> = conn
            .prepare("SELECT revision,state FROM exam_material_type_revisions_v2 ORDER BY revision")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, vec![(1, "superseded".into()), (2, "active".into())]);
    }

    #[test]
    fn teacher_confirms_start_student_and_absence_without_creating_page_match() {
        let root = test_root("grouping-confirm");
        let second = root.join("IMG_2.jpg");
        let first = root.join("IMG_1.jpg");
        write_jpeg(&second, b"paper-second-group");
        write_jpeg(&first, b"paper-first-group");
        let conn = seed();
        let request = FixedIntakeRequest {
            assessment_version_id: 1,
            student_paths: vec![
                second.to_string_lossy().into_owned(),
                first.to_string_lossy().into_owned(),
            ],
            answer_path: None,
            answer_text: None,
            expected_pages_per_attempt: 1,
            material_type: Some("ordinary_paper".into()),
            idempotency_key: "grouping-confirm-intake".into(),
        };
        let prepared = prepare_fixed_intake(&conn, &root, &request).unwrap();
        assert!(!prepared.grouping_confirmed);
        assert_eq!(prepared.grouping_roster.len(), 3);

        let confirmed =
            confirm_intake_grouping(&conn, prepared.batch_id, "1", &["2".to_string()]).unwrap();
        assert!(confirmed.grouping_confirmed);
        assert_eq!(confirmed.grouping_first_student_no, "1");
        assert_eq!(confirmed.grouping_last_student_no, "10");
        assert_eq!(confirmed.student_group_count, 2);

        let assignment_json: String = conn
            .query_row(
                "SELECT assignments_json FROM exam_ordered_grouping_decisions_v2
                 WHERE ingest_batch_id=?1 AND state='active'",
                [prepared.batch_id],
                |row| row.get(0),
            )
            .unwrap();
        let assignments: serde_json::Value = serde_json::from_str(&assignment_json).unwrap();
        let student_nos = assignments["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["student_no"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(student_nos, vec!["1", "10"]);
        let page_match_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM exam_page_match_revisions_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(page_match_count, 0);

        let retried = prepare_fixed_intake(&conn, &root, &request).unwrap();
        assert!(retried.grouping_confirmed);
        assert_eq!(retried.grouping_first_student_no.as_deref(), Some("1"));
        assert_eq!(retried.grouping_last_student_no.as_deref(), Some("10"));
        assert_eq!(retried.material_type_decision, "teacher_confirmed");
        let material_revisions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM exam_material_type_revisions_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(material_revisions, 1);
    }

    #[test]
    fn quality_confirmation_maps_clear_groups_and_only_blocks_the_retake_group() {
        let root = test_root("quality-map");
        let fourth = root.join("IMG_4.jpg");
        let second = root.join("IMG_2.jpg");
        let third = root.join("IMG_3.jpg");
        let first = root.join("IMG_1.jpg");
        write_jpeg(&fourth, b"paper-fourth-quality");
        write_jpeg(&second, b"paper-second-quality");
        write_jpeg(&third, b"paper-third-quality");
        write_jpeg(&first, b"paper-first-quality");
        let conn = seed();
        let request = FixedIntakeRequest {
            assessment_version_id: 1,
            student_paths: vec![
                fourth.to_string_lossy().into_owned(),
                second.to_string_lossy().into_owned(),
                third.to_string_lossy().into_owned(),
                first.to_string_lossy().into_owned(),
            ],
            answer_path: None,
            answer_text: None,
            expected_pages_per_attempt: 2,
            material_type: Some("ordinary_paper".into()),
            idempotency_key: "quality-map-intake".into(),
        };
        let prepared = prepare_fixed_intake(&conn, &root, &request).unwrap();
        confirm_intake_grouping(&conn, prepared.batch_id, "1", &["2".to_string()]).unwrap();
        let evidence = intake_grouping_evidence(&conn, prepared.batch_id).unwrap();
        assert_eq!(evidence.len(), 2);
        assert_eq!(
            evidence
                .iter()
                .map(|group| group.student_no.as_str())
                .collect::<Vec<_>>(),
            vec!["1", "10"]
        );

        let rejected_page_id = evidence[0].pages[0].page_id;
        let second_rejected_page_id = evidence[0].pages[1].page_id;
        assert_eq!(evidence[0].pages.len(), 2);
        assert_eq!(evidence[1].pages.len(), 2);
        let result = confirm_intake_grouping_quality(
            &conn,
            prepared.batch_id,
            &[rejected_page_id, second_rejected_page_id],
        )
        .unwrap();
        assert_eq!(
            (result.mapped_group_count, result.rejected_group_count),
            (1, 1)
        );
        let counts: (i64, i64, i64, i64) = conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_page_quality_revisions_v2),
                   (SELECT COUNT(*) FROM exam_attempts_v2),
                   (SELECT COUNT(*) FROM exam_page_match_revisions_v2),
                   (SELECT COUNT(*) FROM exam_grade_decisions_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(counts, (4, 1, 2, 0));
        let mapped_student_no: String = conn
            .query_row(
                "SELECT s.student_no FROM exam_page_match_revisions_v2 m
                 JOIN exam_attempts_v2 a ON a.id=m.attempt_id
                 JOIN students s ON s.id=a.student_id
                 WHERE m.state='active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mapped_student_no, "10");
        let rejected_group_match_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM exam_page_match_revisions_v2 m
                 JOIN exam_ingest_pages_v2 p ON p.id=m.page_id
                 WHERE p.id IN (?1,?2)",
                [evidence[0].pages[0].page_id, evidence[0].pages[1].page_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            rejected_group_match_count, 0,
            "同一学生任一页需重拍时，该学生整组都不能建立正式归属"
        );
        let rejected_state: String = conn
            .query_row(
                "SELECT state FROM exam_ingest_pages_v2 WHERE id=?1",
                [rejected_page_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rejected_state, "needs_review");

        let retried = confirm_intake_grouping_quality(
            &conn,
            prepared.batch_id,
            &[rejected_page_id, second_rejected_page_id],
        )
        .unwrap();
        assert_eq!(retried.mapped_group_count, 1);
        let attempt_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM exam_attempts_v2", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(attempt_count, 1, "相同质量确认重试不得新增 attempt");

        let retake = root.join("IMG_retake_1.jpg");
        write_jpeg(&retake, b"paper-first-quality-retake");
        let first_replaced = replace_intake_rejected_page(
            &conn,
            &root,
            prepared.batch_id,
            rejected_page_id,
            &retake.to_string_lossy(),
        )
        .unwrap();
        assert!(!first_replaced.activated_student);
        assert_eq!(
            (
                first_replaced.mapped_group_count,
                first_replaced.rejected_group_count
            ),
            (1, 1)
        );
        let refreshed = intake_grouping_evidence(&conn, prepared.batch_id).unwrap();
        assert_eq!(
            refreshed[0].pages[0].replaced_page_id,
            Some(rejected_page_id)
        );
        assert_eq!(
            refreshed[0].pages[0].page_id,
            first_replaced.replacement_page_id
        );
        assert_eq!(
            refreshed[0].pages[0].quality_result.as_deref(),
            Some("pass")
        );
        assert_eq!(refreshed[0].pages[0].match_decision, None);
        assert_eq!(
            refreshed[0].pages[1].quality_result.as_deref(),
            Some("reject")
        );

        let second_retake = root.join("IMG_retake_2.jpg");
        write_jpeg(&second_retake, b"paper-second-quality-retake");
        let replaced = replace_intake_rejected_page(
            &conn,
            &root,
            prepared.batch_id,
            second_rejected_page_id,
            &second_retake.to_string_lossy(),
        )
        .unwrap();
        assert!(replaced.activated_student);
        assert_eq!(
            (replaced.mapped_group_count, replaced.rejected_group_count),
            (2, 0)
        );
        let completed = intake_grouping_evidence(&conn, prepared.batch_id).unwrap();
        assert_eq!(
            completed[0].pages[0].match_decision.as_deref(),
            Some("teacher_confirmed")
        );
        assert_eq!(
            completed[0].pages[1].match_decision.as_deref(),
            Some("teacher_confirmed")
        );
        let states: (String, String, String, String) = conn
            .query_row(
                "SELECT
                   (SELECT state FROM exam_ingest_pages_v2 WHERE id=?1),
                   (SELECT state FROM exam_ingest_pages_v2 WHERE id=?2),
                   (SELECT state FROM exam_ingest_pages_v2 WHERE id=?3),
                   (SELECT state FROM exam_ingest_pages_v2 WHERE id=?4)",
                [
                    rejected_page_id,
                    second_rejected_page_id,
                    first_replaced.replacement_page_id,
                    replaced.replacement_page_id,
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            states,
            (
                "voided".into(),
                "voided".into(),
                "matched".into(),
                "matched".into()
            )
        );
        let final_counts: (i64, i64, i64, i64) = conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_attempts_v2),
                   (SELECT COUNT(*) FROM exam_page_match_revisions_v2 WHERE state='active'),
                   (SELECT COUNT(*) FROM exam_ordered_grouping_activations_v2),
                   (SELECT COUNT(*) FROM exam_grade_decisions_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(final_counts, (2, 4, 3, 0));
    }

    #[test]
    fn invalid_retake_page_rolls_back_without_quality_or_identity_facts() {
        let root = test_root("quality-invalid");
        let student = root.join("IMG_1.jpg");
        write_jpeg(&student, b"paper-quality-invalid");
        let conn = seed();
        let request = FixedIntakeRequest {
            assessment_version_id: 1,
            student_paths: vec![student.to_string_lossy().into_owned()],
            answer_path: None,
            answer_text: None,
            expected_pages_per_attempt: 1,
            material_type: Some("ordinary_paper".into()),
            idempotency_key: "quality-invalid-intake".into(),
        };
        let prepared = prepare_fixed_intake(&conn, &root, &request).unwrap();
        confirm_intake_grouping(&conn, prepared.batch_id, "1", &[]).unwrap();
        assert!(confirm_intake_grouping_quality(&conn, prepared.batch_id, &[999]).is_err());
        let counts: (i64, i64, i64) = conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_page_quality_revisions_v2),
                   (SELECT COUNT(*) FROM exam_attempts_v2),
                   (SELECT COUNT(*) FROM exam_page_match_revisions_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(counts, (0, 0, 0));
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
            material_type: None,
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
            material_type: None,
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
            material_type: None,
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
