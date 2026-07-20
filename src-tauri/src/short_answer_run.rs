//! 答题卡简答题 `answer_grade` run 编排。
//!
//! 输入在短数据库锁内冻结，provider 调用在锁外执行，最终输出再以短事务写回。

use module_exam::service::subjective::{self, ShortAnswerGradeAnalysis};
use module_exam::short_answer_grading::{
    ShortAnswerGradeErrorCode, ShortAnswerGradeFailure, ShortAnswerGradeOutput,
    ShortAnswerGradeRequest, ShortAnswerGraderDescriptor, SHORT_ANSWER_GRADE_SCHEMA_VERSION,
};
use rusqlite::Connection;
use suite_core::db::repo::ai_runs;
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AiRunStatus;

pub enum BeginShortAnswerGradeRun {
    Execute { ai_run_id: i64 },
    Completed(Box<ShortAnswerGradeAnalysis>),
}

pub fn load_input(
    conn: &Connection,
    transcription_revision_id: i64,
) -> CoreResult<ShortAnswerGradeRequest> {
    subjective::load_short_answer_grade_request(conn, transcription_revision_id)
}

pub fn begin(
    conn: &mut Connection,
    input: &ShortAnswerGradeRequest,
    descriptor: &ShortAnswerGraderDescriptor,
    idempotency_key: &str,
) -> CoreResult<BeginShortAnswerGradeRun> {
    input.validate()?;
    descriptor.validate()?;
    if idempotency_key.trim().is_empty() {
        return Err(CoreError::Invalid("简答题评分幂等键不能为空".into()));
    }
    let business_ref_id = input.transcription_revision_id.to_string();
    let input_hash = input.input_hash()?;
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: idempotency_key.trim(),
            run_type: "answer_grade",
            source_module: "exam",
            business_ref_type: "subjective_transcription_revision",
            business_ref_id: &business_ref_id,
            input_artifact_id: Some(input.crop_artifact_id),
            provider: &descriptor.provider,
            model_name: &descriptor.model_name,
            model_version: &descriptor.model_version,
            config_version: &descriptor.config_version,
            prompt_or_rule_version: &descriptor.rule_version,
            input_hash: &input_hash,
            retry_of_ai_run_id: None,
        },
    )?;
    match run.status {
        AiRunStatus::Pending => {
            ai_runs::start(conn, run.id, &time::utc_now_rfc3339(), None)?;
            Ok(BeginShortAnswerGradeRun::Execute { ai_run_id: run.id })
        }
        AiRunStatus::Succeeded => Ok(BeginShortAnswerGradeRun::Completed(Box::new(
            subjective::record_short_answer_grade_ai_run(conn, run.id)?,
        ))),
        AiRunStatus::Failed => Err(CoreError::Invalid(
            "该简答题评分尝试已失败，请使用新的幂等键重试".into(),
        )),
        AiRunStatus::Processing => Err(CoreError::Invalid(
            "该简答题正在生成评分建议，请勿重复提交".into(),
        )),
        AiRunStatus::Voided => Err(CoreError::Invalid(
            "该简答题评分已作废，请使用新的幂等键重试".into(),
        )),
    }
}

pub fn finish(
    conn: &mut Connection,
    input: &ShortAnswerGradeRequest,
    ai_run_id: i64,
    result: Result<ShortAnswerGradeOutput, ShortAnswerGradeFailure>,
) -> CoreResult<ShortAnswerGradeAnalysis> {
    let finished_at = time::utc_now_rfc3339();
    match result {
        Ok(output) => match output.to_json_against(input) {
            Ok(output_json) => {
                ai_runs::finalize_succeeded(
                    conn,
                    ai_run_id,
                    &hashing::sha256_hex(output_json.as_bytes()),
                    Some(output.confidence),
                    &output_json,
                    &finished_at,
                )?;
                subjective::record_short_answer_grade_ai_run(conn, ai_run_id)
            }
            Err(_) => {
                let failure = ShortAnswerGradeFailure {
                    schema_version: SHORT_ANSWER_GRADE_SCHEMA_VERSION,
                    code: ShortAnswerGradeErrorCode::InvalidOutput,
                    safe_message: "简答题评分结果未通过证据校验，已交给老师判定".into(),
                    retryable: false,
                };
                ai_runs::finalize_failed(conn, ai_run_id, &failure.to_json()?, &finished_at)?;
                Err(CoreError::Invalid(failure.safe_message))
            }
        },
        Err(failure) => {
            ai_runs::finalize_failed(conn, ai_run_id, &failure.to_json()?, &finished_at)?;
            Err(CoreError::Invalid(failure.safe_message))
        }
    }
}
