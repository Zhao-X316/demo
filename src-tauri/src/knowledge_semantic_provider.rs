//! 火山方舟 K1 受控语义检索适配器。
//!
//! 本地数据库先冻结老师有权访问的有限候选清单；模型只能返回清单内题目版本 ID
//! 及相关性解释，不能创建、修改、合并或发布题目。

use std::time::Duration;

use module_knowledge::semantic_search::{
    validate_output, SemanticQuestionSearcher, SemanticSearchInput, SemanticSearchOutput,
};
use serde_json::{json, Value};
use suite_core::error::{CoreError, CoreResult};

use crate::secrets::VolcanoCreds;
use crate::vlm::{ARK_URL, DEFAULT_MODEL};

const MODEL_VERSION: &str = "ark-chat-completions-v3";
const CONFIG_VERSION: &str = "k1-semantic-catalog-rerank-v1";
const RULE_VERSION: &str = "k1-semantic-search-suggestion-only-v1";

pub struct ArkSemanticQuestionSearcher {
    api_key: String,
    model: String,
    endpoint: String,
}

impl ArkSemanticQuestionSearcher {
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

fn prompt(input: &SemanticSearchInput) -> CoreResult<String> {
    let catalog = serde_json::to_string(input)
        .map_err(|error| CoreError::Parse(format!("语义检索输入序列化失败：{error}")))?;
    Ok(format!(
        "你是历史教师私有题库的语义找题助手。老师要按意思查找题目。\
         你只能在完整输入 candidates 中按语义相关性排序，绝不能创造 ID、补写题目、\
         修改答案、合并题目、发布题目、给学生评分或形成学习证据。\n\
         完整输入：{catalog}\n\
         最多返回 resultLimit 条，score 从高到低，reason 用不超过 60 个中文字符解释\
         为什么题目符合老师要找的意思。没有足够相关候选时可以少返回或返回空数组。\n\
         只输出 JSON，不要 Markdown。结构必须是：\
         {{\"schemaVersion\":1,\"state\":\"ready|needs_review|blocked\",\
         \"confidence\":0~1,\"issueCodes\":[string],\"matches\":[{{\
         \"questionVersionPublicId\":string,\"score\":0~1,\"reason\":string}}]}}。\n\
         只有排序明确、confidence 不低于 0.75 且无问题码时才能 state=ready；\
         含义含混时 needs_review；无法判断时 blocked 且 matches=[]。"
    ))
}

impl SemanticQuestionSearcher for ArkSemanticQuestionSearcher {
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

    fn search(&self, input: &SemanticSearchInput) -> CoreResult<SemanticSearchOutput> {
        module_knowledge::semantic_search::validate_input(input)?;
        if self.api_key.trim().is_empty() {
            return Err(CoreError::Invalid(
                "未配置 AI 服务，可改用本机关键词查找".into(),
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
            .map_err(|_| CoreError::Invalid("语义找题客户端初始化失败".into()))?;
        let response = client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    CoreError::Invalid("语义找题超时，可稍后重试或改用关键词查找".into())
                } else {
                    CoreError::Invalid("语义找题服务暂不可用，可改用关键词查找".into())
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(CoreError::Invalid(if status.as_u16() == 429 {
                "语义找题请求过多，可稍后重试".into()
            } else {
                "语义找题服务返回异常，可改用关键词查找".into()
            }));
        }
        let envelope: Value = response
            .json()
            .map_err(|_| CoreError::Invalid("语义找题响应无法解析".into()))?;
        let raw = envelope
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Invalid("语义找题未返回内容".into()))?;
        let output: SemanticSearchOutput = serde_json::from_str(strip_json_fence(raw))
            .map_err(|_| CoreError::Invalid("语义找题结果格式无效".into()))?;
        validate_output(input, &output)?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use module_knowledge::semantic_search::{
        candidate_snapshot_hash, SemanticSearchCandidate, SemanticSearchMatch,
        SemanticSearchOption, SEMANTIC_SEARCH_INPUT_VERSION, SEMANTIC_SEARCH_SCHEMA_VERSION,
    };

    use super::*;

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

    fn wire_output(id: &str) -> Value {
        json!({
            "schemaVersion": 1,
            "state": "ready",
            "confidence": 0.95,
            "issueCodes": [],
            "matches": [{
                "questionVersionPublicId": id,
                "score": 0.97,
                "reason": "直接考查中国近代史开端"
            }]
        })
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
    fn adapter_accepts_catalog_bound_output() {
        let body = json!({
            "choices":[{"message":{"content":wire_output("question-1").to_string()}}]
        })
        .to_string();
        let provider = ArkSemanticQuestionSearcher::for_test(serve_once(body));
        assert_eq!(provider.search(&input()).unwrap(), output("question-1"));
    }

    #[test]
    fn adapter_rejects_out_of_catalog_output() {
        let body = json!({
            "choices":[{"message":{"content":wire_output("outside").to_string()}}]
        })
        .to_string();
        let provider = ArkSemanticQuestionSearcher::for_test(serve_once(body));
        assert!(provider.search(&input()).is_err());
    }
}
