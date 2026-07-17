//! 仓库外真实三材料试点工作区的可恢复一键编排。
//!
//! 第一次运行在工作区就绪后冻结机器影子结果；老师完成同一批材料的人工/辅助成对
//! 观察后，再次运行会生成老师报告与总证据包。所有阶段都只写仓库外受限目录，
//! 既有结果按 hash 幂等复用，输入漂移或跨会话混用时失败关闭。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use suite_core::error::{CoreError, CoreResult};

use crate::material_golden::{MaterialGoldenManifest, MaterialGoldenPredictionSet};
use crate::pilot_data_gate::PilotDataGateManifest;
use crate::pilot_data_rights::PilotDataRightsEvidenceBundle;
use crate::pilot_evidence_bundle::{
    assemble_pilot_evidence_bundle, write_pilot_evidence_bundle_once, PilotEvidenceBundle,
};
use crate::pilot_workspace::{
    inspect_pilot_workspace, read_json, set_mode, PilotWorkspaceIndex, INDEX_FILE,
    PILOT_WORKSPACE_SCHEMA_VERSION,
};
use crate::shadow_pilot::{
    evaluate_shadow_pilot_session, write_shadow_pilot_result_once, ShadowPilotMaterialInput,
    ShadowPilotSessionRequest, ShadowPilotSessionResult,
};
use crate::teacher_shadow::{
    evaluate_teacher_shadow, write_teacher_shadow_report_once, TeacherShadowObservationSet,
    TeacherShadowReport, TeacherShadowWorkflow,
};

pub const PILOT_WORKSPACE_RUN_SCHEMA_VERSION: i64 = 1;

const SHADOW_RESULT_PATH: &str = "results/shadow_result_v1.json";
const TEACHER_REPORT_PATH: &str = "results/teacher_shadow_report_v2.json";
const EVIDENCE_BUNDLE_PATH: &str = "results/pilot_evidence_bundle_v1.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PilotWorkspaceShadowMetadata {
    pub started_at: String,
    pub predictions_generated_at: String,
    pub completed_at: String,
    pub provider_ref_sha256: String,
    pub model_ref_sha256: String,
    pub provider_config_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PilotWorkspaceAdvanceRequest {
    pub root: PathBuf,
    pub evaluated_on: String,
    /// 仅首次生成机器影子结果时需要；后续运行从既有不可变结果恢复。
    pub shadow_metadata: Option<PilotWorkspaceShadowMetadata>,
    pub teacher_report_generated_at: String,
    pub evidence_assembled_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotWorkspaceAdvanceStage {
    WorkspaceInputsNotReady,
    ShadowMetadataRequired,
    TeacherObservationsRequired,
    ThresholdReviewReady,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotWorkspaceAdvanceStatus {
    pub schema_version: i64,
    pub workspace_id: String,
    pub session_id: String,
    pub stage: PilotWorkspaceAdvanceStage,
    pub blocker_codes: Vec<String>,
    pub shadow_result_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_result_sha256: Option<String>,
    pub teacher_report_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teacher_report_sha256: Option<String>,
    pub evidence_bundle_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_bundle_sha256: Option<String>,
    pub threshold_review_ready: bool,
    pub production_accuracy_claim_allowed: bool,
    pub release_authorized: bool,
}

fn status(
    index: &PilotWorkspaceIndex,
    stage: PilotWorkspaceAdvanceStage,
    blocker_codes: Vec<String>,
    shadow: Option<&ShadowPilotSessionResult>,
    teacher: Option<&TeacherShadowReport>,
    bundle: Option<&PilotEvidenceBundle>,
) -> PilotWorkspaceAdvanceStatus {
    PilotWorkspaceAdvanceStatus {
        schema_version: PILOT_WORKSPACE_RUN_SCHEMA_VERSION,
        workspace_id: index.workspace_id.clone(),
        session_id: index.session_id.clone(),
        stage,
        blocker_codes,
        shadow_result_path: SHADOW_RESULT_PATH.into(),
        shadow_result_sha256: shadow.map(|value| value.result_sha256.clone()),
        teacher_report_path: TEACHER_REPORT_PATH.into(),
        teacher_report_sha256: teacher.map(|value| value.report_sha256.clone()),
        evidence_bundle_path: EVIDENCE_BUNDLE_PATH.into(),
        evidence_bundle_sha256: bundle.map(|value| value.bundle_sha256.clone()),
        threshold_review_ready: bundle.is_some(),
        production_accuracy_claim_allowed: false,
        release_authorized: false,
    }
}

fn material_inputs(
    root: &Path,
    index: &PilotWorkspaceIndex,
) -> CoreResult<Vec<ShadowPilotMaterialInput>> {
    index
        .materials
        .iter()
        .map(|material| {
            Ok(ShadowPilotMaterialInput {
                manifest: read_json::<MaterialGoldenManifest>(root, &material.manifest_path)?,
                predictions: read_json::<MaterialGoldenPredictionSet>(
                    root,
                    &material.predictions_path,
                )?,
            })
        })
        .collect()
}

fn evaluate_shadow(
    request: &PilotWorkspaceAdvanceRequest,
    index: &PilotWorkspaceIndex,
    inputs: Vec<ShadowPilotMaterialInput>,
    gate: &PilotDataGateManifest,
    rights: &PilotDataRightsEvidenceBundle,
    metadata: &PilotWorkspaceShadowMetadata,
) -> CoreResult<ShadowPilotSessionResult> {
    evaluate_shadow_pilot_session(
        &ShadowPilotSessionRequest {
            session_id: index.session_id.clone(),
            evaluated_on: request.evaluated_on.clone(),
            started_at: metadata.started_at.clone(),
            predictions_generated_at: metadata.predictions_generated_at.clone(),
            provider_ref_sha256: metadata.provider_ref_sha256.clone(),
            model_ref_sha256: metadata.model_ref_sha256.clone(),
            provider_config_sha256: metadata.provider_config_sha256.clone(),
        },
        inputs,
        Some(gate),
        Some(rights),
        &metadata.completed_at,
    )
}

fn stored_shadow_metadata(result: &ShadowPilotSessionResult) -> PilotWorkspaceShadowMetadata {
    PilotWorkspaceShadowMetadata {
        started_at: result.payload.started_at.clone(),
        predictions_generated_at: result.payload.predictions_generated_at.clone(),
        completed_at: result.payload.completed_at.clone(),
        provider_ref_sha256: result.payload.provider_ref_sha256.clone(),
        model_ref_sha256: result.payload.model_ref_sha256.clone(),
        provider_config_sha256: result.payload.provider_config_sha256.clone(),
    }
}

fn require_same_hash(actual: &str, expected: &str, label: &str) -> CoreResult<()> {
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!(
            "{label} 与当前工作区输入不一致，拒绝覆盖或跨批次复用"
        )))
    }
}

fn teacher_observation_blockers(
    observations: &TeacherShadowObservationSet,
    shadow: &ShadowPilotSessionResult,
) -> Vec<String> {
    let expected_datasets = shadow
        .payload
        .material_results
        .iter()
        .map(|result| (result.material_kind, result.dataset_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut dataset_mismatch = false;
    let mut shadow_mismatch = false;
    for observation in &observations.observations {
        if expected_datasets.get(&observation.material_kind).copied()
            != Some(observation.dataset_id.as_str())
        {
            dataset_mismatch = true;
        }
        if observation.workflow == TeacherShadowWorkflow::AssistedReview
            && observation.shadow_result_sha256.as_deref() != Some(shadow.result_sha256.as_str())
        {
            shadow_mismatch = true;
        }
    }
    let mut blockers = Vec::new();
    if dataset_mismatch {
        blockers.push("TEACHER_OBSERVATIONS_DATASET_MISMATCH".into());
    }
    if shadow_mismatch {
        blockers.push("TEACHER_OBSERVATIONS_SHADOW_BINDING_MISMATCH".into());
    }
    blockers
}

/// 推进工作区到当前能够安全达到的最远阶段。
///
/// 未准备好真实输入时不写任何结果；首次机器结果写入后，如果老师观察尚未完成，
/// 返回稳定 blocker。再次运行会验证既有结果仍与当前输入一致，再生成报告和总包。
pub fn advance_pilot_workspace(
    request: &PilotWorkspaceAdvanceRequest,
) -> CoreResult<PilotWorkspaceAdvanceStatus> {
    let workspace = inspect_pilot_workspace(&request.root, &request.evaluated_on)?;
    let index: PilotWorkspaceIndex = read_json(&request.root, INDEX_FILE)?;
    if index.schema_version != PILOT_WORKSPACE_SCHEMA_VERSION
        || index.workspace_id != workspace.workspace_id
        || index.gate_id != workspace.gate_id
    {
        return Err(CoreError::Invalid("工作区索引与就绪检查身份不一致".into()));
    }
    if !workspace.ready_for_provider_shadow {
        return Ok(status(
            &index,
            PilotWorkspaceAdvanceStage::WorkspaceInputsNotReady,
            workspace.blocker_codes,
            None,
            None,
            None,
        ));
    }

    let gate: PilotDataGateManifest = read_json(&request.root, &index.gate_path)?;
    let rights: PilotDataRightsEvidenceBundle =
        read_json(&request.root, &index.rights_evidence_path)?;
    let shadow_path = request.root.join(SHADOW_RESULT_PATH);
    let shadow = if shadow_path.exists() {
        let existing: ShadowPilotSessionResult = read_json(&request.root, SHADOW_RESULT_PATH)?;
        if existing.payload.session_id != index.session_id {
            return Err(CoreError::Invalid(
                "既有机器影子结果属于不同工作区会话".into(),
            ));
        }
        let mut stored_request = request.clone();
        stored_request.evaluated_on = existing.payload.evaluated_on.clone();
        let candidate = evaluate_shadow(
            &stored_request,
            &index,
            material_inputs(&request.root, &index)?,
            &gate,
            &rights,
            &stored_shadow_metadata(&existing),
        )?;
        require_same_hash(
            &candidate.result_sha256,
            &existing.result_sha256,
            "既有机器影子结果",
        )?;
        existing
    } else {
        let Some(metadata) = request.shadow_metadata.as_ref() else {
            return Ok(status(
                &index,
                PilotWorkspaceAdvanceStage::ShadowMetadataRequired,
                vec!["SHADOW_RUN_METADATA_REQUIRED".into()],
                None,
                None,
                None,
            ));
        };
        let candidate = evaluate_shadow(
            request,
            &index,
            material_inputs(&request.root, &index)?,
            &gate,
            &rights,
            metadata,
        )?;
        let stored = write_shadow_pilot_result_once(&shadow_path, &candidate)?;
        set_mode(&shadow_path, 0o600)?;
        stored
    };

    if !workspace.teacher_observations_present {
        return Ok(status(
            &index,
            PilotWorkspaceAdvanceStage::TeacherObservationsRequired,
            vec!["TEACHER_OBSERVATIONS_NOT_READY".into()],
            Some(&shadow),
            None,
            None,
        ));
    }
    let observations: TeacherShadowObservationSet =
        read_json(&request.root, &index.teacher_observations_path)?;
    if observations.session_id != index.session_id {
        return Ok(status(
            &index,
            PilotWorkspaceAdvanceStage::TeacherObservationsRequired,
            vec!["TEACHER_OBSERVATIONS_SESSION_MISMATCH".into()],
            Some(&shadow),
            None,
            None,
        ));
    }
    let observation_blockers = teacher_observation_blockers(&observations, &shadow);
    if !observation_blockers.is_empty() {
        return Ok(status(
            &index,
            PilotWorkspaceAdvanceStage::TeacherObservationsRequired,
            observation_blockers,
            Some(&shadow),
            None,
            None,
        ));
    }

    let teacher_path = request.root.join(TEACHER_REPORT_PATH);
    let teacher = if teacher_path.exists() {
        let existing: TeacherShadowReport = read_json(&request.root, TEACHER_REPORT_PATH)?;
        let candidate = evaluate_teacher_shadow(&observations, &existing.payload.generated_at)?;
        require_same_hash(
            &candidate.report_sha256,
            &existing.report_sha256,
            "既有老师影子报告",
        )?;
        existing
    } else {
        let candidate =
            evaluate_teacher_shadow(&observations, &request.teacher_report_generated_at)?;
        let stored = write_teacher_shadow_report_once(&teacher_path, &candidate)?;
        set_mode(&teacher_path, 0o600)?;
        stored
    };

    let bundle_path = request.root.join(EVIDENCE_BUNDLE_PATH);
    let bundle = if bundle_path.exists() {
        let existing: PilotEvidenceBundle = read_json(&request.root, EVIDENCE_BUNDLE_PATH)?;
        let candidate = assemble_pilot_evidence_bundle(
            &gate,
            &rights,
            &shadow,
            &teacher,
            &existing.payload.assembled_at,
        )?;
        require_same_hash(
            &candidate.bundle_sha256,
            &existing.bundle_sha256,
            "既有试点总证据包",
        )?;
        existing
    } else {
        let candidate = assemble_pilot_evidence_bundle(
            &gate,
            &rights,
            &shadow,
            &teacher,
            &request.evidence_assembled_at,
        )?;
        let stored = write_pilot_evidence_bundle_once(&bundle_path, &candidate)?;
        set_mode(&bundle_path, 0o600)?;
        stored
    };

    Ok(status(
        &index,
        PilotWorkspaceAdvanceStage::ThresholdReviewReady,
        Vec::new(),
        Some(&shadow),
        Some(&teacher),
        Some(&bundle),
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use suite_core::domain::{hashing, ids};

    use super::*;
    use crate::material_golden::{
        MaterialGoldenKind, MaterialGoldenRoute, MaterialGoldenSourceKind,
        MaterialGoldenStorageScope,
    };
    use crate::pilot_data_gate::{
        tests::{approved_real_gate, approved_rights_evidence},
        PilotDataType,
    };
    use crate::pilot_data_rights::{
        PilotDatasetAsset, PilotDatasetAssetState, PilotDatasetManifest,
        PILOT_DATASET_SCHEMA_VERSION,
    };
    use crate::pilot_workspace::{
        create_pilot_workspace, inspect_pilot_workspace, PilotWorkspaceRequest,
    };
    use crate::teacher_shadow::{
        TeacherShadowObservation, TeacherShadowWorkflow, TEACHER_SHADOW_SCHEMA_VERSION,
    };

    fn replace_json<T: Serialize>(path: &Path, value: &T) {
        let mut bytes = serde_json::to_vec_pretty(value).unwrap();
        bytes.push(b'\n');
        fs::write(path, bytes).unwrap();
        set_mode(path, 0o600).unwrap();
    }

    fn workspace_request(root: PathBuf) -> PilotWorkspaceRequest {
        PilotWorkspaceRequest {
            root,
            starts_on: "2026-07-16".into(),
            ends_on: "2026-07-30".into(),
            purpose: "三材料受限影子试点".into(),
            responsible_party_ref_sha256: hashing::sha256_hex(b"responsible-opaque-ref"),
            class_scope_sha256: hashing::sha256_hex(b"class-scope-opaque-ref"),
            retention_policy_version: "pilot-local-retention-v1".into(),
            notice_version: "pilot-notice-v1".into(),
        }
    }

    fn advance_request(
        root: PathBuf,
        shadow_metadata: Option<PilotWorkspaceShadowMetadata>,
    ) -> PilotWorkspaceAdvanceRequest {
        PilotWorkspaceAdvanceRequest {
            root,
            evaluated_on: "2026-07-16".into(),
            shadow_metadata,
            teacher_report_generated_at: "2026-07-16T09:05:00Z".into(),
            evidence_assembled_at: "2026-07-16T09:10:00Z".into(),
        }
    }

    fn shadow_metadata() -> PilotWorkspaceShadowMetadata {
        PilotWorkspaceShadowMetadata {
            started_at: "2026-07-16T08:00:00Z".into(),
            predictions_generated_at: "2026-07-16T08:05:00Z".into(),
            completed_at: "2026-07-16T08:10:00Z".into(),
            provider_ref_sha256: "a".repeat(64),
            model_ref_sha256: "b".repeat(64),
            provider_config_sha256: "c".repeat(64),
        }
    }

    fn fixture_json(kind: MaterialGoldenKind) -> (&'static str, &'static str, &'static [u8]) {
        match kind {
            MaterialGoldenKind::OrdinaryPaper => (
                include_str!("../tests/fixtures/material_golden/ordinary_paper_manifest_v1.json"),
                include_str!(
                    "../tests/fixtures/material_golden/ordinary_paper_predictions_v1.json"
                ),
                b"ordinary-real-asset",
            ),
            MaterialGoldenKind::AnswerSheet => (
                include_str!("../tests/fixtures/material_golden/answer_sheet_manifest_v1.json"),
                include_str!("../tests/fixtures/material_golden/answer_sheet_predictions_v1.json"),
                b"answer-sheet-real-asset",
            ),
            MaterialGoldenKind::Dictation => (
                include_str!("../tests/fixtures/material_golden/dictation_manifest_v1.json"),
                include_str!("../tests/fixtures/material_golden/dictation_predictions_v1.json"),
                b"dictation-real-asset",
            ),
        }
    }

    fn prepare_ready_workspace(root: &Path) -> PilotWorkspaceIndex {
        let mut index =
            create_pilot_workspace(&workspace_request(root.to_path_buf())).expect("scaffold");
        let gate = approved_real_gate();
        let rights = approved_rights_evidence();
        index.gate_id = gate.gate_id.clone();
        index.rights_dataset_id = rights.dataset_id.clone();
        replace_json(&root.join(INDEX_FILE), &index);
        replace_json(&root.join(&index.gate_path), &gate);
        replace_json(&root.join(&index.rights_evidence_path), &rights);

        let mut dataset_assets = Vec::new();
        for (position, material) in index.materials.iter().enumerate() {
            let slug = match material.material_kind {
                MaterialGoldenKind::OrdinaryPaper => "ordinary_paper",
                MaterialGoldenKind::AnswerSheet => "answer_sheet",
                MaterialGoldenKind::Dictation => "dictation",
            };
            let (_, _, bytes) = fixture_json(material.material_kind);
            let relative_path = format!("assets/{slug}/asset-001");
            fs::write(root.join(&relative_path), bytes).unwrap();
            set_mode(&root.join(&relative_path), 0o600).unwrap();
            dataset_assets.push(PilotDatasetAsset {
                asset_id: format!("asset-{:03}", position + 1),
                student_ref_sha256: "d".repeat(64),
                batch_ref_sha256: "e".repeat(64),
                data_type: PilotDataType::StudentPageImage,
                artifact_sha256: hashing::sha256_hex(bytes),
                byte_size: bytes.len() as u64,
                relative_path,
                state: PilotDatasetAssetState::Active,
                deletion_request_sha256: None,
                deleted_at: None,
                deletion_receipt_sha256: None,
            });
        }
        replace_json(
            &root.join(&index.dataset_path),
            &PilotDatasetManifest {
                schema_version: PILOT_DATASET_SCHEMA_VERSION,
                dataset_id: rights.dataset_id.clone(),
                gate_id: gate.gate_id.clone(),
                assets: dataset_assets,
                completed_operations: Vec::new(),
            },
        );

        let gate_hash = gate.policy_sha256().unwrap();
        for material in &index.materials {
            let (manifest_json, predictions_json, bytes) = fixture_json(material.material_kind);
            let mut manifest: MaterialGoldenManifest = serde_json::from_str(manifest_json).unwrap();
            manifest.dataset_id = material.dataset_id.clone();
            manifest.production_accuracy_claim_allowed = true;
            manifest.governance.storage_scope = MaterialGoldenStorageScope::LocalRestricted;
            manifest.governance.personal_identifiers_removed = true;
            manifest.governance.privacy_reviewed = true;
            manifest.governance.privacy_reviewed_by = "reviewer-opaque-ref".into();
            manifest.governance.pilot_gate_id = Some(gate.gate_id.clone());
            manifest.governance.pilot_gate_policy_sha256 = Some(gate_hash.clone());
            for case in &mut manifest.cases {
                case.source_kind = MaterialGoldenSourceKind::RealPhoto;
                case.artifact_sha256 = Some(hashing::sha256_hex(bytes));
                case.annotated_by = "annotator-opaque-ref".into();
            }
            let mut predictions: MaterialGoldenPredictionSet =
                serde_json::from_str(predictions_json).unwrap();
            predictions.dataset_id = material.dataset_id.clone();
            replace_json(&root.join(&material.manifest_path), &manifest);
            replace_json(&root.join(&material.predictions_path), &predictions);
        }
        let readiness = inspect_pilot_workspace(root, "2026-07-16").unwrap();
        assert!(
            readiness.ready_for_provider_shadow,
            "{:?}",
            readiness.blocker_codes
        );
        index
    }

    fn observations(
        index: &PilotWorkspaceIndex,
        shadow_result_sha256: &str,
    ) -> TeacherShadowObservationSet {
        let mut values = Vec::new();
        for material in &index.materials {
            let slug = match material.material_kind {
                MaterialGoldenKind::OrdinaryPaper => "ordinary",
                MaterialGoldenKind::AnswerSheet => "answer-sheet",
                MaterialGoldenKind::Dictation => "dictation",
            };
            for workflow in [
                TeacherShadowWorkflow::ManualBaseline,
                TeacherShadowWorkflow::AssistedReview,
            ] {
                let assisted = workflow == TeacherShadowWorkflow::AssistedReview;
                values.push(TeacherShadowObservation {
                    observation_id: format!(
                        "obs-{slug}-{}",
                        if assisted { "assisted" } else { "manual" }
                    ),
                    comparison_group_id: format!("group-{slug}"),
                    material_kind: material.material_kind,
                    workflow,
                    dataset_id: material.dataset_id.clone(),
                    sample_scope_sha256: hashing::sha256_hex(slug.as_bytes()),
                    teacher_ref_sha256: "f".repeat(64),
                    shadow_result_sha256: assisted.then(|| shadow_result_sha256.to_owned()),
                    started_at: "2026-07-16T08:15:00Z".into(),
                    completed_at: "2026-07-16T09:00:00Z".into(),
                    submission_count: 40,
                    page_count: 40,
                    item_count: 400,
                    teacher_active_seconds: if assisted { 1200 } else { 2400 },
                    machine_wait_seconds: if assisted { 300 } else { 0 },
                    teacher_action_count: if assisted { 30 } else { 400 },
                    exception_review_count: if assisted { 20 } else { 0 },
                    corrected_machine_decision_count: if assisted { 4 } else { 0 },
                    completed_without_help: true,
                });
            }
        }
        TeacherShadowObservationSet {
            schema_version: TEACHER_SHADOW_SCHEMA_VERSION,
            session_id: index.session_id.clone(),
            observations: values,
        }
    }

    #[test]
    fn unready_workspace_never_writes_results() {
        let root = std::env::temp_dir().join(format!("jiaofu-pilot-run-{}", ids::new_public_id()));
        create_pilot_workspace(&workspace_request(root.clone())).unwrap();
        let result =
            advance_pilot_workspace(&advance_request(root.clone(), Some(shadow_metadata())))
                .unwrap();
        assert_eq!(
            result.stage,
            PilotWorkspaceAdvanceStage::WorkspaceInputsNotReady
        );
        assert!(!result.threshold_review_ready);
        assert!(!root.join(SHADOW_RESULT_PATH).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ready_workspace_resumes_from_shadow_to_immutable_evidence_bundle() {
        let root = std::env::temp_dir().join(format!("jiaofu-pilot-run-{}", ids::new_public_id()));
        let index = prepare_ready_workspace(&root);

        let missing_metadata =
            advance_pilot_workspace(&advance_request(root.clone(), None)).unwrap();
        assert_eq!(
            missing_metadata.stage,
            PilotWorkspaceAdvanceStage::ShadowMetadataRequired
        );
        assert_eq!(
            missing_metadata.blocker_codes,
            vec!["SHADOW_RUN_METADATA_REQUIRED"]
        );
        assert!(!root.join(SHADOW_RESULT_PATH).exists());

        let first =
            advance_pilot_workspace(&advance_request(root.clone(), Some(shadow_metadata())))
                .unwrap();
        assert_eq!(
            first.stage,
            PilotWorkspaceAdvanceStage::TeacherObservationsRequired
        );
        assert_eq!(first.blocker_codes, vec!["TEACHER_OBSERVATIONS_NOT_READY"]);
        let shadow_hash = first.shadow_result_sha256.clone().unwrap();
        assert!(root.join(SHADOW_RESULT_PATH).is_file());
        assert!(!root.join(TEACHER_REPORT_PATH).exists());

        let mut wrong_observations = observations(&index, &"0".repeat(64));
        wrong_observations.observations[0].dataset_id = index.materials[1].dataset_id.clone();
        wrong_observations.observations[1].dataset_id = index.materials[1].dataset_id.clone();
        replace_json(
            &root.join(&index.teacher_observations_path),
            &wrong_observations,
        );
        let rejected = advance_pilot_workspace(&advance_request(root.clone(), None)).unwrap();
        assert_eq!(
            rejected.stage,
            PilotWorkspaceAdvanceStage::TeacherObservationsRequired
        );
        assert_eq!(
            rejected.blocker_codes,
            vec![
                "TEACHER_OBSERVATIONS_DATASET_MISMATCH",
                "TEACHER_OBSERVATIONS_SHADOW_BINDING_MISMATCH"
            ]
        );
        assert!(!root.join(TEACHER_REPORT_PATH).exists());

        replace_json(
            &root.join(&index.teacher_observations_path),
            &observations(&index, &shadow_hash),
        );
        let second = advance_pilot_workspace(&advance_request(root.clone(), None)).unwrap();
        assert_eq!(
            second.stage,
            PilotWorkspaceAdvanceStage::ThresholdReviewReady
        );
        assert!(second.threshold_review_ready);
        assert!(!second.production_accuracy_claim_allowed);
        assert!(!second.release_authorized);
        assert!(root.join(TEACHER_REPORT_PATH).is_file());
        assert!(root.join(EVIDENCE_BUNDLE_PATH).is_file());

        let mut next_day = advance_request(root.clone(), None);
        next_day.evaluated_on = "2026-07-17".into();
        let repeated = advance_pilot_workspace(&next_day).unwrap();
        assert_eq!(repeated, second);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for relative in [
                SHADOW_RESULT_PATH,
                TEACHER_REPORT_PATH,
                EVIDENCE_BUNDLE_PATH,
            ] {
                assert_eq!(
                    fs::metadata(root.join(relative))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
        }

        let ordinary = index
            .materials
            .iter()
            .find(|material| material.material_kind == MaterialGoldenKind::OrdinaryPaper)
            .unwrap();
        let mut predictions: MaterialGoldenPredictionSet =
            read_json(&root, &ordinary.predictions_path).unwrap();
        predictions.predictions[0].predicted_route = MaterialGoldenRoute::ReviewRequired;
        replace_json(&root.join(&ordinary.predictions_path), &predictions);
        let drift = advance_pilot_workspace(&advance_request(root.clone(), None));
        assert!(drift.is_err());

        fs::remove_dir_all(root).unwrap();
    }
}
