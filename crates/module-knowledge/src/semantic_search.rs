//! K1 受控语义检索契约。
//!
//! 结构化权限、质量、题型和教材范围由本地数据库先筛选。AI 只能在冻结候选清单
//! 中按“意思是否相关”排序，不能创建题目、改答案、合并题目或形成学习证据。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};

pub const SEMANTIC_SEARCH_SCHEMA_VERSION: i64 = 1;
pub const SEMANTIC_SEARCH_INPUT_VERSION: &str = "k1-semantic-search-input-v1";
pub const MAX_SEMANTIC_CANDIDATES: usize = 80;
pub const MAX_SEMANTIC_RESULTS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSearchOption {
    pub label: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSearchCandidate {
    pub question_version_public_id: String,
    pub revision: i64,
    pub owner_scope: String,
    pub owner_label: String,
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub max_score: f64,
    pub quality_level: String,
    pub options: Vec<SemanticSearchOption>,
    pub knowledge_titles: Vec<String>,
    pub ability_titles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSearchInput {
    pub schema_version: i64,
    pub input_version: String,
    pub query: String,
    pub catalog_snapshot_hash: String,
    pub result_limit: i64,
    pub candidates: Vec<SemanticSearchCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSearchMatch {
    pub question_version_public_id: String,
    pub score: f64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticSearchOutput {
    #[serde(alias = "schemaVersion")]
    pub schema_version: i64,
    pub state: String,
    pub confidence: f64,
    #[serde(alias = "issueCodes")]
    pub issue_codes: Vec<String>,
    pub matches: Vec<SemanticSearchMatch>,
}

pub trait SemanticQuestionSearcher: Send + Sync {
    fn provider(&self) -> &str;
    fn model_name(&self) -> &str;
    fn model_version(&self) -> &str;
    fn config_version(&self) -> &str;
    fn rule_version(&self) -> &str;
    fn search(&self, input: &SemanticSearchInput) -> CoreResult<SemanticSearchOutput>;
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn valid_question_type(value: &str) -> bool {
    matches!(
        value,
        "single" | "multiple" | "true_false" | "fill_blank" | "short_answer"
    )
}

fn valid_quality(value: &str) -> bool {
    matches!(value, "C0" | "L0" | "L1" | "L2" | "L3" | "L4")
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn candidate_snapshot_hash(candidates: &[SemanticSearchCandidate]) -> CoreResult<String> {
    let bytes = serde_json::to_vec(candidates)
        .map_err(|error| CoreError::Parse(format!("语义候选清单序列化失败：{error}")))?;
    Ok(hashing::sha256_hex(&bytes))
}

pub fn validate_input(input: &SemanticSearchInput) -> CoreResult<()> {
    if input.schema_version != SEMANTIC_SEARCH_SCHEMA_VERSION
        || input.input_version != SEMANTIC_SEARCH_INPUT_VERSION
    {
        return Err(CoreError::Invalid("语义检索输入版本不受支持".into()));
    }
    required(&input.query, "要查找的意思")?;
    let query_len = input.query.trim().chars().count();
    if !(2..=100).contains(&query_len) {
        return Err(CoreError::Invalid(
            "按意思查找需要输入 2～100 个字符".into(),
        ));
    }
    if input.candidates.is_empty()
        || input.candidates.len() > MAX_SEMANTIC_CANDIDATES
        || !(1..=MAX_SEMANTIC_RESULTS as i64).contains(&input.result_limit)
        || input.result_limit as usize > input.candidates.len()
        || !valid_hash(&input.catalog_snapshot_hash)
    {
        return Err(CoreError::Invalid("语义检索候选范围或返回数量非法".into()));
    }
    let expected_hash = candidate_snapshot_hash(&input.candidates)?;
    if !input
        .catalog_snapshot_hash
        .eq_ignore_ascii_case(&expected_hash)
    {
        return Err(CoreError::Invalid("语义检索候选清单 hash 不一致".into()));
    }
    let mut ids = BTreeSet::new();
    for candidate in &input.candidates {
        required(&candidate.question_version_public_id, "候选题目版本")?;
        required(&candidate.owner_label, "候选题目来源")?;
        required(&candidate.stem, "候选题干")?;
        if !ids.insert(candidate.question_version_public_id.as_str())
            || candidate.revision <= 0
            || !matches!(candidate.owner_scope.as_str(), "personal" | "official")
            || candidate.owner_label.chars().count() > 30
            || !valid_question_type(&candidate.question_type)
            || !valid_quality(&candidate.quality_level)
            || !candidate.max_score.is_finite()
            || candidate.max_score <= 0.0
            || candidate.stem.chars().count() > 500
            || candidate
                .material_text
                .as_deref()
                .is_some_and(|value| value.chars().count() > 500)
            || candidate.options.len() > 12
            || candidate.options.iter().any(|option| {
                option.label.trim().is_empty()
                    || option.label.chars().count() > 16
                    || option.content.trim().is_empty()
                    || option.content.chars().count() > 200
            })
            || candidate.knowledge_titles.len() > 20
            || candidate.ability_titles.len() > 12
            || candidate
                .knowledge_titles
                .iter()
                .chain(candidate.ability_titles.iter())
                .any(|title| title.trim().is_empty() || title.chars().count() > 100)
        {
            return Err(CoreError::Invalid(
                "语义检索候选包含重复项或越界内容".into(),
            ));
        }
    }
    Ok(())
}

pub fn input_hash(input: &SemanticSearchInput) -> CoreResult<String> {
    validate_input(input)?;
    let bytes = serde_json::to_vec(input)
        .map_err(|error| CoreError::Parse(format!("语义检索输入序列化失败：{error}")))?;
    Ok(hashing::sha256_hex(&bytes))
}

pub fn validate_output(
    input: &SemanticSearchInput,
    output: &SemanticSearchOutput,
) -> CoreResult<()> {
    validate_input(input)?;
    if output.schema_version != SEMANTIC_SEARCH_SCHEMA_VERSION
        || !matches!(output.state.as_str(), "ready" | "needs_review" | "blocked")
        || !output.confidence.is_finite()
        || !(0.0..=1.0).contains(&output.confidence)
        || output.matches.len() > input.result_limit as usize
        || output.issue_codes.len() > 20
        || output
            .issue_codes
            .iter()
            .any(|code| code.trim().is_empty() || code.chars().count() > 100)
    {
        return Err(CoreError::Invalid("语义检索输出状态非法".into()));
    }
    if output.state == "blocked" && !output.matches.is_empty() {
        return Err(CoreError::Invalid("blocked 语义检索不得返回候选".into()));
    }
    if output.state == "ready"
        && (output.confidence < 0.75 || !output.issue_codes.is_empty() || output.matches.is_empty())
    {
        return Err(CoreError::Invalid(
            "低置信度或有问题的语义结果不能标记为 ready".into(),
        ));
    }
    if output.state != "ready" && output.issue_codes.is_empty() {
        return Err(CoreError::Invalid(
            "待确认或阻断的语义结果必须说明问题".into(),
        ));
    }
    let candidate_ids = input
        .candidates
        .iter()
        .map(|candidate| candidate.question_version_public_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut previous_score = 1.0;
    for matched in &output.matches {
        if !candidate_ids.contains(matched.question_version_public_id.as_str())
            || !seen.insert(matched.question_version_public_id.as_str())
            || !matched.score.is_finite()
            || !(0.0..=1.0).contains(&matched.score)
            || matched.score > previous_score + f64::EPSILON
            || matched.reason.trim().is_empty()
            || matched.reason.chars().count() > 200
        {
            return Err(CoreError::Invalid(
                "语义检索返回了越界、重复或未按分数排序的候选".into(),
            ));
        }
        previous_score = matched.score;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, stem: &str) -> SemanticSearchCandidate {
        SemanticSearchCandidate {
            question_version_public_id: id.into(),
            revision: 1,
            owner_scope: "personal".into(),
            owner_label: "我的题库".into(),
            question_type: "single".into(),
            stem: stem.into(),
            material_text: None,
            max_score: 1.0,
            quality_level: "L1".into(),
            options: vec![
                SemanticSearchOption {
                    label: "A".into(),
                    content: "1840年".into(),
                },
                SemanticSearchOption {
                    label: "B".into(),
                    content: "1842年".into(),
                },
            ],
            knowledge_titles: vec!["鸦片战争爆发时间".into()],
            ability_titles: vec!["事实识记与提取".into()],
        }
    }

    fn input() -> SemanticSearchInput {
        let candidates = vec![
            candidate("question-1", "中国近代史开始于哪次战争？"),
            candidate("question-2", "《南京条约》签订于哪一年？"),
        ];
        SemanticSearchInput {
            schema_version: SEMANTIC_SEARCH_SCHEMA_VERSION,
            input_version: SEMANTIC_SEARCH_INPUT_VERSION.into(),
            query: "寻找考查中国近代史开端的题".into(),
            catalog_snapshot_hash: candidate_snapshot_hash(&candidates).unwrap(),
            result_limit: 2,
            candidates,
        }
    }

    #[test]
    fn validates_catalog_bound_ranked_output() {
        let input = input();
        let output = SemanticSearchOutput {
            schema_version: 1,
            state: "ready".into(),
            confidence: 0.93,
            issue_codes: vec![],
            matches: vec![
                SemanticSearchMatch {
                    question_version_public_id: "question-1".into(),
                    score: 0.96,
                    reason: "题目直接询问近代史开端".into(),
                },
                SemanticSearchMatch {
                    question_version_public_id: "question-2".into(),
                    score: 0.41,
                    reason: "同属鸦片战争但不直接考查开端".into(),
                },
            ],
        };
        validate_output(&input, &output).unwrap();
        assert_eq!(input_hash(&input).unwrap().len(), 64);
    }

    #[test]
    fn rejects_unknown_duplicate_or_unsorted_matches() {
        let input = input();
        for matches in [
            vec![SemanticSearchMatch {
                question_version_public_id: "outside".into(),
                score: 0.9,
                reason: "越界".into(),
            }],
            vec![
                SemanticSearchMatch {
                    question_version_public_id: "question-1".into(),
                    score: 0.5,
                    reason: "第一条".into(),
                },
                SemanticSearchMatch {
                    question_version_public_id: "question-2".into(),
                    score: 0.8,
                    reason: "排序错误".into(),
                },
            ],
        ] {
            let output = SemanticSearchOutput {
                schema_version: 1,
                state: "needs_review".into(),
                confidence: 0.6,
                issue_codes: vec!["needs_teacher_review".into()],
                matches,
            };
            assert!(validate_output(&input, &output).is_err());
        }
    }

    #[test]
    fn rejects_changed_snapshot_and_blocked_with_matches() {
        let mut changed_input = input();
        changed_input.candidates[0].stem = "内容发生变化".into();
        assert!(validate_input(&changed_input).is_err());

        let input = input();
        let output = SemanticSearchOutput {
            schema_version: 1,
            state: "blocked".into(),
            confidence: 0.0,
            issue_codes: vec!["provider_uncertain".into()],
            matches: vec![SemanticSearchMatch {
                question_version_public_id: "question-1".into(),
                score: 0.6,
                reason: "不应返回".into(),
            }],
        };
        assert!(validate_output(&input, &output).is_err());
    }
}
