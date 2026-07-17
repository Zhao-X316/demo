//! 真实三材料影子试点的 hash-only 总验收包。
//!
//! 本层只把已批准数据闸门、数据权利证据、三材料机器影子结果与老师成对
//! 耗时报告绑定到同一会话。它不读取原图、OCR/答案正文、学生身份或文件路径，
//! 也不能授权发布。任一 hash、会话、数据集或时间顺序不一致都 fail-closed。

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use suite_core::domain::{hashing, ids};
use suite_core::error::{CoreError, CoreResult};

use crate::material_golden::MaterialGoldenKind;
use crate::pilot_data_gate::{PilotDataGateManifest, PilotGateScopeKind};
use crate::pilot_data_rights::PilotDataRightsEvidenceBundle;
use crate::shadow_pilot::{
    ShadowPilotDataMode, ShadowPilotSessionResult, SHADOW_PILOT_SAFETY_POLICY_VERSION,
};
use crate::teacher_shadow::{TeacherShadowMaterialResult, TeacherShadowReport};

pub const PILOT_EVIDENCE_BUNDLE_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PilotEvidenceMaterialBinding {
    pub material_kind: MaterialGoldenKind,
    pub dataset_id: String,
    pub manifest_sha256: String,
    pub predictions_sha256: String,
    pub calibration_report_sha256: String,
    pub teacher_sample_scope_sha256s: Vec<String>,
    pub teacher_pair_count: u64,
    pub teacher_submission_count: u64,
    pub corrected_machine_decision_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PilotEvidenceBundlePayload {
    pub schema_version: i64,
    pub session_id: String,
    pub assembled_at: String,
    pub evaluated_on: String,
    pub gate_id: String,
    pub gate_policy_sha256: String,
    pub rights_evidence_sha256: String,
    pub shadow_request_sha256: String,
    pub shadow_result_sha256: String,
    pub teacher_report_sha256: String,
    pub provider_ref_sha256: String,
    pub model_ref_sha256: String,
    pub provider_config_sha256: String,
    pub safety_policy_version: String,
    pub material_bindings: Vec<PilotEvidenceMaterialBinding>,
    pub safety_finding_codes: Vec<String>,
    pub review_reason_codes: Vec<String>,
    pub production_accuracy_metrics_eligible: bool,
    /// 只表示证据可以交给老师/工程人员进行阈值评审，不是发布放行。
    pub threshold_review_ready: bool,
    pub quality_review_required: bool,
    pub release_authorized: bool,
    pub content_excluded: bool,
    pub paths_excluded: bool,
    pub student_identity_excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PilotEvidenceBundle {
    pub payload: PilotEvidenceBundlePayload,
    pub bundle_sha256: String,
}

fn parse_time(value: &str, field: &str) -> CoreResult<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value.trim())
        .map_err(|_| CoreError::Invalid(format!("{field} 必须是 RFC3339")))
}

fn opaque_id(value: &str, field: &str) -> CoreResult<()> {
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

fn normalized_sha256(value: &str, field: &str) -> CoreResult<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(format!(
            "{field} 必须是 64 位十六进制 sha256"
        )));
    }
    Ok(value)
}

fn canonical_sha256<T: Serialize>(value: &T, field: &str) -> CoreResult<String> {
    serde_json::to_vec(value)
        .map(|bytes| hashing::sha256_hex(&bytes))
        .map_err(|error| CoreError::Config(format!("{field} 序列化失败：{error}")))
}

fn payload_sha256(payload: &PilotEvidenceBundlePayload) -> CoreResult<String> {
    canonical_sha256(payload, "试点总验收包")
}

impl PilotEvidenceBundle {
    pub fn validate(&self) -> CoreResult<()> {
        let payload = &self.payload;
        if payload.schema_version != PILOT_EVIDENCE_BUNDLE_SCHEMA_VERSION {
            return Err(CoreError::Invalid("试点总验收包 schema 版本不支持".into()));
        }
        opaque_id(&payload.session_id, "session_id")?;
        opaque_id(&payload.gate_id, "gate_id")?;
        parse_time(&payload.assembled_at, "验收包生成时间")?;
        for (value, field) in [
            (&payload.gate_policy_sha256, "闸门策略 hash"),
            (&payload.rights_evidence_sha256, "数据权利证据 hash"),
            (&payload.shadow_request_sha256, "影子请求 hash"),
            (&payload.shadow_result_sha256, "影子结果 hash"),
            (&payload.teacher_report_sha256, "老师报告 hash"),
            (&payload.provider_ref_sha256, "provider 引用"),
            (&payload.model_ref_sha256, "模型引用"),
            (&payload.provider_config_sha256, "provider 配置引用"),
        ] {
            normalized_sha256(value, field)?;
        }
        if payload.safety_policy_version != SHADOW_PILOT_SAFETY_POLICY_VERSION
            || !payload.production_accuracy_metrics_eligible
            || !payload.threshold_review_ready
            || payload.release_authorized
            || !payload.content_excluded
            || !payload.paths_excluded
            || !payload.student_identity_excluded
        {
            return Err(CoreError::Invalid(
                "试点总验收包必须绑定真实指标资格、排除敏感内容且不得授权发布".into(),
            ));
        }
        if payload.material_bindings.len() != 3 {
            return Err(CoreError::Invalid("试点总验收包必须包含三类材料".into()));
        }
        let mut kinds = BTreeSet::new();
        let mut previous_kind = None;
        for material in &payload.material_bindings {
            if !kinds.insert(material.material_kind) {
                return Err(CoreError::Invalid("试点材料类型不能重复".into()));
            }
            if previous_kind.is_some_and(|previous| previous >= material.material_kind) {
                return Err(CoreError::Invalid("试点材料绑定必须按类型稳定排序".into()));
            }
            previous_kind = Some(material.material_kind);
            opaque_id(&material.dataset_id, "dataset_id")?;
            normalized_sha256(&material.manifest_sha256, "黄金集清单 hash")?;
            normalized_sha256(&material.predictions_sha256, "预测集 hash")?;
            normalized_sha256(&material.calibration_report_sha256, "校准报告 hash")?;
            if material.teacher_pair_count == 0
                || material.teacher_submission_count == 0
                || material.teacher_sample_scope_sha256s.is_empty()
            {
                return Err(CoreError::Invalid(
                    "每类材料必须有老师成对观察、作业量和抽样范围".into(),
                ));
            }
            let sample_hashes = material
                .teacher_sample_scope_sha256s
                .iter()
                .map(|value| normalized_sha256(value, "老师抽样范围 hash"))
                .collect::<CoreResult<BTreeSet<_>>>()?;
            if sample_hashes.into_iter().collect::<Vec<_>>()
                != material.teacher_sample_scope_sha256s
            {
                return Err(CoreError::Invalid(
                    "老师抽样范围 hash 必须排序且不重复".into(),
                ));
            }
        }
        let required = BTreeSet::from([
            MaterialGoldenKind::OrdinaryPaper,
            MaterialGoldenKind::AnswerSheet,
            MaterialGoldenKind::Dictation,
        ]);
        if kinds != required {
            return Err(CoreError::Invalid("试点三类材料覆盖不完整".into()));
        }
        let mut safety = payload.safety_finding_codes.clone();
        safety.sort();
        safety.dedup();
        if safety != payload.safety_finding_codes {
            return Err(CoreError::Invalid("试点安全发现码必须排序且不重复".into()));
        }
        let mut review = payload.review_reason_codes.clone();
        review.sort();
        review.dedup();
        if review != payload.review_reason_codes
            || payload.quality_review_required == payload.review_reason_codes.is_empty()
        {
            return Err(CoreError::Invalid("质量复核标记与原因码不一致".into()));
        }
        if normalized_sha256(&self.bundle_sha256, "验收包 hash")? != payload_sha256(payload)? {
            return Err(CoreError::Invalid("试点总验收包 hash 不匹配".into()));
        }
        Ok(())
    }
}

fn teacher_by_material(
    teacher_report: &TeacherShadowReport,
) -> BTreeMap<MaterialGoldenKind, &TeacherShadowMaterialResult> {
    teacher_report
        .payload
        .material_results
        .iter()
        .map(|result| (result.material_kind, result))
        .collect()
}

/// 把真实数据闸门、机器三材料结果和老师成对观察绑定为一份不可发布的证据包。
pub fn assemble_pilot_evidence_bundle(
    gate: &PilotDataGateManifest,
    rights_evidence: &PilotDataRightsEvidenceBundle,
    shadow_result: &ShadowPilotSessionResult,
    teacher_report: &TeacherShadowReport,
    assembled_at: &str,
) -> CoreResult<PilotEvidenceBundle> {
    gate.validate()?;
    rights_evidence.validate()?;
    shadow_result.validate()?;
    teacher_report.validate()?;
    if gate.scope_kind != PilotGateScopeKind::RealPilot
        || shadow_result.payload.data_mode != ShadowPilotDataMode::RealRestricted
        || !shadow_result.payload.production_accuracy_metrics_eligible
    {
        return Err(CoreError::Invalid(
            "总验收包只接受已通过真实数据闸门的三材料影子会话".into(),
        ));
    }
    let gate_report =
        gate.evaluate_with_rights_evidence(&shadow_result.payload.evaluated_on, rights_evidence)?;
    if !gate_report.real_data_allowed {
        return Err(CoreError::Invalid(format!(
            "试点数据闸门未放行：{}",
            gate_report.denial_reasons.join(",")
        )));
    }
    let gate_hash = gate.policy_sha256()?;
    let rights_hash = rights_evidence.evidence_sha256()?;
    if shadow_result.payload.gate_policy_sha256.as_deref() != Some(gate_hash.as_str())
        || shadow_result.payload.rights_evidence_sha256.as_deref() != Some(rights_hash.as_str())
    {
        return Err(CoreError::Invalid(
            "影子会话与当前数据闸门或权利证据 hash 不一致".into(),
        ));
    }
    if teacher_report.payload.session_id != shadow_result.payload.session_id
        || !teacher_report
            .payload
            .shadow_result_sha256
            .eq_ignore_ascii_case(&shadow_result.result_sha256)
    {
        return Err(CoreError::Invalid(
            "老师观察与机器影子会话或结果 hash 不一致".into(),
        ));
    }
    let assembled = parse_time(assembled_at, "验收包生成时间")?;
    if assembled < parse_time(&shadow_result.payload.completed_at, "影子会话完成时间")?
        || assembled < parse_time(&teacher_report.payload.generated_at, "老师报告生成时间")?
    {
        return Err(CoreError::Invalid(
            "总验收包不能早于机器影子会话或老师报告".into(),
        ));
    }
    let teachers = teacher_by_material(teacher_report);
    let mut material_bindings = Vec::with_capacity(3);
    for machine in &shadow_result.payload.material_results {
        let teacher = teachers
            .get(&machine.material_kind)
            .copied()
            .ok_or_else(|| CoreError::Invalid("老师报告缺少对应材料".into()))?;
        if teacher.dataset_ids != vec![machine.dataset_id.clone()] {
            return Err(CoreError::Invalid(format!(
                "{:?} 的老师观察与机器影子数据集不一致",
                machine.material_kind
            )));
        }
        if teacher.metrics.pair_count == 0 || teacher.metrics.submission_count == 0 {
            return Err(CoreError::Invalid("老师影子报告缺少有效成对工作量".into()));
        }
        material_bindings.push(PilotEvidenceMaterialBinding {
            material_kind: machine.material_kind,
            dataset_id: machine.dataset_id.clone(),
            manifest_sha256: machine.manifest_sha256.clone(),
            predictions_sha256: machine.predictions_sha256.clone(),
            calibration_report_sha256: canonical_sha256(&machine.report, "单材料校准报告")?,
            teacher_sample_scope_sha256s: teacher.sample_scope_sha256s.clone(),
            teacher_pair_count: teacher.metrics.pair_count,
            teacher_submission_count: teacher.metrics.submission_count,
            corrected_machine_decision_count: teacher
                .metrics
                .assisted_corrected_machine_decision_count,
        });
    }
    material_bindings.sort_by_key(|binding| binding.material_kind);

    let mut review_reasons = BTreeSet::new();
    if !shadow_result.payload.safety_finding_codes.is_empty() {
        review_reasons.insert("MACHINE_SAFETY_FINDINGS".to_owned());
    }
    if teacher_report
        .payload
        .overall
        .assisted_corrected_machine_decision_count
        > 0
    {
        review_reasons.insert("TEACHER_CORRECTED_MACHINE_DECISIONS".to_owned());
    }
    if teacher_report
        .payload
        .overall
        .completed_without_help_pair_count
        < teacher_report.payload.overall.pair_count
    {
        review_reasons.insert("TEACHER_HELP_REQUIRED".to_owned());
    }
    let quality_review_required = !review_reasons.is_empty();
    let payload = PilotEvidenceBundlePayload {
        schema_version: PILOT_EVIDENCE_BUNDLE_SCHEMA_VERSION,
        session_id: shadow_result.payload.session_id.clone(),
        assembled_at: assembled_at.to_owned(),
        evaluated_on: shadow_result.payload.evaluated_on.clone(),
        gate_id: gate.gate_id.clone(),
        gate_policy_sha256: gate_hash,
        rights_evidence_sha256: rights_hash,
        shadow_request_sha256: shadow_result.payload.request_sha256.clone(),
        shadow_result_sha256: shadow_result.result_sha256.clone(),
        teacher_report_sha256: teacher_report.report_sha256.clone(),
        provider_ref_sha256: shadow_result.payload.provider_ref_sha256.clone(),
        model_ref_sha256: shadow_result.payload.model_ref_sha256.clone(),
        provider_config_sha256: shadow_result.payload.provider_config_sha256.clone(),
        safety_policy_version: shadow_result.payload.safety_policy_version.clone(),
        material_bindings,
        safety_finding_codes: shadow_result.payload.safety_finding_codes.clone(),
        review_reason_codes: review_reasons.into_iter().collect(),
        production_accuracy_metrics_eligible: true,
        threshold_review_ready: true,
        quality_review_required,
        release_authorized: false,
        content_excluded: true,
        paths_excluded: true,
        student_identity_excluded: true,
    };
    let bundle = PilotEvidenceBundle {
        bundle_sha256: payload_sha256(&payload)?,
        payload,
    };
    bundle.validate()?;
    Ok(bundle)
}

fn read_existing(path: &Path) -> CoreResult<PilotEvidenceBundle> {
    let metadata = fs::symlink_metadata(path).map_err(|error| CoreError::Io(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CoreError::Invalid(
            "试点总验收包目标必须是普通文件，不能是符号链接".into(),
        ));
    }
    let bundle: PilotEvidenceBundle =
        serde_json::from_slice(&fs::read(path).map_err(|error| CoreError::Io(error.to_string()))?)
            .map_err(|error| CoreError::Invalid(format!("既有试点总验收包非法：{error}")))?;
    bundle.validate()?;
    Ok(bundle)
}

pub fn write_pilot_evidence_bundle_once(
    path: &Path,
    bundle: &PilotEvidenceBundle,
) -> CoreResult<PilotEvidenceBundle> {
    bundle.validate()?;
    if path.exists() {
        let existing = read_existing(path)?;
        if existing.bundle_sha256 == bundle.bundle_sha256 {
            return Ok(existing);
        }
        return Err(CoreError::Invalid(
            "同一输出位置已存在不同试点总验收包，拒绝覆盖".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::Invalid("试点总验收包缺少父目录".into()))?;
    let parent_meta =
        fs::symlink_metadata(parent).map_err(|error| CoreError::Io(error.to_string()))?;
    if parent_meta.file_type().is_symlink() || !parent_meta.is_dir() {
        return Err(CoreError::Invalid(
            "试点总验收包父目录必须是已存在的普通目录".into(),
        ));
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| CoreError::Invalid("试点总验收包文件名非法".into()))?;
    let pending_path: PathBuf =
        parent.join(format!(".{file_name}.pending-{}", ids::new_public_id()));
    let bytes = serde_json::to_vec_pretty(bundle)
        .map_err(|error| CoreError::Config(format!("试点总验收包序列化失败：{error}")))?;
    let write_result = (|| -> CoreResult<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&pending_path)
            .map_err(|error| CoreError::Io(error.to_string()))?;
        file.write_all(&bytes)
            .map_err(|error| CoreError::Io(error.to_string()))?;
        file.sync_all()
            .map_err(|error| CoreError::Io(error.to_string()))?;
        fs::hard_link(&pending_path, path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CoreError::Invalid("试点总验收包输出已被另一请求占用".into())
            } else {
                CoreError::Io(error.to_string())
            }
        })?;
        Ok(())
    })();
    let _ = fs::remove_file(&pending_path);
    if let Err(error) = write_result {
        if path.exists() {
            let existing = read_existing(path)?;
            if existing.bundle_sha256 == bundle.bundle_sha256 {
                return Ok(existing);
            }
        }
        return Err(error);
    }
    read_existing(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material_golden::{
        MaterialGoldenManifest, MaterialGoldenPrediction, MaterialGoldenPredictionSet,
        MaterialGoldenPredictionUnit, MaterialGoldenSourceKind, MaterialGoldenStorageScope,
    };
    use crate::pilot_data_gate::tests::{approved_real_gate, approved_rights_evidence};
    use crate::shadow_pilot::{
        evaluate_shadow_pilot_session, ShadowPilotMaterialInput, ShadowPilotSessionRequest,
    };
    use crate::teacher_shadow::{
        evaluate_teacher_shadow, TeacherShadowObservation, TeacherShadowObservationSet,
        TeacherShadowWorkflow, TEACHER_SHADOW_SCHEMA_VERSION,
    };

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

    fn real_shadow() -> (
        PilotDataGateManifest,
        PilotDataRightsEvidenceBundle,
        ShadowPilotSessionResult,
    ) {
        let gate = approved_real_gate();
        let rights = approved_rights_evidence();
        let gate_hash = gate.policy_sha256().unwrap();
        let inputs = manifests()
            .into_iter()
            .map(|mut manifest| {
                manifest.governance.storage_scope = MaterialGoldenStorageScope::LocalRestricted;
                manifest.governance.pilot_gate_id = Some(gate.gate_id.clone());
                manifest.governance.pilot_gate_policy_sha256 = Some(gate_hash.clone());
                manifest.production_accuracy_claim_allowed = true;
                for case in &mut manifest.cases {
                    case.source_kind = MaterialGoldenSourceKind::RealPhoto;
                    case.artifact_sha256 = Some(hashing::sha256_hex(case.case_id.as_bytes()));
                }
                ShadowPilotMaterialInput {
                    predictions: perfect_predictions(&manifest),
                    manifest,
                }
            })
            .collect();
        let request = ShadowPilotSessionRequest {
            session_id: "pilot-evidence-session-001".into(),
            evaluated_on: "2026-07-16".into(),
            started_at: "2026-07-16T08:00:00Z".into(),
            predictions_generated_at: "2026-07-16T08:05:00Z".into(),
            provider_ref_sha256: "a".repeat(64),
            model_ref_sha256: "b".repeat(64),
            provider_config_sha256: "c".repeat(64),
        };
        let shadow = evaluate_shadow_pilot_session(
            &request,
            inputs,
            Some(&gate),
            Some(&rights),
            "2026-07-16T08:10:00Z",
        )
        .unwrap();
        (gate, rights, shadow)
    }

    fn teacher_report(shadow: &ShadowPilotSessionResult) -> TeacherShadowReport {
        let mut observations = Vec::new();
        for material in &shadow.payload.material_results {
            let group = match material.material_kind {
                MaterialGoldenKind::OrdinaryPaper => "ordinary",
                MaterialGoldenKind::AnswerSheet => "answer-sheet",
                MaterialGoldenKind::Dictation => "dictation",
            };
            for workflow in [
                TeacherShadowWorkflow::ManualBaseline,
                TeacherShadowWorkflow::AssistedReview,
            ] {
                observations.push(TeacherShadowObservation {
                    observation_id: format!(
                        "obs-{group}-{}",
                        if workflow == TeacherShadowWorkflow::ManualBaseline {
                            "manual"
                        } else {
                            "assisted"
                        }
                    ),
                    comparison_group_id: format!("group-{group}"),
                    material_kind: material.material_kind,
                    workflow,
                    dataset_id: material.dataset_id.clone(),
                    sample_scope_sha256: hashing::sha256_hex(group.as_bytes()),
                    teacher_ref_sha256: "d".repeat(64),
                    shadow_result_sha256: (workflow == TeacherShadowWorkflow::AssistedReview)
                        .then(|| shadow.result_sha256.clone()),
                    started_at: "2026-07-16T08:15:00Z".into(),
                    completed_at: "2026-07-16T09:00:00Z".into(),
                    submission_count: 40,
                    page_count: 40,
                    item_count: 400,
                    teacher_active_seconds: if workflow == TeacherShadowWorkflow::ManualBaseline {
                        2400
                    } else {
                        1200
                    },
                    machine_wait_seconds: if workflow == TeacherShadowWorkflow::AssistedReview {
                        300
                    } else {
                        0
                    },
                    teacher_action_count: if workflow == TeacherShadowWorkflow::AssistedReview {
                        30
                    } else {
                        400
                    },
                    exception_review_count: if workflow == TeacherShadowWorkflow::AssistedReview {
                        20
                    } else {
                        0
                    },
                    corrected_machine_decision_count: if workflow
                        == TeacherShadowWorkflow::AssistedReview
                    {
                        4
                    } else {
                        0
                    },
                    completed_without_help: true,
                });
            }
        }
        evaluate_teacher_shadow(
            &TeacherShadowObservationSet {
                schema_version: TEACHER_SHADOW_SCHEMA_VERSION,
                session_id: shadow.payload.session_id.clone(),
                observations,
            },
            "2026-07-16T09:05:00Z",
        )
        .unwrap()
    }

    #[test]
    fn real_three_material_reports_assemble_into_one_hash_only_bundle() {
        let (gate, rights, shadow) = real_shadow();
        let teacher = teacher_report(&shadow);
        let bundle = assemble_pilot_evidence_bundle(
            &gate,
            &rights,
            &shadow,
            &teacher,
            "2026-07-16T09:10:00Z",
        )
        .unwrap();
        assert_eq!(bundle.payload.material_bindings.len(), 3);
        assert!(bundle.payload.threshold_review_ready);
        assert!(bundle.payload.quality_review_required);
        assert_eq!(
            bundle.payload.review_reason_codes,
            vec!["TEACHER_CORRECTED_MACHINE_DECISIONS"]
        );
        assert!(!bundle.payload.release_authorized);
        let json = serde_json::to_string(&bundle).unwrap();
        assert!(!json.contains("/Users/"));
        assert!(!json.contains("student_name"));
        assert!(!json.contains("answer_text"));
    }

    #[test]
    fn synthetic_or_cross_session_inputs_fail_closed() {
        let (gate, rights, shadow) = real_shadow();
        let mut teacher = teacher_report(&shadow);
        teacher.payload.session_id = "another-session".into();
        teacher.report_sha256 = canonical_sha256(&teacher.payload, "test").unwrap();
        assert!(assemble_pilot_evidence_bundle(
            &gate,
            &rights,
            &shadow,
            &teacher,
            "2026-07-16T09:10:00Z"
        )
        .is_err());

        let synthetic = evaluate_shadow_pilot_session(
            &ShadowPilotSessionRequest {
                session_id: "synthetic-session".into(),
                evaluated_on: "2026-07-16".into(),
                started_at: "2026-07-16T08:00:00Z".into(),
                predictions_generated_at: "2026-07-16T08:05:00Z".into(),
                provider_ref_sha256: "a".repeat(64),
                model_ref_sha256: "b".repeat(64),
                provider_config_sha256: "c".repeat(64),
            },
            manifests()
                .into_iter()
                .map(|manifest| ShadowPilotMaterialInput {
                    predictions: perfect_predictions(&manifest),
                    manifest,
                })
                .collect(),
            None,
            None,
            "2026-07-16T08:10:00Z",
        )
        .unwrap();
        let synthetic_teacher = teacher_report(&synthetic);
        assert!(assemble_pilot_evidence_bundle(
            &gate,
            &rights,
            &synthetic,
            &synthetic_teacher,
            "2026-07-16T09:10:00Z"
        )
        .is_err());
    }

    #[test]
    fn teacher_dataset_or_shadow_hash_drift_is_rejected() {
        let (gate, rights, shadow) = real_shadow();
        let mut teacher = teacher_report(&shadow);
        teacher.payload.material_results[0].dataset_ids = vec!["wrong-dataset".into()];
        teacher.report_sha256 = canonical_sha256(&teacher.payload, "test").unwrap();
        assert!(assemble_pilot_evidence_bundle(
            &gate,
            &rights,
            &shadow,
            &teacher,
            "2026-07-16T09:10:00Z"
        )
        .unwrap_err()
        .to_string()
        .contains("数据集不一致"));

        let mut teacher = teacher_report(&shadow);
        teacher.payload.shadow_result_sha256 = "e".repeat(64);
        teacher.report_sha256 = canonical_sha256(&teacher.payload, "test").unwrap();
        assert!(assemble_pilot_evidence_bundle(
            &gate,
            &rights,
            &shadow,
            &teacher,
            "2026-07-16T09:10:00Z"
        )
        .unwrap_err()
        .to_string()
        .contains("结果 hash 不一致"));
    }

    #[test]
    fn bundle_write_is_idempotent_and_never_overwrites_another_bundle() {
        let (gate, rights, shadow) = real_shadow();
        let teacher = teacher_report(&shadow);
        let bundle = assemble_pilot_evidence_bundle(
            &gate,
            &rights,
            &shadow,
            &teacher,
            "2026-07-16T09:10:00Z",
        )
        .unwrap();
        let root = std::env::temp_dir().join(format!(
            "jiaofu-pilot-evidence-bundle-{}",
            ids::new_public_id()
        ));
        fs::create_dir_all(&root).unwrap();
        let output = root.join("bundle.json");
        let first = write_pilot_evidence_bundle_once(&output, &bundle).unwrap();
        let repeated = write_pilot_evidence_bundle_once(&output, &bundle).unwrap();
        assert_eq!(first, repeated);

        let changed = assemble_pilot_evidence_bundle(
            &gate,
            &rights,
            &shadow,
            &teacher,
            "2026-07-16T09:11:00Z",
        )
        .unwrap();
        assert!(write_pilot_evidence_bundle_once(&output, &changed).is_err());
        assert_eq!(read_existing(&output).unwrap(), bundle);
        fs::remove_dir_all(root).unwrap();
    }
}
