//! M1.1 ASR 运行账本编排。
//!
//! `begin_asr_run` 只在短事务中抢占 submission 并创建/启动 `ai_run`；
//! 外部网络调用由 Tauri 在锁外执行；成功或失败再用短事务收尾。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::{ai_runs, submissions};
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AiRunStatus;
use suite_core::ports::RecognizedWord;

use crate::db::structured::{self, RecTranscript, STRUCTURED_SCORING_SCHEMA_VERSION};
use crate::service::recognition;

#[derive(Debug, Clone, Copy)]
pub struct AsrRunDescriptor<'a> {
    pub provider: &'a str,
    pub model_name: &'a str,
    pub model_version: &'a str,
    pub config_version: &'a str,
    pub prompt_or_rule_version: &'a str,
}

#[derive(Debug, Clone)]
pub enum BeginAsrRun {
    Execute { ai_run_id: i64, request_id: String },
    Cached { transcript: RecTranscript },
}

#[derive(Debug, Clone, Copy)]
pub struct FinishAsrSuccessInput<'a> {
    pub submission_id: i64,
    pub raw_transcript: &'a str,
    pub normalized_transcript: &'a str,
    pub normalization_version: &'a str,
    pub words: &'a [RecognizedWord],
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct WordSegment {
    text: String,
    start_ms: u64,
    end_ms: u64,
}

fn latest_matching_run_id(
    conn: &Connection,
    submission_id: i64,
    input_hash: &str,
    descriptor: &AsrRunDescriptor<'_>,
) -> CoreResult<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM ai_runs
             WHERE run_type='asr'
               AND source_module='recitation'
               AND business_ref_type='submission'
               AND business_ref_id=?1
               AND input_hash=?2
               AND provider=?3
               AND model_name=?4
               AND model_version=?5
               AND config_version=?6
               AND prompt_or_rule_version=?7
             ORDER BY id DESC LIMIT 1",
            params![
                submission_id.to_string(),
                input_hash,
                descriptor.provider,
                descriptor.model_name,
                descriptor.model_version,
                descriptor.config_version,
                descriptor.prompt_or_rule_version,
            ],
            |row| row.get(0),
        )
        .optional()?)
}

fn run_matches(
    run: &suite_core::models::AiRun,
    submission_id: i64,
    input_hash: &str,
    descriptor: &AsrRunDescriptor<'_>,
) -> bool {
    run.run_type == "asr"
        && run.source_module == "recitation"
        && run.business_ref_type == "submission"
        && run.business_ref_id == submission_id.to_string()
        && run.input_hash == input_hash
        && run.provider == descriptor.provider
        && run.model_name == descriptor.model_name
        && run.model_version == descriptor.model_version
        && run.config_version == descriptor.config_version
        && run.prompt_or_rule_version == descriptor.prompt_or_rule_version
}

fn claim_if_needed(conn: &Connection, submission_id: i64, preclaimed: bool) -> CoreResult<()> {
    let submission = submissions::get(conn, submission_id)?
        .ok_or_else(|| CoreError::NotFound(format!("submission {submission_id}")))?;
    if submission.module.as_str() != "recitation" {
        return Err(CoreError::Invalid(
            "只有背诵提交可以进入 ASR 运行账本".into(),
        ));
    }
    if preclaimed {
        if submission.recognize_status != "processing" {
            return Err(CoreError::Invalid(
                "预抢占的背诵提交已不在 processing，请刷新后重试".into(),
            ));
        }
        Ok(())
    } else {
        recognition::claim(conn, submission_id)
    }
}

/// 创建或恢复一条 ASR run。`preclaimed=true` 仅用于智能导入已经在同一流程中抢占的提交。
pub fn begin_asr_run(
    conn: &Connection,
    submission_id: i64,
    input_hash: &str,
    descriptor: &AsrRunDescriptor<'_>,
    preclaimed: bool,
) -> CoreResult<BeginAsrRun> {
    let transaction = conn.unchecked_transaction()?;
    claim_if_needed(&transaction, submission_id, preclaimed)?;

    if let Some(transcript) =
        structured::active_transcript_for_submission(&transaction, submission_id)?
    {
        let run = ai_runs::get_by_id(&transaction, transcript.asr_ai_run_id)?
            .ok_or_else(|| CoreError::NotFound(format!("ai run {}", transcript.asr_ai_run_id)))?;
        if run.status == AiRunStatus::Succeeded
            && run_matches(&run, submission_id, input_hash, descriptor)
        {
            transaction.commit()?;
            return Ok(BeginAsrRun::Cached { transcript });
        }
    }

    let latest = latest_matching_run_id(&transaction, submission_id, input_hash, descriptor)?
        .map(|id| {
            ai_runs::get_by_id(&transaction, id)?
                .ok_or_else(|| CoreError::NotFound(format!("ai run {id}")))
        })
        .transpose()?;

    if let Some(run) = latest.as_ref() {
        match run.status {
            AiRunStatus::Succeeded => {
                let transcript =
                    structured::record_transcript_from_ai_run_inner(&transaction, run.id)?;
                transaction.commit()?;
                return Ok(BeginAsrRun::Cached { transcript });
            }
            AiRunStatus::Processing => {
                return Err(CoreError::Invalid("该录音正在识别，请勿重复提交".into()));
            }
            AiRunStatus::Pending => {
                let started = ai_runs::start(&transaction, run.id, &time::utc_now_rfc3339(), None)?;
                transaction.commit()?;
                return Ok(BeginAsrRun::Execute {
                    ai_run_id: started.id,
                    request_id: started.public_id,
                });
            }
            AiRunStatus::Failed => {}
            AiRunStatus::Voided => {
                return Err(CoreError::Invalid("该 ASR run 已作废，不能继续".into()));
            }
        }
    }

    let retry_of = latest.as_ref().map(|run| run.id);
    let suffix = retry_of
        .map(|id| format!("retry-{id}"))
        .unwrap_or_else(|| "initial".to_string());
    let idempotency_key = format!(
        "recitation:asr:{submission_id}:{}:{suffix}",
        &input_hash[..input_hash.len().min(16)]
    );
    let run = ai_runs::create_or_get(
        &transaction,
        &ai_runs::NewAiRun {
            idempotency_key: &idempotency_key,
            run_type: "asr",
            source_module: "recitation",
            business_ref_type: "submission",
            business_ref_id: &submission_id.to_string(),
            input_artifact_id: None,
            provider: descriptor.provider,
            model_name: descriptor.model_name,
            model_version: descriptor.model_version,
            config_version: descriptor.config_version,
            prompt_or_rule_version: descriptor.prompt_or_rule_version,
            input_hash,
            retry_of_ai_run_id: retry_of,
        },
    )?;
    let started = ai_runs::start(&transaction, run.id, &time::utc_now_rfc3339(), None)?;
    transaction.commit()?;
    Ok(BeginAsrRun::Execute {
        ai_run_id: started.id,
        request_id: started.public_id,
    })
}

pub fn finish_asr_success(
    conn: &Connection,
    ai_run_id: i64,
    input: &FinishAsrSuccessInput<'_>,
) -> CoreResult<RecTranscript> {
    if input.duration_ms < 0 {
        return Err(CoreError::Invalid("ASR 时长不能小于 0".into()));
    }
    let word_segments = input
        .words
        .iter()
        .map(|word| WordSegment {
            text: word.text.clone(),
            start_ms: word.start_ms,
            end_ms: word.end_ms,
        })
        .collect::<Vec<_>>();
    let output = serde_json::json!({
        "schema_version": STRUCTURED_SCORING_SCHEMA_VERSION,
        "submission_id": input.submission_id,
        "raw_transcript": input.raw_transcript,
        "normalized_transcript": input.normalized_transcript,
        "normalization_version": input.normalization_version,
        "word_segments": word_segments,
        "duration_ms": input.duration_ms
    });
    let output_json = serde_json::to_string(&output)
        .map_err(|error| CoreError::Invalid(format!("ASR 输出序列化失败: {error}")))?;
    let output_hash = hashing::sha256_hex(output_json.as_bytes());
    let transaction = conn.unchecked_transaction()?;
    let run = ai_runs::get_by_id(&transaction, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai run {ai_run_id}")))?;
    if run.business_ref_id != input.submission_id.to_string() {
        return Err(CoreError::Invalid(
            "ASR run 与 submission 作用域不一致".into(),
        ));
    }
    ai_runs::finalize_succeeded(
        &transaction,
        ai_run_id,
        &output_hash,
        None,
        &output_json,
        &time::utc_now_rfc3339(),
    )?;
    let transcript = structured::record_transcript_from_ai_run_inner(&transaction, ai_run_id)?;
    transaction.commit()?;
    Ok(transcript)
}

pub fn finish_asr_failure(
    conn: &Connection,
    ai_run_id: i64,
    submission_id: i64,
    message: &str,
) -> CoreResult<()> {
    let (error_code, retryable) = recognition::classify_failure(message);
    let error_json = serde_json::json!({
        "schema_version": STRUCTURED_SCORING_SCHEMA_VERSION,
        "error_code": error_code,
        "error_message": message.chars().take(500).collect::<String>(),
        "retryable": retryable
    })
    .to_string();
    let transaction = conn.unchecked_transaction()?;
    let run = ai_runs::get_by_id(&transaction, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai run {ai_run_id}")))?;
    if run.business_ref_id != submission_id.to_string() {
        return Err(CoreError::Invalid(
            "ASR run 与 submission 作用域不一致".into(),
        ));
    }
    ai_runs::finalize_failed(
        &transaction,
        ai_run_id,
        &error_json,
        &time::utc_now_rfc3339(),
    )?;
    recognition::mark_failed(&transaction, submission_id, message)?;
    transaction.commit()?;
    Ok(())
}

pub fn transcript_words(transcript: &RecTranscript) -> CoreResult<Vec<RecognizedWord>> {
    let words: Vec<WordSegment> = serde_json::from_str(&transcript.word_segments_json)
        .map_err(|error| CoreError::Invalid(format!("ASR 词级时间段解析失败: {error}")))?;
    Ok(words
        .into_iter()
        .map(|word| RecognizedWord {
            text: word.text,
            start_ms: word.start_ms,
            end_ms: word.end_ms,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::repo::submissions::{self, NewSubmission};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{MediaType, ModuleKey};

    fn setup() -> (Connection, i64) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::recitation_migrations()).unwrap();
        let submission_id = submissions::insert(
            &conn,
            &NewSubmission {
                module: ModuleKey::Recitation,
                task_id: None,
                student_id: None,
                ref_id: None,
                media_type: MediaType::Audio,
                file_path: "/tmp/recitation-asr.m4a",
                file_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                duration_ms: None,
                parsed_meta: None,
                anomaly_type: None,
                status: "pending",
            },
        )
        .unwrap();
        (conn, submission_id)
    }

    fn descriptor<'a>() -> AsrRunDescriptor<'a> {
        AsrRunDescriptor {
            provider: "fixture",
            model_name: "fixture-asr",
            model_version: "1",
            config_version: "1",
            prompt_or_rule_version: "1",
        }
    }

    #[test]
    fn asr_success_is_cached_and_materialized_once() {
        let (conn, submission_id) = setup();
        let begin = begin_asr_run(
            &conn,
            submission_id,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &descriptor(),
            false,
        )
        .unwrap();
        let ai_run_id = match begin {
            BeginAsrRun::Execute { ai_run_id, .. } => ai_run_id,
            _ => panic!("first call must execute"),
        };
        let words = [RecognizedWord {
            text: "自强".into(),
            start_ms: 100,
            end_ms: 500,
        }];
        let transcript = finish_asr_success(
            &conn,
            ai_run_id,
            &FinishAsrSuccessInput {
                submission_id,
                raw_transcript: "自强",
                normalized_transcript: "自强",
                normalization_version: "recitation-normalize-v1",
                words: &words,
                duration_ms: 600,
            },
        )
        .unwrap();
        assert_eq!(transcript_words(&transcript).unwrap()[0].text, "自强");
        submissions::set_recognition_failure(&conn, submission_id, r#"{"schema_version":1}"#)
            .unwrap();
        let cached = begin_asr_run(
            &conn,
            submission_id,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &descriptor(),
            false,
        )
        .unwrap();
        assert!(matches!(cached, BeginAsrRun::Cached { .. }));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM rec_transcripts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn failed_run_has_retry_lineage_and_no_transcript() {
        let (conn, submission_id) = setup();
        let first = begin_asr_run(
            &conn,
            submission_id,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &descriptor(),
            false,
        )
        .unwrap();
        let first_id = match first {
            BeginAsrRun::Execute { ai_run_id, .. } => ai_run_id,
            _ => panic!("first call must execute"),
        };
        finish_asr_failure(&conn, first_id, submission_id, "火山识别超时").unwrap();
        let retry = begin_asr_run(
            &conn,
            submission_id,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &descriptor(),
            false,
        )
        .unwrap();
        let retry_id = match retry {
            BeginAsrRun::Execute { ai_run_id, .. } => ai_run_id,
            _ => panic!("retry must execute"),
        };
        let retry_run = ai_runs::get_by_id(&conn, retry_id).unwrap().unwrap();
        assert_eq!(retry_run.retry_of_ai_run_id, Some(first_id));
        let transcript_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM rec_transcripts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(transcript_count, 0);
    }

    #[test]
    fn concurrent_processing_run_is_rejected() {
        let (conn, submission_id) = setup();
        begin_asr_run(
            &conn,
            submission_id,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &descriptor(),
            false,
        )
        .unwrap();
        let error = begin_asr_run(
            &conn,
            submission_id,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &descriptor(),
            true,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("正在识别"));
    }

    #[test]
    fn transcript_materialization_failure_rolls_back_ai_run_success() {
        let (conn, submission_id) = setup();
        let begin = begin_asr_run(
            &conn,
            submission_id,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            &descriptor(),
            false,
        )
        .unwrap();
        let ai_run_id = match begin {
            BeginAsrRun::Execute { ai_run_id, .. } => ai_run_id,
            _ => panic!("first call must execute"),
        };
        conn.execute_batch(
            "CREATE TEMP TRIGGER fail_transcript_materialization
             BEFORE INSERT ON rec_transcripts
             BEGIN SELECT RAISE(ABORT, 'injected transcript failure'); END;",
        )
        .unwrap();
        let error = finish_asr_success(
            &conn,
            ai_run_id,
            &FinishAsrSuccessInput {
                submission_id,
                raw_transcript: "自强",
                normalized_transcript: "自强",
                normalization_version: "recitation-normalize-v1",
                words: &[],
                duration_ms: 600,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("injected transcript failure"));
        assert_eq!(
            ai_runs::get_by_id(&conn, ai_run_id)
                .unwrap()
                .unwrap()
                .status,
            AiRunStatus::Processing
        );
        assert!(
            structured::active_transcript_for_submission(&conn, submission_id)
                .unwrap()
                .is_none()
        );
    }
}
