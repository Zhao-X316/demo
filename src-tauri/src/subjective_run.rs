//! 答题卡主观题区的手写 OCR run 编排。
//!
//! 文件读取和供应商调用均在数据库锁外；OCR 请求不加载答案或 rubric。

use std::path::PathBuf;

use module_exam::dictation_recognition::{
    DictationFailure, DictationOcrOutput, DictationOcrRequest, DictationRecognizerDescriptor,
};
use module_exam::service::subjective::{self, SubjectiveTranscriptionRevision};
use rusqlite::Connection;
use suite_core::db::repo::{ai_runs, artifacts};
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRunStatus, ArchiveStatus, ArtifactKind};

pub struct SubjectiveOcrRunMetadata {
    answer_region_revision_id: i64,
    input_artifact_id: i64,
    input_artifact_sha256: String,
    mime_type: String,
    archived_path: PathBuf,
}

pub struct SubjectiveOcrRunInput {
    answer_region_revision_id: i64,
    input_artifact_id: i64,
    input_artifact_sha256: String,
    mime_type: String,
    image_bytes: Vec<u8>,
    input_hash: String,
}

impl SubjectiveOcrRunInput {
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

pub enum BeginSubjectiveOcrRun {
    Execute { ai_run_id: i64 },
    Completed(Box<SubjectiveTranscriptionRevision>),
}

pub fn load_metadata(
    conn: &Connection,
    answer_region_revision_id: i64,
) -> CoreResult<SubjectiveOcrRunMetadata> {
    let scope = subjective::load_region_scope(conn, answer_region_revision_id)?;
    let artifact = artifacts::get_by_id(conn, scope.crop_artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{}", scope.crop_artifact_id)))?;
    if artifact.archive_status != ArchiveStatus::Ready || artifact.kind != ArtifactKind::Crop {
        return Err(CoreError::Invalid("主观题区裁剪当前不可读取".into()));
    }
    Ok(SubjectiveOcrRunMetadata {
        answer_region_revision_id,
        input_artifact_id: artifact.id,
        input_artifact_sha256: artifact.sha256,
        mime_type: artifact.mime_type,
        archived_path: artifact.archived_path.into(),
    })
}

pub fn load_input(metadata: SubjectiveOcrRunMetadata) -> CoreResult<SubjectiveOcrRunInput> {
    let image_bytes = std::fs::read(&metadata.archived_path)
        .map_err(|_| CoreError::Invalid("主观题区裁剪缺失或不可读".into()))?;
    if hashing::sha256_hex(&image_bytes) != metadata.input_artifact_sha256 {
        return Err(CoreError::Invalid("主观题区裁剪与登记 hash 不一致".into()));
    }
    let mut input = SubjectiveOcrRunInput {
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

pub fn begin(
    conn: &mut Connection,
    input: &SubjectiveOcrRunInput,
    descriptor: &DictationRecognizerDescriptor,
    idempotency_key: &str,
) -> CoreResult<BeginSubjectiveOcrRun> {
    descriptor.validate()?;
    if idempotency_key.trim().is_empty() {
        return Err(CoreError::Invalid("手写 OCR 幂等键不能为空".into()));
    }
    let business_ref_id = input.answer_region_revision_id.to_string();
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: idempotency_key.trim(),
            run_type: "handwriting_ocr",
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
            Ok(BeginSubjectiveOcrRun::Execute { ai_run_id: run.id })
        }
        AiRunStatus::Succeeded | AiRunStatus::Failed => {
            Ok(BeginSubjectiveOcrRun::Completed(Box::new(
                subjective::record_ocr_ai_run_transcription(conn, run.id)?,
            )))
        }
        AiRunStatus::Processing => Err(CoreError::Invalid(
            "该主观题区正在识别，请勿重复提交".into(),
        )),
        AiRunStatus::Voided => Err(CoreError::Invalid(
            "该手写 OCR 已作废，请使用新的幂等键重试".into(),
        )),
    }
}

pub fn finish(
    conn: &mut Connection,
    input: &SubjectiveOcrRunInput,
    ai_run_id: i64,
    result: Result<DictationOcrOutput, DictationFailure>,
) -> CoreResult<SubjectiveTranscriptionRevision> {
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
                    safe_message: "手写 OCR 未通过安全校验，已转入老师复核".into(),
                    retryable: false,
                };
                ai_runs::finalize_failed(conn, ai_run_id, &failure.to_json()?, &finished_at)?;
            }
        },
        Err(failure) => {
            ai_runs::finalize_failed(conn, ai_run_id, &failure.to_json()?, &finished_at)?;
        }
    }
    subjective::record_ocr_ai_run_transcription(conn, ai_run_id)
}
