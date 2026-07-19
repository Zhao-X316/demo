//! K1 知识/能力链接建议 AI run 编排。
//!
//! 输入在短事务内冻结，外部调用不持 SQLite 锁；成功输出先写不可变 ai_run，
//! 再落 K1 建议草稿，老师确认仍由独立命令完成。

use module_knowledge::db::link_reviews::{self, LinkSuggestionDraftView};
use module_knowledge::link_suggestion::{
    input_hash, validate_output, LinkSuggester, LinkSuggestionInput, LinkSuggestionOutput,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use suite_core::db::repo::ai_runs;
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRun, AiRunStatus};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkRunFailure {
    pub schema_version: i64,
    pub code: String,
    pub safe_message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSuggestionAnalysisResult {
    pub ai_run_id: i64,
    pub status: String,
    pub suggestion: Option<LinkSuggestionDraftView>,
    pub output: Option<LinkSuggestionOutput>,
    pub failure: Option<LinkRunFailure>,
}

pub enum BeginLinkRun {
    Execute { ai_run_id: i64 },
    Completed(Box<LinkSuggestionAnalysisResult>),
}

fn failure(error: CoreError) -> LinkRunFailure {
    LinkRunFailure {
        schema_version: 1,
        code: "link_suggestion_failed".into(),
        safe_message: error.to_string(),
        retryable: true,
    }
}

fn result_from_run(run: &AiRun) -> CoreResult<LinkSuggestionAnalysisResult> {
    match run.status {
        AiRunStatus::Succeeded => {
            let output: LinkSuggestionOutput = serde_json::from_str(
                run.output_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("知识链接成功 run 缺少输出".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("知识链接 run 输出无法解析：{error}")))?;
            Ok(LinkSuggestionAnalysisResult {
                ai_run_id: run.id,
                status: "succeeded".into(),
                suggestion: None,
                output: Some(output),
                failure: None,
            })
        }
        AiRunStatus::Failed => {
            let failure = serde_json::from_str(
                run.error_meta_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("知识链接失败 run 缺少错误信息".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("知识链接错误信息无法解析：{error}")))?;
            Ok(LinkSuggestionAnalysisResult {
                ai_run_id: run.id,
                status: "failed".into(),
                suggestion: None,
                output: None,
                failure: Some(failure),
            })
        }
        _ => Err(CoreError::Invalid("知识链接 run 尚未结束".into())),
    }
}

pub fn begin(
    conn: &Connection,
    input: &LinkSuggestionInput,
    suggester: &dyn LinkSuggester,
    request_key: &str,
) -> CoreResult<BeginLinkRun> {
    if request_key.trim().is_empty() {
        return Err(CoreError::Invalid("知识链接建议请求键不能为空".into()));
    }
    let hash = input_hash(input)?;
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: request_key.trim(),
            run_type: "knowledge_link_suggest",
            source_module: "knowledge",
            business_ref_type: "k1_question_version",
            business_ref_id: &input.question_version_public_id,
            input_artifact_id: None,
            provider: suggester.provider(),
            model_name: suggester.model_name(),
            model_version: suggester.model_version(),
            config_version: suggester.config_version(),
            prompt_or_rule_version: suggester.rule_version(),
            input_hash: &hash,
            retry_of_ai_run_id: None,
        },
    )?;
    match run.status {
        AiRunStatus::Pending => {
            ai_runs::start(conn, run.id, &time::utc_now_rfc3339(), None)?;
            Ok(BeginLinkRun::Execute { ai_run_id: run.id })
        }
        AiRunStatus::Succeeded | AiRunStatus::Failed => {
            Ok(BeginLinkRun::Completed(Box::new(result_from_run(&run)?)))
        }
        AiRunStatus::Processing => Err(CoreError::Invalid(
            "知识链接建议正在生成，请勿重复提交".into(),
        )),
        AiRunStatus::Voided => Err(CoreError::Invalid(
            "知识链接建议 run 已作废，请使用新请求键".into(),
        )),
    }
}

pub fn finish(
    conn: &Connection,
    input: &LinkSuggestionInput,
    ai_run_id: i64,
    result: CoreResult<LinkSuggestionOutput>,
) -> CoreResult<LinkSuggestionAnalysisResult> {
    let now = time::utc_now_rfc3339();
    match result {
        Ok(output) => match validate_output(input, &output) {
            Ok(()) => {
                let output_json = serde_json::to_string(&output).map_err(|error| {
                    CoreError::Parse(format!("知识链接建议输出序列化失败：{error}"))
                })?;
                ai_runs::finalize_succeeded(
                    conn,
                    ai_run_id,
                    &hashing::sha256_hex(output_json.as_bytes()),
                    Some(output.confidence),
                    &output_json,
                    &now,
                )?;
            }
            Err(error) => {
                let failure = failure(error);
                ai_runs::finalize_failed(
                    conn,
                    ai_run_id,
                    &serde_json::to_string(&failure).map_err(|error| {
                        CoreError::Parse(format!("知识链接错误序列化失败：{error}"))
                    })?,
                    &now,
                )?;
            }
        },
        Err(error) => {
            let failure = failure(error);
            ai_runs::finalize_failed(
                conn,
                ai_run_id,
                &serde_json::to_string(&failure).map_err(|error| {
                    CoreError::Parse(format!("知识链接错误序列化失败：{error}"))
                })?,
                &now,
            )?;
        }
    }
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    result_from_run(&run)
}

pub fn materialize(
    conn: &mut Connection,
    owner_id: &str,
    input: &LinkSuggestionInput,
    mut result: LinkSuggestionAnalysisResult,
) -> CoreResult<LinkSuggestionAnalysisResult> {
    if let Some(output) = result.output.as_ref() {
        result.suggestion = Some(link_reviews::materialize_suggestion(
            conn,
            owner_id,
            result.ai_run_id,
            input,
            output,
        )?);
    }
    Ok(result)
}
