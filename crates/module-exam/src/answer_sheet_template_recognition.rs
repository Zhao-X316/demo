//! 固定答题卡首张空白模板的 provider-neutral 识别合同。
//!
//! 视觉模型只能从老师选择的空白答题卡中提出定位方式、题号和涂点格位候选；
//! 候选必须完整覆盖当前页全部客观题，并且只有老师一次确认后才会成为 active 模板。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};

use crate::answer_sheet_recognition::{
    AnswerSheetAlignmentMode, AnswerSheetAnchor, AnswerSheetItemTemplate,
    AnswerSheetSubjectiveKind, AnswerSheetSubjectiveRegionTemplate, AnswerSheetTemplateDefinition,
    LocalOmrPolicy, ANSWER_SHEET_TEMPLATE_SCHEMA_VERSION,
};
use crate::objective_recognition::ObjectiveQuestionType;

pub const ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION: i64 = 1;
pub const ANSWER_SHEET_TEMPLATE_READY_CONFIDENCE: f64 = 0.95;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerSheetTemplateQuestionType {
    Single,
    Multiple,
    TrueFalse,
    FillBlank,
    ShortAnswer,
}

impl AnswerSheetTemplateQuestionType {
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

    fn objective(self) -> Option<ObjectiveQuestionType> {
        match self {
            Self::Single => Some(ObjectiveQuestionType::Single),
            Self::Multiple => Some(ObjectiveQuestionType::Multiple),
            Self::TrueFalse => Some(ObjectiveQuestionType::TrueFalse),
            Self::FillBlank | Self::ShortAnswer => None,
        }
    }

    fn subjective(self) -> Option<AnswerSheetSubjectiveKind> {
        match self {
            Self::FillBlank => Some(AnswerSheetSubjectiveKind::FillBlank),
            Self::ShortAnswer => Some(AnswerSheetSubjectiveKind::ShortAnswer),
            Self::Single | Self::Multiple | Self::TrueFalse => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerSheetTemplateItemSpec {
    pub assessment_item_id: i64,
    pub order_index: i64,
    pub question_type: AnswerSheetTemplateQuestionType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerSheetTemplateRecognizerDescriptor {
    pub provider: String,
    pub model_name: String,
    pub model_version: String,
    pub config_version: String,
    pub rule_version: String,
}

impl AnswerSheetTemplateRecognizerDescriptor {
    pub fn validate(&self) -> CoreResult<()> {
        for (value, field) in [
            (&self.provider, "provider"),
            (&self.model_name, "model_name"),
            (&self.model_version, "model_version"),
            (&self.config_version, "config_version"),
            (&self.rule_version, "rule_version"),
        ] {
            if value.trim().is_empty() {
                return Err(CoreError::Invalid(format!(
                    "答题卡模板识别 {field} 不能为空"
                )));
            }
        }
        Ok(())
    }
}

pub struct AnswerSheetTemplateRecognitionRequest<'a> {
    pub assessment_version_id: i64,
    pub page_no: i64,
    pub template_version: &'a str,
    pub blank_artifact_id: i64,
    pub blank_artifact_sha256: &'a str,
    pub mime_type: &'a str,
    pub image_bytes: &'a [u8],
    pub items: &'a [AnswerSheetTemplateItemSpec],
}

impl AnswerSheetTemplateRecognitionRequest<'_> {
    pub fn validate(&self) -> CoreResult<()> {
        if self.assessment_version_id <= 0 || self.page_no <= 0 || self.blank_artifact_id <= 0 {
            return Err(CoreError::Invalid(
                "答题卡模板识别的作业、页码和空白图 artifact 必须为正数".into(),
            ));
        }
        if self.template_version.trim().is_empty() || self.image_bytes.is_empty() {
            return Err(CoreError::Invalid("答题卡模板版本和空白图不能为空".into()));
        }
        if !matches!(
            self.mime_type.trim().to_ascii_lowercase().as_str(),
            "image/jpeg" | "image/png" | "image/webp"
        ) {
            return Err(CoreError::Invalid(
                "答题卡空白模板只接受 JPEG、PNG 或 WebP 图片".into(),
            ));
        }
        let sha256 = self.blank_artifact_sha256.trim().to_ascii_lowercase();
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(CoreError::Invalid(
                "答题卡空白图 hash 必须是 64 位 SHA-256".into(),
            ));
        }
        if hashing::sha256_hex(self.image_bytes) != sha256 {
            return Err(CoreError::Invalid(
                "答题卡空白图字节与登记 artifact hash 不一致".into(),
            ));
        }
        if self.items.is_empty() {
            return Err(CoreError::Invalid(
                "答题卡模板识别必须携带当前页题目清单".into(),
            ));
        }
        let mut ids = BTreeSet::new();
        for item in self.items {
            if item.assessment_item_id <= 0
                || item.order_index < 0
                || !ids.insert(item.assessment_item_id)
            {
                return Err(CoreError::Invalid(
                    "答题卡当前页题目 id 必须有效且不能重复".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn input_hash(&self) -> CoreResult<String> {
        self.validate()?;
        let canonical = serde_json::json!({
            "schema_version": ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION,
            "assessment_version_id": self.assessment_version_id,
            "page_no": self.page_no,
            "template_version": self.template_version.trim(),
            "blank_artifact_id": self.blank_artifact_id,
            "blank_artifact_sha256": self.blank_artifact_sha256.trim().to_ascii_lowercase(),
            "mime_type": self.mime_type.trim().to_ascii_lowercase(),
            "items": self.items,
        });
        Ok(hashing::sha256_hex(
            &serde_json::to_vec(&canonical)
                .map_err(|error| CoreError::Parse(format!("答题卡模板输入 hash 失败：{error}")))?,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerSheetTemplateRecognitionState {
    Ready,
    NeedsReview,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerSheetTemplateRecognitionOutput {
    pub schema_version: i64,
    pub assessment_version_id: i64,
    pub page_no: i64,
    pub template_version: String,
    pub blank_artifact_id: i64,
    pub blank_artifact_sha256: String,
    pub input_hash: String,
    pub descriptor: AnswerSheetTemplateRecognizerDescriptor,
    pub state: AnswerSheetTemplateRecognitionState,
    pub canvas_width: u32,
    pub canvas_height: u32,
    #[serde(default)]
    pub alignment_mode: AnswerSheetAlignmentMode,
    pub anchors: Vec<AnswerSheetAnchor>,
    pub items: Vec<AnswerSheetItemTemplate>,
    #[serde(default)]
    pub subjective_regions: Vec<AnswerSheetSubjectiveRegionTemplate>,
    pub policy: LocalOmrPolicy,
    pub confidence: f64,
    #[serde(default)]
    pub issue_codes: Vec<String>,
}

impl AnswerSheetTemplateRecognitionOutput {
    pub fn validate_against(
        &self,
        request: &AnswerSheetTemplateRecognitionRequest<'_>,
    ) -> CoreResult<()> {
        request.validate()?;
        if self.schema_version != ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION
            || self.assessment_version_id != request.assessment_version_id
            || self.page_no != request.page_no
            || self.template_version.trim() != request.template_version.trim()
            || self.blank_artifact_id != request.blank_artifact_id
            || !self
                .blank_artifact_sha256
                .trim()
                .eq_ignore_ascii_case(request.blank_artifact_sha256.trim())
            || self.input_hash.trim().to_ascii_lowercase() != request.input_hash()?
        {
            return Err(CoreError::Invalid(
                "答题卡模板输出与当前空白图、作业或页码不一致".into(),
            ));
        }
        self.descriptor.validate()?;
        let blank = image::load_from_memory(request.image_bytes)
            .map_err(|_| CoreError::Invalid("答题卡空白图无法解码".into()))?;
        if self.canvas_width != blank.width() || self.canvas_height != blank.height() {
            return Err(CoreError::Invalid(
                "答题卡模板画布尺寸必须与空白原图完全一致".into(),
            ));
        }
        if !self.confidence.is_finite() || !(0.0..=1.0).contains(&self.confidence) {
            return Err(CoreError::Invalid("答题卡模板置信度必须位于 0~1".into()));
        }

        let expected_objective = request
            .items
            .iter()
            .filter_map(|item| {
                item.question_type
                    .objective()
                    .map(|question_type| (item.assessment_item_id, question_type))
            })
            .collect::<BTreeMap<_, _>>();
        let expected_subjective = request
            .items
            .iter()
            .filter_map(|item| {
                item.question_type
                    .subjective()
                    .map(|question_type| (item.assessment_item_id, question_type))
            })
            .collect::<BTreeMap<_, _>>();
        let actual_objective = self
            .items
            .iter()
            .map(|item| (item.assessment_item_id, item.question_type))
            .collect::<BTreeMap<_, _>>();
        let actual_subjective = self
            .subjective_regions
            .iter()
            .map(|region| (region.assessment_item_id, region.question_type))
            .collect::<BTreeMap<_, _>>();
        if actual_objective
            .keys()
            .any(|id| !expected_objective.contains_key(id))
            || actual_subjective
                .keys()
                .any(|id| !expected_subjective.contains_key(id))
        {
            return Err(CoreError::Invalid(
                "答题卡模板输出包含当前作业页之外的题目".into(),
            ));
        }

        let definition = self.definition();
        match self.state {
            AnswerSheetTemplateRecognitionState::Ready => {
                definition.validate()?;
                if actual_objective != expected_objective
                    || actual_subjective != expected_subjective
                    || self.confidence < ANSWER_SHEET_TEMPLATE_READY_CONFIDENCE
                    || !self.issue_codes.is_empty()
                {
                    return Err(CoreError::Invalid(
                        "答题卡 ready 模板必须完整覆盖当前页题目、无问题且置信度不低于 0.95".into(),
                    ));
                }
            }
            AnswerSheetTemplateRecognitionState::NeedsReview
            | AnswerSheetTemplateRecognitionState::Blocked => {
                if self.issue_codes.is_empty() {
                    return Err(CoreError::Invalid(
                        "答题卡非 ready 模板必须给出明确问题码".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn definition(&self) -> AnswerSheetTemplateDefinition {
        AnswerSheetTemplateDefinition {
            schema_version: ANSWER_SHEET_TEMPLATE_SCHEMA_VERSION,
            assessment_version_id: self.assessment_version_id,
            template_version: self.template_version.clone(),
            page_no: self.page_no,
            canvas_width: self.canvas_width,
            canvas_height: self.canvas_height,
            blank_artifact_id: self.blank_artifact_id,
            blank_artifact_sha256: self.blank_artifact_sha256.clone(),
            alignment_mode: self.alignment_mode,
            anchors: self.anchors.clone(),
            items: self.items.clone(),
            subjective_regions: self.subjective_regions.clone(),
            policy: self.policy.clone(),
        }
    }

    pub fn to_json_against(
        &self,
        request: &AnswerSheetTemplateRecognitionRequest<'_>,
    ) -> CoreResult<String> {
        self.validate_against(request)?;
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("答题卡模板输出序列化失败：{error}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerSheetTemplateRecognitionErrorCode {
    DecodeFailed,
    ProviderUnavailable,
    RateLimited,
    Timeout,
    InvalidOutput,
    TemplateMismatch,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerSheetTemplateRecognitionFailure {
    pub schema_version: i64,
    pub code: AnswerSheetTemplateRecognitionErrorCode,
    pub safe_message: String,
    pub retryable: bool,
}

impl AnswerSheetTemplateRecognitionFailure {
    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION
            || self.safe_message.trim().is_empty()
        {
            return Err(CoreError::Invalid("答题卡模板识别失败信息不完整".into()));
        }
        Ok(())
    }

    pub fn to_json(&self) -> CoreResult<String> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("答题卡模板错误序列化失败：{error}")))
    }
}

pub trait AnswerSheetTemplateRecognizer: Send + Sync {
    fn descriptor(&self) -> AnswerSheetTemplateRecognizerDescriptor;
    fn recognize(
        &self,
        request: &AnswerSheetTemplateRecognitionRequest<'_>,
    ) -> Result<AnswerSheetTemplateRecognitionOutput, AnswerSheetTemplateRecognitionFailure>;
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageOutputFormat};

    use super::*;
    use crate::answer_sheet_recognition::{AnswerSheetAnchor, SheetRect};
    use crate::objective_recognition::ObjectiveMarkCell;

    fn request<'a>(bytes: &'a [u8], hash: &'a str) -> AnswerSheetTemplateRecognitionRequest<'a> {
        let items = Box::leak(Box::new([AnswerSheetTemplateItemSpec {
            assessment_item_id: 11,
            order_index: 0,
            question_type: AnswerSheetTemplateQuestionType::Single,
        }]));
        AnswerSheetTemplateRecognitionRequest {
            assessment_version_id: 7,
            page_no: 1,
            template_version: "sheet-v1",
            blank_artifact_id: 9,
            blank_artifact_sha256: hash,
            mime_type: "image/jpeg",
            image_bytes: bytes,
            items,
        }
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> SheetRect {
        SheetRect {
            x,
            y,
            width,
            height,
        }
    }

    fn blank_image() -> Vec<u8> {
        let mut bytes = Vec::new();
        DynamicImage::new_rgb8(1200, 1800)
            .write_to(&mut Cursor::new(&mut bytes), ImageOutputFormat::Png)
            .unwrap();
        bytes
    }

    fn output(
        request: &AnswerSheetTemplateRecognitionRequest<'_>,
    ) -> AnswerSheetTemplateRecognitionOutput {
        let anchor = |key: &str, x: f64, y: f64| AnswerSheetAnchor {
            key: key.into(),
            expected: rect(x, y, 0.03, 0.03),
            search: rect(
                (x - 0.02).clamp(0.0, 0.92),
                (y - 0.02).clamp(0.0, 0.92),
                0.08,
                0.08,
            ),
        };
        AnswerSheetTemplateRecognitionOutput {
            schema_version: ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION,
            assessment_version_id: 7,
            page_no: 1,
            template_version: "sheet-v1".into(),
            blank_artifact_id: 9,
            blank_artifact_sha256: request.blank_artifact_sha256.into(),
            input_hash: request.input_hash().unwrap(),
            descriptor: AnswerSheetTemplateRecognizerDescriptor {
                provider: "fixture".into(),
                model_name: "fixture-vision".into(),
                model_version: "v1".into(),
                config_version: "v1".into(),
                rule_version: "v1".into(),
            },
            state: AnswerSheetTemplateRecognitionState::Ready,
            canvas_width: 1200,
            canvas_height: 1800,
            alignment_mode: AnswerSheetAlignmentMode::PrintedAnchors,
            anchors: vec![
                anchor("top_left", 0.02, 0.02),
                anchor("top_right", 0.95, 0.02),
                anchor("bottom_left", 0.02, 0.95),
                anchor("bottom_right", 0.95, 0.95),
            ],
            items: vec![AnswerSheetItemTemplate {
                assessment_item_id: 11,
                region_index: 0,
                question_type: ObjectiveQuestionType::Single,
                region: rect(0.1, 0.2, 0.8, 0.08),
                cells: vec![
                    ObjectiveMarkCell {
                        label: "A".into(),
                        x: 0.1,
                        y: 0.1,
                        width: 0.1,
                        height: 0.8,
                    },
                    ObjectiveMarkCell {
                        label: "B".into(),
                        x: 0.3,
                        y: 0.1,
                        width: 0.1,
                        height: 0.8,
                    },
                ],
            }],
            subjective_regions: Vec::new(),
            policy: LocalOmrPolicy {
                blank_max_ratio: 0.04,
                marked_min_ratio: 0.14,
                pixel_delta_threshold: 24,
                cell_inset_ratio: 0.18,
            },
            confidence: 0.99,
            issue_codes: vec![],
        }
    }

    #[test]
    fn ready_candidate_must_cover_exact_current_page_items() {
        let bytes = blank_image();
        let hash = hashing::sha256_hex(&bytes);
        let request = request(&bytes, &hash);
        let candidate = output(&request);
        candidate.validate_against(&request).unwrap();
    }

    #[test]
    fn ready_candidate_cannot_silently_omit_an_item() {
        let bytes = blank_image();
        let hash = hashing::sha256_hex(&bytes);
        let request = request(&bytes, &hash);
        let mut candidate = output(&request);
        candidate.items.clear();
        assert!(candidate.validate_against(&request).is_err());
    }

    #[test]
    fn ready_candidate_can_use_page_contour_without_printed_anchors() {
        let bytes = blank_image();
        let hash = hashing::sha256_hex(&bytes);
        let request = request(&bytes, &hash);
        let mut candidate = output(&request);
        candidate.alignment_mode = AnswerSheetAlignmentMode::PageContour;
        candidate.anchors.clear();
        candidate.validate_against(&request).unwrap();
        assert_eq!(
            candidate.definition().alignment_mode,
            AnswerSheetAlignmentMode::PageContour
        );
    }

    #[test]
    fn ready_candidate_separates_objective_cells_from_subjective_regions() {
        let bytes = blank_image();
        let hash = hashing::sha256_hex(&bytes);
        let items = Box::leak(Box::new([
            AnswerSheetTemplateItemSpec {
                assessment_item_id: 11,
                order_index: 0,
                question_type: AnswerSheetTemplateQuestionType::Single,
            },
            AnswerSheetTemplateItemSpec {
                assessment_item_id: 12,
                order_index: 1,
                question_type: AnswerSheetTemplateQuestionType::ShortAnswer,
            },
        ]));
        let request = AnswerSheetTemplateRecognitionRequest {
            assessment_version_id: 7,
            page_no: 1,
            template_version: "sheet-v1",
            blank_artifact_id: 9,
            blank_artifact_sha256: &hash,
            mime_type: "image/jpeg",
            image_bytes: &bytes,
            items,
        };
        let mut candidate = output(&request);
        candidate.subjective_regions = vec![AnswerSheetSubjectiveRegionTemplate {
            assessment_item_id: 12,
            region_index: 0,
            question_type: AnswerSheetSubjectiveKind::ShortAnswer,
            region: rect(0.1, 0.35, 0.8, 0.45),
        }];
        candidate.validate_against(&request).unwrap();

        candidate.subjective_regions[0].question_type = AnswerSheetSubjectiveKind::FillBlank;
        assert!(candidate.validate_against(&request).is_err());
    }
}
