//! 固定格式默写的 provider-neutral 模板与 OCR 合同。
//!
//! 模板模型只定位题号/行栏；OCR 模型只读取学生字迹。OCR 请求故意不包含标准答案，
//! 避免模型根据答案反向“修正”学生原文。所有机器输出仍只是证据，不能确认分数或发布。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};

use crate::dictation::DictationRecognitionState;
use crate::ordinary_paper_recognition::NormalizedRect;

pub const DICTATION_TEMPLATE_SCHEMA_VERSION: i64 = 1;
pub const DICTATION_OCR_SCHEMA_VERSION: i64 = 1;
pub const DICTATION_READY_CONFIDENCE: f64 = 0.95;

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("默写{field}不能为空")))
    } else {
        Ok(())
    }
}

fn unit(value: f64, field: &str) -> CoreResult<()> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!("默写{field}必须位于 0~1")))
    }
}

fn sha256(value: &str, field: &str) -> CoreResult<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(format!("默写{field}必须是 SHA-256")));
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationQuestionType {
    FillBlank,
    ShortAnswer,
}

impl DictationQuestionType {
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "fill_blank" => Some(Self::FillBlank),
            "short_answer" => Some(Self::ShortAnswer),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictationItemSpec {
    pub assessment_item_id: i64,
    pub order_index: i64,
    pub question_type: DictationQuestionType,
}

impl DictationItemSpec {
    fn validate(&self) -> CoreResult<()> {
        if self.assessment_item_id <= 0 || self.order_index < 0 {
            return Err(CoreError::Invalid("默写题目 id 或顺序非法".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictationRecognizerDescriptor {
    pub provider: String,
    pub model_name: String,
    pub model_version: String,
    pub config_version: String,
    pub rule_version: String,
}

impl DictationRecognizerDescriptor {
    pub fn validate(&self) -> CoreResult<()> {
        for (value, field) in [
            (&self.provider, "provider"),
            (&self.model_name, "model_name"),
            (&self.model_version, "model_version"),
            (&self.config_version, "config_version"),
            (&self.rule_version, "rule_version"),
        ] {
            required(value, field)?;
        }
        Ok(())
    }
}

pub struct DictationTemplateRequest<'a> {
    pub assessment_version_id: i64,
    pub page_no: i64,
    pub blank_artifact_id: i64,
    pub blank_artifact_sha256: &'a str,
    pub mime_type: &'a str,
    pub image_bytes: &'a [u8],
    pub template_version: &'a str,
    pub items: &'a [DictationItemSpec],
}

impl DictationTemplateRequest<'_> {
    pub fn validate(&self) -> CoreResult<()> {
        if self.assessment_version_id <= 0 || self.page_no <= 0 || self.blank_artifact_id <= 0 {
            return Err(CoreError::Invalid("默写模板作用域非法".into()));
        }
        required(self.template_version, "模板版本")?;
        let expected = sha256(self.blank_artifact_sha256, "空白页 hash")?;
        if hashing::sha256_hex(self.image_bytes) != expected {
            return Err(CoreError::Invalid(
                "默写空白页与 artifact hash 不一致".into(),
            ));
        }
        if !matches!(
            self.mime_type.trim().to_ascii_lowercase().as_str(),
            "image/jpeg" | "image/png" | "image/webp"
        ) {
            return Err(CoreError::Invalid("默写模板只接受 JPEG/PNG/WebP".into()));
        }
        if self.items.is_empty() {
            return Err(CoreError::Invalid("默写模板必须带当前页题目".into()));
        }
        let mut ids = BTreeSet::new();
        let mut orders = BTreeSet::new();
        for item in self.items {
            item.validate()?;
            if !ids.insert(item.assessment_item_id) || !orders.insert(item.order_index) {
                return Err(CoreError::Invalid("默写题目 id/顺序不能重复".into()));
            }
        }
        Ok(())
    }

    pub fn input_hash(&self) -> CoreResult<String> {
        self.validate()?;
        let value = serde_json::json!({
            "schema_version": DICTATION_TEMPLATE_SCHEMA_VERSION,
            "assessment_version_id": self.assessment_version_id,
            "page_no": self.page_no,
            "blank_artifact_id": self.blank_artifact_id,
            "blank_artifact_sha256": self.blank_artifact_sha256.trim().to_ascii_lowercase(),
            "mime_type": self.mime_type.trim().to_ascii_lowercase(),
            "template_version": self.template_version.trim(),
            "items": self.items,
        });
        Ok(hashing::sha256_hex(&serde_json::to_vec(&value).map_err(
            |error| CoreError::Parse(format!("默写模板输入 hash 失败：{error}")),
        )?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationTemplateState {
    Ready,
    NeedsReview,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationRegionProposal {
    pub assessment_item_id: i64,
    pub region_index: i64,
    pub bbox: NormalizedRect,
    pub mapping_confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationTemplateOutput {
    pub schema_version: i64,
    pub assessment_version_id: i64,
    pub page_no: i64,
    pub blank_artifact_id: i64,
    pub blank_artifact_sha256: String,
    pub input_hash: String,
    pub template_version: String,
    pub descriptor: DictationRecognizerDescriptor,
    pub state: DictationTemplateState,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub regions: Vec<DictationRegionProposal>,
    pub confidence: f64,
    #[serde(default)]
    pub issue_codes: Vec<String>,
}

impl DictationTemplateOutput {
    pub fn validate_against(&self, request: &DictationTemplateRequest<'_>) -> CoreResult<()> {
        request.validate()?;
        self.descriptor.validate()?;
        if self.schema_version != DICTATION_TEMPLATE_SCHEMA_VERSION
            || self.assessment_version_id != request.assessment_version_id
            || self.page_no != request.page_no
            || self.blank_artifact_id != request.blank_artifact_id
            || self.blank_artifact_sha256
                != request.blank_artifact_sha256.trim().to_ascii_lowercase()
            || self.input_hash != request.input_hash()?
            || self.template_version.trim() != request.template_version.trim()
            || self.canvas_width == 0
            || self.canvas_height == 0
        {
            return Err(CoreError::Invalid("默写模板输出与当前输入不一致".into()));
        }
        unit(self.confidence, "模板总置信度")?;
        let expected = request
            .items
            .iter()
            .map(|item| item.assessment_item_id)
            .collect::<BTreeSet<_>>();
        let mut actual = BTreeSet::new();
        for region in &self.regions {
            if region.region_index < 0 || !actual.insert(region.assessment_item_id) {
                return Err(CoreError::Invalid("默写模板题区重复或顺序非法".into()));
            }
            region.bbox.validate("默写题区")?;
            unit(region.mapping_confidence, "题区置信度")?;
        }
        if actual != expected {
            return Err(CoreError::Invalid(
                "默写模板必须完整覆盖当前页全部题目".into(),
            ));
        }
        let ready = self.state == DictationTemplateState::Ready;
        if ready
            != (self.confidence >= DICTATION_READY_CONFIDENCE
                && self.issue_codes.is_empty()
                && self
                    .regions
                    .iter()
                    .all(|region| region.mapping_confidence >= DICTATION_READY_CONFIDENCE))
        {
            return Err(CoreError::Invalid("默写模板 ready 状态与证据不一致".into()));
        }
        Ok(())
    }

    pub fn to_json_against(&self, request: &DictationTemplateRequest<'_>) -> CoreResult<String> {
        self.validate_against(request)?;
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("默写模板输出序列化失败：{error}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationErrorCode {
    ProviderUnavailable,
    RateLimited,
    Timeout,
    InvalidOutput,
    TemplateMismatch,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictationFailure {
    pub schema_version: i64,
    pub code: DictationErrorCode,
    pub safe_message: String,
    pub retryable: bool,
}

impl DictationFailure {
    pub fn to_json(&self) -> CoreResult<String> {
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("默写失败信息序列化失败：{error}")))
    }
}

pub trait DictationTemplateRecognizer {
    fn descriptor(&self) -> DictationRecognizerDescriptor;
    fn recognize(
        &self,
        request: &DictationTemplateRequest<'_>,
    ) -> Result<DictationTemplateOutput, DictationFailure>;
}

pub struct DictationOcrRequest<'a> {
    pub answer_region_revision_id: i64,
    pub crop_artifact_id: i64,
    pub crop_artifact_sha256: &'a str,
    pub mime_type: &'a str,
    pub image_bytes: &'a [u8],
}

impl DictationOcrRequest<'_> {
    pub fn validate(&self) -> CoreResult<()> {
        if self.answer_region_revision_id <= 0 || self.crop_artifact_id <= 0 {
            return Err(CoreError::Invalid("默写 OCR 题区或 artifact 非法".into()));
        }
        let expected = sha256(self.crop_artifact_sha256, "裁剪 hash")?;
        if hashing::sha256_hex(self.image_bytes) != expected {
            return Err(CoreError::Invalid(
                "默写 OCR 裁剪与 artifact hash 不一致".into(),
            ));
        }
        if !matches!(
            self.mime_type.trim().to_ascii_lowercase().as_str(),
            "image/jpeg" | "image/png" | "image/webp"
        ) {
            return Err(CoreError::Invalid("默写 OCR 只接受图片裁剪".into()));
        }
        Ok(())
    }

    pub fn input_hash(&self) -> CoreResult<String> {
        self.validate()?;
        let value = serde_json::json!({
            "schema_version": DICTATION_OCR_SCHEMA_VERSION,
            "answer_region_revision_id": self.answer_region_revision_id,
            "crop_artifact_id": self.crop_artifact_id,
            "crop_artifact_sha256": self.crop_artifact_sha256.trim().to_ascii_lowercase(),
            "mime_type": self.mime_type.trim().to_ascii_lowercase(),
        });
        Ok(hashing::sha256_hex(&serde_json::to_vec(&value).map_err(
            |error| CoreError::Parse(format!("默写 OCR 输入 hash 失败：{error}")),
        )?))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationOcrOutput {
    pub schema_version: i64,
    pub answer_region_revision_id: i64,
    pub crop_artifact_id: i64,
    pub crop_artifact_sha256: String,
    pub input_hash: String,
    pub descriptor: DictationRecognizerDescriptor,
    pub state: DictationRecognitionState,
    pub raw_text: Option<String>,
    pub normalized_text: Option<String>,
    pub confidence: Option<f64>,
    #[serde(default)]
    pub issue_codes: Vec<String>,
}

impl DictationOcrOutput {
    pub fn validate_against(&self, request: &DictationOcrRequest<'_>) -> CoreResult<()> {
        request.validate()?;
        self.descriptor.validate()?;
        if self.schema_version != DICTATION_OCR_SCHEMA_VERSION
            || self.answer_region_revision_id != request.answer_region_revision_id
            || self.crop_artifact_id != request.crop_artifact_id
            || self.crop_artifact_sha256 != request.crop_artifact_sha256.trim().to_ascii_lowercase()
            || self.input_hash != request.input_hash()?
        {
            return Err(CoreError::Invalid("默写 OCR 输出与当前裁剪不一致".into()));
        }
        match self.state {
            DictationRecognitionState::Recognized => {
                required(self.raw_text.as_deref().unwrap_or(""), " OCR 原文")?;
                required(
                    self.normalized_text.as_deref().unwrap_or(""),
                    " OCR 规范文本",
                )?;
                unit(
                    self.confidence
                        .ok_or_else(|| CoreError::Invalid("已识别默写必须给出置信度".into()))?,
                    " OCR 置信度",
                )?;
            }
            _ => {
                if self.raw_text.is_some()
                    || self.normalized_text.is_some()
                    || self.confidence.is_some()
                {
                    return Err(CoreError::Invalid(
                        "非 recognized 默写不能伪造 OCR 文本或置信度".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn to_json_against(&self, request: &DictationOcrRequest<'_>) -> CoreResult<String> {
        self.validate_against(request)?;
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("默写 OCR 输出序列化失败：{error}")))
    }
}

pub trait DictationOcrRecognizer {
    fn descriptor(&self) -> DictationRecognizerDescriptor;
    fn recognize(
        &self,
        request: &DictationOcrRequest<'_>,
    ) -> Result<DictationOcrOutput, DictationFailure>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> DictationItemSpec {
        DictationItemSpec {
            assessment_item_id: 7,
            order_index: 0,
            question_type: DictationQuestionType::FillBlank,
        }
    }

    fn descriptor() -> DictationRecognizerDescriptor {
        DictationRecognizerDescriptor {
            provider: "fixture".into(),
            model_name: "fixture".into(),
            model_version: "v1".into(),
            config_version: "v1".into(),
            rule_version: "v1".into(),
        }
    }

    #[test]
    fn ready_template_requires_exact_item_coverage_and_high_confidence() {
        let bytes = b"blank";
        let hash = hashing::sha256_hex(bytes);
        let items = vec![item()];
        let request = DictationTemplateRequest {
            assessment_version_id: 2,
            page_no: 1,
            blank_artifact_id: 3,
            blank_artifact_sha256: &hash,
            mime_type: "image/jpeg",
            image_bytes: bytes,
            template_version: "fixed-v1",
            items: &items,
        };
        let output = DictationTemplateOutput {
            schema_version: 1,
            assessment_version_id: 2,
            page_no: 1,
            blank_artifact_id: 3,
            blank_artifact_sha256: hash.clone(),
            input_hash: request.input_hash().unwrap(),
            template_version: "fixed-v1".into(),
            descriptor: descriptor(),
            state: DictationTemplateState::Ready,
            canvas_width: 1200,
            canvas_height: 1600,
            regions: vec![DictationRegionProposal {
                assessment_item_id: 7,
                region_index: 0,
                bbox: NormalizedRect {
                    x: 0.1,
                    y: 0.2,
                    width: 0.8,
                    height: 0.1,
                },
                mapping_confidence: 0.99,
            }],
            confidence: 0.99,
            issue_codes: vec![],
        };
        output.validate_against(&request).unwrap();
    }

    #[test]
    fn ocr_contract_never_receives_or_invents_standard_answer() {
        let bytes = b"student-crop";
        let hash = hashing::sha256_hex(bytes);
        let request = DictationOcrRequest {
            answer_region_revision_id: 8,
            crop_artifact_id: 9,
            crop_artifact_sha256: &hash,
            mime_type: "image/jpeg",
            image_bytes: bytes,
        };
        let output = DictationOcrOutput {
            schema_version: 1,
            answer_region_revision_id: 8,
            crop_artifact_id: 9,
            crop_artifact_sha256: hash.clone(),
            input_hash: request.input_hash().unwrap(),
            descriptor: descriptor(),
            state: DictationRecognitionState::Recognized,
            raw_text: Some("一八四零年".into()),
            normalized_text: Some("一八四零年".into()),
            confidence: Some(0.97),
            issue_codes: vec![],
        };
        output.validate_against(&request).unwrap();
        assert_eq!(output.raw_text.as_deref(), Some("一八四零年"));
    }
}
