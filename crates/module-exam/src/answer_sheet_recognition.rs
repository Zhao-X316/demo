//! 固定答题卡模板与本地可复现 OMR。
//!
//! 答题卡不走普通卷整页 VLM：模板固定空白卡、锚点、题号和格位；页面配准完成后，
//! 本识别器对空白模板裁剪和学生裁剪做像素差分。清晰结果只形成 objective
//! observation；双涂、浅涂、擦除或模板不一致必须交老师复核。

use std::collections::BTreeSet;

use image::DynamicImage;
use serde::{Deserialize, Serialize};
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};

use crate::objective_recognition::{
    ObjectiveMarkCell, ObjectiveMarkMeasurement, ObjectiveQuestionType,
    ObjectiveRecognitionErrorCode, ObjectiveRecognitionFailure, ObjectiveRecognitionOutput,
    ObjectiveRecognitionRequest, ObjectiveRecognitionState, ObjectiveRecognizedAnswer,
    ObjectiveRecognizer, ObjectiveRecognizerDescriptor, OBJECTIVE_RECOGNITION_SCHEMA_VERSION,
};

pub const ANSWER_SHEET_TEMPLATE_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl SheetRect {
    pub fn validate(&self, field: &str) -> CoreResult<()> {
        for value in [self.x, self.y, self.width, self.height] {
            if !value.is_finite() {
                return Err(CoreError::Invalid(format!(
                    "答题卡 {field} 坐标必须是有限数值"
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
                "答题卡 {field} 必须位于 0~1 页面坐标内"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerSheetAnchor {
    pub key: String,
    pub expected: SheetRect,
    pub search: SheetRect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerSheetItemTemplate {
    pub assessment_item_id: i64,
    pub region_index: i64,
    pub question_type: ObjectiveQuestionType,
    pub region: SheetRect,
    /// 相对于 `region` 裁剪的 0~1 格位坐标。
    pub cells: Vec<ObjectiveMarkCell>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalOmrPolicy {
    /// 低于该像素差分比例视为空白。
    pub blank_max_ratio: f64,
    /// 高于该像素差分比例视为清晰填涂。
    pub marked_min_ratio: f64,
    /// 学生像素比空白模板至少暗多少才计为新增墨迹。
    pub pixel_delta_threshold: u8,
    /// 忽略每个格位的印刷边框比例。
    pub cell_inset_ratio: f64,
}

impl LocalOmrPolicy {
    pub fn validate(&self) -> CoreResult<()> {
        if !self.blank_max_ratio.is_finite()
            || !self.marked_min_ratio.is_finite()
            || !(0.0..1.0).contains(&self.blank_max_ratio)
            || !(0.0..1.0).contains(&self.marked_min_ratio)
            || self.blank_max_ratio >= self.marked_min_ratio
            || self.pixel_delta_threshold == 0
            || !(0.0..0.45).contains(&self.cell_inset_ratio)
        {
            return Err(CoreError::Invalid("答题卡本地 OMR 阈值非法".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerSheetTemplateDefinition {
    pub schema_version: i64,
    pub assessment_version_id: i64,
    pub template_version: String,
    pub page_no: i64,
    pub canvas_width: u32,
    pub canvas_height: u32,
    pub blank_artifact_id: i64,
    pub blank_artifact_sha256: String,
    pub anchors: Vec<AnswerSheetAnchor>,
    pub items: Vec<AnswerSheetItemTemplate>,
    pub policy: LocalOmrPolicy,
}

impl AnswerSheetTemplateDefinition {
    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != ANSWER_SHEET_TEMPLATE_SCHEMA_VERSION
            || self.assessment_version_id <= 0
            || self.page_no <= 0
            || self.blank_artifact_id <= 0
            || self.template_version.trim().is_empty()
            || self.canvas_width < 100
            || self.canvas_height < 100
            || self.items.is_empty()
        {
            return Err(CoreError::Invalid("答题卡模板头信息不完整".into()));
        }
        validate_sha256(&self.blank_artifact_sha256)?;
        self.policy.validate()?;

        let mut anchor_keys = BTreeSet::new();
        for anchor in &self.anchors {
            let key = anchor.key.trim().to_ascii_lowercase();
            if !anchor_keys.insert(key) {
                return Err(CoreError::Invalid("答题卡锚点为空或重复".into()));
            }
            anchor.expected.validate("锚点标准区域")?;
            anchor.search.validate("锚点搜索区域")?;
        }
        if anchor_keys
            != BTreeSet::from([
                "bottom_left".to_string(),
                "bottom_right".to_string(),
                "top_left".to_string(),
                "top_right".to_string(),
            ])
        {
            return Err(CoreError::Invalid(
                "固定答题卡必须且只能包含四角锚点".into(),
            ));
        }

        let mut item_keys = BTreeSet::new();
        for item in &self.items {
            if item.assessment_item_id <= 0
                || item.region_index < 0
                || !item_keys.insert((item.assessment_item_id, item.region_index))
                || item.cells.len() < 2
            {
                return Err(CoreError::Invalid("答题卡题号/题区映射非法或重复".into()));
            }
            item.region.validate("答案题区")?;
            let mut labels = BTreeSet::new();
            for cell in &item.cells {
                cell.validate()?;
                if !labels.insert(cell.label.trim().to_ascii_uppercase()) {
                    return Err(CoreError::Invalid("答题卡格位标签重复".into()));
                }
            }
            if item.question_type == ObjectiveQuestionType::TrueFalse
                && labels != BTreeSet::from(["FALSE".to_string(), "TRUE".to_string()])
            {
                return Err(CoreError::Invalid(
                    "答题卡判断题必须且只能映射 TRUE/FALSE".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn template_hash(&self) -> CoreResult<String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|error| CoreError::Parse(format!("答题卡模板序列化失败：{error}")))?;
        Ok(hashing::sha256_hex(&bytes))
    }

    pub fn to_json(&self) -> CoreResult<String> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("答题卡模板序列化失败：{error}")))
    }
}

/// 在页面配准和题区裁剪完成后，对单题裁剪做本地模板差分。
pub struct LocalAnswerSheetOmr {
    blank_crop_bytes: Vec<u8>,
    policy: LocalOmrPolicy,
}

impl LocalAnswerSheetOmr {
    pub fn new(blank_crop_bytes: Vec<u8>, policy: LocalOmrPolicy) -> CoreResult<Self> {
        if blank_crop_bytes.is_empty() {
            return Err(CoreError::Invalid("答题卡空白模板裁剪不能为空".into()));
        }
        policy.validate()?;
        decode_gray(&blank_crop_bytes).map_err(|failure| {
            CoreError::Invalid(format!("答题卡空白模板不可解码：{}", failure.safe_message))
        })?;
        Ok(Self {
            blank_crop_bytes,
            policy,
        })
    }
}

impl ObjectiveRecognizer for LocalAnswerSheetOmr {
    fn descriptor(&self) -> ObjectiveRecognizerDescriptor {
        ObjectiveRecognizerDescriptor {
            provider: "local".into(),
            model_name: "answer-sheet-pixel-diff-omr".into(),
            model_version: "1".into(),
            config_version: format!(
                "blank-{:.4}-marked-{:.4}-delta-{}-inset-{:.3}",
                self.policy.blank_max_ratio,
                self.policy.marked_min_ratio,
                self.policy.pixel_delta_threshold,
                self.policy.cell_inset_ratio
            ),
            rule_version: "answer-sheet-local-omr-v1".into(),
        }
    }

    fn recognize(
        &self,
        request: &ObjectiveRecognitionRequest<'_>,
    ) -> Result<ObjectiveRecognitionOutput, ObjectiveRecognitionFailure> {
        request.validate().map_err(invalid_request)?;
        let blank = decode_gray(&self.blank_crop_bytes)?;
        let student = decode_gray(request.image_bytes)?;
        if blank.dimensions() != student.dimensions() {
            return Err(failure(
                ObjectiveRecognitionErrorCode::TemplateMismatch,
                "学生答题区与空白模板尺寸不一致",
                false,
            ));
        }
        let mut measurements = Vec::with_capacity(request.cells.len());
        let mut marked = Vec::new();
        let mut uncertain = Vec::new();
        for cell in request.cells {
            let ratio = measure_added_ink(&blank, &student, cell, &self.policy)?;
            let normalized = cell.label.trim().to_ascii_uppercase();
            let clear =
                ratio <= self.policy.blank_max_ratio || ratio >= self.policy.marked_min_ratio;
            measurements.push(ObjectiveMarkMeasurement {
                label: normalized.clone(),
                ink_ratio: ratio,
                confidence: if clear { 0.99 } else { 0.60 },
            });
            if ratio >= self.policy.marked_min_ratio {
                marked.push(normalized);
            } else if ratio > self.policy.blank_max_ratio {
                uncertain.push(normalized);
            }
        }

        let (result_state, answer, confidence, issue_codes) =
            classify(request.question_type, &marked, &uncertain, &measurements);
        let output = ObjectiveRecognitionOutput {
            schema_version: OBJECTIVE_RECOGNITION_SCHEMA_VERSION,
            answer_region_revision_id: request.answer_region_revision_id,
            input_artifact_id: request.input_artifact_id,
            input_artifact_sha256: request.input_artifact_sha256.trim().to_ascii_lowercase(),
            input_hash: request.input_hash().map_err(invalid_request)?,
            question_type: request.question_type,
            template_version: request.template_version.trim().to_string(),
            descriptor: self.descriptor(),
            result_state,
            answer,
            confidence,
            issue_codes,
            measurements,
        };
        output.validate().map_err(invalid_output)?;
        Ok(output)
    }
}

fn classify(
    question_type: ObjectiveQuestionType,
    marked: &[String],
    uncertain: &[String],
    measurements: &[ObjectiveMarkMeasurement],
) -> (
    ObjectiveRecognitionState,
    Option<ObjectiveRecognizedAnswer>,
    Option<f64>,
    Vec<String>,
) {
    if !uncertain.is_empty() {
        let best = measurements
            .iter()
            .max_by(|left, right| left.ink_ratio.total_cmp(&right.ink_ratio))
            .map(|measurement| measurement.label.clone())
            .unwrap_or_else(|| "UNKNOWN".into());
        let answer = answer_for(question_type, &[best], false);
        return (
            ObjectiveRecognitionState::LowConfidence,
            answer,
            Some(0.70),
            vec!["UNCERTAIN_MARK".into()],
        );
    }
    if marked.is_empty() {
        return (
            ObjectiveRecognitionState::Blank,
            None,
            Some(0.99),
            Vec::new(),
        );
    }
    let multiple_is_conflict = matches!(
        question_type,
        ObjectiveQuestionType::Single | ObjectiveQuestionType::TrueFalse
    ) && marked.len() > 1;
    if multiple_is_conflict {
        return (
            ObjectiveRecognitionState::Altered,
            answer_for(question_type, marked, true),
            Some(0.80),
            vec!["MULTIPLE_MARKS".into()],
        );
    }
    (
        ObjectiveRecognitionState::Recognized,
        answer_for(question_type, marked, false),
        Some(0.99),
        Vec::new(),
    )
}

fn answer_for(
    question_type: ObjectiveQuestionType,
    labels: &[String],
    ambiguous: bool,
) -> Option<ObjectiveRecognizedAnswer> {
    match question_type {
        ObjectiveQuestionType::Single | ObjectiveQuestionType::Multiple => {
            Some(ObjectiveRecognizedAnswer::SelectedLabels {
                selected_labels: labels.to_vec(),
            })
        }
        ObjectiveQuestionType::TrueFalse if ambiguous => {
            Some(ObjectiveRecognizedAnswer::AmbiguousTrueFalse {
                selected_values: vec![false, true],
            })
        }
        ObjectiveQuestionType::TrueFalse => {
            labels
                .first()
                .map(|label| ObjectiveRecognizedAnswer::TrueFalse {
                    selected: label == "TRUE",
                })
        }
    }
}

fn measure_added_ink(
    blank: &image::GrayImage,
    student: &image::GrayImage,
    cell: &ObjectiveMarkCell,
    policy: &LocalOmrPolicy,
) -> Result<f64, ObjectiveRecognitionFailure> {
    let width = blank.width();
    let height = blank.height();
    let mut x0 = (cell.x * width as f64).floor() as u32;
    let mut y0 = (cell.y * height as f64).floor() as u32;
    let mut x1 = ((cell.x + cell.width) * width as f64).ceil() as u32;
    let mut y1 = ((cell.y + cell.height) * height as f64).ceil() as u32;
    x1 = x1.min(width);
    y1 = y1.min(height);
    let inset_x = ((x1.saturating_sub(x0)) as f64 * policy.cell_inset_ratio).round() as u32;
    let inset_y = ((y1.saturating_sub(y0)) as f64 * policy.cell_inset_ratio).round() as u32;
    x0 = x0.saturating_add(inset_x);
    y0 = y0.saturating_add(inset_y);
    x1 = x1.saturating_sub(inset_x);
    y1 = y1.saturating_sub(inset_y);
    if x0 >= x1 || y0 >= y1 {
        return Err(failure(
            ObjectiveRecognitionErrorCode::RegionOutOfBounds,
            "答题卡格位超出裁剪范围",
            false,
        ));
    }
    let mut changed = 0_u64;
    let total = u64::from(x1 - x0) * u64::from(y1 - y0);
    for y in y0..y1 {
        for x in x0..x1 {
            let baseline = blank.get_pixel(x, y).0[0];
            let observed = student.get_pixel(x, y).0[0];
            if baseline.saturating_sub(observed) >= policy.pixel_delta_threshold {
                changed += 1;
            }
        }
    }
    Ok(changed as f64 / total as f64)
}

fn decode_gray(bytes: &[u8]) -> Result<image::GrayImage, ObjectiveRecognitionFailure> {
    image::load_from_memory(bytes)
        .map(DynamicImage::into_luma8)
        .map_err(|_| {
            failure(
                ObjectiveRecognitionErrorCode::DecodeFailed,
                "答题卡图片无法解码",
                false,
            )
        })
}

fn invalid_request(error: CoreError) -> ObjectiveRecognitionFailure {
    failure(
        ObjectiveRecognitionErrorCode::InvalidOutput,
        &format!("答题卡识别输入无效：{error}"),
        false,
    )
}

fn invalid_output(error: CoreError) -> ObjectiveRecognitionFailure {
    failure(
        ObjectiveRecognitionErrorCode::InvalidOutput,
        &format!("答题卡本地识别输出无效：{error}"),
        false,
    )
}

fn failure(
    code: ObjectiveRecognitionErrorCode,
    safe_message: &str,
    retryable: bool,
) -> ObjectiveRecognitionFailure {
    ObjectiveRecognitionFailure {
        schema_version: OBJECTIVE_RECOGNITION_SCHEMA_VERSION,
        code,
        safe_message: safe_message.into(),
        retryable,
    }
}

fn validate_sha256(value: &str) -> CoreResult<()> {
    let value = value.trim();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(
            "答题卡空白模板 hash 必须为 64 位 sha256".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use image::codecs::png::PngEncoder;
    use image::{ColorType, ImageEncoder, Luma};

    use super::*;

    fn png_bytes(image: &image::GrayImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        PngEncoder::new(&mut bytes)
            .write_image(image.as_raw(), image.width(), image.height(), ColorType::L8)
            .unwrap();
        bytes
    }

    fn cells() -> Vec<ObjectiveMarkCell> {
        vec![
            ObjectiveMarkCell {
                label: "A".into(),
                x: 0.0,
                y: 0.0,
                width: 0.5,
                height: 1.0,
            },
            ObjectiveMarkCell {
                label: "B".into(),
                x: 0.5,
                y: 0.0,
                width: 0.5,
                height: 1.0,
            },
        ]
    }

    fn request<'a>(
        bytes: &'a [u8],
        hash: &'a str,
        cells: &'a [ObjectiveMarkCell],
    ) -> ObjectiveRecognitionRequest<'a> {
        ObjectiveRecognitionRequest {
            answer_region_revision_id: 7,
            input_artifact_id: 8,
            input_artifact_sha256: hash,
            mime_type: "image/png",
            image_bytes: bytes,
            question_type: ObjectiveQuestionType::Single,
            template_version: "answer-sheet-v1",
            cells,
        }
    }

    fn policy() -> LocalOmrPolicy {
        LocalOmrPolicy {
            blank_max_ratio: 0.02,
            marked_min_ratio: 0.20,
            pixel_delta_threshold: 40,
            cell_inset_ratio: 0.10,
        }
    }

    #[test]
    fn local_omr_recognizes_one_clear_mark() {
        let blank = image::GrayImage::from_pixel(100, 40, Luma([255]));
        let mut student = blank.clone();
        for y in 5..35 {
            for x in 5..45 {
                student.put_pixel(x, y, Luma([0]));
            }
        }
        let blank = png_bytes(&blank);
        let student = png_bytes(&student);
        let hash = hashing::sha256_hex(&student);
        let recognizer = LocalAnswerSheetOmr::new(blank, policy()).unwrap();
        let cells = cells();
        let output = recognizer
            .recognize(&request(&student, &hash, &cells))
            .unwrap();
        assert_eq!(output.result_state, ObjectiveRecognitionState::Recognized);
        assert_eq!(
            output.answer,
            Some(ObjectiveRecognizedAnswer::SelectedLabels {
                selected_labels: vec!["A".into()]
            })
        );
    }

    #[test]
    fn local_omr_routes_double_mark_to_teacher_review() {
        let blank = image::GrayImage::from_pixel(100, 40, Luma([255]));
        let student = image::GrayImage::from_pixel(100, 40, Luma([0]));
        let blank = png_bytes(&blank);
        let student = png_bytes(&student);
        let hash = hashing::sha256_hex(&student);
        let recognizer = LocalAnswerSheetOmr::new(blank, policy()).unwrap();
        let cells = cells();
        let output = recognizer
            .recognize(&request(&student, &hash, &cells))
            .unwrap();
        assert_eq!(output.result_state, ObjectiveRecognitionState::Altered);
        assert_eq!(output.issue_codes, vec!["MULTIPLE_MARKS"]);
    }

    #[test]
    fn template_requires_four_anchors_and_unique_question_mapping() {
        let definition = AnswerSheetTemplateDefinition {
            schema_version: 1,
            assessment_version_id: 1,
            template_version: "answer-sheet-v1".into(),
            page_no: 1,
            canvas_width: 1000,
            canvas_height: 1400,
            blank_artifact_id: 1,
            blank_artifact_sha256: "a".repeat(64),
            anchors: vec![],
            items: vec![AnswerSheetItemTemplate {
                assessment_item_id: 1,
                region_index: 0,
                question_type: ObjectiveQuestionType::Single,
                region: SheetRect {
                    x: 0.1,
                    y: 0.1,
                    width: 0.5,
                    height: 0.1,
                },
                cells: cells(),
            }],
            policy: policy(),
        };
        assert!(definition.validate().is_err());
    }
}
