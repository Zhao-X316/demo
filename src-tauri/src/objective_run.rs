//! 固定卷客观题识别 run 编排。
//!
//! 数据库锁只用于读取已确认链、抢占 run 和完成落账；图片读取与外部网络调用均在
//! 锁外执行。provider 成功或失败都先写不可变 `ai_run`，再映射进既有 observation。

use std::path::PathBuf;

use module_exam::objective_recognition::{
    ObjectiveMarkCell, ObjectiveQuestionType, ObjectiveRecognitionFailure,
    ObjectiveRecognitionOutput, ObjectiveRecognitionRequest, ObjectiveRecognizerDescriptor,
};
use module_exam::service::objective::{self, ObjectiveObservationResult};
use rusqlite::{Connection, OptionalExtension};
use suite_core::db::repo::{ai_runs, artifacts};
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRunStatus, ArchiveStatus};

pub struct ObjectiveRunMetadata {
    answer_region_revision_id: i64,
    input_artifact_id: i64,
    input_artifact_sha256: String,
    mime_type: String,
    archived_path: PathBuf,
    question_type: ObjectiveQuestionType,
    template_version: String,
    cells: Vec<ObjectiveMarkCell>,
}

pub struct ObjectiveRunInput {
    answer_region_revision_id: i64,
    input_artifact_id: i64,
    input_artifact_sha256: String,
    mime_type: String,
    image_bytes: Vec<u8>,
    question_type: ObjectiveQuestionType,
    template_version: String,
    cells: Vec<ObjectiveMarkCell>,
    input_hash: String,
}

impl ObjectiveRunInput {
    pub fn request(&self) -> ObjectiveRecognitionRequest<'_> {
        ObjectiveRecognitionRequest {
            answer_region_revision_id: self.answer_region_revision_id,
            input_artifact_id: self.input_artifact_id,
            input_artifact_sha256: &self.input_artifact_sha256,
            mime_type: &self.mime_type,
            image_bytes: &self.image_bytes,
            question_type: self.question_type,
            template_version: &self.template_version,
            cells: &self.cells,
        }
    }

    pub fn answer_region_revision_id(&self) -> i64 {
        self.answer_region_revision_id
    }

    pub fn input_artifact_id(&self) -> i64 {
        self.input_artifact_id
    }

    pub fn input_hash(&self) -> &str {
        &self.input_hash
    }
}

pub enum BeginObjectiveRun {
    Execute { ai_run_id: i64 },
    Completed(Box<ObjectiveObservationResult>),
}

pub fn load_metadata(
    conn: &Connection,
    answer_region_revision_id: i64,
) -> CoreResult<ObjectiveRunMetadata> {
    if answer_region_revision_id <= 0 {
        return Err(CoreError::Invalid("客观题题区 id 必须为正数".into()));
    }
    let (bbox_json, crop_artifact_id, question_type, template_version) = conn
        .query_row(
            "SELECT r.bbox_json,r.crop_artifact_id,q.question_type,al.template_version
             FROM exam_answer_region_revisions_v2 r
             JOIN exam_page_alignment_revisions_v2 al
               ON al.id=r.alignment_revision_id AND al.state='active'
                  AND al.decision='teacher_confirmed'
             JOIN exam_page_match_revisions_v2 m
               ON m.id=al.match_revision_id AND m.state='active'
                  AND m.decision='teacher_confirmed'
             JOIN exam_attempts_v2 at ON at.id=m.attempt_id AND at.state<>'voided'
             JOIN exam_assessment_items_v2 i
               ON i.id=r.assessment_item_id
                  AND i.assessment_version_id=at.assessment_version_id AND i.state='active'
             JOIN k1_question_versions q ON q.id=i.question_version_id
             JOIN k1_answer_key_versions ak
               ON ak.id=i.answer_key_version_id AND ak.state='confirmed'
             WHERE r.id=?1 AND r.state='active' AND r.decision='teacher_confirmed'",
            [answer_region_revision_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid(
                "真实识别只允许处理当前老师已确认的学生、页面、配准、题区和答案版本".into(),
            )
        })?;
    let question_type = ObjectiveQuestionType::from_db(&question_type)
        .ok_or_else(|| CoreError::Invalid("当前题区不是支持的选择或判断题".into()))?;
    let bbox: serde_json::Value = serde_json::from_str(&bbox_json)
        .map_err(|error| CoreError::Invalid(format!("题区坐标 JSON 无效：{error}")))?;
    let cells: Vec<ObjectiveMarkCell> =
        serde_json::from_value(bbox.get("mark_cells").cloned().ok_or_else(|| {
            CoreError::Invalid("题区尚未确认答题格坐标，不能让模型猜测；请先补录模板".into())
        })?)
        .map_err(|error| CoreError::Invalid(format!("答题格坐标无效：{error}")))?;
    let artifact = artifacts::get_by_id(conn, crop_artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{crop_artifact_id}")))?;
    if artifact.archive_status != ArchiveStatus::Ready {
        return Err(CoreError::Invalid("题区裁剪文件当前不可读取".into()));
    }
    Ok(ObjectiveRunMetadata {
        answer_region_revision_id,
        input_artifact_id: artifact.id,
        input_artifact_sha256: artifact.sha256,
        mime_type: artifact.mime_type,
        archived_path: artifact.archived_path.into(),
        question_type,
        template_version,
        cells,
    })
}

/// 在数据库锁外读取不可变裁剪，并再次核对内容 hash。
pub fn load_input(metadata: ObjectiveRunMetadata) -> CoreResult<ObjectiveRunInput> {
    let image_bytes = std::fs::read(&metadata.archived_path)
        .map_err(|_| CoreError::Invalid("题区裁剪文件缺失或不可读".into()))?;
    if hashing::sha256_hex(&image_bytes) != metadata.input_artifact_sha256 {
        return Err(CoreError::Invalid(
            "题区裁剪文件与已登记 artifact hash 不一致".into(),
        ));
    }
    let mut input = ObjectiveRunInput {
        answer_region_revision_id: metadata.answer_region_revision_id,
        input_artifact_id: metadata.input_artifact_id,
        input_artifact_sha256: metadata.input_artifact_sha256,
        mime_type: metadata.mime_type,
        image_bytes,
        question_type: metadata.question_type,
        template_version: metadata.template_version,
        cells: metadata.cells,
        input_hash: String::new(),
    };
    input.input_hash = input.request().input_hash()?;
    Ok(input)
}

pub fn begin(
    conn: &Connection,
    input: &ObjectiveRunInput,
    descriptor: &ObjectiveRecognizerDescriptor,
    idempotency_key: &str,
) -> CoreResult<BeginObjectiveRun> {
    descriptor.validate()?;
    if idempotency_key.trim().is_empty() {
        return Err(CoreError::Invalid("客观题识别幂等键不能为空".into()));
    }
    let business_ref_id = input.answer_region_revision_id().to_string();
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: idempotency_key.trim(),
            run_type: "omr",
            source_module: "exam",
            business_ref_type: "answer_region_revision",
            business_ref_id: &business_ref_id,
            input_artifact_id: Some(input.input_artifact_id()),
            provider: &descriptor.provider,
            model_name: &descriptor.model_name,
            model_version: &descriptor.model_version,
            config_version: &descriptor.config_version,
            prompt_or_rule_version: &descriptor.rule_version,
            input_hash: input.input_hash(),
            retry_of_ai_run_id: None,
        },
    )?;
    match run.status {
        AiRunStatus::Pending => {
            ai_runs::start(conn, run.id, &time::utc_now_rfc3339(), None)?;
            Ok(BeginObjectiveRun::Execute { ai_run_id: run.id })
        }
        AiRunStatus::Succeeded | AiRunStatus::Failed => Ok(BeginObjectiveRun::Completed(Box::new(
            objective::record_omr_ai_run_observation(
                conn,
                run.id,
                &observation_idempotency_key(idempotency_key),
            )?,
        ))),
        AiRunStatus::Processing => Err(CoreError::Invalid("该题区正在识别，请勿重复提交".into())),
        AiRunStatus::Voided => Err(CoreError::Invalid(
            "该识别任务已作废，请使用新的幂等键重试".into(),
        )),
    }
}

pub fn finish(
    conn: &Connection,
    ai_run_id: i64,
    idempotency_key: &str,
    result: Result<ObjectiveRecognitionOutput, ObjectiveRecognitionFailure>,
) -> CoreResult<ObjectiveObservationResult> {
    let finished_at = time::utc_now_rfc3339();
    match result {
        Ok(output) => {
            let output_json = output.to_json()?;
            ai_runs::finalize_succeeded(
                conn,
                ai_run_id,
                &hashing::sha256_hex(output_json.as_bytes()),
                output.confidence,
                &output_json,
                &finished_at,
            )?;
        }
        Err(failure) => {
            let failure_json = failure.to_json()?;
            ai_runs::finalize_failed(conn, ai_run_id, &failure_json, &finished_at)?;
        }
    }
    objective::record_omr_ai_run_observation(
        conn,
        ai_run_id,
        &observation_idempotency_key(idempotency_key),
    )
}

fn observation_idempotency_key(run_key: &str) -> String {
    format!("{}:observation", run_key.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_key_is_stable_and_scoped_to_run_key() {
        assert_eq!(
            observation_idempotency_key(" omr:region:1:attempt:2 "),
            "omr:region:1:attempt:2:observation"
        );
    }
}
