//! 固定版式默写的空白模板与逐题 OCR run 编排。
//!
//! 空白模板只定位题区；逐题 OCR 只读取学生裁剪，故意不加载答案或 rubric。
//! 文件读取和供应商调用都由命令层放在数据库锁外。

use std::path::{Path, PathBuf};

use module_exam::dictation_recognition::{
    DictationFailure, DictationItemSpec, DictationOcrOutput, DictationOcrRequest,
    DictationQuestionType, DictationRecognizerDescriptor, DictationTemplateOutput,
    DictationTemplateRequest, DictationTemplateState,
};
use module_exam::service::dictation_pipeline::{
    self, DictationTemplateConfirmation, DictationTemplateRevision, DictationTranscriptionResult,
};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use suite_core::db::repo::{ai_runs, artifacts};
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRun, AiRunStatus, ArchiveStatus, Artifact, ArtifactKind, PrivacyClass};

use crate::exam_intake::{archive_bytes, register_artifact};

const BLANK_TEMPLATE_PROCESSING_VERSION: &str = "fixed-dictation-blank-template-v1";

pub struct PreparedDictationTemplate {
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub mime_type: String,
    pub original_name: String,
    pub original_path: PathBuf,
    pub archived: crate::exam_intake::ArchivedFile,
}

pub struct DictationTemplateRunInput {
    assessment_version_id: i64,
    page_no: i64,
    template_version: String,
    blank_artifact: Artifact,
    image_bytes: Vec<u8>,
    items: Vec<DictationItemSpec>,
    input_hash: String,
}

impl DictationTemplateRunInput {
    pub fn request(&self) -> DictationTemplateRequest<'_> {
        DictationTemplateRequest {
            assessment_version_id: self.assessment_version_id,
            page_no: self.page_no,
            blank_artifact_id: self.blank_artifact.id,
            blank_artifact_sha256: &self.blank_artifact.sha256,
            mime_type: &self.blank_artifact.mime_type,
            image_bytes: &self.image_bytes,
            template_version: &self.template_version,
            items: &self.items,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DictationTemplateRunResult {
    pub ai_run_id: i64,
    pub status: String,
    pub output: Option<DictationTemplateOutput>,
    pub failure: Option<DictationFailure>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationTemplateStatus {
    pub assessment_version_id: i64,
    pub page_no: i64,
    pub active_template: Option<DictationTemplateRevision>,
}

pub enum BeginDictationTemplateRun {
    Execute { ai_run_id: i64 },
    Completed(Box<DictationTemplateRunResult>),
}

pub struct DictationOcrRunMetadata {
    answer_region_revision_id: i64,
    input_artifact_id: i64,
    input_artifact_sha256: String,
    mime_type: String,
    archived_path: PathBuf,
}

pub struct DictationOcrRunInput {
    answer_region_revision_id: i64,
    input_artifact_id: i64,
    input_artifact_sha256: String,
    mime_type: String,
    image_bytes: Vec<u8>,
    input_hash: String,
}

impl DictationOcrRunInput {
    pub fn request(&self) -> DictationOcrRequest<'_> {
        DictationOcrRequest {
            answer_region_revision_id: self.answer_region_revision_id,
            crop_artifact_id: self.input_artifact_id,
            crop_artifact_sha256: &self.input_artifact_sha256,
            mime_type: &self.mime_type,
            image_bytes: &self.image_bytes,
        }
    }
}

pub enum BeginDictationOcrRun {
    Execute { ai_run_id: i64 },
    Completed(Box<DictationTranscriptionResult>),
}

fn image_format(path: &Path) -> CoreResult<(&'static str, &'static str)> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Ok(("image/jpeg", "jpg")),
        "png" => Ok(("image/png", "png")),
        "webp" => Ok(("image/webp", "webp")),
        _ => Err(CoreError::Invalid(
            "默写空白页只支持 JPG、PNG 或 WebP 图片".into(),
        )),
    }
}

pub fn prepare_blank_template(
    path: &str,
    data_dir: &Path,
) -> CoreResult<PreparedDictationTemplate> {
    let original_path = PathBuf::from(path);
    if !original_path.is_file() {
        return Err(CoreError::Invalid("默写空白页文件不存在".into()));
    }
    let (mime_type, extension) = image_format(&original_path)?;
    let bytes = std::fs::read(&original_path)
        .map_err(|error| CoreError::Io(format!("读取默写空白页失败：{error}")))?;
    image::load_from_memory(&bytes)
        .map_err(|_| CoreError::Invalid("默写空白页图片无法解码".into()))?;
    let sha256 = hashing::sha256_hex(&bytes);
    let archived = archive_bytes(
        &bytes,
        &sha256,
        extension,
        &data_dir.join("archive/exam/dictation/templates"),
    )?;
    Ok(PreparedDictationTemplate {
        bytes,
        sha256,
        mime_type: mime_type.into(),
        original_name: original_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("blank-dictation")
            .into(),
        original_path,
        archived,
    })
}

pub fn register_blank_template(
    conn: &Connection,
    prepared: &PreparedDictationTemplate,
) -> CoreResult<Artifact> {
    register_artifact(
        conn,
        &prepared.archived,
        &prepared.sha256,
        prepared.bytes.len() as i64,
        ArtifactKind::Image,
        &prepared.mime_type,
        Some(&prepared.original_name),
        Some(&prepared.original_path),
        None,
        None,
        BLANK_TEMPLATE_PROCESSING_VERSION,
        PrivacyClass::TeachingContent,
    )
}

fn page_scope(conn: &Connection, reference_page_id: i64) -> CoreResult<(i64, i64, String)> {
    if reference_page_id <= 0 {
        return Err(CoreError::Invalid("默写参考页面 id 必须为正数".into()));
    }
    conn.query_row(
        "SELECT at.assessment_version_id,m.page_no,v.template_version
         FROM exam_ingest_pages_v2 p
         JOIN exam_page_quality_revisions_v2 q
           ON q.page_id=p.id AND q.state='active' AND q.result='pass'
         JOIN exam_page_match_revisions_v2 m
           ON m.page_id=p.id AND m.state='active' AND m.decision='teacher_confirmed'
         JOIN exam_attempts_v2 at ON at.id=m.attempt_id AND at.state<>'voided'
         JOIN exam_assessment_versions_v2 v
           ON v.id=at.assessment_version_id AND v.state='confirmed'
         JOIN exam_material_type_revisions_v2 mt
           ON mt.ingest_batch_id=p.batch_id AND mt.state='active'
              AND mt.decision='teacher_confirmed' AND mt.material_type='dictation'
         WHERE p.id=?1 AND p.state<>'voided'",
        [reference_page_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        },
    )
    .optional()?
    .ok_or_else(|| {
        CoreError::Invalid("默写模板只允许绑定质量、学生/页码和材料类型均已确认的页面".into())
    })
    .and_then(|(assessment_version_id, page_no, template_version)| {
        let template_version = template_version
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| CoreError::Invalid("当前作业尚未固定默写模板版本".into()))?;
        Ok((assessment_version_id, page_no, template_version))
    })
}

fn load_items(
    conn: &Connection,
    assessment_version_id: i64,
    page_no: i64,
) -> CoreResult<Vec<DictationItemSpec>> {
    let mut stmt = conn.prepare(
        "SELECT i.id,i.order_index,q.question_type
         FROM exam_assessment_items_v2 i
         JOIN k1_question_versions q ON q.id=i.question_version_id
         WHERE i.assessment_version_id=?1 AND i.state='active'
           AND COALESCE(CAST(json_extract(i.presentation_snapshot_json,'$.page_no') AS INTEGER),1)=?2
         ORDER BY i.order_index,i.id",
    )?;
    let rows = stmt.query_map((assessment_version_id, page_no), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut items = Vec::new();
    for row in rows {
        let (assessment_item_id, order_index, question_type) = row?;
        let question_type = DictationQuestionType::from_db(&question_type)
            .ok_or_else(|| CoreError::Invalid("固定默写当前只接受填空或简答作业项".into()))?;
        items.push(DictationItemSpec {
            assessment_item_id,
            order_index,
            question_type,
        });
    }
    if items.is_empty() {
        return Err(CoreError::Invalid("当前默写页没有可绑定的题目".into()));
    }
    Ok(items)
}

pub fn load_template_input(
    conn: &Connection,
    reference_page_id: i64,
    blank_artifact_id: i64,
) -> CoreResult<DictationTemplateRunInput> {
    let (assessment_version_id, page_no, template_version) = page_scope(conn, reference_page_id)?;
    let blank_artifact = artifacts::get_by_id(conn, blank_artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{blank_artifact_id}")))?;
    if blank_artifact.archive_status != ArchiveStatus::Ready
        || blank_artifact.privacy_class != PrivacyClass::TeachingContent
        || !matches!(
            blank_artifact.kind,
            ArtifactKind::Image | ArtifactKind::Page
        )
    {
        return Err(CoreError::Invalid(
            "默写空白页必须是已归档的教学内容图片".into(),
        ));
    }
    let image_bytes = std::fs::read(&blank_artifact.archived_path)
        .map_err(|_| CoreError::Invalid("默写空白页归档缺失或不可读".into()))?;
    if hashing::sha256_hex(&image_bytes) != blank_artifact.sha256 {
        return Err(CoreError::Invalid("默写空白页归档 hash 不一致".into()));
    }
    let items = load_items(conn, assessment_version_id, page_no)?;
    let mut input = DictationTemplateRunInput {
        assessment_version_id,
        page_no,
        template_version,
        blank_artifact,
        image_bytes,
        items,
        input_hash: String::new(),
    };
    input.input_hash = input.request().input_hash()?;
    Ok(input)
}

pub fn template_status(
    conn: &Connection,
    reference_page_id: i64,
) -> CoreResult<DictationTemplateStatus> {
    let (assessment_version_id, page_no, _) = page_scope(conn, reference_page_id)?;
    Ok(DictationTemplateStatus {
        assessment_version_id,
        page_no,
        active_template: dictation_pipeline::get_active_template(
            conn,
            assessment_version_id,
            page_no,
        )?,
    })
}

pub fn begin_template(
    conn: &Connection,
    input: &DictationTemplateRunInput,
    descriptor: &DictationRecognizerDescriptor,
    idempotency_key: &str,
) -> CoreResult<BeginDictationTemplateRun> {
    descriptor.validate()?;
    if idempotency_key.trim().is_empty() {
        return Err(CoreError::Invalid("默写模板分析幂等键不能为空".into()));
    }
    let business_ref_id = format!("{}:{}", input.assessment_version_id, input.page_no);
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: idempotency_key.trim(),
            run_type: "dictation_template",
            source_module: "exam",
            business_ref_type: "assessment_page",
            business_ref_id: &business_ref_id,
            input_artifact_id: Some(input.blank_artifact.id),
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
            Ok(BeginDictationTemplateRun::Execute { ai_run_id: run.id })
        }
        AiRunStatus::Succeeded | AiRunStatus::Failed => Ok(BeginDictationTemplateRun::Completed(
            Box::new(template_result_from_run(input, &run)?),
        )),
        AiRunStatus::Processing => Err(CoreError::Invalid(
            "该默写空白页正在分析，请勿重复提交".into(),
        )),
        AiRunStatus::Voided => Err(CoreError::Invalid(
            "该默写模板分析已作废，请使用新的幂等键重试".into(),
        )),
    }
}

pub fn finish_template(
    conn: &Connection,
    input: &DictationTemplateRunInput,
    ai_run_id: i64,
    result: Result<DictationTemplateOutput, DictationFailure>,
) -> CoreResult<DictationTemplateRunResult> {
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
                let failure = DictationFailure {
                    schema_version: 1,
                    code: module_exam::dictation_recognition::DictationErrorCode::InvalidOutput,
                    safe_message: "默写模板未通过安全校验，已转入老师复核".into(),
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
    template_result_from_run(input, &run)
}

fn template_result_from_run(
    input: &DictationTemplateRunInput,
    run: &AiRun,
) -> CoreResult<DictationTemplateRunResult> {
    match run.status {
        AiRunStatus::Succeeded => {
            let output: DictationTemplateOutput = serde_json::from_str(
                run.output_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("成功的默写模板 run 缺少输出".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("默写模板输出无法解析：{error}")))?;
            output.validate_against(&input.request())?;
            Ok(DictationTemplateRunResult {
                ai_run_id: run.id,
                status: "succeeded".into(),
                output: Some(output),
                failure: None,
            })
        }
        AiRunStatus::Failed => {
            let failure: DictationFailure = serde_json::from_str(
                run.error_meta_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("失败的默写模板 run 缺少错误信息".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("默写模板错误无法解析：{error}")))?;
            Ok(DictationTemplateRunResult {
                ai_run_id: run.id,
                status: "failed".into(),
                output: None,
                failure: Some(failure),
            })
        }
        _ => Err(CoreError::Invalid("默写模板 run 尚未形成最终结果".into())),
    }
}

pub fn confirm_template(
    conn: &mut Connection,
    reference_page_id: i64,
    ai_run_id: i64,
    confirmed_by: &str,
) -> CoreResult<DictationTemplateConfirmation> {
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    if run.status != AiRunStatus::Succeeded || run.run_type != "dictation_template" {
        return Err(CoreError::Invalid("只能确认已成功的默写模板候选".into()));
    }
    let blank_artifact_id = run
        .input_artifact_id
        .ok_or_else(|| CoreError::Invalid("默写模板 run 缺少空白图".into()))?;
    let input = load_template_input(conn, reference_page_id, blank_artifact_id)?;
    let output: DictationTemplateOutput = serde_json::from_str(
        run.output_json
            .as_deref()
            .ok_or_else(|| CoreError::Invalid("默写模板 run 缺少候选输出".into()))?,
    )
    .map_err(|error| CoreError::Parse(format!("默写模板候选无法解析：{error}")))?;
    output.validate_against(&input.request())?;
    if output.state != DictationTemplateState::Ready {
        return Err(CoreError::Invalid(
            "有分歧或受阻的默写模板不能直接确认".into(),
        ));
    }
    dictation_pipeline::confirm_template_and_policies(conn, &output, ai_run_id, confirmed_by)
}

pub fn load_ocr_metadata(
    conn: &Connection,
    answer_region_revision_id: i64,
) -> CoreResult<DictationOcrRunMetadata> {
    if answer_region_revision_id <= 0 {
        return Err(CoreError::Invalid("默写题区 id 必须为正数".into()));
    }
    let crop_artifact_id = conn
        .query_row(
            "SELECT r.crop_artifact_id
             FROM exam_answer_region_revisions_v2 r
             JOIN exam_page_alignment_revisions_v2 al
               ON al.id=r.alignment_revision_id AND al.state='active'
                  AND al.decision='teacher_confirmed'
             JOIN exam_page_match_revisions_v2 m
               ON m.id=al.match_revision_id AND m.state='active'
                  AND m.decision='teacher_confirmed'
             JOIN exam_attempts_v2 at ON at.id=m.attempt_id AND at.state<>'voided'
             JOIN exam_dictation_policy_revisions_v2 p
               ON p.assessment_item_id=r.assessment_item_id AND p.state='active'
             WHERE r.id=?1 AND r.state='active' AND r.decision='teacher_confirmed'",
            [answer_region_revision_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("默写 OCR 题区尚未完成老师确认".into()))?;
    let artifact = artifacts::get_by_id(conn, crop_artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{crop_artifact_id}")))?;
    if artifact.archive_status != ArchiveStatus::Ready || artifact.kind != ArtifactKind::Crop {
        return Err(CoreError::Invalid("默写题区裁剪当前不可读取".into()));
    }
    Ok(DictationOcrRunMetadata {
        answer_region_revision_id,
        input_artifact_id: artifact.id,
        input_artifact_sha256: artifact.sha256,
        mime_type: artifact.mime_type,
        archived_path: artifact.archived_path.into(),
    })
}

pub fn load_ocr_input(metadata: DictationOcrRunMetadata) -> CoreResult<DictationOcrRunInput> {
    let image_bytes = std::fs::read(&metadata.archived_path)
        .map_err(|_| CoreError::Invalid("默写题区裁剪缺失或不可读".into()))?;
    if hashing::sha256_hex(&image_bytes) != metadata.input_artifact_sha256 {
        return Err(CoreError::Invalid("默写题区裁剪与登记 hash 不一致".into()));
    }
    let mut input = DictationOcrRunInput {
        answer_region_revision_id: metadata.answer_region_revision_id,
        input_artifact_id: metadata.input_artifact_id,
        input_artifact_sha256: metadata.input_artifact_sha256,
        mime_type: metadata.mime_type,
        image_bytes,
        input_hash: String::new(),
    };
    input.input_hash = input.request().input_hash()?;
    Ok(input)
}

pub fn begin_ocr(
    conn: &mut Connection,
    input: &DictationOcrRunInput,
    descriptor: &DictationRecognizerDescriptor,
    idempotency_key: &str,
) -> CoreResult<BeginDictationOcrRun> {
    descriptor.validate()?;
    if idempotency_key.trim().is_empty() {
        return Err(CoreError::Invalid("默写 OCR 幂等键不能为空".into()));
    }
    let business_ref_id = input.answer_region_revision_id.to_string();
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: idempotency_key.trim(),
            run_type: "dictation_ocr",
            source_module: "exam",
            business_ref_type: "answer_region_revision",
            business_ref_id: &business_ref_id,
            input_artifact_id: Some(input.input_artifact_id),
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
            Ok(BeginDictationOcrRun::Execute { ai_run_id: run.id })
        }
        AiRunStatus::Succeeded | AiRunStatus::Failed => {
            Ok(BeginDictationOcrRun::Completed(Box::new(
                dictation_pipeline::record_ocr_ai_run_transcription(conn, run.id)?,
            )))
        }
        AiRunStatus::Processing => Err(CoreError::Invalid(
            "该默写题区正在识别，请勿重复提交".into(),
        )),
        AiRunStatus::Voided => Err(CoreError::Invalid(
            "该默写 OCR 已作废，请使用新的幂等键重试".into(),
        )),
    }
}

pub fn finish_ocr(
    conn: &mut Connection,
    input: &DictationOcrRunInput,
    ai_run_id: i64,
    result: Result<DictationOcrOutput, DictationFailure>,
) -> CoreResult<DictationTranscriptionResult> {
    let finished_at = time::utc_now_rfc3339();
    match result {
        Ok(output) => match output.to_json_against(&input.request()) {
            Ok(output_json) => {
                ai_runs::finalize_succeeded(
                    conn,
                    ai_run_id,
                    &hashing::sha256_hex(output_json.as_bytes()),
                    output.confidence,
                    &output_json,
                    &finished_at,
                )?;
            }
            Err(_) => {
                let failure = DictationFailure {
                    schema_version: 1,
                    code: module_exam::dictation_recognition::DictationErrorCode::InvalidOutput,
                    safe_message: "默写 OCR 未通过安全校验，已转入老师复核".into(),
                    retryable: false,
                };
                ai_runs::finalize_failed(conn, ai_run_id, &failure.to_json()?, &finished_at)?;
            }
        },
        Err(failure) => {
            ai_runs::finalize_failed(conn, ai_run_id, &failure.to_json()?, &finished_at)?;
        }
    }
    dictation_pipeline::record_ocr_ai_run_transcription(conn, ai_run_id)
}
