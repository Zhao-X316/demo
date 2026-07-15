//! 答案图片/文本结构化的 provider-neutral 合同。
//!
//! 请求只携带老师上传的 teaching_content 与当前作业题目清单，不携带学生作答，也
//! 不携带当前标准答案。模型只能生成带来源锚点的候选，不能确认答案或改写 K1。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};

pub const ANSWER_SOURCE_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerSourceQuestionType {
    Single,
    Multiple,
    TrueFalse,
    FillBlank,
    ShortAnswer,
}

impl AnswerSourceQuestionType {
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "single" => Some(Self::Single),
            "multiple" => Some(Self::Multiple),
            "true_false" => Some(Self::TrueFalse),
            "fill_blank" => Some(Self::FillBlank),
            "short_answer" => Some(Self::ShortAnswer),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerSourceItemSpec {
    pub assessment_item_id: i64,
    pub order_index: i64,
    pub question_no: String,
    pub question_type: AnswerSourceQuestionType,
    pub stem: String,
    pub max_score: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerSourceState {
    Ready,
    NeedsReview,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerSourceEntry {
    pub assessment_item_id: i64,
    pub answer_json: Value,
    pub source_anchor: Value,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerSourceRecognizerDescriptor {
    pub provider: String,
    pub model_name: String,
    pub model_version: String,
    pub config_version: String,
    pub rule_version: String,
}

impl AnswerSourceRecognizerDescriptor {
    pub fn validate(&self) -> CoreResult<()> {
        for (label, value) in [
            ("provider", &self.provider),
            ("model_name", &self.model_name),
            ("model_version", &self.model_version),
            ("config_version", &self.config_version),
            ("rule_version", &self.rule_version),
        ] {
            if value.trim().is_empty() {
                return Err(CoreError::Invalid(format!("答案结构化 {label} 不能为空")));
            }
        }
        Ok(())
    }
}

pub struct AnswerSourceRecognitionRequest<'a> {
    pub ingest_batch_id: i64,
    pub source_artifact_id: i64,
    pub source_artifact_sha256: &'a str,
    pub source_format: &'a str,
    pub mime_type: &'a str,
    pub source_bytes: &'a [u8],
    pub source_text: Option<&'a str>,
    pub items: &'a [AnswerSourceItemSpec],
}

impl AnswerSourceRecognitionRequest<'_> {
    pub fn validate(&self) -> CoreResult<()> {
        if self.ingest_batch_id <= 0 || self.source_artifact_id <= 0 {
            return Err(CoreError::Invalid(
                "答案结构化批次/artifact id 必须为正数".into(),
            ));
        }
        if self.source_artifact_sha256.len() != 64
            || !self
                .source_artifact_sha256
                .chars()
                .all(|value| value.is_ascii_hexdigit())
        {
            return Err(CoreError::Invalid("答案资料 hash 非法".into()));
        }
        if !matches!(self.source_format, "jpeg" | "text") {
            return Err(CoreError::Invalid(
                "当前答案结构化纵切只接受图片或文本；PDF 已归档但需后续解析器".into(),
            ));
        }
        if self.source_bytes.is_empty() || self.items.is_empty() {
            return Err(CoreError::Invalid("答案资料和题目清单不能为空".into()));
        }
        if self.source_format == "text"
            && self
                .source_text
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
        {
            return Err(CoreError::Invalid("文本答案资料不能为空".into()));
        }
        let mut ids = BTreeSet::new();
        for item in self.items {
            if item.assessment_item_id <= 0
                || item.order_index < 0
                || item.question_no.trim().is_empty()
                || item.stem.trim().is_empty()
                || !item.max_score.is_finite()
                || item.max_score <= 0.0
                || !ids.insert(item.assessment_item_id)
            {
                return Err(CoreError::Invalid("答案结构化题目清单非法或重复".into()));
            }
        }
        Ok(())
    }

    pub fn input_hash(&self) -> CoreResult<String> {
        self.validate()?;
        let payload = serde_json::json!({
            "schema_version": ANSWER_SOURCE_SCHEMA_VERSION,
            "ingest_batch_id": self.ingest_batch_id,
            "source_artifact_id": self.source_artifact_id,
            "source_artifact_sha256": self.source_artifact_sha256.to_ascii_lowercase(),
            "source_format": self.source_format,
            "mime_type": self.mime_type.to_ascii_lowercase(),
            "items": self.items,
        });
        let bytes = serde_json::to_vec(&payload)
            .map_err(|error| CoreError::Parse(format!("答案结构化输入序列化失败：{error}")))?;
        Ok(hashing::sha256_hex(&bytes))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerSourceRecognitionOutput {
    pub schema_version: i64,
    pub ingest_batch_id: i64,
    pub source_artifact_id: i64,
    pub source_artifact_sha256: String,
    pub input_hash: String,
    pub descriptor: AnswerSourceRecognizerDescriptor,
    pub state: AnswerSourceState,
    pub entries: Vec<AnswerSourceEntry>,
    pub confidence: f64,
    pub issue_codes: Vec<String>,
}

fn schema_object(value: &Value, label: &str) -> CoreResult<()> {
    if !value.is_object()
        || value
            .get("schema_version")
            .and_then(Value::as_i64)
            .is_none()
    {
        return Err(CoreError::Invalid(format!(
            "答案结构化 {label} 必须是带整数 schema_version 的对象"
        )));
    }
    Ok(())
}

impl AnswerSourceRecognitionOutput {
    pub fn validate_against(&self, request: &AnswerSourceRecognitionRequest<'_>) -> CoreResult<()> {
        request.validate()?;
        self.descriptor.validate()?;
        if self.schema_version != ANSWER_SOURCE_SCHEMA_VERSION
            || self.ingest_batch_id != request.ingest_batch_id
            || self.source_artifact_id != request.source_artifact_id
            || self.source_artifact_sha256
                != request.source_artifact_sha256.trim().to_ascii_lowercase()
            || self.input_hash != request.input_hash()?
        {
            return Err(CoreError::Invalid(
                "答案结构化输出身份链与当前请求不一致".into(),
            ));
        }
        if !self.confidence.is_finite() || !(0.0..=1.0).contains(&self.confidence) {
            return Err(CoreError::Invalid("答案结构化总置信度必须位于 0~1".into()));
        }
        if self.state != AnswerSourceState::Ready && self.issue_codes.is_empty() {
            return Err(CoreError::Invalid(
                "非 ready 答案结构化结果必须携带问题码".into(),
            ));
        }
        let allowed = request
            .items
            .iter()
            .map(|item| item.assessment_item_id)
            .collect::<BTreeSet<_>>();
        let mut seen = BTreeSet::new();
        for entry in &self.entries {
            if !allowed.contains(&entry.assessment_item_id)
                || !seen.insert(entry.assessment_item_id)
            {
                return Err(CoreError::Invalid(
                    "答案结构化输出包含越界或重复题目".into(),
                ));
            }
            schema_object(&entry.answer_json, "候选答案")?;
            schema_object(&entry.source_anchor, "来源锚点")?;
            if !entry.confidence.is_finite() || !(0.0..=1.0).contains(&entry.confidence) {
                return Err(CoreError::Invalid(
                    "答案结构化逐题置信度必须位于 0~1".into(),
                ));
            }
        }
        if self.state == AnswerSourceState::Ready
            && (seen != allowed
                || self.confidence < 0.95
                || self.entries.iter().any(|entry| entry.confidence < 0.95))
        {
            return Err(CoreError::Invalid(
                "ready 答案结构化必须覆盖全部题目且置信度不低于 0.95".into(),
            ));
        }
        Ok(())
    }

    pub fn to_json_against(
        &self,
        request: &AnswerSourceRecognitionRequest<'_>,
    ) -> CoreResult<String> {
        self.validate_against(request)?;
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("答案结构化输出序列化失败：{error}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerSourceErrorCode {
    ProviderUnavailable,
    Timeout,
    RateLimited,
    UnsupportedFormat,
    InvalidOutput,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerSourceFailure {
    pub schema_version: i64,
    pub code: AnswerSourceErrorCode,
    pub safe_message: String,
    pub retryable: bool,
}

impl AnswerSourceFailure {
    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != ANSWER_SOURCE_SCHEMA_VERSION
            || self.safe_message.trim().is_empty()
        {
            return Err(CoreError::Invalid("答案结构化失败信息非法".into()));
        }
        Ok(())
    }

    pub fn to_json(&self) -> CoreResult<String> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("答案结构化错误序列化失败：{error}")))
    }
}

pub trait AnswerSourceRecognizer: Send + Sync {
    fn descriptor(&self) -> AnswerSourceRecognizerDescriptor;
    fn recognize(
        &self,
        request: &AnswerSourceRecognitionRequest<'_>,
    ) -> Result<AnswerSourceRecognitionOutput, AnswerSourceFailure>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn items() -> Vec<AnswerSourceItemSpec> {
        vec![
            AnswerSourceItemSpec {
                assessment_item_id: 11,
                order_index: 0,
                question_no: "1".into(),
                question_type: AnswerSourceQuestionType::Single,
                stem: "鸦片战争爆发于哪一年？".into(),
                max_score: 1.0,
            },
            AnswerSourceItemSpec {
                assessment_item_id: 12,
                order_index: 1,
                question_no: "2".into(),
                question_type: AnswerSourceQuestionType::TrueFalse,
                stem: "鸦片战争爆发于1840年。".into(),
                max_score: 1.0,
            },
        ]
    }

    fn request<'a>(items: &'a [AnswerSourceItemSpec]) -> AnswerSourceRecognitionRequest<'a> {
        AnswerSourceRecognitionRequest {
            ingest_batch_id: 7,
            source_artifact_id: 9,
            source_artifact_sha256: HASH,
            source_format: "text",
            mime_type: "text/plain",
            source_bytes: b"1.A 2.TRUE",
            source_text: Some("1.A 2.TRUE"),
            items,
        }
    }

    #[test]
    fn ready_output_must_cover_only_current_items() {
        let items = items();
        let request = request(&items);
        let descriptor = AnswerSourceRecognizerDescriptor {
            provider: "fixture".into(),
            model_name: "fixture".into(),
            model_version: "v1".into(),
            config_version: "v1".into(),
            rule_version: "v1".into(),
        };
        let mut output = AnswerSourceRecognitionOutput {
            schema_version: 1,
            ingest_batch_id: 7,
            source_artifact_id: 9,
            source_artifact_sha256: "a".repeat(64),
            input_hash: request.input_hash().unwrap(),
            descriptor,
            state: AnswerSourceState::Ready,
            entries: vec![
                AnswerSourceEntry {
                    assessment_item_id: 11,
                    answer_json: serde_json::json!({"schema_version":1,"correct_labels":["A"]}),
                    source_anchor: serde_json::json!({"schema_version":1,"line":1}),
                    confidence: 0.99,
                },
                AnswerSourceEntry {
                    assessment_item_id: 12,
                    answer_json: serde_json::json!({"schema_version":1,"correct":true}),
                    source_anchor: serde_json::json!({"schema_version":1,"line":1}),
                    confidence: 0.99,
                },
            ],
            confidence: 0.99,
            issue_codes: vec![],
        };
        output.validate_against(&request).unwrap();
        output.entries.pop();
        assert!(output.validate_against(&request).is_err());
    }

    #[test]
    fn input_hash_is_bound_to_source_and_questions_without_any_answer_key() {
        let items = items();
        let request = request(&items);
        let serialized = serde_json::to_string(&items).unwrap();
        assert!(!serialized.contains("correct_labels"));
        assert_eq!(request.input_hash().unwrap().len(), 64);
    }
}
