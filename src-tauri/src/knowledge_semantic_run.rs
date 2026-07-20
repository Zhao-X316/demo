//! K1 受控语义检索 AI run 编排。
//!
//! 候选清单在短锁内冻结；外部 AI 调用不持 SQLite 锁。成功结果追加写入 ai_runs，
//! 相同输入与模型配置直接复用；结果只用于本次找题展示。

use module_knowledge::semantic_search::{
    input_hash, validate_output, SemanticQuestionSearcher, SemanticSearchInput,
    SemanticSearchOutput,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use suite_core::db::repo::ai_runs;
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRun, AiRunStatus};

const RUN_TYPE: &str = "question_semantic_search";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SemanticRunFailure {
    pub schema_version: i64,
    pub code: String,
    pub safe_message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSearchAnalysisResult {
    pub ai_run_id: i64,
    pub status: String,
    pub output: Option<SemanticSearchOutput>,
    pub failure: Option<SemanticRunFailure>,
}

pub enum BeginSemanticRun {
    Execute { ai_run_id: i64 },
    Completed(Box<SemanticSearchAnalysisResult>),
}

fn failure(error: CoreError) -> SemanticRunFailure {
    let message = error.to_string();
    SemanticRunFailure {
        schema_version: 1,
        code: if message.contains("未配置") {
            "semantic_search_not_configured"
        } else if message.contains("超时") {
            "semantic_search_timeout"
        } else {
            "semantic_search_failed"
        }
        .into(),
        safe_message: message,
        retryable: true,
    }
}

fn result_from_run(run: &AiRun) -> CoreResult<SemanticSearchAnalysisResult> {
    match run.status {
        AiRunStatus::Succeeded => {
            let output: SemanticSearchOutput = serde_json::from_str(
                run.output_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("语义找题成功 run 缺少输出".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("语义找题 run 输出无法解析：{error}")))?;
            Ok(SemanticSearchAnalysisResult {
                ai_run_id: run.id,
                status: "succeeded".into(),
                output: Some(output),
                failure: None,
            })
        }
        AiRunStatus::Failed => {
            let failure = serde_json::from_str(
                run.error_meta_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("语义找题失败 run 缺少错误信息".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("语义找题错误信息无法解析：{error}")))?;
            Ok(SemanticSearchAnalysisResult {
                ai_run_id: run.id,
                status: "failed".into(),
                output: None,
                failure: Some(failure),
            })
        }
        _ => Err(CoreError::Invalid("语义找题 run 尚未结束".into())),
    }
}

pub fn begin(
    conn: &Connection,
    input: &SemanticSearchInput,
    searcher: &dyn SemanticQuestionSearcher,
    request_key: &str,
) -> CoreResult<BeginSemanticRun> {
    if request_key.trim().is_empty() {
        return Err(CoreError::Invalid("语义找题请求键不能为空".into()));
    }
    let hash = input_hash(input)?;
    if let Some(cached) = ai_runs::find_succeeded_cache(
        conn,
        RUN_TYPE,
        &hash,
        searcher.provider(),
        searcher.model_name(),
        searcher.model_version(),
        searcher.config_version(),
        searcher.rule_version(),
    )? {
        return Ok(BeginSemanticRun::Completed(Box::new(result_from_run(
            &cached,
        )?)));
    }
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: request_key.trim(),
            run_type: RUN_TYPE,
            source_module: "knowledge",
            business_ref_type: "k1_semantic_catalog",
            business_ref_id: &input.catalog_snapshot_hash,
            input_artifact_id: None,
            provider: searcher.provider(),
            model_name: searcher.model_name(),
            model_version: searcher.model_version(),
            config_version: searcher.config_version(),
            prompt_or_rule_version: searcher.rule_version(),
            input_hash: &hash,
            retry_of_ai_run_id: None,
        },
    )?;
    match run.status {
        AiRunStatus::Pending => {
            ai_runs::start(conn, run.id, &time::utc_now_rfc3339(), None)?;
            Ok(BeginSemanticRun::Execute { ai_run_id: run.id })
        }
        AiRunStatus::Succeeded | AiRunStatus::Failed => Ok(BeginSemanticRun::Completed(Box::new(
            result_from_run(&run)?,
        ))),
        AiRunStatus::Processing => Err(CoreError::Invalid(
            "相同语义找题请求正在处理，请勿重复提交".into(),
        )),
        AiRunStatus::Voided => Err(CoreError::Invalid(
            "语义找题 run 已作废，请使用新请求键".into(),
        )),
    }
}

pub fn finish(
    conn: &Connection,
    input: &SemanticSearchInput,
    ai_run_id: i64,
    result: CoreResult<SemanticSearchOutput>,
) -> CoreResult<SemanticSearchAnalysisResult> {
    let now = time::utc_now_rfc3339();
    match result {
        Ok(output) => match validate_output(input, &output) {
            Ok(()) => {
                let output_json = serde_json::to_string(&output).map_err(|error| {
                    CoreError::Parse(format!("语义找题输出序列化失败：{error}"))
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
                        CoreError::Parse(format!("语义找题错误序列化失败：{error}"))
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
                    CoreError::Parse(format!("语义找题错误序列化失败：{error}"))
                })?,
                &now,
            )?;
        }
    }
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    result_from_run(&run)
}

/// 应用在外部调用期间退出时，下一次启动把悬空 run 明确收口为可重试失败。
///
/// 语义找题没有业务副作用，因此这里只修复运行账本，不创建或修改任何题目。
pub fn recover_interrupted(conn: &Connection) -> CoreResult<usize> {
    let finished_at = time::utc_now_rfc3339();
    let error_meta_json = serde_json::json!({
        "schema_version": 1,
        "code": "semantic_search_interrupted",
        "safe_message": "上次语义找题在应用退出前未完成，可重新查找",
        "retryable": true
    })
    .to_string();
    Ok(conn.execute(
        "UPDATE ai_runs
         SET status='failed', error_meta_json=?1, finished_at=?2
         WHERE run_type=?3 AND status IN ('pending','processing')",
        rusqlite::params![error_meta_json, finished_at, RUN_TYPE],
    )?)
}

#[cfg(test)]
mod tests {
    use module_knowledge::semantic_search::{
        candidate_snapshot_hash, SemanticSearchCandidate, SemanticSearchMatch,
        SemanticSearchOption, SEMANTIC_SEARCH_INPUT_VERSION, SEMANTIC_SEARCH_SCHEMA_VERSION,
    };
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    use super::*;

    struct FakeSearcher;

    impl SemanticQuestionSearcher for FakeSearcher {
        fn provider(&self) -> &str {
            "fake"
        }
        fn model_name(&self) -> &str {
            "fake-model"
        }
        fn model_version(&self) -> &str {
            "v1"
        }
        fn config_version(&self) -> &str {
            "v1"
        }
        fn rule_version(&self) -> &str {
            "v1"
        }
        fn search(&self, _input: &SemanticSearchInput) -> CoreResult<SemanticSearchOutput> {
            unreachable!()
        }
    }

    fn input() -> SemanticSearchInput {
        let candidates = vec![SemanticSearchCandidate {
            question_version_public_id: "question-1".into(),
            revision: 1,
            owner_scope: "personal".into(),
            owner_label: "我的题库".into(),
            question_type: "single".into(),
            stem: "中国近代史开始于哪次战争？".into(),
            material_text: None,
            max_score: 1.0,
            quality_level: "L3".into(),
            options: vec![SemanticSearchOption {
                label: "A".into(),
                content: "鸦片战争".into(),
            }],
            knowledge_titles: vec!["中国近代史开端".into()],
            ability_titles: vec!["事实识记与提取".into()],
        }];
        SemanticSearchInput {
            schema_version: SEMANTIC_SEARCH_SCHEMA_VERSION,
            input_version: SEMANTIC_SEARCH_INPUT_VERSION.into(),
            query: "找考查近代史开端的题".into(),
            catalog_snapshot_hash: candidate_snapshot_hash(&candidates).unwrap(),
            result_limit: 1,
            candidates,
        }
    }

    fn output(id: &str) -> SemanticSearchOutput {
        SemanticSearchOutput {
            schema_version: 1,
            state: "ready".into(),
            confidence: 0.95,
            issue_codes: vec![],
            matches: vec![SemanticSearchMatch {
                question_version_public_id: id.into(),
                score: 0.97,
                reason: "直接考查中国近代史开端".into(),
            }],
        }
    }

    fn connection() -> Connection {
        let connection = open_in_memory().unwrap();
        run_migrations(&connection, CORE_MIGRATIONS).unwrap();
        connection
    }

    #[test]
    fn successful_result_is_audited_and_reused_by_input_hash() {
        let connection = connection();
        let input = input();
        let BeginSemanticRun::Execute { ai_run_id } =
            begin(&connection, &input, &FakeSearcher, "semantic-1").unwrap()
        else {
            panic!("expected execute")
        };
        let completed = finish(&connection, &input, ai_run_id, Ok(output("question-1"))).unwrap();
        assert_eq!(completed.status, "succeeded");
        assert!(matches!(
            begin(&connection, &input, &FakeSearcher, "semantic-2").unwrap(),
            BeginSemanticRun::Completed(result)
                if result.ai_run_id == ai_run_id && result.status == "succeeded"
        ));
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM ai_runs WHERE run_type='question_semantic_search'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );
    }

    #[test]
    fn invalid_provider_output_is_persisted_as_safe_failure() {
        let connection = connection();
        let input = input();
        let BeginSemanticRun::Execute { ai_run_id } =
            begin(&connection, &input, &FakeSearcher, "semantic-invalid").unwrap()
        else {
            panic!("expected execute")
        };
        let completed = finish(&connection, &input, ai_run_id, Ok(output("outside"))).unwrap();
        assert_eq!(completed.status, "failed");
        assert_eq!(completed.failure.unwrap().code, "semantic_search_failed");
        assert!(ai_runs::get_by_id(&connection, ai_run_id)
            .unwrap()
            .unwrap()
            .output_json
            .is_none());
    }

    #[test]
    fn startup_recovery_closes_interrupted_run_and_allows_new_request() {
        let connection = connection();
        let input = input();
        let BeginSemanticRun::Execute { ai_run_id } =
            begin(&connection, &input, &FakeSearcher, "semantic-interrupted").unwrap()
        else {
            panic!("expected execute")
        };
        assert_eq!(recover_interrupted(&connection).unwrap(), 1);
        let recovered = ai_runs::get_by_id(&connection, ai_run_id).unwrap().unwrap();
        assert_eq!(recovered.status, AiRunStatus::Failed);
        assert!(recovered
            .error_meta_json
            .as_deref()
            .is_some_and(|value| value.contains("semantic_search_interrupted")));
        assert!(matches!(
            begin(
                &connection,
                &input,
                &FakeSearcher,
                "semantic-after-restart"
            )
            .unwrap(),
            BeginSemanticRun::Execute { ai_run_id: next } if next != ai_run_id
        ));
    }
}
