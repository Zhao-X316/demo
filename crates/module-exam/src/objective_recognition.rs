//! 固定模板客观题 OMR port 与可重复校准合同。
//!
//! 这里不选择具体供应商，也不把合成夹具当作真实准确率证据。provider 只负责把
//! 当前答案区域裁剪转成结构化 observation；评分、老师终审和发布仍复用 T6 既有服务。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};

pub const OBJECTIVE_RECOGNITION_SCHEMA_VERSION: i64 = 1;
pub const OBJECTIVE_RECOGNITION_CONFIDENCE_THRESHOLD: f64 = 0.95;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveQuestionType {
    Single,
    Multiple,
    TrueFalse,
}

impl ObjectiveQuestionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Multiple => "multiple",
            Self::TrueFalse => "true_false",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "single" => Some(Self::Single),
            "multiple" => Some(Self::Multiple),
            "true_false" => Some(Self::TrueFalse),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveMarkCell {
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ObjectiveMarkCell {
    pub fn validate(&self) -> CoreResult<()> {
        if self.label.trim().is_empty() {
            return Err(CoreError::Invalid("OMR 标记单元标签不能为空".into()));
        }
        for (value, field) in [
            (self.x, "x"),
            (self.y, "y"),
            (self.width, "width"),
            (self.height, "height"),
        ] {
            if !value.is_finite() {
                return Err(CoreError::Invalid(format!(
                    "OMR 标记单元 {field} 必须是有限数值"
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
            return Err(CoreError::Invalid(
                "OMR 标记单元必须位于 0~1 归一化题区内".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveRecognizerDescriptor {
    pub provider: String,
    pub model_name: String,
    pub model_version: String,
    pub config_version: String,
    pub rule_version: String,
}

impl ObjectiveRecognizerDescriptor {
    pub fn validate(&self) -> CoreResult<()> {
        for (value, field) in [
            (&self.provider, "provider"),
            (&self.model_name, "model_name"),
            (&self.model_version, "model_version"),
            (&self.config_version, "config_version"),
            (&self.rule_version, "rule_version"),
        ] {
            if value.trim().is_empty() {
                return Err(CoreError::Invalid(format!("OMR {field} 不能为空")));
            }
        }
        Ok(())
    }
}

pub struct ObjectiveRecognitionRequest<'a> {
    pub answer_region_revision_id: i64,
    pub input_artifact_id: i64,
    pub input_artifact_sha256: &'a str,
    pub mime_type: &'a str,
    pub image_bytes: &'a [u8],
    pub question_type: ObjectiveQuestionType,
    pub template_version: &'a str,
    pub cells: &'a [ObjectiveMarkCell],
}

impl ObjectiveRecognitionRequest<'_> {
    pub fn validate(&self) -> CoreResult<()> {
        if self.answer_region_revision_id <= 0 || self.input_artifact_id <= 0 {
            return Err(CoreError::Invalid(
                "OMR 题区和 artifact id 必须为正数".into(),
            ));
        }
        if self.image_bytes.is_empty() {
            return Err(CoreError::Invalid("OMR 输入图片不能为空".into()));
        }
        if self.mime_type.trim().is_empty() || self.template_version.trim().is_empty() {
            return Err(CoreError::Invalid(
                "OMR mime type 和模板版本不能为空".into(),
            ));
        }
        let expected_hash = normalized_sha256(self.input_artifact_sha256, "OMR artifact hash")?;
        if hashing::sha256_hex(self.image_bytes) != expected_hash {
            return Err(CoreError::Invalid(
                "OMR 输入字节与 artifact hash 不一致".into(),
            ));
        }
        if self.cells.len() < 2 {
            return Err(CoreError::Invalid("OMR 客观题至少需要两个标记单元".into()));
        }
        let mut labels = BTreeSet::new();
        for cell in self.cells {
            cell.validate()?;
            if !labels.insert(cell.label.trim().to_ascii_uppercase()) {
                return Err(CoreError::Invalid("OMR 标记单元标签不能重复".into()));
            }
        }
        if self.question_type == ObjectiveQuestionType::TrueFalse {
            let normalized = labels.iter().map(String::as_str).collect::<BTreeSet<_>>();
            if normalized != BTreeSet::from(["FALSE", "TRUE"]) {
                return Err(CoreError::Invalid(
                    "判断题模板必须且只能包含 TRUE/FALSE 两个标记单元".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn input_hash(&self) -> CoreResult<String> {
        self.validate()?;
        let canonical = serde_json::json!({
            "schema_version": OBJECTIVE_RECOGNITION_SCHEMA_VERSION,
            "answer_region_revision_id": self.answer_region_revision_id,
            "input_artifact_id": self.input_artifact_id,
            "input_artifact_sha256": self.input_artifact_sha256.trim().to_ascii_lowercase(),
            "mime_type": self.mime_type.trim().to_ascii_lowercase(),
            "question_type": self.question_type,
            "template_version": self.template_version.trim(),
            "cells": self.cells,
        });
        Ok(hashing::sha256_hex(
            &serde_json::to_vec(&canonical)
                .map_err(|error| CoreError::Parse(format!("OMR 输入 hash 失败：{error}")))?,
        ))
    }
}

pub trait ObjectiveRecognizer: Send + Sync {
    fn descriptor(&self) -> ObjectiveRecognizerDescriptor;

    fn recognize(
        &self,
        request: &ObjectiveRecognitionRequest<'_>,
    ) -> Result<ObjectiveRecognitionOutput, ObjectiveRecognitionFailure>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveRecognitionState {
    Recognized,
    Blank,
    Altered,
    LowConfidence,
}

impl ObjectiveRecognitionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recognized => "recognized",
            Self::Blank => "blank",
            Self::Altered => "altered",
            Self::LowConfidence => "low_confidence",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObjectiveRecognizedAnswer {
    SelectedLabels { selected_labels: Vec<String> },
    TrueFalse { selected: bool },
    AmbiguousTrueFalse { selected_values: Vec<bool> },
}

impl ObjectiveRecognizedAnswer {
    pub fn observation_json(&self) -> Value {
        match self {
            Self::SelectedLabels { selected_labels } => serde_json::json!({
                "schema_version": 1,
                "selected_labels": selected_labels,
            }),
            Self::TrueFalse { selected } => serde_json::json!({
                "schema_version": 1,
                "selected": selected,
            }),
            Self::AmbiguousTrueFalse { selected_values } => serde_json::json!({
                "schema_version": 1,
                "selected_values": selected_values,
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveMarkMeasurement {
    pub label: String,
    pub ink_ratio: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveRecognitionOutput {
    pub schema_version: i64,
    pub answer_region_revision_id: i64,
    pub input_artifact_id: i64,
    pub input_artifact_sha256: String,
    pub input_hash: String,
    pub question_type: ObjectiveQuestionType,
    pub template_version: String,
    pub descriptor: ObjectiveRecognizerDescriptor,
    pub result_state: ObjectiveRecognitionState,
    pub answer: Option<ObjectiveRecognizedAnswer>,
    pub confidence: Option<f64>,
    pub issue_codes: Vec<String>,
    pub measurements: Vec<ObjectiveMarkMeasurement>,
}

impl ObjectiveRecognitionOutput {
    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != OBJECTIVE_RECOGNITION_SCHEMA_VERSION {
            return Err(CoreError::Invalid(
                "OMR 输出 schema_version 不受支持".into(),
            ));
        }
        if self.answer_region_revision_id <= 0 || self.input_artifact_id <= 0 {
            return Err(CoreError::Invalid(
                "OMR 输出题区和 artifact id 必须为正数".into(),
            ));
        }
        normalized_sha256(&self.input_artifact_sha256, "OMR output artifact hash")?;
        normalized_sha256(&self.input_hash, "OMR output input hash")?;
        if self.template_version.trim().is_empty() {
            return Err(CoreError::Invalid("OMR 输出模板版本不能为空".into()));
        }
        self.descriptor.validate()?;
        if self
            .confidence
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(CoreError::Invalid("OMR 输出置信度必须位于 0~1".into()));
        }
        let mut measurement_labels = BTreeSet::new();
        for measurement in &self.measurements {
            if measurement.label.trim().is_empty()
                || !measurement.ink_ratio.is_finite()
                || !(0.0..=1.0).contains(&measurement.ink_ratio)
                || !measurement.confidence.is_finite()
                || !(0.0..=1.0).contains(&measurement.confidence)
            {
                return Err(CoreError::Invalid("OMR 标记测量值非法".into()));
            }
            if !measurement_labels.insert(measurement.label.trim().to_ascii_uppercase()) {
                return Err(CoreError::Invalid("OMR 标记测量标签不能重复".into()));
            }
        }
        match self.result_state {
            ObjectiveRecognitionState::Blank => {
                if self.answer.is_some() {
                    return Err(CoreError::Invalid("空白 OMR 输出不能携带答案".into()));
                }
            }
            ObjectiveRecognitionState::Recognized => {
                if self
                    .confidence
                    .is_none_or(|value| value < OBJECTIVE_RECOGNITION_CONFIDENCE_THRESHOLD)
                {
                    return Err(CoreError::Invalid(
                        "低于 0.95 的 OMR 输出必须标记 low_confidence".into(),
                    ));
                }
                validate_answer(self.question_type, self.answer.as_ref(), false)?;
            }
            ObjectiveRecognitionState::LowConfidence => {
                if self
                    .confidence
                    .is_none_or(|value| value >= OBJECTIVE_RECOGNITION_CONFIDENCE_THRESHOLD)
                {
                    return Err(CoreError::Invalid(
                        "low_confidence OMR 输出必须携带低于 0.95 的置信度".into(),
                    ));
                }
                validate_answer(self.question_type, self.answer.as_ref(), false)?;
            }
            ObjectiveRecognitionState::Altered => {
                if self.confidence.is_none() || self.issue_codes.is_empty() {
                    return Err(CoreError::Invalid(
                        "涂改/冲突 OMR 输出必须携带置信度和问题码".into(),
                    ));
                }
                validate_answer(self.question_type, self.answer.as_ref(), true)?;
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> CoreResult<String> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("OMR 输出序列化失败：{error}")))
    }

    pub fn observed_answer_json(&self) -> CoreResult<Option<String>> {
        self.validate()?;
        self.answer
            .as_ref()
            .map(|answer| {
                serde_json::to_string(&answer.observation_json()).map_err(|error| {
                    CoreError::Parse(format!("OMR observation 序列化失败：{error}"))
                })
            })
            .transpose()
    }
}

fn validate_answer(
    question_type: ObjectiveQuestionType,
    answer: Option<&ObjectiveRecognizedAnswer>,
    allow_ambiguous: bool,
) -> CoreResult<()> {
    match (question_type, answer) {
        (
            ObjectiveQuestionType::Single,
            Some(ObjectiveRecognizedAnswer::SelectedLabels { selected_labels }),
        ) if (!allow_ambiguous && selected_labels.len() == 1)
            || (allow_ambiguous && !selected_labels.is_empty()) =>
        {
            validate_labels(selected_labels)
        }
        (
            ObjectiveQuestionType::Multiple,
            Some(ObjectiveRecognizedAnswer::SelectedLabels { selected_labels }),
        ) if !selected_labels.is_empty() => validate_labels(selected_labels),
        (ObjectiveQuestionType::TrueFalse, Some(ObjectiveRecognizedAnswer::TrueFalse { .. })) => {
            Ok(())
        }
        (
            ObjectiveQuestionType::TrueFalse,
            Some(ObjectiveRecognizedAnswer::AmbiguousTrueFalse { selected_values }),
        ) if allow_ambiguous
            && selected_values.len() == 2
            && selected_values.contains(&true)
            && selected_values.contains(&false) =>
        {
            Ok(())
        }
        _ => Err(CoreError::Invalid(
            "OMR 输出答案与题型/结果状态不一致".into(),
        )),
    }
}

fn validate_labels(labels: &[String]) -> CoreResult<()> {
    let mut normalized = BTreeSet::new();
    for label in labels {
        let label = label.trim().to_ascii_uppercase();
        if label.is_empty() || !normalized.insert(label) {
            return Err(CoreError::Invalid("OMR 输出选项标签为空或重复".into()));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveRecognitionErrorCode {
    DecodeFailed,
    UnsupportedMedia,
    TemplateMismatch,
    RegionOutOfBounds,
    ProviderUnavailable,
    Timeout,
    RateLimited,
    InvalidOutput,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveRecognitionFailure {
    pub schema_version: i64,
    pub code: ObjectiveRecognitionErrorCode,
    pub safe_message: String,
    pub retryable: bool,
}

impl ObjectiveRecognitionFailure {
    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != OBJECTIVE_RECOGNITION_SCHEMA_VERSION {
            return Err(CoreError::Invalid(
                "OMR 失败 schema_version 不受支持".into(),
            ));
        }
        let message = self.safe_message.trim();
        if message.is_empty() || message.chars().count() > 500 {
            return Err(CoreError::Invalid("OMR 脱敏错误说明为空或过长".into()));
        }
        let lowered = message.to_ascii_lowercase();
        if [
            "authorization",
            "bearer ",
            "api_key",
            "apikey",
            "access_token",
        ]
        .iter()
        .any(|needle| lowered.contains(needle))
        {
            return Err(CoreError::Invalid(
                "OMR 错误说明疑似包含凭据，不允许持久化".into(),
            ));
        }
        Ok(())
    }

    pub fn to_json(&self) -> CoreResult<String> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("OMR 失败序列化失败：{error}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoldenSourceKind {
    SyntheticContract,
    RealScan,
    RealPhoto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoldenExpectedState {
    Recognized,
    Blank,
    Altered,
    LowConfidence,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveGoldenCase {
    pub case_id: String,
    pub source_kind: GoldenSourceKind,
    pub material_kind: String,
    pub question_type: ObjectiveQuestionType,
    pub expected_state: GoldenExpectedState,
    pub expected_answer: Option<ObjectiveRecognizedAnswer>,
    pub artifact_sha256: Option<String>,
    pub issue_tags: Vec<String>,
    pub annotation_revision: i64,
    pub annotated_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveGoldenManifest {
    pub schema_version: i64,
    pub dataset_id: String,
    pub production_accuracy_claim_allowed: bool,
    pub cases: Vec<ObjectiveGoldenCase>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveGoldenPrediction {
    pub case_id: String,
    pub predicted_state: GoldenExpectedState,
    pub predicted_answer: Option<ObjectiveRecognizedAnswer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveCalibrationReport {
    pub schema_version: i64,
    pub dataset_id: String,
    pub case_count: usize,
    pub state_match_count: usize,
    pub answer_match_count: usize,
    pub false_accept_count: usize,
    pub false_reject_count: usize,
    pub review_required_count: usize,
    pub missing_prediction_count: usize,
    pub production_accuracy_claim_allowed: bool,
}

impl ObjectiveGoldenManifest {
    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != OBJECTIVE_RECOGNITION_SCHEMA_VERSION
            || self.dataset_id.trim().is_empty()
            || self.cases.is_empty()
        {
            return Err(CoreError::Invalid("OMR 黄金集头信息不完整".into()));
        }
        let mut ids = BTreeSet::new();
        let mut has_synthetic = false;
        for case in &self.cases {
            if case.case_id.trim().is_empty()
                || !ids.insert(case.case_id.trim().to_string())
                || case.material_kind.trim().is_empty()
                || case.annotation_revision <= 0
                || case.annotated_by.trim().is_empty()
            {
                return Err(CoreError::Invalid(
                    "OMR 黄金集样本元数据不完整或重复".into(),
                ));
            }
            match case.source_kind {
                GoldenSourceKind::SyntheticContract => has_synthetic = true,
                GoldenSourceKind::RealScan | GoldenSourceKind::RealPhoto => {
                    normalized_sha256(
                        case.artifact_sha256.as_deref().ok_or_else(|| {
                            CoreError::Invalid("真实 OMR 样本必须记录 artifact hash".into())
                        })?,
                        "OMR golden artifact hash",
                    )?;
                }
            }
        }
        if self.production_accuracy_claim_allowed && has_synthetic {
            return Err(CoreError::Invalid(
                "含合成样本的合同集不能宣称生产准确率".into(),
            ));
        }
        Ok(())
    }

    pub fn coverage_gaps(&self) -> Vec<String> {
        let present = self
            .cases
            .iter()
            .flat_map(|case| case.issue_tags.iter())
            .map(|tag| tag.trim().to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        [
            "normal",
            "blank",
            "multiple_marks",
            "erasure",
            "shadow",
            "rotation",
            "blur",
            "template_mismatch",
        ]
        .into_iter()
        .filter(|tag| !present.contains(*tag))
        .map(str::to_string)
        .collect()
    }
}

/// 按冻结标注计算可解释的 OMR 校准结果。
///
/// `false_accept` 指标只表示“本应进入异常却被识别为清晰”，不等价于最终错判通过；
/// 老师终审仍是独立闸门。
pub fn evaluate_objective_calibration(
    manifest: &ObjectiveGoldenManifest,
    predictions: &[ObjectiveGoldenPrediction],
) -> CoreResult<ObjectiveCalibrationReport> {
    manifest.validate()?;
    let mut by_id = std::collections::BTreeMap::new();
    for prediction in predictions {
        if prediction.case_id.trim().is_empty()
            || by_id
                .insert(prediction.case_id.trim(), prediction)
                .is_some()
        {
            return Err(CoreError::Invalid("OMR 校准预测 case_id 为空或重复".into()));
        }
    }
    let mut state_match_count = 0;
    let mut answer_match_count = 0;
    let mut false_accept_count = 0;
    let mut false_reject_count = 0;
    let mut review_required_count = 0;
    let mut missing_prediction_count = 0;
    for case in &manifest.cases {
        let Some(prediction) = by_id.get(case.case_id.as_str()) else {
            missing_prediction_count += 1;
            continue;
        };
        if prediction.predicted_state == case.expected_state {
            state_match_count += 1;
        }
        if prediction.predicted_answer == case.expected_answer {
            answer_match_count += 1;
        }
        let expected_clear = case.expected_state == GoldenExpectedState::Recognized;
        let predicted_clear = prediction.predicted_state == GoldenExpectedState::Recognized;
        if !expected_clear && predicted_clear {
            false_accept_count += 1;
        }
        if expected_clear && !predicted_clear {
            false_reject_count += 1;
        }
        if !predicted_clear {
            review_required_count += 1;
        }
    }
    Ok(ObjectiveCalibrationReport {
        schema_version: OBJECTIVE_RECOGNITION_SCHEMA_VERSION,
        dataset_id: manifest.dataset_id.clone(),
        case_count: manifest.cases.len(),
        state_match_count,
        answer_match_count,
        false_accept_count,
        false_reject_count,
        review_required_count,
        missing_prediction_count,
        production_accuracy_claim_allowed: manifest.production_accuracy_claim_allowed,
    })
}

fn normalized_sha256(value: &str, field: &str) -> CoreResult<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(format!(
            "{field} 必须是 64 位十六进制 sha256"
        )));
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn descriptor() -> ObjectiveRecognizerDescriptor {
        ObjectiveRecognizerDescriptor {
            provider: "fixture".into(),
            model_name: "fixed-template-omr".into(),
            model_version: "1".into(),
            config_version: "config-v1".into(),
            rule_version: "rule-v1".into(),
        }
    }

    fn output(state: ObjectiveRecognitionState) -> ObjectiveRecognitionOutput {
        ObjectiveRecognitionOutput {
            schema_version: 1,
            answer_region_revision_id: 7,
            input_artifact_id: 9,
            input_artifact_sha256: "a".repeat(64),
            input_hash: "b".repeat(64),
            question_type: ObjectiveQuestionType::Single,
            template_version: "template-v1".into(),
            descriptor: descriptor(),
            result_state: state,
            answer: Some(ObjectiveRecognizedAnswer::SelectedLabels {
                selected_labels: vec!["A".into()],
            }),
            confidence: Some(0.99),
            issue_codes: Vec::new(),
            measurements: vec![ObjectiveMarkMeasurement {
                label: "A".into(),
                ink_ratio: 0.72,
                confidence: 0.99,
            }],
        }
    }

    #[test]
    fn request_hash_binds_artifact_template_and_cells() {
        let bytes = b"fixture-image";
        let hash = hashing::sha256_hex(bytes);
        let cells = vec![
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
        ];
        let request = ObjectiveRecognitionRequest {
            answer_region_revision_id: 7,
            input_artifact_id: 9,
            input_artifact_sha256: &hash,
            mime_type: "image/png",
            image_bytes: bytes,
            question_type: ObjectiveQuestionType::Single,
            template_version: "template-v1",
            cells: &cells,
        };
        assert_eq!(request.input_hash().unwrap().len(), 64);

        let wrong_hash = "0".repeat(64);
        let invalid = ObjectiveRecognitionRequest {
            input_artifact_sha256: &wrong_hash,
            ..request
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn state_contract_separates_clear_low_blank_and_multiple_marks() {
        output(ObjectiveRecognitionState::Recognized)
            .validate()
            .unwrap();

        let mut low = output(ObjectiveRecognitionState::LowConfidence);
        low.confidence = Some(0.72);
        low.validate().unwrap();

        let mut blank = output(ObjectiveRecognitionState::Blank);
        blank.answer = None;
        blank.confidence = Some(0.98);
        blank.validate().unwrap();

        let mut multiple = output(ObjectiveRecognitionState::Altered);
        multiple.answer = Some(ObjectiveRecognizedAnswer::SelectedLabels {
            selected_labels: vec!["A".into(), "B".into()],
        });
        multiple.confidence = Some(0.97);
        multiple.issue_codes = vec!["MULTIPLE_MARKS".into()];
        multiple.validate().unwrap();

        let mut invalid = output(ObjectiveRecognitionState::Recognized);
        invalid.confidence = Some(0.70);
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn sanitized_failure_rejects_credential_like_content() {
        let safe = ObjectiveRecognitionFailure {
            schema_version: 1,
            code: ObjectiveRecognitionErrorCode::TemplateMismatch,
            safe_message: "模板锚点不匹配".into(),
            retryable: false,
        };
        safe.validate().unwrap();

        let unsafe_message = ObjectiveRecognitionFailure {
            safe_message: "Bearer secret-value".into(),
            ..safe
        };
        assert!(unsafe_message.validate().is_err());
    }

    #[test]
    fn synthetic_contract_manifest_cannot_claim_production_accuracy() {
        let manifest: ObjectiveGoldenManifest = serde_json::from_str(include_str!(
            "../tests/fixtures/objective_omr/synthetic_contract_manifest_v1.json"
        ))
        .unwrap();
        manifest.validate().unwrap();
        assert!(manifest.coverage_gaps().is_empty());
        assert!(!manifest.production_accuracy_claim_allowed);

        let predictions = manifest
            .cases
            .iter()
            .map(|case| ObjectiveGoldenPrediction {
                case_id: case.case_id.clone(),
                predicted_state: case.expected_state,
                predicted_answer: case.expected_answer.clone(),
            })
            .collect::<Vec<_>>();
        let report = evaluate_objective_calibration(&manifest, &predictions).unwrap();
        assert_eq!(report.case_count, 8);
        assert_eq!(report.state_match_count, 8);
        assert_eq!(report.false_accept_count, 0);
        assert_eq!(report.missing_prediction_count, 0);
        assert!(!report.production_accuracy_claim_allowed);

        let mut invalid = manifest;
        invalid.production_accuracy_claim_allowed = true;
        assert!(invalid.validate().is_err());
    }
}
