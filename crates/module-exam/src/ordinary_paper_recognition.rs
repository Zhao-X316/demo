//! 固定普通试卷整页分析的 provider-neutral 合同。
//!
//! provider 只输出页面质量、配准和题区候选；它不能确认学生身份、题区、分数、
//! 成绩发布或学习证据。真实落库仍必须经过 M2 既有 revision 与老师终审服务。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};

pub const ORDINARY_PAPER_SCHEMA_VERSION: i64 = 1;
pub const ORDINARY_PAPER_READY_CONFIDENCE: f64 = 0.95;

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("普通试卷 {field} 不能为空")))
    } else {
        Ok(())
    }
}

fn unit(value: f64, field: &str) -> CoreResult<()> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!("普通试卷 {field} 必须位于 0~1")))
    }
}

fn normalized_sha256(value: &str, field: &str) -> CoreResult<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(format!(
            "普通试卷 {field} 必须是 64 位十六进制 SHA-256"
        )));
    }
    Ok(normalized)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrdinaryPaperQuestionType {
    Single,
    Multiple,
    TrueFalse,
}

impl OrdinaryPaperQuestionType {
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "single" => Some(Self::Single),
            "multiple" => Some(Self::Multiple),
            "true_false" => Some(Self::TrueFalse),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdinaryPaperItemSpec {
    pub assessment_item_id: i64,
    pub order_index: i64,
    pub question_type: OrdinaryPaperQuestionType,
}

impl OrdinaryPaperItemSpec {
    fn validate(&self) -> CoreResult<()> {
        if self.assessment_item_id <= 0 || self.order_index < 0 {
            return Err(CoreError::Invalid(
                "普通试卷题目 id 必须为正数且顺序不能为负数".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl NormalizedRect {
    pub fn validate(&self, field: &str) -> CoreResult<()> {
        for (value, name) in [
            (self.x, "x"),
            (self.y, "y"),
            (self.width, "width"),
            (self.height, "height"),
        ] {
            if !value.is_finite() {
                return Err(CoreError::Invalid(format!(
                    "普通试卷 {field}.{name} 必须是有限数值"
                )));
            }
        }
        if self.x < 0.0
            || self.y < 0.0
            || self.width <= 0.0
            || self.height <= 0.0
            || self.x + self.width > 1.0
            || self.y + self.height > 1.0
        {
            return Err(CoreError::Invalid(format!(
                "普通试卷 {field} 必须位于 0~1 归一化页面内"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrdinaryPaperMarkCell {
    pub label: String,
    pub rect: NormalizedRect,
}

impl OrdinaryPaperMarkCell {
    fn validate(&self) -> CoreResult<()> {
        required(&self.label, "答题格标签")?;
        self.rect.validate("答题格")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdinaryPaperRecognizerDescriptor {
    pub provider: String,
    pub model_name: String,
    pub model_version: String,
    pub config_version: String,
    pub rule_version: String,
}

impl OrdinaryPaperRecognizerDescriptor {
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

pub struct OrdinaryPaperRecognitionRequest<'a> {
    pub page_id: i64,
    pub input_artifact_id: i64,
    pub input_artifact_sha256: &'a str,
    pub mime_type: &'a str,
    pub image_bytes: &'a [u8],
    pub expected_page_no: i64,
    pub template_version: &'a str,
    pub items: &'a [OrdinaryPaperItemSpec],
}

impl OrdinaryPaperRecognitionRequest<'_> {
    pub fn validate(&self) -> CoreResult<()> {
        if self.page_id <= 0 || self.input_artifact_id <= 0 || self.expected_page_no <= 0 {
            return Err(CoreError::Invalid(
                "普通试卷页面、artifact 和页码必须为正数".into(),
            ));
        }
        required(self.template_version, "模板版本")?;
        normalized_sha256(self.input_artifact_sha256, "artifact hash")?;
        if hashing::sha256_hex(self.image_bytes)
            != self.input_artifact_sha256.trim().to_ascii_lowercase()
        {
            return Err(CoreError::Invalid(
                "普通试卷输入字节与 artifact hash 不一致".into(),
            ));
        }
        if !matches!(
            self.mime_type.trim().to_ascii_lowercase().as_str(),
            "image/jpeg" | "image/png" | "image/webp"
        ) {
            return Err(CoreError::Invalid(
                "普通试卷整页分析只接受 JPEG/PNG/WebP 图片".into(),
            ));
        }
        if self.items.is_empty() {
            return Err(CoreError::Invalid(
                "普通试卷整页分析必须带当前页题目清单".into(),
            ));
        }
        let mut ids = BTreeSet::new();
        let mut orders = BTreeSet::new();
        for item in self.items {
            item.validate()?;
            if !ids.insert(item.assessment_item_id) || !orders.insert(item.order_index) {
                return Err(CoreError::Invalid(
                    "普通试卷当前页题目 id 和顺序不能重复".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn input_hash(&self) -> CoreResult<String> {
        self.validate()?;
        let canonical = serde_json::json!({
            "schema_version": ORDINARY_PAPER_SCHEMA_VERSION,
            "page_id": self.page_id,
            "input_artifact_id": self.input_artifact_id,
            "input_artifact_sha256": self.input_artifact_sha256.trim().to_ascii_lowercase(),
            "mime_type": self.mime_type.trim().to_ascii_lowercase(),
            "expected_page_no": self.expected_page_no,
            "template_version": self.template_version.trim(),
            "items": self.items,
        });
        Ok(hashing::sha256_hex(
            &serde_json::to_vec(&canonical)
                .map_err(|error| CoreError::Parse(format!("普通试卷输入 hash 失败：{error}")))?,
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrdinaryPaperQualityResult {
    Pass,
    NeedsReview,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrdinaryPaperQuality {
    pub blur_score: f64,
    pub glare_score: f64,
    pub brightness_score: f64,
    pub perspective_score: f64,
    pub rotation_degrees: f64,
    pub crop_complete: bool,
    pub result: OrdinaryPaperQualityResult,
    pub issue_codes: Vec<String>,
}

impl OrdinaryPaperQuality {
    fn validate(&self) -> CoreResult<()> {
        unit(self.blur_score, "模糊度")?;
        unit(self.glare_score, "反光度")?;
        unit(self.brightness_score, "亮度")?;
        unit(self.perspective_score, "透视完整度")?;
        if !self.rotation_degrees.is_finite() || !(-180.0..=180.0).contains(&self.rotation_degrees)
        {
            return Err(CoreError::Invalid(
                "普通试卷旋转角度必须位于 -180~180".into(),
            ));
        }
        if self.result != OrdinaryPaperQualityResult::Pass && self.issue_codes.is_empty() {
            return Err(CoreError::Invalid(
                "普通试卷非通过质量结论必须携带问题码".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrdinaryPaperAlignment {
    pub template_version: String,
    pub matrix: [f64; 9],
    pub confidence: f64,
}

impl OrdinaryPaperAlignment {
    fn validate(&self) -> CoreResult<()> {
        required(&self.template_version, "配准模板版本")?;
        if self.matrix.iter().any(|value| !value.is_finite()) {
            return Err(CoreError::Invalid(
                "普通试卷配准矩阵必须包含 9 个有限数值".into(),
            ));
        }
        unit(self.confidence, "配准置信度")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrdinaryPaperRegionProposal {
    pub assessment_item_id: i64,
    pub region_index: i64,
    pub bbox: NormalizedRect,
    pub mapping_confidence: f64,
    pub mark_cells: Vec<OrdinaryPaperMarkCell>,
}

impl OrdinaryPaperRegionProposal {
    fn validate(&self, question_type: OrdinaryPaperQuestionType) -> CoreResult<()> {
        if self.assessment_item_id <= 0 || self.region_index < 0 {
            return Err(CoreError::Invalid(
                "普通试卷题区 id 必须为正数且区域顺序不能为负数".into(),
            ));
        }
        self.bbox.validate("题区")?;
        unit(self.mapping_confidence, "题区映射置信度")?;
        let mut labels = BTreeSet::new();
        for cell in &self.mark_cells {
            cell.validate()?;
            if !labels.insert(cell.label.trim().to_ascii_uppercase()) {
                return Err(CoreError::Invalid(
                    "普通试卷同一题区的答题格标签不能重复".into(),
                ));
            }
        }
        if question_type == OrdinaryPaperQuestionType::TrueFalse
            && !labels.is_empty()
            && labels != BTreeSet::from(["FALSE".to_string(), "TRUE".to_string()])
        {
            return Err(CoreError::Invalid(
                "普通试卷判断题答题格必须且只能使用 TRUE/FALSE".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrdinaryPaperRecognitionState {
    Ready,
    NeedsReview,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrdinaryPaperRecognitionOutput {
    pub schema_version: i64,
    pub page_id: i64,
    pub input_artifact_id: i64,
    pub input_artifact_sha256: String,
    pub input_hash: String,
    pub expected_page_no: i64,
    pub descriptor: OrdinaryPaperRecognizerDescriptor,
    pub state: OrdinaryPaperRecognitionState,
    pub quality: OrdinaryPaperQuality,
    pub alignment: Option<OrdinaryPaperAlignment>,
    pub regions: Vec<OrdinaryPaperRegionProposal>,
    pub confidence: f64,
    pub issue_codes: Vec<String>,
}

impl OrdinaryPaperRecognitionOutput {
    pub fn validate_against(
        &self,
        request: &OrdinaryPaperRecognitionRequest<'_>,
    ) -> CoreResult<()> {
        request.validate()?;
        if self.schema_version != ORDINARY_PAPER_SCHEMA_VERSION {
            return Err(CoreError::Invalid(
                "普通试卷输出 schema_version 不受支持".into(),
            ));
        }
        if self.page_id != request.page_id
            || self.input_artifact_id != request.input_artifact_id
            || self.expected_page_no != request.expected_page_no
            || normalized_sha256(&self.input_artifact_sha256, "output artifact hash")?
                != request.input_artifact_sha256.trim().to_ascii_lowercase()
            || normalized_sha256(&self.input_hash, "output input hash")? != request.input_hash()?
        {
            return Err(CoreError::Invalid(
                "普通试卷输出与当前页面输入版本不一致".into(),
            ));
        }
        self.descriptor.validate()?;
        self.quality.validate()?;
        unit(self.confidence, "整页置信度")?;
        if let Some(alignment) = &self.alignment {
            alignment.validate()?;
            if alignment.template_version.trim() != request.template_version.trim() {
                return Err(CoreError::Invalid(
                    "普通试卷输出配准模板与请求版本不一致".into(),
                ));
            }
        }

        let item_types = request
            .items
            .iter()
            .map(|item| (item.assessment_item_id, item.question_type))
            .collect::<BTreeMap<_, _>>();
        let mut region_keys = BTreeSet::new();
        let mut covered_items = BTreeSet::new();
        for region in &self.regions {
            let question_type = item_types
                .get(&region.assessment_item_id)
                .ok_or_else(|| CoreError::Invalid("普通试卷输出包含当前作业页之外的题目".into()))?;
            if !region_keys.insert((region.assessment_item_id, region.region_index)) {
                return Err(CoreError::Invalid("普通试卷输出不能包含重复题区".into()));
            }
            covered_items.insert(region.assessment_item_id);
            region.validate(*question_type)?;
        }

        match self.state {
            OrdinaryPaperRecognitionState::Ready => {
                if self.quality.result != OrdinaryPaperQualityResult::Pass
                    || self.alignment.is_none()
                    || self.confidence < ORDINARY_PAPER_READY_CONFIDENCE
                    || !self.issue_codes.is_empty()
                    || covered_items != item_types.keys().copied().collect()
                    || self
                        .regions
                        .iter()
                        .any(|region| region.mapping_confidence < ORDINARY_PAPER_READY_CONFIDENCE)
                    || self.alignment.as_ref().is_some_and(|alignment| {
                        alignment.confidence < ORDINARY_PAPER_READY_CONFIDENCE
                    })
                {
                    return Err(CoreError::Invalid(
                        "普通试卷 ready 输出必须质量通过、配准/题区齐全且置信度不低于 0.95".into(),
                    ));
                }
            }
            OrdinaryPaperRecognitionState::NeedsReview | OrdinaryPaperRecognitionState::Blocked => {
                if self.issue_codes.is_empty() {
                    return Err(CoreError::Invalid(
                        "普通试卷非 ready 输出必须携带明确问题码".into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn to_json_against(
        &self,
        request: &OrdinaryPaperRecognitionRequest<'_>,
    ) -> CoreResult<String> {
        self.validate_against(request)?;
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("普通试卷输出序列化失败：{error}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrdinaryPaperRecognitionErrorCode {
    UnsupportedMedia,
    TemplateMismatch,
    ProviderUnavailable,
    Timeout,
    RateLimited,
    InvalidOutput,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdinaryPaperRecognitionFailure {
    pub schema_version: i64,
    pub code: OrdinaryPaperRecognitionErrorCode,
    pub safe_message: String,
    pub retryable: bool,
}

impl OrdinaryPaperRecognitionFailure {
    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != ORDINARY_PAPER_SCHEMA_VERSION {
            return Err(CoreError::Invalid(
                "普通试卷失败 schema_version 不受支持".into(),
            ));
        }
        required(&self.safe_message, "失败提示")?;
        let lower = self.safe_message.to_ascii_lowercase();
        if ["authorization", "bearer ", "api_key", "token=", "sk-"]
            .iter()
            .any(|needle| lower.contains(needle))
        {
            return Err(CoreError::Invalid("普通试卷失败信息疑似包含凭据".into()));
        }
        Ok(())
    }

    pub fn to_json(&self) -> CoreResult<String> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("普通试卷失败序列化失败：{error}")))
    }
}

pub trait OrdinaryPaperRecognizer: Send + Sync {
    fn descriptor(&self) -> OrdinaryPaperRecognizerDescriptor;

    fn recognize(
        &self,
        request: &OrdinaryPaperRecognitionRequest<'_>,
    ) -> Result<OrdinaryPaperRecognitionOutput, OrdinaryPaperRecognitionFailure>;
}

#[cfg(test)]
mod tests {
    use super::*;

    const BYTES: &[u8] = b"ordinary-paper-contract";

    fn items() -> Vec<OrdinaryPaperItemSpec> {
        vec![
            OrdinaryPaperItemSpec {
                assessment_item_id: 11,
                order_index: 0,
                question_type: OrdinaryPaperQuestionType::Single,
            },
            OrdinaryPaperItemSpec {
                assessment_item_id: 12,
                order_index: 1,
                question_type: OrdinaryPaperQuestionType::TrueFalse,
            },
        ]
    }

    fn build_request<'a>(
        items: &'a [OrdinaryPaperItemSpec],
    ) -> OrdinaryPaperRecognitionRequest<'a> {
        OrdinaryPaperRecognitionRequest {
            page_id: 7,
            input_artifact_id: 9,
            input_artifact_sha256: Box::leak(hashing::sha256_hex(BYTES).into_boxed_str()),
            mime_type: "image/jpeg",
            image_bytes: BYTES,
            expected_page_no: 1,
            template_version: "ordinary-template-v1",
            items,
        }
    }

    fn descriptor() -> OrdinaryPaperRecognizerDescriptor {
        OrdinaryPaperRecognizerDescriptor {
            provider: "fixture".into(),
            model_name: "fixture-vision".into(),
            model_version: "fixture-v1".into(),
            config_version: "ordinary-page-v1".into(),
            rule_version: "ordinary-json-v1".into(),
        }
    }

    fn rect(y: f64) -> NormalizedRect {
        NormalizedRect {
            x: 0.1,
            y,
            width: 0.8,
            height: 0.2,
        }
    }

    fn ready_output(
        request: &OrdinaryPaperRecognitionRequest<'_>,
    ) -> OrdinaryPaperRecognitionOutput {
        OrdinaryPaperRecognitionOutput {
            schema_version: ORDINARY_PAPER_SCHEMA_VERSION,
            page_id: request.page_id,
            input_artifact_id: request.input_artifact_id,
            input_artifact_sha256: request.input_artifact_sha256.into(),
            input_hash: request.input_hash().unwrap(),
            expected_page_no: request.expected_page_no,
            descriptor: descriptor(),
            state: OrdinaryPaperRecognitionState::Ready,
            quality: OrdinaryPaperQuality {
                blur_score: 0.03,
                glare_score: 0.02,
                brightness_score: 0.82,
                perspective_score: 0.99,
                rotation_degrees: 0.0,
                crop_complete: true,
                result: OrdinaryPaperQualityResult::Pass,
                issue_codes: vec![],
            },
            alignment: Some(OrdinaryPaperAlignment {
                template_version: request.template_version.into(),
                matrix: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                confidence: 0.99,
            }),
            regions: vec![
                OrdinaryPaperRegionProposal {
                    assessment_item_id: 11,
                    region_index: 0,
                    bbox: rect(0.1),
                    mapping_confidence: 0.99,
                    mark_cells: vec![],
                },
                OrdinaryPaperRegionProposal {
                    assessment_item_id: 12,
                    region_index: 0,
                    bbox: rect(0.5),
                    mapping_confidence: 0.98,
                    mark_cells: vec![
                        OrdinaryPaperMarkCell {
                            label: "TRUE".into(),
                            rect: NormalizedRect {
                                x: 0.1,
                                y: 0.1,
                                width: 0.2,
                                height: 0.3,
                            },
                        },
                        OrdinaryPaperMarkCell {
                            label: "FALSE".into(),
                            rect: NormalizedRect {
                                x: 0.6,
                                y: 0.1,
                                width: 0.2,
                                height: 0.3,
                            },
                        },
                    ],
                },
            ],
            confidence: 0.98,
            issue_codes: vec![],
        }
    }

    #[test]
    fn request_hash_is_stable_and_binds_item_order() {
        let items = items();
        let request = build_request(&items);
        assert_eq!(request.input_hash().unwrap(), request.input_hash().unwrap());

        let mut reversed = items.clone();
        reversed.reverse();
        assert_ne!(
            request.input_hash().unwrap(),
            build_request(&reversed).input_hash().unwrap()
        );
    }

    #[test]
    fn ready_output_requires_all_current_items_and_strict_confidence() {
        let items = items();
        let request = build_request(&items);
        let output = ready_output(&request);
        output.validate_against(&request).unwrap();

        let mut missing = output.clone();
        missing.regions.pop();
        assert!(missing.validate_against(&request).is_err());

        let mut low = output;
        low.regions[0].mapping_confidence = 0.94;
        assert!(low.validate_against(&request).is_err());
    }

    #[test]
    fn non_ready_output_must_explain_the_review_route() {
        let items = items();
        let request = build_request(&items);
        let mut output = ready_output(&request);
        output.state = OrdinaryPaperRecognitionState::NeedsReview;
        output.confidence = 0.72;
        output.issue_codes = vec!["PAGE_PERSPECTIVE_UNCERTAIN".into()];
        output.alignment.as_mut().unwrap().confidence = 0.72;
        output.validate_against(&request).unwrap();

        output.issue_codes.clear();
        assert!(output.validate_against(&request).is_err());
    }

    #[test]
    fn output_cannot_smuggle_other_assessment_items() {
        let items = items();
        let request = build_request(&items);
        let mut output = ready_output(&request);
        output.regions[0].assessment_item_id = 999;
        assert!(output.validate_against(&request).is_err());
    }

    #[test]
    fn failure_rejects_credential_like_messages() {
        let failure = OrdinaryPaperRecognitionFailure {
            schema_version: ORDINARY_PAPER_SCHEMA_VERSION,
            code: OrdinaryPaperRecognitionErrorCode::ProviderUnavailable,
            safe_message: "Bearer secret".into(),
            retryable: true,
        };
        assert!(failure.to_json().is_err());
    }
}
