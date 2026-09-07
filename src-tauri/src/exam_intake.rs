//! T6.1b 固定试卷一站式入站：本地归档、PDF 单页拆分、B1 页面登记与 B3a 预检。
//!
//! 本模块只把老师选择的资料变成不可变 artifact 和可恢复入站事实；不调用 OCR/OMR，
//! 不创建机器评分、老师判定或成绩发布。上传答案在本批仅登记为 teaching_content 资料，
//! 不能伪装成老师已经确认的正式答案。

mod contracts;
mod files;

pub use contracts::{
    FixedIntakeDocumentSummary, FixedIntakeOption, FixedIntakeRequest, FixedIntakeResult,
    GroupingConfirmationResult, GroupingQualityConfirmationResult, GroupingRetakeResult,
    GroupingRosterStudent, MaterialTypeConfirmationResult, PageCycleSuggestion,
};
pub(crate) use files::{archive_bytes, ArchivedFile};
use files::{archive_source, prepare_path, prepare_text, PreparedSource, SourceFormat};
#[cfg(test)]
use files::{exif_capture_time_from_tiff, source_format};

use std::path::Path;

use rusqlite::{Connection, OptionalExtension};
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

const ACTOR: &str = "teacher";
const ORIGINAL_STUDENT_VERSION: &str = "exam-intake-original-student-v1";
const ORIGINAL_ANSWER_VERSION: &str = "exam-intake-original-answer-v1";
const PDF_PAGE_VERSION: &str = "exam-intake-pdf-page-v1";
const RETAKE_STUDENT_VERSION: &str = "exam-intake-retake-student-v1";

pub(crate) struct PreparedFixedIntake {
    student_sources: Vec<PreparedSource>,
    answer_source: Option<PreparedSource>,
    order_confidence: f64,
    order_conflict_codes: Vec<String>,
    page_cycle: PageCycleSuggestion,
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
#[path = "exam_intake_tests.rs"]
mod tests;
