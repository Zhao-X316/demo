//! 普通试卷、答题卡和默写三材料影子试点的单会话编排。
//!
//! 本层只消费冻结黄金清单和 provider 预测，输出 hash、聚合计数与安全发现；不读取
//! 学生身份，不保存文件路径或作答正文，不创建成绩、发布或学习证据。即使会话无安全
//! 发现，`release_authorized` 也固定为 false，正式放行必须走独立人工验收。

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};
use suite_core::domain::{hashing, ids};
use suite_core::error::{CoreError, CoreResult};

use crate::material_golden::{
    evaluate_material_calibration, evaluate_real_material_calibration, MaterialCalibrationReport,
    MaterialGoldenKind, MaterialGoldenManifest, MaterialGoldenPredictionSet,
};
use crate::pilot_data_gate::PilotDataGateManifest;
use crate::pilot_data_rights::PilotDataRightsEvidenceBundle;

pub const SHADOW_PILOT_SCHEMA_VERSION: i64 = 1;
pub const SHADOW_PILOT_SAFETY_POLICY_VERSION: &str = "three-material-shadow-safety-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadowPilotDataMode {
    SyntheticContract,
    RealRestricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadowPilotOutcome {
    Completed,
    CompletedWithSafetyFindings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowPilotSessionRequest {
    pub session_id: String,
    pub evaluated_on: String,
    pub started_at: String,
    pub predictions_generated_at: String,
    pub provider_ref_sha256: String,
    pub model_ref_sha256: String,
    pub provider_config_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowPilotMaterialInput {
    pub manifest: MaterialGoldenManifest,
    pub predictions: MaterialGoldenPredictionSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowPilotMaterialResult {
    pub material_kind: MaterialGoldenKind,
    pub dataset_id: String,
    pub manifest_sha256: String,
    pub predictions_sha256: String,
    pub report: MaterialCalibrationReport,
    pub safety_finding_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowPilotSessionPayload {
    pub schema_version: i64,
    pub session_id: String,
    pub request_sha256: String,
    pub data_mode: ShadowPilotDataMode,
    pub evaluated_on: String,
    pub started_at: String,
    pub predictions_generated_at: String,
    pub completed_at: String,
    pub provider_ref_sha256: String,
    pub model_ref_sha256: String,
    pub provider_config_sha256: String,
    pub safety_policy_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_policy_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rights_evidence_sha256: Option<String>,
    pub material_results: Vec<ShadowPilotMaterialResult>,
    pub outcome: ShadowPilotOutcome,
    pub safety_finding_codes: Vec<String>,
    /// 仅表示真实清单已满足“可计算准确率”的治理条件，不代表指标达标或获准发布。
    pub production_accuracy_metrics_eligible: bool,
    /// 影子试点结果不能自行批准发布。
    pub release_authorized: bool,
    pub content_excluded: bool,
    pub paths_excluded: bool,
    pub student_identity_excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowPilotSessionResult {
    pub payload: ShadowPilotSessionPayload,
    pub result_sha256: String,
}

#[derive(Debug, Serialize)]
struct ShadowPilotMaterialBinding<'a> {
    material_kind: MaterialGoldenKind,
    dataset_id: &'a str,
    manifest_sha256: &'a str,
    predictions_sha256: &'a str,
}

#[derive(Debug, Serialize)]
struct ShadowPilotRequestIdentity<'a> {
    schema_version: i64,
    session_id: &'a str,
    data_mode: ShadowPilotDataMode,
    evaluated_on: &'a str,
    started_at: &'a str,
    predictions_generated_at: &'a str,
    provider_ref_sha256: &'a str,
    model_ref_sha256: &'a str,
    provider_config_sha256: &'a str,
    safety_policy_version: &'a str,
    gate_policy_sha256: Option<&'a str>,
    rights_evidence_sha256: Option<&'a str>,
    materials: Vec<ShadowPilotMaterialBinding<'a>>,
}

fn normalized_sha256(value: &str, field: &str) -> CoreResult<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(format!(
            "{field} 必须是 64 位十六进制 sha256"
        )));
    }
    Ok(value)
}

fn parse_time(value: &str, field: &str) -> CoreResult<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value.trim())
        .map_err(|_| CoreError::Invalid(format!("{field} 必须是 RFC3339")))
}

fn parse_date(value: &str, field: &str) -> CoreResult<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid(format!("{field} 必须是 YYYY-MM-DD")))
}

fn safe_opaque_id(value: &str, field: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(CoreError::Invalid(format!(
            "{field} 必须是不超过 128 字符的不透明内部 ID"
        )));
    }
    Ok(())
}

fn canonical_sha256<T: Serialize>(value: &T, field: &str) -> CoreResult<String> {
    serde_json::to_vec(value)
        .map(|bytes| hashing::sha256_hex(&bytes))
        .map_err(|error| CoreError::Config(format!("{field} 序列化失败：{error}")))
}

impl ShadowPilotSessionRequest {
    pub fn validate(&self, completed_at: &str) -> CoreResult<()> {
        safe_opaque_id(&self.session_id, "session_id")?;
        let evaluated_on = parse_date(&self.evaluated_on, "评估日期")?;
        let started_at = parse_time(&self.started_at, "会话开始时间")?;
        let predictions_generated_at = parse_time(&self.predictions_generated_at, "预测生成时间")?;
        let completed_at = parse_time(completed_at, "会话完成时间")?;
        if predictions_generated_at < started_at || completed_at < predictions_generated_at {
            return Err(CoreError::Invalid(
                "影子试点时间顺序必须为开始→预测生成→完成".into(),
            ));
        }
        if evaluated_on < started_at.date_naive() || evaluated_on > completed_at.date_naive() {
            return Err(CoreError::Invalid(
                "评估日期必须落在影子试点会话时间范围内".into(),
            ));
        }
        normalized_sha256(&self.provider_ref_sha256, "provider 引用")?;
        normalized_sha256(&self.model_ref_sha256, "模型引用")?;
        normalized_sha256(&self.provider_config_sha256, "provider 配置引用")?;
        Ok(())
    }
}

fn safety_findings(kind: MaterialGoldenKind, report: &MaterialCalibrationReport) -> Vec<String> {
    let prefix = match kind {
        MaterialGoldenKind::OrdinaryPaper => "ORDINARY_PAPER",
        MaterialGoldenKind::AnswerSheet => "ANSWER_SHEET",
        MaterialGoldenKind::Dictation => "DICTATION",
    };
    let mut findings = BTreeSet::new();
    if report.unsafe_ready_count > 0 {
        findings.insert(format!("{prefix}:UNSAFE_READY"));
    }
    if report.unit_false_accept_count > 0 {
        findings.insert(format!("{prefix}:FALSE_ACCEPT"));
    }
    if report.unit_wrong_value_count > 0 {
        findings.insert(format!("{prefix}:WRONG_RECOGNIZED_VALUE"));
    }
    if report.missing_prediction_count > 0
        || report.unexpected_prediction_count > 0
        || report.missing_unit_count > 0
        || report.unexpected_unit_count > 0
    {
        findings.insert(format!("{prefix}:INCOMPLETE_OR_UNEXPECTED_OUTPUT"));
    }
    if !report.coverage_gaps.is_empty() {
        findings.insert(format!("{prefix}:COVERAGE_GAP"));
    }
    findings.into_iter().collect()
}

fn request_identity_sha256(payload: &ShadowPilotSessionPayload) -> CoreResult<String> {
    let identity = ShadowPilotRequestIdentity {
        schema_version: payload.schema_version,
        session_id: &payload.session_id,
        data_mode: payload.data_mode,
        evaluated_on: &payload.evaluated_on,
        started_at: &payload.started_at,
        predictions_generated_at: &payload.predictions_generated_at,
        provider_ref_sha256: &payload.provider_ref_sha256,
        model_ref_sha256: &payload.model_ref_sha256,
        provider_config_sha256: &payload.provider_config_sha256,
        safety_policy_version: &payload.safety_policy_version,
        gate_policy_sha256: payload.gate_policy_sha256.as_deref(),
        rights_evidence_sha256: payload.rights_evidence_sha256.as_deref(),
        materials: payload
            .material_results
            .iter()
            .map(|result| ShadowPilotMaterialBinding {
                material_kind: result.material_kind,
                dataset_id: &result.dataset_id,
                manifest_sha256: &result.manifest_sha256,
                predictions_sha256: &result.predictions_sha256,
            })
            .collect(),
    };
    canonical_sha256(&identity, "影子试点请求身份")
}

impl ShadowPilotSessionResult {
    fn new(mut payload: ShadowPilotSessionPayload) -> CoreResult<Self> {
        payload.request_sha256 = request_identity_sha256(&payload)?;
        let result_sha256 = canonical_sha256(&payload, "影子试点结果")?;
        let result = Self {
            payload,
            result_sha256,
        };
        result.validate()?;
        Ok(result)
    }

    pub fn validate(&self) -> CoreResult<()> {
        let payload = &self.payload;
        if payload.schema_version != SHADOW_PILOT_SCHEMA_VERSION {
            return Err(CoreError::Invalid("影子试点 schema 版本不支持".into()));
        }
        safe_opaque_id(&payload.session_id, "session_id")?;
        parse_date(&payload.evaluated_on, "评估日期")?;
        let started_at = parse_time(&payload.started_at, "会话开始时间")?;
        let predictions_generated_at =
            parse_time(&payload.predictions_generated_at, "预测生成时间")?;
        let completed_at = parse_time(&payload.completed_at, "会话完成时间")?;
        if predictions_generated_at < started_at || completed_at < predictions_generated_at {
            return Err(CoreError::Invalid("影子试点结果时间顺序非法".into()));
        }
        normalized_sha256(&payload.provider_ref_sha256, "provider 引用")?;
        normalized_sha256(&payload.model_ref_sha256, "模型引用")?;
        normalized_sha256(&payload.provider_config_sha256, "provider 配置引用")?;
        normalized_sha256(&payload.request_sha256, "影子试点请求 hash")?;
        if let Some(hash) = payload.gate_policy_sha256.as_deref() {
            normalized_sha256(hash, "闸门策略 hash")?;
        }
        if let Some(hash) = payload.rights_evidence_sha256.as_deref() {
            normalized_sha256(hash, "数据权利证据 hash")?;
        }
        if payload.safety_policy_version != SHADOW_PILOT_SAFETY_POLICY_VERSION
            || payload.release_authorized
            || !payload.content_excluded
            || !payload.paths_excluded
            || !payload.student_identity_excluded
        {
            return Err(CoreError::Invalid(
                "影子试点必须使用当前安全策略、排除敏感内容且不得自行授权发布".into(),
            ));
        }
        match payload.data_mode {
            ShadowPilotDataMode::SyntheticContract => {
                if payload.gate_policy_sha256.is_some()
                    || payload.rights_evidence_sha256.is_some()
                    || payload.production_accuracy_metrics_eligible
                {
                    return Err(CoreError::Invalid(
                        "合成合同会话不得借用真实闸门或声明生产指标资格".into(),
                    ));
                }
            }
            ShadowPilotDataMode::RealRestricted => {
                if payload.gate_policy_sha256.is_none() || payload.rights_evidence_sha256.is_none()
                {
                    return Err(CoreError::Invalid(
                        "真实影子会话必须冻结闸门和数据权利证据 hash".into(),
                    ));
                }
            }
        }
        if payload.material_results.len() != 3 {
            return Err(CoreError::Invalid(
                "影子试点必须同时包含普通试卷、答题卡和默写".into(),
            ));
        }
        let mut kinds = BTreeSet::new();
        let mut datasets = BTreeSet::new();
        let mut findings = BTreeSet::new();
        for result in &payload.material_results {
            if !kinds.insert(result.material_kind) || !datasets.insert(result.dataset_id.as_str()) {
                return Err(CoreError::Invalid(
                    "影子试点材料类型或 dataset 身份重复".into(),
                ));
            }
            safe_opaque_id(&result.dataset_id, "dataset_id")?;
            normalized_sha256(&result.manifest_sha256, "黄金集清单 hash")?;
            normalized_sha256(&result.predictions_sha256, "预测集 hash")?;
            if result.report.material_kind != result.material_kind
                || result.report.dataset_id != result.dataset_id
                || result.safety_finding_codes
                    != safety_findings(result.material_kind, &result.report)
            {
                return Err(CoreError::Invalid(
                    "影子试点材料报告身份或安全发现不一致".into(),
                ));
            }
            findings.extend(result.safety_finding_codes.iter().cloned());
        }
        let required = BTreeSet::from([
            MaterialGoldenKind::OrdinaryPaper,
            MaterialGoldenKind::AnswerSheet,
            MaterialGoldenKind::Dictation,
        ]);
        if kinds != required {
            return Err(CoreError::Invalid("影子试点三类材料覆盖不完整".into()));
        }
        let expected_findings = findings.into_iter().collect::<Vec<_>>();
        if payload.safety_finding_codes != expected_findings {
            return Err(CoreError::Invalid("会话安全发现汇总不一致".into()));
        }
        let expected_outcome = if payload.safety_finding_codes.is_empty() {
            ShadowPilotOutcome::Completed
        } else {
            ShadowPilotOutcome::CompletedWithSafetyFindings
        };
        if payload.outcome != expected_outcome {
            return Err(CoreError::Invalid(
                "影子试点 outcome 与安全发现不一致".into(),
            ));
        }
        if payload.production_accuracy_metrics_eligible
            != (payload.data_mode == ShadowPilotDataMode::RealRestricted
                && payload
                    .material_results
                    .iter()
                    .all(|result| result.report.production_accuracy_claim_allowed))
        {
            return Err(CoreError::Invalid("生产指标资格汇总不一致".into()));
        }
        if payload.request_sha256 != request_identity_sha256(payload)? {
            return Err(CoreError::Invalid("影子试点请求 hash 不匹配".into()));
        }
        let expected_result_hash = canonical_sha256(payload, "影子试点结果")?;
        if self.result_sha256 != expected_result_hash {
            return Err(CoreError::Invalid("影子试点结果 hash 不匹配".into()));
        }
        Ok(())
    }
}

/// 一次只读完成三材料评估。合成和真实数据不能混用；真实数据必须携带当前有效闸门。
pub fn evaluate_shadow_pilot_session(
    request: &ShadowPilotSessionRequest,
    mut inputs: Vec<ShadowPilotMaterialInput>,
    gate: Option<&PilotDataGateManifest>,
    rights_evidence: Option<&PilotDataRightsEvidenceBundle>,
    completed_at: &str,
) -> CoreResult<ShadowPilotSessionResult> {
    request.validate(completed_at)?;
    if inputs.len() != 3 {
        return Err(CoreError::Invalid(
            "影子试点必须一次提供普通试卷、答题卡和默写三类输入".into(),
        ));
    }
    inputs.sort_by_key(|input| input.manifest.material_kind);
    let mut kinds = BTreeSet::new();
    let mut dataset_ids = BTreeSet::new();
    let real_count = inputs
        .iter()
        .filter(|input| input.manifest.contains_real_data())
        .count();
    let data_mode = match real_count {
        0 => ShadowPilotDataMode::SyntheticContract,
        3 => ShadowPilotDataMode::RealRestricted,
        _ => {
            return Err(CoreError::Invalid(
                "影子试点不得混用合成合同集和真实材料集".into(),
            ))
        }
    };
    match data_mode {
        ShadowPilotDataMode::SyntheticContract => {
            if gate.is_some() || rights_evidence.is_some() {
                return Err(CoreError::Invalid(
                    "合成合同会话不得借用真实材料闸门".into(),
                ));
            }
        }
        ShadowPilotDataMode::RealRestricted => {
            if gate.is_none() || rights_evidence.is_none() {
                return Err(CoreError::Invalid(
                    "真实影子会话必须提供闸门和数据权利证据".into(),
                ));
            }
        }
    }

    let gate_policy_sha256 = gate.map(PilotDataGateManifest::policy_sha256).transpose()?;
    let rights_evidence_sha256 = rights_evidence
        .map(PilotDataRightsEvidenceBundle::evidence_sha256)
        .transpose()?;
    let mut material_results = Vec::with_capacity(3);
    for input in inputs {
        input.manifest.validate()?;
        input.predictions.validate_for(&input.manifest)?;
        safe_opaque_id(&input.manifest.dataset_id, "dataset_id")?;
        if !kinds.insert(input.manifest.material_kind)
            || !dataset_ids.insert(input.manifest.dataset_id.clone())
        {
            return Err(CoreError::Invalid(
                "影子试点材料类型或 dataset 身份重复".into(),
            ));
        }
        let manifest_sha256 = canonical_sha256(&input.manifest, "黄金集清单")?;
        let predictions_sha256 = canonical_sha256(&input.predictions, "预测集")?;
        let report = match data_mode {
            ShadowPilotDataMode::SyntheticContract => {
                evaluate_material_calibration(&input.manifest, &input.predictions)?
            }
            ShadowPilotDataMode::RealRestricted => evaluate_real_material_calibration(
                &input.manifest,
                &input.predictions,
                gate.expect("真实模式已校验 gate"),
                rights_evidence.expect("真实模式已校验 rights evidence"),
                &request.evaluated_on,
            )?,
        };
        material_results.push(ShadowPilotMaterialResult {
            material_kind: input.manifest.material_kind,
            dataset_id: input.manifest.dataset_id,
            manifest_sha256,
            predictions_sha256,
            safety_finding_codes: safety_findings(input.manifest.material_kind, &report),
            report,
        });
    }
    let required = BTreeSet::from([
        MaterialGoldenKind::OrdinaryPaper,
        MaterialGoldenKind::AnswerSheet,
        MaterialGoldenKind::Dictation,
    ]);
    if kinds != required {
        return Err(CoreError::Invalid("影子试点三类材料覆盖不完整".into()));
    }
    let safety_finding_codes = material_results
        .iter()
        .flat_map(|result| result.safety_finding_codes.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let outcome = if safety_finding_codes.is_empty() {
        ShadowPilotOutcome::Completed
    } else {
        ShadowPilotOutcome::CompletedWithSafetyFindings
    };
    let production_accuracy_metrics_eligible = data_mode == ShadowPilotDataMode::RealRestricted
        && material_results
            .iter()
            .all(|result| result.report.production_accuracy_claim_allowed);
    ShadowPilotSessionResult::new(ShadowPilotSessionPayload {
        schema_version: SHADOW_PILOT_SCHEMA_VERSION,
        session_id: request.session_id.clone(),
        request_sha256: String::new(),
        data_mode,
        evaluated_on: request.evaluated_on.clone(),
        started_at: request.started_at.clone(),
        predictions_generated_at: request.predictions_generated_at.clone(),
        completed_at: completed_at.to_owned(),
        provider_ref_sha256: normalized_sha256(&request.provider_ref_sha256, "provider 引用")?,
        model_ref_sha256: normalized_sha256(&request.model_ref_sha256, "模型引用")?,
        provider_config_sha256: normalized_sha256(
            &request.provider_config_sha256,
            "provider 配置引用",
        )?,
        safety_policy_version: SHADOW_PILOT_SAFETY_POLICY_VERSION.into(),
        gate_policy_sha256,
        rights_evidence_sha256,
        material_results,
        outcome,
        safety_finding_codes,
        production_accuracy_metrics_eligible,
        release_authorized: false,
        content_excluded: true,
        paths_excluded: true,
        student_identity_excluded: true,
    })
}

fn read_existing_result(path: &Path) -> CoreResult<ShadowPilotSessionResult> {
    let metadata = fs::symlink_metadata(path).map_err(|error| CoreError::Io(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CoreError::Invalid(
            "影子试点结果目标必须是普通文件，不能是符号链接".into(),
        ));
    }
    let existing: ShadowPilotSessionResult =
        serde_json::from_slice(&fs::read(path).map_err(|error| CoreError::Io(error.to_string()))?)
            .map_err(|error| CoreError::Invalid(format!("既有影子试点结果 JSON 非法：{error}")))?;
    existing.validate()?;
    Ok(existing)
}

/// 不覆盖既有不同请求。先写同目录临时普通文件并 fsync，再用 hard-link 无覆盖提交。
pub fn write_shadow_pilot_result_once(
    path: &Path,
    result: &ShadowPilotSessionResult,
) -> CoreResult<ShadowPilotSessionResult> {
    result.validate()?;
    if path.exists() {
        let existing = read_existing_result(path)?;
        if existing.payload.request_sha256 == result.payload.request_sha256 {
            return Ok(existing);
        }
        return Err(CoreError::Invalid(
            "同一输出位置已存在不同影子试点请求，拒绝覆盖".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::Invalid("影子试点输出缺少父目录".into()))?;
    let parent_meta =
        fs::symlink_metadata(parent).map_err(|error| CoreError::Io(error.to_string()))?;
    if parent_meta.file_type().is_symlink() || !parent_meta.is_dir() {
        return Err(CoreError::Invalid(
            "影子试点输出父目录必须是已存在的普通目录".into(),
        ));
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| CoreError::Invalid("影子试点输出文件名非法".into()))?;
    let pending_path: PathBuf =
        parent.join(format!(".{file_name}.pending-{}", ids::new_public_id()));
    let bytes = serde_json::to_vec_pretty(result)
        .map_err(|error| CoreError::Config(format!("影子试点结果序列化失败：{error}")))?;
    let write_result = (|| -> CoreResult<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending_path)
            .map_err(|error| CoreError::Io(error.to_string()))?;
        file.write_all(&bytes)
            .map_err(|error| CoreError::Io(error.to_string()))?;
        file.sync_all()
            .map_err(|error| CoreError::Io(error.to_string()))?;
        fs::hard_link(&pending_path, path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CoreError::Invalid("影子试点输出已被另一请求占用".into())
            } else {
                CoreError::Io(error.to_string())
            }
        })?;
        Ok(())
    })();
    let _ = fs::remove_file(&pending_path);
    if let Err(error) = write_result {
        if path.exists() {
            let existing = read_existing_result(path)?;
            if existing.payload.request_sha256 == result.payload.request_sha256 {
                return Ok(existing);
            }
        }
        return Err(error);
    }
    read_existing_result(path)
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
        use crate::material_golden::{MaterialGoldenPrediction, MaterialGoldenPredictionUnit};

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

    fn inputs() -> Vec<ShadowPilotMaterialInput> {
        manifests()
            .into_iter()
            .map(|manifest| ShadowPilotMaterialInput {
                predictions: perfect_predictions(&manifest),
                manifest,
            })
            .collect()
    }

    fn request() -> ShadowPilotSessionRequest {
        ShadowPilotSessionRequest {
            session_id: "shadow-contract-session-001".into(),
            evaluated_on: "2026-07-16".into(),
            started_at: "2026-07-16T08:00:00Z".into(),
            predictions_generated_at: "2026-07-16T08:05:00Z".into(),
            provider_ref_sha256: "a".repeat(64),
            model_ref_sha256: "b".repeat(64),
            provider_config_sha256: "c".repeat(64),
        }
    }

    #[test]
    fn synthetic_three_material_session_is_hash_only_and_never_authorizes_release() {
        let result =
            evaluate_shadow_pilot_session(&request(), inputs(), None, None, "2026-07-16T08:10:00Z")
                .unwrap();
        assert_eq!(
            result.payload.data_mode,
            ShadowPilotDataMode::SyntheticContract
        );
        assert_eq!(result.payload.outcome, ShadowPilotOutcome::Completed);
        assert_eq!(result.payload.material_results.len(), 3);
        assert!(result.payload.safety_finding_codes.is_empty());
        assert!(!result.payload.production_accuracy_metrics_eligible);
        assert!(!result.payload.release_authorized);
        result.validate().unwrap();
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("/Users/"));
        assert!(!json.contains("student_name"));
        assert!(!json.contains("answer_text"));
    }

    #[test]
    fn all_three_distinct_material_kinds_are_required_and_cannot_mix_real_data() {
        let mut missing = inputs();
        missing.pop();
        assert!(evaluate_shadow_pilot_session(
            &request(),
            missing,
            None,
            None,
            "2026-07-16T08:10:00Z"
        )
        .is_err());

        let mut mixed = inputs();
        let gate = crate::pilot_data_gate::tests::approved_real_gate();
        mixed[0].manifest.governance.storage_scope =
            crate::material_golden::MaterialGoldenStorageScope::LocalRestricted;
        mixed[0].manifest.governance.pilot_gate_id = Some(gate.gate_id.clone());
        mixed[0].manifest.governance.pilot_gate_policy_sha256 = Some(gate.policy_sha256().unwrap());
        for case in &mut mixed[0].manifest.cases {
            case.source_kind = crate::material_golden::MaterialGoldenSourceKind::RealPhoto;
            case.artifact_sha256 = Some("d".repeat(64));
        }
        assert!(evaluate_shadow_pilot_session(
            &request(),
            mixed,
            None,
            None,
            "2026-07-16T08:10:00Z"
        )
        .is_err());
    }

    #[test]
    fn unsafe_ready_and_wrong_recognized_value_become_session_safety_findings() {
        let mut inputs = inputs();
        let ordinary = inputs
            .iter_mut()
            .find(|input| input.manifest.material_kind == MaterialGoldenKind::OrdinaryPaper)
            .unwrap();
        ordinary.predictions.predictions[0].predicted_units[0].predicted_value_sha256 =
            Some("f".repeat(64));
        ordinary.predictions.predictions[1].predicted_route =
            crate::material_golden::MaterialGoldenRoute::ReadyForBatchConfirm;
        let result =
            evaluate_shadow_pilot_session(&request(), inputs, None, None, "2026-07-16T08:10:00Z")
                .unwrap();
        assert_eq!(
            result.payload.outcome,
            ShadowPilotOutcome::CompletedWithSafetyFindings
        );
        assert_eq!(
            result.payload.safety_finding_codes,
            vec![
                "ORDINARY_PAPER:UNSAFE_READY",
                "ORDINARY_PAPER:WRONG_RECOGNIZED_VALUE"
            ]
        );
        assert!(!result.payload.release_authorized);
    }

    #[test]
    fn real_session_requires_current_gate_and_rights_evidence_for_all_materials() {
        use crate::material_golden::{MaterialGoldenSourceKind, MaterialGoldenStorageScope};
        use crate::pilot_data_gate::tests::{approved_real_gate, approved_rights_evidence};

        let gate = approved_real_gate();
        let rights = approved_rights_evidence();
        let gate_hash = gate.policy_sha256().unwrap();
        let mut inputs = inputs();
        for input in &mut inputs {
            input.manifest.governance.storage_scope = MaterialGoldenStorageScope::LocalRestricted;
            input.manifest.governance.pilot_gate_id = Some(gate.gate_id.clone());
            input.manifest.governance.pilot_gate_policy_sha256 = Some(gate_hash.clone());
            input.manifest.production_accuracy_claim_allowed = true;
            for case in &mut input.manifest.cases {
                case.source_kind = MaterialGoldenSourceKind::RealPhoto;
                case.artifact_sha256 = Some(hashing::sha256_hex(case.case_id.as_bytes()));
            }
        }
        assert!(evaluate_shadow_pilot_session(
            &request(),
            inputs.clone(),
            None,
            None,
            "2026-07-16T08:10:00Z"
        )
        .is_err());
        let result = evaluate_shadow_pilot_session(
            &request(),
            inputs,
            Some(&gate),
            Some(&rights),
            "2026-07-16T08:10:00Z",
        )
        .unwrap();
        assert_eq!(
            result.payload.data_mode,
            ShadowPilotDataMode::RealRestricted
        );
        assert!(result.payload.production_accuracy_metrics_eligible);
        assert!(!result.payload.release_authorized);
        assert_eq!(result.payload.gate_policy_sha256, Some(gate_hash));
        assert_eq!(
            result.payload.rights_evidence_sha256,
            Some(rights.evidence_sha256().unwrap())
        );
    }

    #[test]
    fn result_write_is_atomic_idempotent_and_never_overwrites_another_request() {
        let root =
            std::env::temp_dir().join(format!("jiaofu-shadow-pilot-{}", ids::new_public_id()));
        fs::create_dir_all(&root).unwrap();
        let output = root.join("result.json");
        let result =
            evaluate_shadow_pilot_session(&request(), inputs(), None, None, "2026-07-16T08:10:00Z")
                .unwrap();
        let written = write_shadow_pilot_result_once(&output, &result).unwrap();
        assert_eq!(written, result);

        let later_same_request =
            evaluate_shadow_pilot_session(&request(), inputs(), None, None, "2026-07-16T08:11:00Z")
                .unwrap();
        let replay = write_shadow_pilot_result_once(&output, &later_same_request).unwrap();
        assert_eq!(replay, result);

        let mut changed_request = request();
        changed_request.provider_config_sha256 = "d".repeat(64);
        let changed = evaluate_shadow_pilot_session(
            &changed_request,
            inputs(),
            None,
            None,
            "2026-07-16T08:11:00Z",
        )
        .unwrap();
        assert!(write_shadow_pilot_result_once(&output, &changed).is_err());
        assert_eq!(read_existing_result(&output).unwrap(), result);
        fs::remove_dir_all(root).unwrap();
    }
}
