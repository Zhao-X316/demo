//! K1 知识/能力链接 AI 建议契约。
//!
//! 模型只能从输入目录中选择已有知识节点和能力维度，不能创建正式 link set。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};

pub const LINK_SUGGESTION_SCHEMA_VERSION: i64 = 1;
pub const LINK_SUGGESTION_INPUT_VERSION: &str = "k1-link-suggestion-input-v1";
pub const MAX_KNOWLEDGE_CANDIDATES: usize = 500;
pub const MAX_ABILITY_CANDIDATES: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSuggestionSource {
    pub source_type: String,
    pub source_public_id: String,
    pub label: String,
    pub detail: String,
    pub order_index: i64,
    pub required_for_l3: bool,
    pub required_knowledge_relation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSuggestionKnowledgeCandidate {
    pub public_id: String,
    pub code: Option<String>,
    pub title: String,
    pub curriculum_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSuggestionAbilityCandidate {
    pub public_id: String,
    pub code: String,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSuggestionInput {
    pub schema_version: i64,
    pub input_version: String,
    pub question_version_public_id: String,
    pub question_content_hash: String,
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub knowledge_map_public_id: String,
    pub knowledge_map_revision: i64,
    pub textbook_title: String,
    pub sources: Vec<LinkSuggestionSource>,
    pub knowledge_candidates: Vec<LinkSuggestionKnowledgeCandidate>,
    pub ability_candidates: Vec<LinkSuggestionAbilityCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedKnowledgeLink {
    pub knowledge_node_public_id: String,
    pub relation_type: String,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedAbilityLink {
    pub ability_dimension_public_id: String,
    pub evidence_strength: f64,
    pub response_mode: String,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLinkSuggestion {
    pub source_type: String,
    pub source_public_id: String,
    pub knowledge_links: Vec<SuggestedKnowledgeLink>,
    pub ability_links: Vec<SuggestedAbilityLink>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSuggestionOutput {
    pub schema_version: i64,
    pub state: String,
    pub confidence: f64,
    pub issue_codes: Vec<String>,
    pub source_suggestions: Vec<SourceLinkSuggestion>,
}

pub trait LinkSuggester: Send + Sync {
    fn provider(&self) -> &str;
    fn model_name(&self) -> &str;
    fn model_version(&self) -> &str;
    fn config_version(&self) -> &str;
    fn rule_version(&self) -> &str;
    fn suggest(&self, input: &LinkSuggestionInput) -> CoreResult<LinkSuggestionOutput>;
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

fn valid_source_type(value: &str) -> bool {
    matches!(
        value,
        "question" | "option" | "answer_slot" | "rubric_point"
    )
}

fn valid_relation(source_type: &str, relation: &str) -> bool {
    match source_type {
        "question" => matches!(relation, "direct_assessment" | "context" | "prerequisite"),
        "option" => matches!(
            relation,
            "answer_basis" | "distractor" | "misconception" | "context"
        ),
        "answer_slot" => matches!(relation, "direct_assessment" | "answer_basis"),
        "rubric_point" => relation == "rubric_basis",
        _ => false,
    }
}

fn valid_response_mode(value: &str) -> bool {
    matches!(
        value,
        "recognition" | "recall" | "structured_response" | "source_analysis" | "argumentation"
    )
}

pub fn validate_input(input: &LinkSuggestionInput) -> CoreResult<()> {
    if input.schema_version != LINK_SUGGESTION_SCHEMA_VERSION
        || input.input_version != LINK_SUGGESTION_INPUT_VERSION
    {
        return Err(CoreError::Invalid("知识链接建议输入版本不受支持".into()));
    }
    for (value, label) in [
        (&input.question_version_public_id, "题目版本"),
        (&input.question_content_hash, "题目内容 hash"),
        (&input.stem, "题干"),
        (&input.knowledge_map_public_id, "知识地图"),
        (&input.textbook_title, "教材"),
    ] {
        required(value, label)?;
    }
    if input.question_content_hash.len() != 64
        || !input
            .question_content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || !valid_question_type(&input.question_type)
        || input.knowledge_map_revision <= 0
    {
        return Err(CoreError::Invalid("知识链接建议题目或地图输入非法".into()));
    }
    if input.sources.is_empty()
        || input.knowledge_candidates.is_empty()
        || input.ability_candidates.is_empty()
        || input.knowledge_candidates.len() > MAX_KNOWLEDGE_CANDIDATES
        || input.ability_candidates.len() > MAX_ABILITY_CANDIDATES
    {
        return Err(CoreError::Invalid(
            "知识链接建议需要有限且非空的来源、知识点和能力目录".into(),
        ));
    }
    let mut source_ids = BTreeSet::new();
    for source in &input.sources {
        if !valid_source_type(&source.source_type)
            || !source_ids.insert((
                source.source_type.as_str(),
                source.source_public_id.as_str(),
            ))
        {
            return Err(CoreError::Invalid("知识链接来源非法或重复".into()));
        }
        required(&source.source_public_id, "链接来源")?;
        required(&source.label, "链接来源名称")?;
        if source.required_for_l3
            && source
                .required_knowledge_relation
                .as_deref()
                .map_or(true, |relation| {
                    !valid_relation(&source.source_type, relation)
                })
        {
            return Err(CoreError::Invalid("L3 必需来源缺少合法关系类型".into()));
        }
    }
    let mut knowledge_ids = BTreeSet::new();
    for candidate in &input.knowledge_candidates {
        required(&candidate.public_id, "知识点")?;
        required(&candidate.title, "知识点名称")?;
        if !knowledge_ids.insert(candidate.public_id.as_str()) {
            return Err(CoreError::Invalid("知识点目录存在重复项".into()));
        }
    }
    let mut ability_ids = BTreeSet::new();
    for candidate in &input.ability_candidates {
        required(&candidate.public_id, "能力维度")?;
        required(&candidate.code, "能力代码")?;
        required(&candidate.title, "能力名称")?;
        if !ability_ids.insert(candidate.public_id.as_str()) {
            return Err(CoreError::Invalid("能力目录存在重复项".into()));
        }
    }
    Ok(())
}

pub fn input_hash(input: &LinkSuggestionInput) -> CoreResult<String> {
    validate_input(input)?;
    let bytes = serde_json::to_vec(input)
        .map_err(|error| CoreError::Parse(format!("知识链接建议输入序列化失败：{error}")))?;
    Ok(hashing::sha256_hex(&bytes))
}

pub fn validate_output(
    input: &LinkSuggestionInput,
    output: &LinkSuggestionOutput,
) -> CoreResult<()> {
    validate_input(input)?;
    if output.schema_version != LINK_SUGGESTION_SCHEMA_VERSION
        || !matches!(output.state.as_str(), "ready" | "needs_review" | "blocked")
        || !output.confidence.is_finite()
        || !(0.0..=1.0).contains(&output.confidence)
    {
        return Err(CoreError::Invalid("知识链接建议输出状态非法".into()));
    }
    let sources = input
        .sources
        .iter()
        .map(|source| {
            (
                (
                    source.source_type.as_str(),
                    source.source_public_id.as_str(),
                ),
                source,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let knowledge_ids = input
        .knowledge_candidates
        .iter()
        .map(|item| item.public_id.as_str())
        .collect::<BTreeSet<_>>();
    let ability_ids = input
        .ability_candidates
        .iter()
        .map(|item| item.public_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen_sources = BTreeSet::new();
    let mut coverage = BTreeMap::new();
    for suggestion in &output.source_suggestions {
        let key = (
            suggestion.source_type.as_str(),
            suggestion.source_public_id.as_str(),
        );
        let source = sources
            .get(&key)
            .ok_or_else(|| CoreError::Invalid("模型返回了清单外链接来源".into()))?;
        if !seen_sources.insert(key) {
            return Err(CoreError::Invalid("模型重复返回同一链接来源".into()));
        }
        let mut knowledge_seen = BTreeSet::new();
        let mut core_relation_found = false;
        for link in &suggestion.knowledge_links {
            if !knowledge_ids.contains(link.knowledge_node_public_id.as_str())
                || !valid_relation(&source.source_type, &link.relation_type)
                || !link.confidence.is_finite()
                || !(0.0..=1.0).contains(&link.confidence)
                || !knowledge_seen.insert((
                    link.knowledge_node_public_id.as_str(),
                    link.relation_type.as_str(),
                ))
            {
                return Err(CoreError::Invalid(
                    "模型返回了越界、重复或非法知识链接".into(),
                ));
            }
            required(&link.reason, "知识链接理由")?;
            if source.required_knowledge_relation.as_deref() == Some(link.relation_type.as_str()) {
                core_relation_found = true;
            }
        }
        let mut ability_seen = BTreeSet::new();
        for link in &suggestion.ability_links {
            if !ability_ids.contains(link.ability_dimension_public_id.as_str())
                || !link.evidence_strength.is_finite()
                || !(0.0..=1.0).contains(&link.evidence_strength)
                || link.evidence_strength <= 0.0
                || !link.confidence.is_finite()
                || !(0.0..=1.0).contains(&link.confidence)
                || !valid_response_mode(&link.response_mode)
                || !ability_seen.insert((
                    link.ability_dimension_public_id.as_str(),
                    link.response_mode.as_str(),
                ))
            {
                return Err(CoreError::Invalid(
                    "模型返回了越界、重复或非法能力链接".into(),
                ));
            }
            required(&link.reason, "能力链接理由")?;
        }
        coverage.insert(
            key,
            (core_relation_found, !suggestion.ability_links.is_empty()),
        );
    }
    let ready = input
        .sources
        .iter()
        .filter(|source| source.required_for_l3)
        .all(|source| {
            coverage
                .get(&(
                    source.source_type.as_str(),
                    source.source_public_id.as_str(),
                ))
                .copied()
                == Some((true, true))
        });
    if output.state == "ready"
        && (!ready || output.confidence < 0.95 || !output.issue_codes.is_empty())
    {
        return Err(CoreError::Invalid(
            "未完整覆盖必需来源的建议不能标记为 ready".into(),
        ));
    }
    if output.state == "blocked" && !output.source_suggestions.is_empty() {
        return Err(CoreError::Invalid("blocked 建议不得携带链接".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> LinkSuggestionInput {
        LinkSuggestionInput {
            schema_version: LINK_SUGGESTION_SCHEMA_VERSION,
            input_version: LINK_SUGGESTION_INPUT_VERSION.into(),
            question_version_public_id: "question-1".into(),
            question_content_hash: "a".repeat(64),
            question_type: "fill_blank".into(),
            stem: "鸦片战争爆发于____年，由____发动。".into(),
            material_text: None,
            knowledge_map_public_id: "map-1".into(),
            knowledge_map_revision: 1,
            textbook_title: "中国历史八年级上册".into(),
            sources: vec![
                LinkSuggestionSource {
                    source_type: "answer_slot".into(),
                    source_public_id: "slot-1".into(),
                    label: "第 1 空".into(),
                    detail: "1840".into(),
                    order_index: 0,
                    required_for_l3: true,
                    required_knowledge_relation: Some("direct_assessment".into()),
                },
                LinkSuggestionSource {
                    source_type: "answer_slot".into(),
                    source_public_id: "slot-2".into(),
                    label: "第 2 空".into(),
                    detail: "英国".into(),
                    order_index: 1,
                    required_for_l3: true,
                    required_knowledge_relation: Some("direct_assessment".into()),
                },
            ],
            knowledge_candidates: vec![LinkSuggestionKnowledgeCandidate {
                public_id: "knowledge-1".into(),
                code: Some("K1".into()),
                title: "鸦片战争".into(),
                curriculum_title: Some("第一课".into()),
            }],
            ability_candidates: vec![LinkSuggestionAbilityCandidate {
                public_id: "ability-1".into(),
                code: "fact_recall".into(),
                title: "事实识记与提取".into(),
                description: None,
            }],
        }
    }

    fn complete_source(source_public_id: &str) -> SourceLinkSuggestion {
        SourceLinkSuggestion {
            source_type: "answer_slot".into(),
            source_public_id: source_public_id.into(),
            knowledge_links: vec![SuggestedKnowledgeLink {
                knowledge_node_public_id: "knowledge-1".into(),
                relation_type: "direct_assessment".into(),
                confidence: 0.98,
                reason: "直接考查".into(),
            }],
            ability_links: vec![SuggestedAbilityLink {
                ability_dimension_public_id: "ability-1".into(),
                evidence_strength: 0.6,
                response_mode: "recall".into(),
                confidence: 0.98,
                reason: "回忆作答".into(),
            }],
        }
    }

    #[test]
    fn ready_requires_every_l3_source_and_no_issues() {
        let source = input();
        let incomplete = LinkSuggestionOutput {
            schema_version: LINK_SUGGESTION_SCHEMA_VERSION,
            state: "ready".into(),
            confidence: 0.98,
            issue_codes: Vec::new(),
            source_suggestions: vec![complete_source("slot-1")],
        };
        assert!(validate_output(&source, &incomplete).is_err());

        let complete = LinkSuggestionOutput {
            source_suggestions: vec![complete_source("slot-1"), complete_source("slot-2")],
            ..incomplete
        };
        validate_output(&source, &complete).unwrap();
    }

    #[test]
    fn output_rejects_catalog_escape_and_zero_strength() {
        let source = input();
        let mut escaped = complete_source("slot-1");
        escaped.knowledge_links[0].knowledge_node_public_id = "invented-knowledge".into();
        let output = LinkSuggestionOutput {
            schema_version: LINK_SUGGESTION_SCHEMA_VERSION,
            state: "needs_review".into(),
            confidence: 0.8,
            issue_codes: vec!["requires_teacher_review".into()],
            source_suggestions: vec![escaped],
        };
        assert!(validate_output(&source, &output).is_err());

        let mut zero_strength = complete_source("slot-1");
        zero_strength.ability_links[0].evidence_strength = 0.0;
        let output = LinkSuggestionOutput {
            source_suggestions: vec![zero_strength],
            ..output
        };
        assert!(validate_output(&source, &output).is_err());
    }
}
