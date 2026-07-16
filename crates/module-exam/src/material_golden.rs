//! 普通试卷、答题卡和默写三类材料共用的黄金集元数据与离线评估器。
//!
//! 清单和报告只保存不可逆 artifact/value hash、冻结标注与聚合计数，不保存学生姓名、
//! 文件路径或作答正文。仓库内合成合同集只能验证契约，不能宣称生产准确率。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use suite_core::error::{CoreError, CoreResult};

use crate::pilot_data_gate::{
    PilotDataGateManifest, PilotDataGateReport, PilotDataType, PilotGateScopeKind,
};

pub const MATERIAL_GOLDEN_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialGoldenKind {
    OrdinaryPaper,
    AnswerSheet,
    Dictation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialGoldenSourceKind {
    SyntheticContract,
    RealScan,
    RealPhoto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialGoldenStorageScope {
    RepositorySynthetic,
    LocalRestricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialGoldenRoute {
    ReadyForBatchConfirm,
    ReviewRequired,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialGoldenUnitState {
    Recognized,
    Blank,
    Altered,
    LowConfidence,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialGoldenGovernance {
    pub storage_scope: MaterialGoldenStorageScope,
    pub personal_identifiers_removed: bool,
    pub privacy_reviewed: bool,
    pub privacy_reviewed_by: String,
    pub retention_policy_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pilot_gate_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pilot_gate_policy_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialGoldenUnit {
    pub unit_key: String,
    pub expected_state: MaterialGoldenUnitState,
    pub expected_value_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialGoldenCase {
    pub case_id: String,
    pub source_kind: MaterialGoldenSourceKind,
    pub artifact_sha256: Option<String>,
    pub expected_route: MaterialGoldenRoute,
    pub expected_units: Vec<MaterialGoldenUnit>,
    pub issue_tags: Vec<String>,
    pub annotation_revision: i64,
    pub annotated_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialGoldenManifest {
    pub schema_version: i64,
    pub dataset_id: String,
    pub material_kind: MaterialGoldenKind,
    pub production_accuracy_claim_allowed: bool,
    pub governance: MaterialGoldenGovernance,
    pub cases: Vec<MaterialGoldenCase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialGoldenPredictionUnit {
    pub unit_key: String,
    pub predicted_state: MaterialGoldenUnitState,
    pub predicted_value_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialGoldenPrediction {
    pub case_id: String,
    pub predicted_route: MaterialGoldenRoute,
    pub predicted_units: Vec<MaterialGoldenPredictionUnit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialGoldenPredictionSet {
    pub schema_version: i64,
    pub dataset_id: String,
    pub predictions: Vec<MaterialGoldenPrediction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialCalibrationReport {
    pub schema_version: i64,
    pub dataset_id: String,
    pub material_kind: MaterialGoldenKind,
    pub case_count: usize,
    pub route_match_count: usize,
    pub unsafe_ready_count: usize,
    pub ready_routed_to_review_count: usize,
    pub missing_prediction_count: usize,
    pub unexpected_prediction_count: usize,
    pub expected_unit_count: usize,
    pub unit_state_match_count: usize,
    pub unit_value_match_count: usize,
    pub unit_false_accept_count: usize,
    pub unit_false_reject_count: usize,
    pub missing_unit_count: usize,
    pub unexpected_unit_count: usize,
    pub coverage_gaps: Vec<String>,
    pub production_accuracy_claim_allowed: bool,
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

fn validate_expected_unit(unit: &MaterialGoldenUnit) -> CoreResult<()> {
    if unit.unit_key.trim().is_empty() {
        return Err(CoreError::Invalid("黄金集 unit_key 不能为空".into()));
    }
    match unit.expected_state {
        MaterialGoldenUnitState::Recognized => {
            normalized_sha256(
                unit.expected_value_sha256
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("recognized 单元必须记录值 hash".into()))?,
                "黄金集 expected value hash",
            )?;
        }
        MaterialGoldenUnitState::Blank | MaterialGoldenUnitState::Failed => {
            if unit.expected_value_sha256.is_some() {
                return Err(CoreError::Invalid(
                    "blank/failed 单元不得伪造识别值 hash".into(),
                ));
            }
        }
        MaterialGoldenUnitState::Altered | MaterialGoldenUnitState::LowConfidence => {
            if let Some(hash) = unit.expected_value_sha256.as_deref() {
                normalized_sha256(hash, "黄金集 expected value hash")?;
            }
        }
    }
    Ok(())
}

fn validate_prediction_unit(unit: &MaterialGoldenPredictionUnit) -> CoreResult<()> {
    if unit.unit_key.trim().is_empty() {
        return Err(CoreError::Invalid("黄金集预测 unit_key 不能为空".into()));
    }
    match unit.predicted_state {
        MaterialGoldenUnitState::Recognized => {
            normalized_sha256(
                unit.predicted_value_sha256
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("recognized 预测必须记录值 hash".into()))?,
                "黄金集 predicted value hash",
            )?;
        }
        MaterialGoldenUnitState::Blank | MaterialGoldenUnitState::Failed => {
            if unit.predicted_value_sha256.is_some() {
                return Err(CoreError::Invalid(
                    "blank/failed 预测不得携带识别值 hash".into(),
                ));
            }
        }
        MaterialGoldenUnitState::Altered | MaterialGoldenUnitState::LowConfidence => {
            if let Some(hash) = unit.predicted_value_sha256.as_deref() {
                normalized_sha256(hash, "黄金集 predicted value hash")?;
            }
        }
    }
    Ok(())
}

impl MaterialGoldenManifest {
    pub fn contains_real_data(&self) -> bool {
        self.cases.iter().any(|case| {
            matches!(
                case.source_kind,
                MaterialGoldenSourceKind::RealScan | MaterialGoldenSourceKind::RealPhoto
            )
        })
    }

    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != MATERIAL_GOLDEN_SCHEMA_VERSION
            || self.dataset_id.trim().is_empty()
            || self.cases.is_empty()
            || self.governance.privacy_reviewed_by.trim().is_empty()
            || self.governance.retention_policy_version.trim().is_empty()
        {
            return Err(CoreError::Invalid("三材料黄金集头信息不完整".into()));
        }

        let mut case_ids = BTreeSet::new();
        let mut has_synthetic = false;
        let mut has_real = false;
        for case in &self.cases {
            if case.case_id.trim().is_empty()
                || !case_ids.insert(case.case_id.trim().to_owned())
                || case.issue_tags.is_empty()
                || case.issue_tags.iter().any(|tag| tag.trim().is_empty())
                || case.annotation_revision <= 0
                || case.annotated_by.trim().is_empty()
            {
                return Err(CoreError::Invalid(
                    "三材料黄金集样本元数据不完整或重复".into(),
                ));
            }
            match case.source_kind {
                MaterialGoldenSourceKind::SyntheticContract => {
                    has_synthetic = true;
                    if case.artifact_sha256.is_some() {
                        return Err(CoreError::Invalid(
                            "合成合同样本不得冒充真实 artifact".into(),
                        ));
                    }
                }
                MaterialGoldenSourceKind::RealScan | MaterialGoldenSourceKind::RealPhoto => {
                    has_real = true;
                    normalized_sha256(
                        case.artifact_sha256.as_deref().ok_or_else(|| {
                            CoreError::Invalid("真实材料黄金集必须记录 artifact hash".into())
                        })?,
                        "三材料 golden artifact hash",
                    )?;
                }
            }
            if case.expected_route != MaterialGoldenRoute::Blocked && case.expected_units.is_empty()
            {
                return Err(CoreError::Invalid(
                    "非 blocked 黄金集样本必须至少标注一个识别单元".into(),
                ));
            }
            let mut unit_keys = BTreeSet::new();
            for unit in &case.expected_units {
                validate_expected_unit(unit)?;
                if !unit_keys.insert(unit.unit_key.trim().to_owned()) {
                    return Err(CoreError::Invalid("黄金集单元键重复".into()));
                }
            }
        }

        match self.governance.storage_scope {
            MaterialGoldenStorageScope::RepositorySynthetic if has_real => {
                return Err(CoreError::Invalid(
                    "真实学生材料不得登记为仓库内合成存储".into(),
                ))
            }
            MaterialGoldenStorageScope::LocalRestricted if has_synthetic && !has_real => {
                return Err(CoreError::Invalid(
                    "纯合成合同集不应伪装成本机受限真实集".into(),
                ))
            }
            _ => {}
        }
        if has_real {
            let gate_id_missing = match self.governance.pilot_gate_id.as_deref() {
                Some(value) => value.trim().is_empty(),
                None => true,
            };
            if gate_id_missing {
                return Err(CoreError::Invalid(
                    "真实材料黄金集必须引用已批准的试点数据闸门".into(),
                ));
            }
            normalized_sha256(
                self.governance
                    .pilot_gate_policy_sha256
                    .as_deref()
                    .ok_or_else(|| {
                        CoreError::Invalid("真实材料黄金集必须冻结闸门策略 hash".into())
                    })?,
                "试点数据闸门策略 hash",
            )?;
        } else if self.governance.pilot_gate_id.is_some()
            || self.governance.pilot_gate_policy_sha256.is_some()
        {
            return Err(CoreError::Invalid(
                "仓库合成合同集不得伪装成已获真实数据闸门放行".into(),
            ));
        }
        if self.production_accuracy_claim_allowed
            && (has_synthetic
                || !has_real
                || self.governance.storage_scope != MaterialGoldenStorageScope::LocalRestricted
                || !self.governance.personal_identifiers_removed
                || !self.governance.privacy_reviewed)
        {
            return Err(CoreError::Invalid(
                "生产准确率只能来自完成隐私审查的本机受限真实材料集".into(),
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
        let required: &[&str] = match self.material_kind {
            MaterialGoldenKind::OrdinaryPaper => &[
                "normal",
                "blur",
                "rotation",
                "missing_page",
                "duplicate_page",
                "template_mismatch",
            ],
            MaterialGoldenKind::AnswerSheet => &[
                "normal",
                "blank",
                "multiple_marks",
                "erasure",
                "shadow",
                "template_mismatch",
            ],
            MaterialGoldenKind::Dictation => &[
                "normal",
                "blank",
                "wrong_text",
                "ambiguous_glyph",
                "erasure",
                "ocr_failed",
            ],
        };
        required
            .iter()
            .filter(|tag| !present.contains(**tag))
            .map(|tag| (*tag).to_owned())
            .collect()
    }
}

/// 验证真实黄金集引用的闸门身份、策略 hash 与当前放行窗口。
pub fn validate_real_material_gate(
    manifest: &MaterialGoldenManifest,
    gate: &PilotDataGateManifest,
    evaluated_on: &str,
) -> CoreResult<PilotDataGateReport> {
    manifest.validate()?;
    if !manifest.contains_real_data() {
        return Err(CoreError::Invalid(
            "合成合同集不需要也不得借用真实试点数据闸门".into(),
        ));
    }
    if gate.scope_kind != PilotGateScopeKind::RealPilot {
        return Err(CoreError::Invalid(
            "合同夹具闸门不能放行真实学生材料".into(),
        ));
    }
    let allows_source_image = gate
        .allowed_data_types
        .contains(&PilotDataType::StudentPageImage)
        || gate
            .allowed_data_types
            .contains(&PilotDataType::AnswerRegionImage);
    if !allows_source_image
        || !gate
            .allowed_data_types
            .contains(&PilotDataType::MachineSuggestion)
        || !gate
            .allowed_data_types
            .contains(&PilotDataType::TeacherDecision)
    {
        return Err(CoreError::Invalid(
            "真实黄金集闸门必须明确允许来源图像、机器建议和老师标注".into(),
        ));
    }
    let expected_gate_id = manifest
        .governance
        .pilot_gate_id
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("真实黄金集缺少 gate_id".into()))?;
    if expected_gate_id != gate.gate_id {
        return Err(CoreError::Invalid("黄金集引用了不同的试点数据闸门".into()));
    }
    let report = gate.evaluate(evaluated_on)?;
    let expected_hash = manifest
        .governance
        .pilot_gate_policy_sha256
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("真实黄金集缺少闸门策略 hash".into()))?;
    if !expected_hash.eq_ignore_ascii_case(&report.policy_sha256) {
        return Err(CoreError::Invalid(
            "试点数据闸门内容已变化，必须重新审批并冻结新 hash".into(),
        ));
    }
    if !report.real_data_allowed {
        return Err(CoreError::Invalid(format!(
            "真实学生数据闸门未放行：{}",
            report.denial_reasons.join(",")
        )));
    }
    Ok(report)
}

impl MaterialGoldenPredictionSet {
    pub fn validate_for(&self, manifest: &MaterialGoldenManifest) -> CoreResult<()> {
        if self.schema_version != MATERIAL_GOLDEN_SCHEMA_VERSION
            || self.dataset_id != manifest.dataset_id
        {
            return Err(CoreError::Invalid(
                "黄金集预测 schema 或 dataset 身份不一致".into(),
            ));
        }
        let mut case_ids = BTreeSet::new();
        for prediction in &self.predictions {
            if prediction.case_id.trim().is_empty()
                || !case_ids.insert(prediction.case_id.trim().to_owned())
            {
                return Err(CoreError::Invalid("黄金集预测 case_id 为空或重复".into()));
            }
            let mut unit_keys = BTreeSet::new();
            for unit in &prediction.predicted_units {
                validate_prediction_unit(unit)?;
                if !unit_keys.insert(unit.unit_key.trim().to_owned()) {
                    return Err(CoreError::Invalid("黄金集预测单元键重复".into()));
                }
            }
        }
        Ok(())
    }
}

/// 只按冻结标注与 hash 计算离线报告，不读取图片、不调用 provider、不产生教学结果。
pub fn evaluate_material_calibration(
    manifest: &MaterialGoldenManifest,
    prediction_set: &MaterialGoldenPredictionSet,
) -> CoreResult<MaterialCalibrationReport> {
    manifest.validate()?;
    if manifest.contains_real_data() {
        return Err(CoreError::Invalid(
            "真实黄金集必须使用带已批准数据闸门的评估入口".into(),
        ));
    }
    prediction_set.validate_for(manifest)?;

    evaluate_material_calibration_validated(manifest, prediction_set)
}

/// 真实材料只能在闸门有效、身份和策略 hash 均匹配时进入只读评估。
pub fn evaluate_real_material_calibration(
    manifest: &MaterialGoldenManifest,
    prediction_set: &MaterialGoldenPredictionSet,
    gate: &PilotDataGateManifest,
    evaluated_on: &str,
) -> CoreResult<MaterialCalibrationReport> {
    validate_real_material_gate(manifest, gate, evaluated_on)?;
    prediction_set.validate_for(manifest)?;
    evaluate_material_calibration_validated(manifest, prediction_set)
}

fn evaluate_material_calibration_validated(
    manifest: &MaterialGoldenManifest,
    prediction_set: &MaterialGoldenPredictionSet,
) -> CoreResult<MaterialCalibrationReport> {
    let predictions = prediction_set
        .predictions
        .iter()
        .map(|prediction| (prediction.case_id.as_str(), prediction))
        .collect::<BTreeMap<_, _>>();
    let expected_case_ids = manifest
        .cases
        .iter()
        .map(|case| case.case_id.as_str())
        .collect::<BTreeSet<_>>();
    let unexpected_prediction_count = predictions
        .keys()
        .filter(|case_id| !expected_case_ids.contains(**case_id))
        .count();

    let mut route_match_count = 0;
    let mut unsafe_ready_count = 0;
    let mut ready_routed_to_review_count = 0;
    let mut missing_prediction_count = 0;
    let mut expected_unit_count = 0;
    let mut unit_state_match_count = 0;
    let mut unit_value_match_count = 0;
    let mut unit_false_accept_count = 0;
    let mut unit_false_reject_count = 0;
    let mut missing_unit_count = 0;
    let mut unexpected_unit_count = 0;

    for case in &manifest.cases {
        expected_unit_count += case.expected_units.len();
        let Some(prediction) = predictions.get(case.case_id.as_str()) else {
            missing_prediction_count += 1;
            missing_unit_count += case.expected_units.len();
            continue;
        };
        if prediction.predicted_route == case.expected_route {
            route_match_count += 1;
        }
        if case.expected_route != MaterialGoldenRoute::ReadyForBatchConfirm
            && prediction.predicted_route == MaterialGoldenRoute::ReadyForBatchConfirm
        {
            unsafe_ready_count += 1;
        }
        if case.expected_route == MaterialGoldenRoute::ReadyForBatchConfirm
            && prediction.predicted_route != MaterialGoldenRoute::ReadyForBatchConfirm
        {
            ready_routed_to_review_count += 1;
        }

        let predicted_units = prediction
            .predicted_units
            .iter()
            .map(|unit| (unit.unit_key.as_str(), unit))
            .collect::<BTreeMap<_, _>>();
        let expected_keys = case
            .expected_units
            .iter()
            .map(|unit| unit.unit_key.as_str())
            .collect::<BTreeSet<_>>();
        unexpected_unit_count += predicted_units
            .keys()
            .filter(|unit_key| !expected_keys.contains(**unit_key))
            .count();
        for expected in &case.expected_units {
            let Some(predicted) = predicted_units.get(expected.unit_key.as_str()) else {
                missing_unit_count += 1;
                continue;
            };
            if predicted.predicted_state == expected.expected_state {
                unit_state_match_count += 1;
            }
            if predicted.predicted_value_sha256 == expected.expected_value_sha256 {
                unit_value_match_count += 1;
            }
            let expected_clear = expected.expected_state == MaterialGoldenUnitState::Recognized;
            let predicted_clear = predicted.predicted_state == MaterialGoldenUnitState::Recognized;
            if !expected_clear && predicted_clear {
                unit_false_accept_count += 1;
            }
            if expected_clear && !predicted_clear {
                unit_false_reject_count += 1;
            }
        }
    }

    Ok(MaterialCalibrationReport {
        schema_version: MATERIAL_GOLDEN_SCHEMA_VERSION,
        dataset_id: manifest.dataset_id.clone(),
        material_kind: manifest.material_kind,
        case_count: manifest.cases.len(),
        route_match_count,
        unsafe_ready_count,
        ready_routed_to_review_count,
        missing_prediction_count,
        unexpected_prediction_count,
        expected_unit_count,
        unit_state_match_count,
        unit_value_match_count,
        unit_false_accept_count,
        unit_false_reject_count,
        missing_unit_count,
        unexpected_unit_count,
        coverage_gaps: manifest.coverage_gaps(),
        production_accuracy_claim_allowed: manifest.production_accuracy_claim_allowed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifests() -> Vec<MaterialGoldenManifest> {
        [
            include_str!("../tests/fixtures/material_golden/ordinary_paper_manifest_v1.json"),
            include_str!("../tests/fixtures/material_golden/answer_sheet_manifest_v1.json"),
            include_str!("../tests/fixtures/material_golden/dictation_manifest_v1.json"),
        ]
        .into_iter()
        .map(|json| serde_json::from_str(json).unwrap())
        .collect()
    }

    fn perfect_predictions(manifest: &MaterialGoldenManifest) -> MaterialGoldenPredictionSet {
        MaterialGoldenPredictionSet {
            schema_version: 1,
            dataset_id: manifest.dataset_id.clone(),
            predictions: manifest
                .cases
                .iter()
                .map(|case| MaterialGoldenPrediction {
                    case_id: case.case_id.clone(),
                    predicted_route: case.expected_route,
                    predicted_units: case
                        .expected_units
                        .iter()
                        .map(|unit| MaterialGoldenPredictionUnit {
                            unit_key: unit.unit_key.clone(),
                            predicted_state: unit.expected_state,
                            predicted_value_sha256: unit.expected_value_sha256.clone(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn three_synthetic_contract_manifests_cover_required_failure_modes() {
        for manifest in manifests() {
            manifest.validate().unwrap();
            assert!(manifest.coverage_gaps().is_empty());
            assert!(!manifest.production_accuracy_claim_allowed);
            assert_eq!(
                manifest.governance.storage_scope,
                MaterialGoldenStorageScope::RepositorySynthetic
            );
        }
    }

    #[test]
    fn perfect_contract_predictions_produce_reproducible_non_production_reports() {
        for manifest in manifests() {
            let predictions = perfect_predictions(&manifest);
            let report = evaluate_material_calibration(&manifest, &predictions).unwrap();
            assert_eq!(report.route_match_count, report.case_count);
            assert_eq!(report.unit_state_match_count, report.expected_unit_count);
            assert_eq!(report.unit_value_match_count, report.expected_unit_count);
            assert_eq!(report.unsafe_ready_count, 0);
            assert_eq!(report.unit_false_accept_count, 0);
            assert_eq!(report.missing_prediction_count, 0);
            assert!(!report.production_accuracy_claim_allowed);
        }
    }

    #[test]
    fn unsafe_ready_and_false_accept_are_reported_separately() {
        let manifest = manifests().remove(0);
        let mut predictions = perfect_predictions(&manifest);
        let review_case = manifest
            .cases
            .iter()
            .find(|case| case.expected_route == MaterialGoldenRoute::ReviewRequired)
            .unwrap();
        let prediction = predictions
            .predictions
            .iter_mut()
            .find(|prediction| prediction.case_id == review_case.case_id)
            .unwrap();
        prediction.predicted_route = MaterialGoldenRoute::ReadyForBatchConfirm;
        prediction.predicted_units[0].predicted_state = MaterialGoldenUnitState::Recognized;
        prediction.predicted_units[0].predicted_value_sha256 = Some("f".repeat(64));

        let report = evaluate_material_calibration(&manifest, &predictions).unwrap();
        assert_eq!(report.unsafe_ready_count, 1);
        assert_eq!(report.unit_false_accept_count, 1);
    }

    #[test]
    fn synthetic_or_unreviewed_data_cannot_claim_production_accuracy() {
        let mut synthetic = manifests().remove(0);
        synthetic.production_accuracy_claim_allowed = true;
        assert!(synthetic.validate().is_err());

        let mut real = synthetic;
        real.production_accuracy_claim_allowed = true;
        real.governance.storage_scope = MaterialGoldenStorageScope::LocalRestricted;
        real.governance.privacy_reviewed = false;
        real.governance.pilot_gate_id = Some("pilot-gate-opaque-001".into());
        real.governance.pilot_gate_policy_sha256 = Some("b".repeat(64));
        for case in &mut real.cases {
            case.source_kind = MaterialGoldenSourceKind::RealPhoto;
            case.artifact_sha256 = Some("a".repeat(64));
        }
        assert!(real.validate().is_err());
        real.governance.privacy_reviewed = true;
        real.validate().unwrap();
    }

    #[test]
    fn real_material_requires_current_matching_approved_gate_before_evaluation() {
        use crate::pilot_data_gate::tests::approved_real_gate;

        let mut manifest = manifests().remove(0);
        let gate = approved_real_gate();
        manifest.governance.storage_scope = MaterialGoldenStorageScope::LocalRestricted;
        manifest.governance.pilot_gate_id = Some(gate.gate_id.clone());
        manifest.governance.pilot_gate_policy_sha256 = Some(gate.policy_sha256().unwrap());
        manifest.governance.privacy_reviewed = true;
        manifest.production_accuracy_claim_allowed = true;
        for case in &mut manifest.cases {
            case.source_kind = MaterialGoldenSourceKind::RealPhoto;
            case.artifact_sha256 = Some("a".repeat(64));
        }
        let predictions = perfect_predictions(&manifest);

        assert!(evaluate_material_calibration(&manifest, &predictions).is_err());
        let report =
            evaluate_real_material_calibration(&manifest, &predictions, &gate, "2026-07-15")
                .unwrap();
        assert!(report.production_accuracy_claim_allowed);

        let mut missing_data_scope = gate.clone();
        missing_data_scope.allowed_data_types.retain(|data_type| {
            !matches!(
                data_type,
                PilotDataType::StudentPageImage | PilotDataType::AnswerRegionImage
            )
        });
        assert!(evaluate_real_material_calibration(
            &manifest,
            &predictions,
            &missing_data_scope,
            "2026-07-15"
        )
        .is_err());

        let mut changed_gate = gate;
        changed_gate.purpose.push_str("，变更用途");
        assert!(evaluate_real_material_calibration(
            &manifest,
            &predictions,
            &changed_gate,
            "2026-07-15"
        )
        .is_err());
        assert!(evaluate_real_material_calibration(
            &manifest,
            &predictions,
            &changed_gate,
            "2026-08-01"
        )
        .is_err());
    }

    #[test]
    fn real_and_synthetic_manifests_cannot_omit_or_borrow_gate_metadata() {
        let mut real = manifests().remove(0);
        real.governance.storage_scope = MaterialGoldenStorageScope::LocalRestricted;
        real.governance.privacy_reviewed = true;
        for case in &mut real.cases {
            case.source_kind = MaterialGoldenSourceKind::RealPhoto;
            case.artifact_sha256 = Some("a".repeat(64));
        }
        assert!(real.validate().is_err());

        let mut synthetic = manifests().remove(0);
        synthetic.governance.pilot_gate_id = Some("pilot-gate-opaque-001".into());
        synthetic.governance.pilot_gate_policy_sha256 = Some("b".repeat(64));
        assert!(synthetic.validate().is_err());
    }
}
