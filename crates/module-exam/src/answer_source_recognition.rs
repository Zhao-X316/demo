//! 答案图片/PDF/文本/Office 结构化的 provider-neutral 合同。
//!
//! 请求只携带老师上传的 teaching_content 与当前作业题目清单，不携带学生作答，也
//! 不携带当前标准答案。模型只能生成带来源锚点的候选，不能确认答案或改写 K1。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};

pub const ANSWER_SOURCE_SCHEMA_VERSION: i64 = 1;
pub const ANSWER_SOURCE_MAX_VISUAL_PAGES: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerSourceVisualPage {
    pub page_no: i64,
    pub mime_type: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
}

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
    pub text_extraction_version: Option<&'a str>,
    pub visualization_version: Option<&'a str>,
    pub visual_pages: &'a [AnswerSourceVisualPage],
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
        if !matches!(
            self.source_format,
            "jpeg" | "pdf" | "text" | "docx" | "xlsx"
        ) {
            return Err(CoreError::Invalid(
                "答案结构化只接受 JPG、PDF、文本、DOCX 或 XLSX".into(),
            ));
        }
        if self.source_bytes.is_empty() || self.items.is_empty() {
            return Err(CoreError::Invalid("答案资料和题目清单不能为空".into()));
        }
        if hashing::sha256_hex(self.source_bytes)
            != self.source_artifact_sha256.trim().to_ascii_lowercase()
        {
            return Err(CoreError::Invalid("答案资料内容与登记 hash 不一致".into()));
        }
        match self.source_format {
            "text" => {
                if self
                    .source_text
                    .map(str::trim)
                    .unwrap_or_default()
                    .is_empty()
                    || self.text_extraction_version.is_some()
                    || self.visualization_version.is_some()
                    || !self.visual_pages.is_empty()
                {
                    return Err(CoreError::Invalid("文本答案资料输入形态非法".into()));
                }
            }
            "docx" | "xlsx" => {
                if self
                    .source_text
                    .map(str::trim)
                    .unwrap_or_default()
                    .is_empty()
                    || self
                        .text_extraction_version
                        .map(str::trim)
                        .unwrap_or_default()
                        .is_empty()
                    || self.visualization_version.is_some()
                    || !self.visual_pages.is_empty()
                {
                    return Err(CoreError::Invalid("Word/Excel 答案资料输入形态非法".into()));
                }
            }
            "jpeg" | "pdf" => {
                if self.source_text.is_some()
                    || self.text_extraction_version.is_some()
                    || self
                        .visualization_version
                        .map(str::trim)
                        .unwrap_or_default()
                        .is_empty()
                    || self.visual_pages.is_empty()
                    || self.visual_pages.len() > ANSWER_SOURCE_MAX_VISUAL_PAGES
                    || (self.source_format == "jpeg" && self.visual_pages.len() != 1)
                {
                    return Err(CoreError::Invalid("图片/PDF 答案资料输入形态非法".into()));
                }
                for (index, page) in self.visual_pages.iter().enumerate() {
                    if page.page_no != (index + 1) as i64
                        || !page.mime_type.trim().eq_ignore_ascii_case("image/jpeg")
                        || page.bytes.is_empty()
                        || page.sha256.len() != 64
                        || !page.sha256.chars().all(|value| value.is_ascii_hexdigit())
                        || hashing::sha256_hex(&page.bytes) != page.sha256.to_ascii_lowercase()
                    {
                        return Err(CoreError::Invalid(
                            "答案资料视觉分页必须连续、可读且 hash 一致".into(),
                        ));
                    }
                }
                if self.source_format == "jpeg"
                    && (self.visual_pages[0].bytes != self.source_bytes
                        || !self.visual_pages[0]
                            .sha256
                            .eq_ignore_ascii_case(self.source_artifact_sha256.trim()))
                {
                    return Err(CoreError::Invalid(
                        "JPG 答案资料视觉页必须对应原始 artifact".into(),
                    ));
                }
            }
            _ => unreachable!(),
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
            "source_text_sha256": self.source_text.map(|value| hashing::sha256_hex(value.as_bytes())),
            "text_extraction_version": self.text_extraction_version,
            "visualization_version": self.visualization_version,
            "visual_pages": self.visual_pages.iter().map(|page| serde_json::json!({
                "page_no": page.page_no,
                "mime_type": page.mime_type.to_ascii_lowercase(),
                "sha256": page.sha256.to_ascii_lowercase(),
            })).collect::<Vec<_>>(),
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
            match request.source_format {
                "jpeg" | "pdf" => {
                    let page = entry
                        .source_anchor
                        .get("page")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| CoreError::Invalid("图片来源锚点缺少页码".into()))?;
                    if page <= 0 || page > request.visual_pages.len() as i64 {
                        return Err(CoreError::Invalid("答案来源锚点页码越界".into()));
                    }
                }
                "text" | "docx" | "xlsx" => {
                    if entry
                        .source_anchor
                        .get("line")
                        .and_then(Value::as_i64)
                        .is_none_or(|line| line <= 0)
                    {
                        return Err(CoreError::Invalid("文本来源锚点缺少有效行号".into()));
                    }
                }
                _ => unreachable!(),
            }
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

    const SOURCE: &[u8] = b"1.A 2.TRUE";

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
            source_artifact_sha256:
                "bef970ade57f0f91db0bebb0d8ba7b9525337025d59113c66f4940a970ba5de5",
            source_format: "text",
            mime_type: "text/plain",
            source_bytes: SOURCE,
            source_text: Some("1.A 2.TRUE"),
            text_extraction_version: None,
            visualization_version: None,
            visual_pages: &[],
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
            source_artifact_sha256: request.source_artifact_sha256.into(),
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

    #[test]
    fn pdf_pages_and_output_anchors_are_bound_to_real_page_numbers() {
        let items = items();
        let source = b"%PDF-fixture";
        let pages = vec![
            AnswerSourceVisualPage {
                page_no: 1,
                mime_type: "image/jpeg".into(),
                sha256: hashing::sha256_hex(b"page-1"),
                bytes: b"page-1".to_vec(),
            },
            AnswerSourceVisualPage {
                page_no: 2,
                mime_type: "image/jpeg".into(),
                sha256: hashing::sha256_hex(b"page-2"),
                bytes: b"page-2".to_vec(),
            },
        ];
        let request = AnswerSourceRecognitionRequest {
            ingest_batch_id: 7,
            source_artifact_id: 9,
            source_artifact_sha256: &hashing::sha256_hex(source),
            source_format: "pdf",
            mime_type: "application/pdf",
            source_bytes: source,
            source_text: None,
            text_extraction_version: None,
            visualization_version: Some("fixture-renderer-v1"),
            visual_pages: &pages,
            items: &items,
        };
        let mut output = AnswerSourceRecognitionOutput {
            schema_version: 1,
            ingest_batch_id: 7,
            source_artifact_id: 9,
            source_artifact_sha256: hashing::sha256_hex(source),
            input_hash: request.input_hash().unwrap(),
            descriptor: AnswerSourceRecognizerDescriptor {
                provider: "fixture".into(),
                model_name: "fixture".into(),
                model_version: "v1".into(),
                config_version: "v1".into(),
                rule_version: "v1".into(),
            },
            state: AnswerSourceState::Ready,
            entries: vec![
                AnswerSourceEntry {
                    assessment_item_id: 11,
                    answer_json: serde_json::json!({"schema_version":1,"correct_labels":["A"]}),
                    source_anchor: serde_json::json!({"schema_version":1,"page":1}),
                    confidence: 0.99,
                },
                AnswerSourceEntry {
                    assessment_item_id: 12,
                    answer_json: serde_json::json!({"schema_version":1,"correct":true}),
                    source_anchor: serde_json::json!({"schema_version":1,"page":2}),
                    confidence: 0.99,
                },
            ],
            confidence: 0.99,
            issue_codes: vec![],
        };
        output.validate_against(&request).unwrap();
        output.entries[1].source_anchor["page"] = serde_json::json!(3);
        assert!(output.validate_against(&request).is_err());
    }

    #[test]
    fn office_input_hash_binds_extracted_text_and_extractor_version() {
        let items = items();
        let source = b"docx-fixture";
        let base = AnswerSourceRecognitionRequest {
            ingest_batch_id: 7,
            source_artifact_id: 9,
            source_artifact_sha256: &hashing::sha256_hex(source),
            source_format: "docx",
            mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            source_bytes: source,
            source_text: Some("第1段\t1.A"),
            text_extraction_version: Some("ooxml-docx-text-v1"),
            visualization_version: None,
            visual_pages: &[],
            items: &items,
        };
        let changed_text = AnswerSourceRecognitionRequest {
            source_text: Some("第1段\t1.B"),
            ..base
        };
        assert_ne!(
            base.input_hash().unwrap(),
            changed_text.input_hash().unwrap()
        );

        let changed_version = AnswerSourceRecognitionRequest {
            source_text: base.source_text,
            text_extraction_version: Some("ooxml-docx-text-v2"),
            ..base
        };
        assert_ne!(
            base.input_hash().unwrap(),
            changed_version.input_hash().unwrap()
        );
    }
}
