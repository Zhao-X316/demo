//! 火山方舟 K1 知识/能力链接建议适配器。
//!
//! 输入不含学生数据；模型只能从已确认知识地图和能力目录中选择候选，输出仍是草稿。

use std::time::Duration;

use module_knowledge::link_suggestion::{
    validate_output, LinkSuggester, LinkSuggestionInput, LinkSuggestionOutput,
};
use serde_json::{json, Value};
use suite_core::error::{CoreError, CoreResult};

use crate::secrets::VolcanoCreds;
use crate::vlm::{ARK_URL, DEFAULT_MODEL};

const MODEL_VERSION: &str = "ark-chat-completions-v3";
const CONFIG_VERSION: &str = "k1-link-catalog-selection-v1";
const RULE_VERSION: &str = "k1-link-suggestion-teacher-review-v1";

pub struct ArkKnowledgeLinkSuggester {
    api_key: String,
    model: String,
    endpoint: String,
}

impl ArkKnowledgeLinkSuggester {
    pub fn from_creds(creds: &VolcanoCreds) -> Self {
        Self {
            api_key: creds.ark_api_key.clone(),
            model: if creds.ark_model.trim().is_empty() {
                DEFAULT_MODEL.into()
            } else {
                creds.ark_model.trim().into()
            },
            endpoint: ARK_URL.into(),
        }
    }

    #[cfg(test)]
    fn for_test(endpoint: String) -> Self {
        Self {
            api_key: "test-key".into(),
            model: "test-model".into(),
            endpoint,
        }
    }
}

fn strip_json_fence(content: &str) -> &str {
    let content = content.trim();
    let content = content
        .strip_prefix("```json")
        .or_else(|| content.strip_prefix("```"))
        .unwrap_or(content)
        .trim();
    content.strip_suffix("```").unwrap_or(content).trim()
}

fn prompt(input: &LinkSuggestionInput) -> CoreResult<String> {
    let catalog = serde_json::to_string(input)
        .map_err(|error| CoreError::Parse(format!("知识链接输入序列化失败：{error}")))?;
    Ok(format!(
        "你是历史教师私有题库的知识与能力链接助手。只能从给定 knowledgeCandidates 和 \
         abilityCandidates 中选择，不能创造 ID、改题、改答案、给学生评分或发布题目。\n\
         完整输入：{catalog}\n\
         只输出 JSON，不要 Markdown。结构必须是：\
         {{\"schemaVersion\":1,\"state\":\"ready|needs_review|blocked\",\
         \"confidence\":0~1,\"issueCodes\":[string],\"sourceSuggestions\":[{{\
         \"sourceType\":string,\"sourcePublicId\":string,\
         \"knowledgeLinks\":[{{\"knowledgeNodePublicId\":string,\"relationType\":string,\
         \"confidence\":0~1,\"reason\":string}}],\
         \"abilityLinks\":[{{\"abilityDimensionPublicId\":string,\"evidenceStrength\":0~1,\
         \"responseMode\":string,\"confidence\":0~1,\"reason\":string}}]}}]}}。\n\
         必需来源必须覆盖 requiredKnowledgeRelation 指定的知识关系并至少有一个能力链接。\
         整题可用 direct_assessment/context/prerequisite；选项只用 answer_basis/distractor/\
         misconception/context；填空槽位用 direct_assessment/answer_basis；评分点只用 rubric_basis。\
         客观题能力通常是 recognition，填空通常是 recall，简答按任务选择 structured_response、\
         source_analysis 或 argumentation。evidenceStrength 必须保守，选择判断不得冒充高阶能力。\
         只有全部必需来源有完整建议、总置信度不低于0.95且无问题码时才能 state=ready；\
         不能确定时 needs_review，目录无法支持时 blocked 且 sourceSuggestions=[]。"
    ))
}

impl LinkSuggester for ArkKnowledgeLinkSuggester {
    fn provider(&self) -> &str {
        "volcengine_ark"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn model_version(&self) -> &str {
        MODEL_VERSION
    }

    fn config_version(&self) -> &str {
        CONFIG_VERSION
    }

    fn rule_version(&self) -> &str {
        RULE_VERSION
    }

    fn suggest(&self, input: &LinkSuggestionInput) -> CoreResult<LinkSuggestionOutput> {
        module_knowledge::link_suggestion::validate_input(input)?;
        if self.api_key.trim().is_empty() {
            return Err(CoreError::Invalid(
                "未配置 AI 服务，可直接由老师手工关联知识点与能力".into(),
            ));
        }
        let body = json!({
            "model": self.model,
            "messages": [{"role":"user","content":prompt(input)?}],
            "temperature": 0.0
        });
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|_| CoreError::Invalid("知识链接建议客户端初始化失败".into()))?;
        let response = client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    CoreError::Invalid("知识链接建议超时，可稍后重试或手工关联".into())
                } else {
                    CoreError::Invalid("知识链接建议服务暂不可用，可手工关联".into())
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(CoreError::Invalid(if status.as_u16() == 429 {
                "知识链接建议请求过多，可稍后重试".into()
            } else {
                "知识链接建议服务返回异常，可手工关联".into()
            }));
        }
        let envelope: Value = response
            .json()
            .map_err(|_| CoreError::Invalid("知识链接建议响应无法解析".into()))?;
        let raw = envelope
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Invalid("知识链接建议未返回内容".into()))?;
        let output: LinkSuggestionOutput = serde_json::from_str(strip_json_fence(raw))
            .map_err(|_| CoreError::Invalid("知识链接建议结果格式无效".into()))?;
        validate_output(input, &output)?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use module_knowledge::link_suggestion::{
        LinkSuggestionAbilityCandidate, LinkSuggestionKnowledgeCandidate, LinkSuggestionSource,
        SourceLinkSuggestion, SuggestedAbilityLink, SuggestedKnowledgeLink,
        LINK_SUGGESTION_INPUT_VERSION, LINK_SUGGESTION_SCHEMA_VERSION,
    };

    use super::*;

    fn input() -> LinkSuggestionInput {
        LinkSuggestionInput {
            schema_version: LINK_SUGGESTION_SCHEMA_VERSION,
            input_version: LINK_SUGGESTION_INPUT_VERSION.into(),
            question_version_public_id: "question-1".into(),
            question_content_hash: "a".repeat(64),
            question_type: "single".into(),
            stem: "鸦片战争爆发于哪一年？".into(),
            material_text: None,
            knowledge_map_public_id: "map-1".into(),
            knowledge_map_revision: 1,
            textbook_title: "中国历史八年级上册".into(),
            sources: vec![LinkSuggestionSource {
                source_type: "question".into(),
                source_public_id: "question-1".into(),
                label: "整道题".into(),
                detail: "鸦片战争爆发于哪一年？".into(),
                order_index: 0,
                required_for_l3: true,
                required_knowledge_relation: Some("direct_assessment".into()),
            }],
            knowledge_candidates: vec![LinkSuggestionKnowledgeCandidate {
                public_id: "knowledge-1".into(),
                code: Some("K1".into()),
                title: "鸦片战争爆发时间".into(),
                curriculum_title: Some("鸦片战争".into()),
            }],
            ability_candidates: vec![LinkSuggestionAbilityCandidate {
                public_id: "ability-1".into(),
                code: "fact_recall".into(),
                title: "事实识记与提取".into(),
                description: None,
            }],
        }
    }

    fn output() -> LinkSuggestionOutput {
        LinkSuggestionOutput {
            schema_version: 1,
            state: "ready".into(),
            confidence: 0.98,
            issue_codes: vec![],
            source_suggestions: vec![SourceLinkSuggestion {
                source_type: "question".into(),
                source_public_id: "question-1".into(),
                knowledge_links: vec![SuggestedKnowledgeLink {
                    knowledge_node_public_id: "knowledge-1".into(),
                    relation_type: "direct_assessment".into(),
                    confidence: 0.99,
                    reason: "直接考查爆发时间".into(),
                }],
                ability_links: vec![SuggestedAbilityLink {
                    ability_dimension_public_id: "ability-1".into(),
                    evidence_strength: 0.4,
                    response_mode: "recognition".into(),
                    confidence: 0.97,
                    reason: "客观题仅提供识别型证据".into(),
                }],
            }],
        }
    }

    fn serve_once(body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 32768];
            let _ = stream.read(&mut request);
            let reply = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(reply.as_bytes()).unwrap();
        });
        format!("http://{address}")
    }

    #[test]
    fn adapter_accepts_only_catalog_bound_output() {
        let body = serde_json::json!({
            "choices":[{"message":{"content":serde_json::to_string(&output()).unwrap()}}]
        })
        .to_string();
        let provider = ArkKnowledgeLinkSuggester::for_test(serve_once(body));
        assert_eq!(provider.suggest(&input()).unwrap(), output());
    }
}
