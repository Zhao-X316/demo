//! 已确认 rubric 上的本地确定性逐点评分。
//!
//! 本服务复用现有 M1.0 正确率/流畅度算法，只追加 `recitation_score` ai_run
//! 与逐点机器建议；没有老师确认 rubric 时返回 `None`，绝不自动造 rubric。

use rusqlite::Connection;
use serde_json::Value;
use suite_core::db::repo::{ai_runs, submissions};
use suite_core::domain::{hashing, normalize, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AiRunStatus;
use suite_core::ports::{Grader, RecognizedWord};

use crate::db::contents;
use crate::db::structured::{self, RecRubricPoint, RecScoreRun, STRUCTURED_SCORING_SCHEMA_VERSION};
use crate::grader::{RecitationGradeInput, RecitationGrader};
use crate::service::ai_pipeline::transcript_words;
use crate::service::scoring::ScoreCfg;

const SCORE_PROVIDER: &str = "local_deterministic";
const SCORE_MODEL: &str = "recitation_point_grader";
const SCORE_MODEL_VERSION: &str = "1";
const SCORE_RULE_VERSION: &str = "recitation-point-rule-v1";

fn item_strings(raw: &str) -> CoreResult<Vec<String>> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| CoreError::Invalid(format!("评分点规则 JSON 无效: {error}")))?;
    let items = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Invalid("评分点规则缺少 items".into()))?;
    Ok(items
        .iter()
        .filter_map(|item| {
            item.as_str()
                .map(str::to_string)
                .or_else(|| item.get("text").and_then(Value::as_str).map(str::to_string))
                .or_else(|| {
                    item.get("value")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .or_else(|| {
                    item.get("canonical")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
        })
        .filter(|item| !item.trim().is_empty())
        .collect())
}

fn normalized_patterns(values: impl IntoIterator<Item = String>, cfg: &ScoreCfg) -> Vec<String> {
    values
        .into_iter()
        .map(|value| normalize::normalize(&value, &cfg.normalize))
        .filter(|value| !value.is_empty())
        .collect()
}

fn matched_pattern<'a>(transcript: &str, patterns: &'a [String]) -> Option<&'a str> {
    patterns
        .iter()
        .find(|pattern| transcript.contains(pattern.as_str()))
        .map(String::as_str)
}

fn evidence_span(words: &[RecognizedWord], pattern: &str, cfg: &ScoreCfg) -> Option<Value> {
    if pattern.is_empty() {
        return None;
    }
    let mut joined = Vec::new();
    let mut ranges = Vec::new();
    for (index, word) in words.iter().enumerate() {
        let normalized = normalize::normalize(&word.text, &cfg.normalize);
        let start = joined.len();
        joined.extend(normalized.chars());
        ranges.push((index, start, joined.len()));
    }
    let needle = pattern.chars().collect::<Vec<_>>();
    let start = joined
        .windows(needle.len())
        .position(|window| window == needle.as_slice())?;
    let end = start + needle.len();
    let first = ranges
        .iter()
        .find(|(_, range_start, range_end)| *range_start <= start && start < *range_end)?
        .0;
    let last = ranges
        .iter()
        .rev()
        .find(|(_, range_start, range_end)| *range_start < end && end <= *range_end)?
        .0;
    Some(serde_json::json!({
        "start_ms": words[first].start_ms,
        "end_ms": words[last].end_ms,
        "text": words[first..=last]
            .iter()
            .map(|word| word.text.as_str())
            .collect::<String>()
    }))
}

fn point_result(
    point: &RecRubricPoint,
    transcript: &str,
    words: &[RecognizedWord],
    cfg: &ScoreCfg,
) -> CoreResult<Value> {
    let contradictions = normalized_patterns(item_strings(&point.contradiction_rules_json)?, cfg);
    let paraphrases = normalized_patterns(item_strings(&point.allowed_paraphrases_json)?, cfg);
    let entities = normalized_patterns(item_strings(&point.required_entities_json)?, cfg);
    let canonical = normalize::normalize(&point.canonical_text, &cfg.normalize);

    let (state, confidence, matched, reason) =
        if let Some(pattern) = matched_pattern(transcript, &contradictions) {
            ("contradiction", 0.95, Some(pattern), "命中已配置的矛盾表述")
        } else if transcript.contains(&canonical) {
            (
                "covered",
                0.95,
                Some(canonical.as_str()),
                "命中评分点标准表述",
            )
        } else if let Some(pattern) = matched_pattern(transcript, &paraphrases) {
            ("covered", 0.90, Some(pattern), "命中允许改述")
        } else if !entities.is_empty() {
            let matched_entities = entities
                .iter()
                .filter(|entity| transcript.contains(entity.as_str()))
                .collect::<Vec<_>>();
            if matched_entities.len() == entities.len() {
                (
                    "covered",
                    0.88,
                    matched_entities.first().map(|item| item.as_str()),
                    "必需实体全部出现",
                )
            } else if !matched_entities.is_empty() {
                (
                    "partial",
                    0.70,
                    matched_entities.first().map(|item| item.as_str()),
                    "只出现部分必需实体",
                )
            } else {
                ("omitted", 0.75, None, "未找到评分点或必需实体")
            }
        } else {
            ("omitted", 0.70, None, "未找到评分点标准表述")
        };
    let spans = matched
        .and_then(|pattern| evidence_span(words, pattern, cfg))
        .or_else(|| {
            matched_pattern(transcript, &entities)
                .and_then(|pattern| evidence_span(words, pattern, cfg))
        })
        .into_iter()
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "rubric_point_id": point.id,
        "machine_state": state,
        "confidence": confidence,
        "evidence_spans": spans,
        "reason": reason
    }))
}

/// 当前答案存在老师确认 rubric 时，生成或复用结构化逐点机器评分。
pub fn record_score_if_ready(
    conn: &Connection,
    submission_id: i64,
    cfg: &ScoreCfg,
) -> CoreResult<Option<RecScoreRun>> {
    let submission = submissions::get(conn, submission_id)?
        .ok_or_else(|| CoreError::NotFound(format!("submission {submission_id}")))?;
    let content_id = submission
        .ref_id
        .ok_or_else(|| CoreError::Invalid("背诵提交尚未绑定内容".into()))?;
    let Some(rubric) = structured::current_confirmed_rubric_for_content(conn, content_id)? else {
        return Ok(None);
    };
    let transcript = structured::active_transcript_for_submission(conn, submission_id)?
        .ok_or_else(|| CoreError::Invalid("背诵提交没有 active transcript".into()))?;
    let content = contents::get_by_id(conn, content_id)?
        .ok_or_else(|| CoreError::NotFound(format!("content {content_id}")))?;
    let words = transcript_words(&transcript)?;
    let grade = RecitationGrader.grade(RecitationGradeInput {
        answer_text: &content.answer_text,
        asr_text: &transcript.raw_transcript,
        words: &words,
        duration_ms: transcript.duration_ms.max(0) as u64,
        normalize_cfg: &cfg.normalize,
        accuracy_cfg: &cfg.accuracy,
        fluency_cfg: &cfg.fluency,
    })?;
    let normalized_transcript = normalize::normalize(&transcript.raw_transcript, &cfg.normalize);
    let points = structured::list_rubric_points(conn, rubric.id)?;
    let point_results = points
        .iter()
        .map(|point| point_result(point, &normalized_transcript, &words, cfg))
        .collect::<CoreResult<Vec<_>>>()?;
    let metrics: Value = serde_json::from_str(&grade.metrics_json)
        .map_err(|error| CoreError::Invalid(format!("现有评分指标 JSON 无效: {error}")))?;
    let config = serde_json::json!({
        "schema_version": STRUCTURED_SCORING_SCHEMA_VERSION,
        "accuracy_threshold": cfg.accuracy.threshold,
        "use_pinyin": cfg.accuracy.use_pinyin,
        "ignore_tone": cfg.accuracy.ignore_tone,
        "remove_fillers": cfg.normalize.remove_fillers,
        "ideal_cps": cfg.fluency.ideal_cps,
        "quality_a_min": cfg.fluency.a_min,
        "quality_b_min": cfg.fluency.b_min
    });
    let input = serde_json::json!({
        "schema_version": STRUCTURED_SCORING_SCHEMA_VERSION,
        "transcript_output_hash": transcript.output_hash,
        "rubric_definition_hash": rubric.definition_hash,
        "config": config
    });
    let input_json = serde_json::to_string(&input)
        .map_err(|error| CoreError::Invalid(format!("结构化评分输入序列化失败: {error}")))?;
    let input_hash = hashing::sha256_hex(input_json.as_bytes());
    let idempotency_key = format!(
        "recitation:score:transcript:{}:rubric:{}:{}",
        transcript.id,
        rubric.id,
        &input_hash[..16]
    );
    let config_hash = hashing::sha256_hex(config.to_string().as_bytes());
    let output = serde_json::json!({
        "schema_version": STRUCTURED_SCORING_SCHEMA_VERSION,
        "submission_id": submission_id,
        "transcript_id": transcript.id,
        "rubric_version_id": rubric.id,
        "overall_suggestion": if grade.pass { "pass" } else { "fail" },
        "accuracy": {
            "schema_version": STRUCTURED_SCORING_SCHEMA_VERSION,
            "score": grade.primary_score,
            "pass": grade.pass,
            "metrics": metrics
        },
        "fluency": {
            "schema_version": STRUCTURED_SCORING_SCHEMA_VERSION,
            "score": grade.secondary_score.unwrap_or(0.0),
            "quality": grade.quality
        },
        "confidence": grade.confidence.clamp(0.0, 1.0),
        "point_results": point_results
    });
    let output_json = serde_json::to_string(&output)
        .map_err(|error| CoreError::Invalid(format!("结构化评分输出序列化失败: {error}")))?;
    let output_hash = hashing::sha256_hex(output_json.as_bytes());
    let transaction = conn.unchecked_transaction()?;
    let run = ai_runs::create_or_get(
        &transaction,
        &ai_runs::NewAiRun {
            idempotency_key: &idempotency_key,
            run_type: "recitation_score",
            source_module: "recitation",
            business_ref_type: "recitation_transcript",
            business_ref_id: &transcript.id.to_string(),
            input_artifact_id: None,
            provider: SCORE_PROVIDER,
            model_name: SCORE_MODEL,
            model_version: SCORE_MODEL_VERSION,
            config_version: &config_hash[..16],
            prompt_or_rule_version: SCORE_RULE_VERSION,
            input_hash: &input_hash,
            retry_of_ai_run_id: None,
        },
    )?;
    if run.status == AiRunStatus::Succeeded {
        let score = structured::record_score_from_ai_run_inner(&transaction, run.id)?;
        transaction.commit()?;
        return Ok(Some(score));
    }
    if run.status != AiRunStatus::Pending {
        return Err(CoreError::Invalid(format!(
            "本地结构化评分 run 状态为 {}，无法继续",
            run.status.as_str()
        )));
    }
    ai_runs::start(&transaction, run.id, &time::utc_now_rfc3339(), None)?;
    ai_runs::finalize_succeeded(
        &transaction,
        run.id,
        &output_hash,
        Some(grade.confidence.clamp(0.0, 1.0)),
        &output_json,
        &time::utc_now_rfc3339(),
    )?;
    let score = structured::record_score_from_ai_run_inner(&transaction, run.id)?;
    transaction.commit()?;
    Ok(Some(score))
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::repo::ai_runs::{self, NewAiRun};
    use suite_core::db::repo::submissions::{self, NewSubmission};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{MediaType, ModuleKey};

    use crate::db::contents::{self, ContentInput};
    use crate::db::structured::{
        confirm_rubric, create_rubric_draft, current_answer_version, record_transcript_from_ai_run,
        CreateRubricDraftInput, RubricPointDraftInput,
    };

    const EMPTY: &str = r#"{"schema_version":1,"items":[]}"#;

    fn setup(with_rubric: bool) -> (Connection, i64) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::recitation_migrations()).unwrap();
        let content = contents::upsert(
            &conn,
            &ContentInput {
                content_no: "M1-SCORE",
                title: "洋务运动口号",
                answer_text: "前期以自强为口号",
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap();
        if with_rubric {
            let answer = current_answer_version(&conn, content.id).unwrap().unwrap();
            let points = [RubricPointDraftInput {
                stable_key: "self-strengthening",
                canonical_text: "前期以自强为口号",
                required_entities_json: r#"{"schema_version":1,"items":["自强"]}"#,
                allowed_paraphrases_json: EMPTY,
                contradiction_rules_json: r#"{"schema_version":1,"items":["前期以求富为口号"]}"#,
                required: true,
                weight: 1.0,
                order_index: 0,
                knowledge_node_id: None,
                knowledge_link_state: "none",
                verified_by: None,
                verified_at: None,
            }];
            let draft = create_rubric_draft(
                &conn,
                &CreateRubricDraftInput {
                    answer_version_id: answer.id,
                    generated_by_ai_run_id: None,
                    created_by: "teacher-1",
                    points: &points,
                },
            )
            .unwrap();
            confirm_rubric(&conn, draft.id, "teacher-1").unwrap();
        }
        let submission_id = submissions::insert(
            &conn,
            &NewSubmission {
                module: ModuleKey::Recitation,
                task_id: None,
                student_id: None,
                ref_id: Some(content.id),
                media_type: MediaType::Audio,
                file_path: "/tmp/m1-score.m4a",
                file_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                duration_ms: Some(1_000),
                parsed_meta: None,
                anomaly_type: None,
                status: "pending",
            },
        )
        .unwrap();
        let output = serde_json::json!({
            "schema_version": 1,
            "submission_id": submission_id,
            "raw_transcript": "前期以自强为口号",
            "normalized_transcript": "前期以自强为口号",
            "normalization_version": "recitation-normalize-v1",
            "word_segments": [
                {"text":"自强","start_ms":200,"end_ms":500}
            ],
            "duration_ms": 1000
        })
        .to_string();
        let output_hash = hashing::sha256_hex(output.as_bytes());
        let run = ai_runs::create_or_get(
            &conn,
            &NewAiRun {
                idempotency_key: "fixture-asr-score",
                run_type: "asr",
                source_module: "recitation",
                business_ref_type: "submission",
                business_ref_id: &submission_id.to_string(),
                input_artifact_id: None,
                provider: "fixture",
                model_name: "fixture",
                model_version: "1",
                config_version: "1",
                prompt_or_rule_version: "1",
                input_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                retry_of_ai_run_id: None,
            },
        )
        .unwrap();
        ai_runs::start(&conn, run.id, "2026-07-17T08:00:00.000Z", None).unwrap();
        ai_runs::finalize_succeeded(
            &conn,
            run.id,
            &output_hash,
            Some(1.0),
            &output,
            "2026-07-17T08:00:01.000Z",
        )
        .unwrap();
        record_transcript_from_ai_run(&conn, run.id).unwrap();
        (conn, submission_id)
    }

    #[test]
    fn no_confirmed_rubric_keeps_legacy_only() {
        let (conn, submission_id) = setup(false);
        assert!(
            record_score_if_ready(&conn, submission_id, &ScoreCfg::default())
                .unwrap()
                .is_none()
        );
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM rec_score_runs", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn confirmed_rubric_creates_idempotent_point_score_without_verdict() {
        let (conn, submission_id) = setup(true);
        let first = record_score_if_ready(&conn, submission_id, &ScoreCfg::default())
            .unwrap()
            .unwrap();
        let same = record_score_if_ready(&conn, submission_id, &ScoreCfg::default())
            .unwrap()
            .unwrap();
        assert_eq!(first.id, same.id);
        let result: (String, String) = conn
            .query_row(
                "SELECT machine_state,evidence_spans_json
                 FROM rec_point_results WHERE score_run_id=?1",
                [first.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(result.0, "covered");
        assert!(result.1.contains("200"));
        let verdict_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM verdicts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(verdict_count, 0);
    }

    #[test]
    fn point_materialization_failure_rolls_back_local_ai_run() {
        let (conn, submission_id) = setup(true);
        conn.execute_batch(
            "CREATE TEMP TRIGGER fail_point_materialization
             BEFORE INSERT ON rec_point_results
             BEGIN SELECT RAISE(ABORT, 'injected point failure'); END;",
        )
        .unwrap();
        let error = record_score_if_ready(&conn, submission_id, &ScoreCfg::default())
            .unwrap_err()
            .to_string();
        assert!(error.contains("injected point failure"));
        let counts: (i64, i64) = conn
            .query_row(
                "SELECT
                   (SELECT count(*) FROM ai_runs WHERE run_type='recitation_score'),
                   (SELECT count(*) FROM rec_score_runs)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(counts, (0, 0));
    }
}
