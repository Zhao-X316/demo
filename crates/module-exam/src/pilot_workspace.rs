//! 仓库外真实材料试点工作区脚手架与只读就绪检查。
//!
//! 工作区只生成 draft 清单、空标注和安全目录，不复制真实学生材料，也不会把草稿
//! 伪装成已批准闸门。所有真实资产仍由老师放在仓库外受限目录，既有 gate/rights/
//! golden/shadow 校验继续作为唯一放行依据。

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use suite_core::domain::{hashing, ids};
use suite_core::error::{CoreError, CoreResult};

use crate::material_golden::{
    validate_real_material_gate, MaterialGoldenGovernance, MaterialGoldenKind,
    MaterialGoldenManifest, MaterialGoldenPredictionSet, MaterialGoldenStorageScope,
    MATERIAL_GOLDEN_SCHEMA_VERSION,
};
use crate::pilot_data_gate::{
    PilotDataGateManifest, PilotDataRightsGate, PilotDataType, PilotDiagnosticGate,
    PilotGateScopeKind, PilotGateState, PilotNoticeGate, PILOT_DATA_GATE_SCHEMA_VERSION,
};
use crate::pilot_data_rights::{
    PilotDataRightsEvidenceBundle, PilotDatasetAsset, PilotDatasetAssetState, PilotDatasetManifest,
    PILOT_DATASET_SCHEMA_VERSION,
};
use crate::teacher_shadow::{TeacherShadowObservationSet, TEACHER_SHADOW_SCHEMA_VERSION};

pub const PILOT_WORKSPACE_SCHEMA_VERSION: i64 = 1;

const INDEX_FILE: &str = "pilot_workspace_v1.json";
const GATE_FILE: &str = "pilot_data_gate_v1.json";
const DATASET_FILE: &str = "pilot_dataset_v1.json";
const RIGHTS_EVIDENCE_FILE: &str = "pilot_data_rights_evidence_v1.json";
const OBSERVATIONS_FILE: &str = "teacher/observations_v1.json";

#[derive(Debug, Clone)]
pub struct PilotWorkspaceRequest {
    pub root: PathBuf,
    pub starts_on: String,
    pub ends_on: String,
    pub purpose: String,
    pub responsible_party_ref_sha256: String,
    pub class_scope_sha256: String,
    pub retention_policy_version: String,
    pub notice_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotWorkspaceMaterial {
    pub material_kind: MaterialGoldenKind,
    pub dataset_id: String,
    pub manifest_path: String,
    pub predictions_path: String,
    pub asset_directory: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotWorkspaceIndex {
    pub schema_version: i64,
    pub workspace_id: String,
    pub gate_id: String,
    pub rights_dataset_id: String,
    pub session_id: String,
    pub state: String,
    pub generated_at: String,
    pub gate_path: String,
    pub dataset_path: String,
    pub rights_evidence_path: String,
    pub teacher_observations_path: String,
    pub materials: Vec<PilotWorkspaceMaterial>,
    pub production_accuracy_claim_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotWorkspaceStatus {
    pub schema_version: i64,
    pub workspace_id: String,
    pub gate_id: String,
    pub evaluated_on: String,
    pub material_case_counts: Vec<(MaterialGoldenKind, usize)>,
    pub teacher_observations_present: bool,
    pub ready_for_provider_shadow: bool,
    pub blocker_codes: Vec<String>,
    pub production_accuracy_claim_allowed: bool,
}

fn repository_root() -> CoreResult<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| CoreError::Config("无法确定仓库根目录".into()))
        .and_then(|path| fs::canonicalize(path).map_err(|error| CoreError::Io(error.to_string())))
}

fn normalized_output_root(root: &Path) -> CoreResult<PathBuf> {
    if !root.is_absolute() {
        return Err(CoreError::Invalid(
            "真实试点工作区必须使用仓库外绝对路径".into(),
        ));
    }
    if root.exists() {
        return Err(CoreError::Invalid(
            "真实试点工作区已存在；为避免覆盖必须使用新目录".into(),
        ));
    }
    let parent = root
        .parent()
        .ok_or_else(|| CoreError::Invalid("真实试点工作区缺少父目录".into()))?;
    let name = root
        .file_name()
        .ok_or_else(|| CoreError::Invalid("真实试点工作区目录名无效".into()))?;
    let canonical_parent =
        fs::canonicalize(parent).map_err(|error| CoreError::Io(error.to_string()))?;
    if !canonical_parent.is_dir() {
        return Err(CoreError::Invalid("真实试点工作区父路径必须是目录".into()));
    }
    let candidate = canonical_parent.join(name);
    if candidate.starts_with(repository_root()?) {
        return Err(CoreError::Invalid(
            "真实学生材料工作区禁止创建在代码仓库内".into(),
        ));
    }
    Ok(candidate)
}

fn safe_relative(relative: &str) -> CoreResult<&Path> {
    let path = Path::new(relative);
    if relative.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(CoreError::Invalid("工作区相对路径不安全".into()));
    }
    Ok(path)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> CoreResult<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| CoreError::Io(error.to_string()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> CoreResult<()> {
    Ok(())
}

fn create_secure_dir(path: &Path) -> CoreResult<()> {
    fs::create_dir(path).map_err(|error| CoreError::Io(error.to_string()))?;
    set_mode(path, 0o700)
}

fn write_new(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| CoreError::Io(error.to_string()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| CoreError::Io(error.to_string()))?;
    set_mode(path, 0o600)
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> CoreResult<()> {
    let mut bytes =
        serde_json::to_vec_pretty(value).map_err(|error| CoreError::Config(error.to_string()))?;
    bytes.push(b'\n');
    write_new(path, &bytes)
}

fn read_json<T: DeserializeOwned>(root: &Path, relative: &str) -> CoreResult<T> {
    let path = root.join(safe_relative(relative)?);
    let metadata = fs::symlink_metadata(&path).map_err(|error| CoreError::Io(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CoreError::Invalid(
            "工作区输入必须是受限目录内普通文件".into(),
        ));
    }
    let canonical_root =
        fs::canonicalize(root).map_err(|error| CoreError::Io(error.to_string()))?;
    let canonical_path =
        fs::canonicalize(&path).map_err(|error| CoreError::Io(error.to_string()))?;
    if !canonical_path.starts_with(canonical_root) {
        return Err(CoreError::Invalid("工作区输入越出受限目录".into()));
    }
    serde_json::from_slice(
        &fs::read(&canonical_path).map_err(|error| CoreError::Io(error.to_string()))?,
    )
    .map_err(|error| CoreError::Invalid(format!("{} JSON 非法：{error}", path.display())))
}

fn material_slug(kind: MaterialGoldenKind) -> &'static str {
    match kind {
        MaterialGoldenKind::OrdinaryPaper => "ordinary_paper",
        MaterialGoldenKind::AnswerSheet => "answer_sheet",
        MaterialGoldenKind::Dictation => "dictation",
    }
}

fn material_entry(kind: MaterialGoldenKind) -> PilotWorkspaceMaterial {
    let slug = material_slug(kind);
    PilotWorkspaceMaterial {
        material_kind: kind,
        dataset_id: format!("{slug}-{}", ids::new_public_id()),
        manifest_path: format!("materials/{slug}_manifest_v1.json"),
        predictions_path: format!("materials/{slug}_predictions_v1.json"),
        asset_directory: format!("assets/{slug}"),
    }
}

fn draft_gate(request: &PilotWorkspaceRequest, gate_id: &str) -> PilotDataGateManifest {
    PilotDataGateManifest {
        schema_version: PILOT_DATA_GATE_SCHEMA_VERSION,
        gate_id: gate_id.to_owned(),
        scope_kind: PilotGateScopeKind::RealPilot,
        state: PilotGateState::Draft,
        purpose: request.purpose.clone(),
        responsible_party_ref_sha256: request.responsible_party_ref_sha256.clone(),
        class_scope_sha256: request.class_scope_sha256.clone(),
        starts_on: request.starts_on.clone(),
        ends_on: request.ends_on.clone(),
        allowed_data_types: vec![
            PilotDataType::StudentPageImage,
            PilotDataType::AnswerRegionImage,
            PilotDataType::OcrTranscript,
            PilotDataType::MachineSuggestion,
            PilotDataType::TeacherDecision,
        ],
        local_retention_days: 30,
        retention_policy_version: request.retention_policy_version.clone(),
        notice: PilotNoticeGate {
            notice_version: request.notice_version.clone(),
            notice_completed: false,
            required_authorization_completed: false,
            local_processing_disclosed: false,
            cloud_processing_disclosed: false,
        },
        rights: PilotDataRightsGate {
            student_export_procedure_version: "pilot-student-export-v1".into(),
            batch_export_procedure_version: "pilot-batch-export-v1".into(),
            student_delete_procedure_version: "pilot-student-delete-v1".into(),
            batch_delete_procedure_version: "pilot-batch-delete-v1".into(),
            deletion_receipt_excludes_content: true,
            dry_run_verified_at: None,
            dry_run_evidence_sha256: None,
        },
        diagnostics: PilotDiagnosticGate {
            default_excludes_student_name: true,
            default_excludes_raw_image: true,
            default_excludes_ocr_text: true,
            default_excludes_answer_text: true,
            explicit_opt_in_required_for_raw_evidence: true,
        },
        cloud_processors: Vec::new(),
        approved_by_ref_sha256: None,
        approved_at: None,
    }
}

fn draft_manifest(
    material: &PilotWorkspaceMaterial,
    gate_id: &str,
    retention_policy_version: &str,
) -> MaterialGoldenManifest {
    MaterialGoldenManifest {
        schema_version: MATERIAL_GOLDEN_SCHEMA_VERSION,
        dataset_id: material.dataset_id.clone(),
        material_kind: material.material_kind,
        production_accuracy_claim_allowed: false,
        governance: MaterialGoldenGovernance {
            storage_scope: MaterialGoldenStorageScope::LocalRestricted,
            personal_identifiers_removed: false,
            privacy_reviewed: false,
            privacy_reviewed_by: "pending-privacy-review".into(),
            retention_policy_version: retention_policy_version.to_owned(),
            pilot_gate_id: Some(gate_id.to_owned()),
            pilot_gate_policy_sha256: None,
        },
        cases: Vec::new(),
    }
}

fn workspace_readme(index: &PilotWorkspaceIndex) -> String {
    format!(
        r#"# 教辅系统三材料真实影子试点工作区

> 状态：`draft`。本目录必须保持在代码仓库外，仅限获批人员访问。当前文件不会放行真实数据，也不能用于生产准确率声明。

## 一次准备，三类共用

- 工作区：`{workspace_id}`
- 数据闸门：`{gate_id}`
- 机器/老师影子会话：`{session_id}`
- 材料：普通试卷、答题卡、默写必须全部存在，禁止用合成夹具补缺。

## 使用顺序

1. 把已去标识的材料放入 `assets/ordinary_paper/`、`assets/answer_sheet/`、`assets/dictation/`。不要保留含姓名/学号的文件名。
2. 填写 `pilot_dataset_v1.json` 的真实文件 hash、大小、内部 ID 和相对路径；不得填写学生姓名。
3. 完成告知、必要授权、云处理说明和供应商最小化策略后，更新 `pilot_data_gate_v1.json`。保持 `state=draft`，直到四类数据权利 dry-run 证据完成。
4. 按仓库 `crates/module-exam/tests/fixtures/material_golden/README.md` 生成四类回执和 `pilot_data_rights_evidence_v1.json`，把证据 hash/时间写回 gate，再由授权人将 gate 改为 `approved`。
5. 在 `materials/` 标注三类真实 manifest；真实 case 必须使用 `real_photo` 或 `real_scan`、artifact hash、预期路由和预期单元。将批准后的 gate policy hash 写入三个 manifest。
6. provider 预测写入三个 `*_predictions_v1.json`。模型、配置和时间由 `run_shadow_pilot` 冻结，不得把学生正文写入报告。
7. 运行只读检查：

```bash
cargo run -p module-exam --example check_real_pilot_workspace -- \
  --workspace "<本工作区绝对路径>" \
  --as-of <YYYY-MM-DD>
```

只有输出 `ready_for_provider_shadow=true` 时才能执行真实三材料机器影子会话。之后再填写 `teacher/observations_v1.json`，生成老师报告和 `results/pilot_evidence_bundle_v1.json`。

## 安全边界

- `results/`、`receipts/` 和 `exports/` 初始为空；一次写入工具不会覆盖已有不同结果。
- 草稿 manifest、空预测、空老师观察均故意 fail-closed。
- 不得把本目录、原图、OCR/答案正文、身份映射、凭据或供应商 token 提交到 Git。
- 总验收包始终 `release_authorized=false`，不能替代老师终审、异模型审查、合并或发布批准。
"#,
        workspace_id = index.workspace_id,
        gate_id = index.gate_id,
        session_id = index.session_id,
    )
}

pub fn create_pilot_workspace(request: &PilotWorkspaceRequest) -> CoreResult<PilotWorkspaceIndex> {
    let root = normalized_output_root(&request.root)?;
    let gate_id = format!("pilot-gate-{}", ids::new_public_id());
    let materials = vec![
        material_entry(MaterialGoldenKind::OrdinaryPaper),
        material_entry(MaterialGoldenKind::AnswerSheet),
        material_entry(MaterialGoldenKind::Dictation),
    ];
    let index = PilotWorkspaceIndex {
        schema_version: PILOT_WORKSPACE_SCHEMA_VERSION,
        workspace_id: format!("pilot-workspace-{}", ids::new_public_id()),
        gate_id: gate_id.clone(),
        rights_dataset_id: format!("pilot-rights-dataset-{}", ids::new_public_id()),
        session_id: format!("pilot-shadow-{}", ids::new_public_id()),
        state: "draft".into(),
        generated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        gate_path: GATE_FILE.into(),
        dataset_path: DATASET_FILE.into(),
        rights_evidence_path: RIGHTS_EVIDENCE_FILE.into(),
        teacher_observations_path: OBSERVATIONS_FILE.into(),
        materials,
        production_accuracy_claim_allowed: false,
    };
    let gate = draft_gate(request, &gate_id);
    gate.validate()?;

    create_secure_dir(&root)?;
    let result = (|| {
        for relative in [
            "assets",
            "assets/ordinary_paper",
            "assets/answer_sheet",
            "assets/dictation",
            "materials",
            "receipts",
            "teacher",
            "results",
            "exports",
        ] {
            create_secure_dir(&root.join(safe_relative(relative)?))?;
        }
        write_json(&root.join(INDEX_FILE), &index)?;
        write_json(&root.join(GATE_FILE), &gate)?;
        write_json(
            &root.join(DATASET_FILE),
            &PilotDatasetManifest {
                schema_version: PILOT_DATASET_SCHEMA_VERSION,
                dataset_id: index.rights_dataset_id.clone(),
                gate_id: gate_id.clone(),
                assets: Vec::new(),
                completed_operations: Vec::new(),
            },
        )?;
        for material in &index.materials {
            write_json(
                &root.join(safe_relative(&material.manifest_path)?),
                &draft_manifest(material, &gate_id, &request.retention_policy_version),
            )?;
            write_json(
                &root.join(safe_relative(&material.predictions_path)?),
                &MaterialGoldenPredictionSet {
                    schema_version: MATERIAL_GOLDEN_SCHEMA_VERSION,
                    dataset_id: material.dataset_id.clone(),
                    predictions: Vec::new(),
                },
            )?;
        }
        write_json(
            &root.join(OBSERVATIONS_FILE),
            &TeacherShadowObservationSet {
                schema_version: TEACHER_SHADOW_SCHEMA_VERSION,
                session_id: index.session_id.clone(),
                observations: Vec::new(),
            },
        )?;
        write_new(&root.join("README.md"), workspace_readme(&index).as_bytes())?;
        write_new(
            &root.join("DO_NOT_COMMIT"),
            b"restricted real-pilot workspace; never commit this directory\n",
        )?;
        Ok(index.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&root);
    }
    result
}

fn add_blocker(blockers: &mut BTreeSet<String>, code: &str) {
    blockers.insert(code.to_owned());
}

fn dataset_asset_is_intact(root: &Path, asset: &PilotDatasetAsset) -> bool {
    if asset.state != PilotDatasetAssetState::Active {
        return false;
    }
    let Ok(relative) = safe_relative(&asset.relative_path) else {
        return false;
    };
    let path = root.join(relative);
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != asset.byte_size
    {
        return false;
    }
    let Ok(canonical_root) = fs::canonicalize(root) else {
        return false;
    };
    let Ok(canonical_path) = fs::canonicalize(&path) else {
        return false;
    };
    canonical_path.starts_with(canonical_root)
        && hashing::sha256_file(&canonical_path)
            .is_ok_and(|actual| actual.eq_ignore_ascii_case(&asset.artifact_sha256))
}

pub fn inspect_pilot_workspace(
    root: &Path,
    evaluated_on: &str,
) -> CoreResult<PilotWorkspaceStatus> {
    if !root.is_absolute() || !root.is_dir() {
        return Err(CoreError::Invalid(
            "真实试点工作区必须是存在的绝对目录".into(),
        ));
    }
    let metadata = fs::symlink_metadata(root).map_err(|error| CoreError::Io(error.to_string()))?;
    if metadata.file_type().is_symlink() {
        return Err(CoreError::Invalid("真实试点工作区不能是符号链接".into()));
    }
    let canonical_root =
        fs::canonicalize(root).map_err(|error| CoreError::Io(error.to_string()))?;
    if canonical_root.starts_with(repository_root()?) {
        return Err(CoreError::Invalid(
            "真实学生材料工作区禁止放在代码仓库内".into(),
        ));
    }
    let index: PilotWorkspaceIndex = read_json(root, INDEX_FILE)?;
    if index.schema_version != PILOT_WORKSPACE_SCHEMA_VERSION
        || index.workspace_id.trim().is_empty()
        || index.gate_id.trim().is_empty()
        || index.materials.len() != 3
        || index.production_accuracy_claim_allowed
    {
        return Err(CoreError::Invalid("真实试点工作区索引非法".into()));
    }

    let mut blockers = BTreeSet::new();
    let gate = read_json::<PilotDataGateManifest>(root, &index.gate_path).ok();
    let rights = read_json::<PilotDataRightsEvidenceBundle>(root, &index.rights_evidence_path).ok();
    let rights_ready = rights
        .as_ref()
        .is_some_and(|evidence| evidence.gate_id == index.gate_id && evidence.validate().is_ok());
    if !rights_ready {
        add_blocker(&mut blockers, "RIGHTS_EVIDENCE_NOT_READY");
    }
    match gate
        .as_ref()
        .zip(rights.as_ref())
        .and_then(|(gate, rights)| {
            gate.evaluate_with_rights_evidence(evaluated_on, rights)
                .ok()
        }) {
        Some(report) if report.real_data_allowed => {}
        _ => add_blocker(&mut blockers, "GATE_NOT_READY"),
    }

    let dataset = read_json::<PilotDatasetManifest>(root, &index.dataset_path).ok();
    let dataset_ready = dataset.as_ref().is_some_and(|dataset| {
        dataset.dataset_id == index.rights_dataset_id
            && dataset.gate_id == index.gate_id
            && dataset.validate().is_ok()
            && dataset
                .assets
                .iter()
                .all(|asset| dataset_asset_is_intact(root, asset))
    });
    if !dataset_ready {
        add_blocker(&mut blockers, "DATASET_NOT_READY");
    }
    let dataset_artifact_hashes = dataset
        .as_ref()
        .filter(|_| dataset_ready)
        .map(|dataset| {
            dataset
                .assets
                .iter()
                .map(|asset| asset.artifact_sha256.to_ascii_lowercase())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();

    let mut material_case_counts = Vec::new();
    let mut kinds = BTreeSet::new();
    for material in &index.materials {
        let slug = material_slug(material.material_kind).to_ascii_uppercase();
        if !kinds.insert(material.material_kind) {
            add_blocker(&mut blockers, "MATERIAL_KIND_DUPLICATED");
            continue;
        }
        let manifest = read_json::<MaterialGoldenManifest>(root, &material.manifest_path).ok();
        let prediction =
            read_json::<MaterialGoldenPredictionSet>(root, &material.predictions_path).ok();
        let manifest_ready = manifest.as_ref().is_some_and(|value| {
            value.dataset_id == material.dataset_id
                && value.material_kind == material.material_kind
                && value.contains_real_data()
                && value.validate().is_ok()
                && value.cases.iter().all(|case| {
                    case.artifact_sha256.as_ref().is_some_and(|hash| {
                        dataset_artifact_hashes.contains(&hash.to_ascii_lowercase())
                    })
                })
                && gate
                    .as_ref()
                    .zip(rights.as_ref())
                    .is_some_and(|(gate, rights)| {
                        validate_real_material_gate(value, gate, rights, evaluated_on).is_ok()
                    })
        });
        if !manifest_ready {
            add_blocker(&mut blockers, &format!("{slug}_MANIFEST_NOT_READY"));
        }
        let prediction_ready =
            manifest
                .as_ref()
                .zip(prediction.as_ref())
                .is_some_and(|(manifest, prediction)| {
                    let expected = manifest
                        .cases
                        .iter()
                        .map(|case| case.case_id.as_str())
                        .collect::<BTreeSet<_>>();
                    let actual = prediction
                        .predictions
                        .iter()
                        .map(|item| item.case_id.as_str())
                        .collect::<BTreeSet<_>>();
                    prediction.validate_for(manifest).is_ok()
                        && !prediction.predictions.is_empty()
                        && expected == actual
                });
        if !prediction_ready {
            add_blocker(&mut blockers, &format!("{slug}_PREDICTIONS_NOT_READY"));
        }
        material_case_counts.push((
            material.material_kind,
            manifest.as_ref().map_or(0, |value| value.cases.len()),
        ));
    }
    if kinds.len() != 3 {
        add_blocker(&mut blockers, "THREE_MATERIALS_REQUIRED");
    }

    let teacher_observations_present =
        read_json::<TeacherShadowObservationSet>(root, &index.teacher_observations_path).is_ok_and(
            |value| {
                value.session_id == index.session_id
                    && !value.observations.is_empty()
                    && value.validate().is_ok()
            },
        );
    let blocker_codes = blockers.into_iter().collect::<Vec<_>>();
    Ok(PilotWorkspaceStatus {
        schema_version: PILOT_WORKSPACE_SCHEMA_VERSION,
        workspace_id: index.workspace_id,
        gate_id: index.gate_id,
        evaluated_on: evaluated_on.to_owned(),
        material_case_counts,
        teacher_observations_present,
        ready_for_provider_shadow: blocker_codes.is_empty(),
        blocker_codes,
        production_accuracy_claim_allowed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material_golden::MaterialGoldenSourceKind;
    use crate::pilot_data_gate::tests::{approved_real_gate, approved_rights_evidence};

    fn request(root: PathBuf) -> PilotWorkspaceRequest {
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

    fn replace_json<T: Serialize>(path: &Path, value: &T) {
        let mut bytes = serde_json::to_vec_pretty(value).unwrap();
        bytes.push(b'\n');
        fs::write(path, bytes).unwrap();
        set_mode(path, 0o600).unwrap();
    }

    #[test]
    fn scaffold_is_outside_repo_private_and_fail_closed() {
        let parent = std::env::temp_dir();
        let root = parent.join(format!("jiaofu-real-pilot-{}", ids::new_public_id()));
        let index = create_pilot_workspace(&request(root.clone())).unwrap();
        assert_eq!(index.materials.len(), 3);
        assert!(!index.production_accuracy_claim_allowed);
        assert!(root.join("DO_NOT_COMMIT").is_file());
        assert!(root.join("assets/ordinary_paper").is_dir());
        assert!(root.join("assets/answer_sheet").is_dir());
        assert!(root.join("assets/dictation").is_dir());

        let gate: PilotDataGateManifest = read_json(&root, GATE_FILE).unwrap();
        assert_eq!(gate.state, PilotGateState::Draft);
        assert!(!gate.evaluate("2026-07-16").unwrap().real_data_allowed);

        let status = inspect_pilot_workspace(&root, "2026-07-16").unwrap();
        assert!(!status.ready_for_provider_shadow);
        assert!(!status.production_accuracy_claim_allowed);
        assert!(status.blocker_codes.contains(&"GATE_NOT_READY".into()));
        assert!(status.blocker_codes.contains(&"DATASET_NOT_READY".into()));
        assert!(status
            .blocker_codes
            .contains(&"RIGHTS_EVIDENCE_NOT_READY".into()));
        assert!(status
            .blocker_codes
            .contains(&"ORDINARY_PAPER_MANIFEST_NOT_READY".into()));
        assert!(status
            .blocker_codes
            .contains(&"ANSWER_SHEET_MANIFEST_NOT_READY".into()));
        assert!(status
            .blocker_codes
            .contains(&"DICTATION_MANIFEST_NOT_READY".into()));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&root).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(root.join(INDEX_FILE))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scaffold_never_overwrites_existing_directory() {
        let root = std::env::temp_dir().join(format!("jiaofu-real-pilot-{}", ids::new_public_id()));
        let first = create_pilot_workspace(&request(root.clone())).unwrap();
        let original =
            fs::read(root.join(INDEX_FILE)).expect("created workspace index should exist");
        assert!(create_pilot_workspace(&request(root.clone())).is_err());
        assert_eq!(fs::read(root.join(INDEX_FILE)).unwrap(), original);
        assert_eq!(
            read_json::<PilotWorkspaceIndex>(&root, INDEX_FILE)
                .unwrap()
                .workspace_id,
            first.workspace_id
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scaffold_refuses_repository_path() {
        let root = repository_root()
            .unwrap()
            .join(format!(".pilot-workspace-{}", ids::new_public_id()));
        assert!(create_pilot_workspace(&request(root)).is_err());
    }

    #[test]
    fn status_refuses_repository_path_even_if_created_manually() {
        let root = repository_root()
            .unwrap()
            .join(format!(".pilot-workspace-{}", ids::new_public_id()));
        create_secure_dir(&root).unwrap();
        assert!(inspect_pilot_workspace(&root, "2026-07-16").is_err());
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn status_becomes_ready_only_with_matching_real_inputs() {
        let root = std::env::temp_dir().join(format!("jiaofu-real-pilot-{}", ids::new_public_id()));
        create_pilot_workspace(&request(root.clone())).unwrap();
        let mut index: PilotWorkspaceIndex = read_json(&root, INDEX_FILE).unwrap();
        let gate = approved_real_gate();
        let rights = approved_rights_evidence();
        index.gate_id = gate.gate_id.clone();
        index.rights_dataset_id = rights.dataset_id.clone();
        replace_json(&root.join(INDEX_FILE), &index);
        replace_json(&root.join(GATE_FILE), &gate);
        replace_json(&root.join(RIGHTS_EVIDENCE_FILE), &rights);
        let asset_specs = [
            (
                MaterialGoldenKind::OrdinaryPaper,
                "assets/ordinary_paper/asset-001",
                b"ordinary-real-asset".as_slice(),
            ),
            (
                MaterialGoldenKind::AnswerSheet,
                "assets/answer_sheet/asset-001",
                b"answer-sheet-real-asset".as_slice(),
            ),
            (
                MaterialGoldenKind::Dictation,
                "assets/dictation/asset-001",
                b"dictation-real-asset".as_slice(),
            ),
        ];
        let mut dataset_assets = Vec::new();
        for (position, (_, relative_path, bytes)) in asset_specs.iter().enumerate() {
            write_new(&root.join(relative_path), bytes).unwrap();
            dataset_assets.push(PilotDatasetAsset {
                asset_id: format!("asset-{:03}", position + 1),
                student_ref_sha256: "a".repeat(64),
                batch_ref_sha256: "b".repeat(64),
                data_type: PilotDataType::StudentPageImage,
                artifact_sha256: hashing::sha256_hex(bytes),
                byte_size: bytes.len() as u64,
                relative_path: (*relative_path).into(),
                state: PilotDatasetAssetState::Active,
                deletion_request_sha256: None,
                deleted_at: None,
                deletion_receipt_sha256: None,
            });
        }
        replace_json(
            &root.join(DATASET_FILE),
            &PilotDatasetManifest {
                schema_version: PILOT_DATASET_SCHEMA_VERSION,
                dataset_id: rights.dataset_id.clone(),
                gate_id: gate.gate_id.clone(),
                assets: dataset_assets,
                completed_operations: Vec::new(),
            },
        );

        for material in &index.materials {
            let (manifest_json, predictions_json) = match material.material_kind {
                MaterialGoldenKind::OrdinaryPaper => (
                    include_str!(
                        "../tests/fixtures/material_golden/ordinary_paper_manifest_v1.json"
                    ),
                    include_str!(
                        "../tests/fixtures/material_golden/ordinary_paper_predictions_v1.json"
                    ),
                ),
                MaterialGoldenKind::AnswerSheet => (
                    include_str!("../tests/fixtures/material_golden/answer_sheet_manifest_v1.json"),
                    include_str!(
                        "../tests/fixtures/material_golden/answer_sheet_predictions_v1.json"
                    ),
                ),
                MaterialGoldenKind::Dictation => (
                    include_str!("../tests/fixtures/material_golden/dictation_manifest_v1.json"),
                    include_str!("../tests/fixtures/material_golden/dictation_predictions_v1.json"),
                ),
            };
            let mut manifest: MaterialGoldenManifest = serde_json::from_str(manifest_json).unwrap();
            manifest.dataset_id = material.dataset_id.clone();
            manifest.production_accuracy_claim_allowed = true;
            manifest.governance.storage_scope = MaterialGoldenStorageScope::LocalRestricted;
            manifest.governance.personal_identifiers_removed = true;
            manifest.governance.privacy_reviewed = true;
            manifest.governance.privacy_reviewed_by = "reviewer-opaque-ref".into();
            manifest.governance.pilot_gate_id = Some(gate.gate_id.clone());
            manifest.governance.pilot_gate_policy_sha256 = Some(gate.policy_sha256().unwrap());
            let artifact_sha256 = asset_specs
                .iter()
                .find(|(kind, _, _)| *kind == material.material_kind)
                .map(|(_, _, bytes)| hashing::sha256_hex(bytes))
                .unwrap();
            for case in &mut manifest.cases {
                case.source_kind = MaterialGoldenSourceKind::RealPhoto;
                case.artifact_sha256 = Some(artifact_sha256.clone());
                case.annotated_by = "annotator-opaque-ref".into();
            }
            let mut predictions: MaterialGoldenPredictionSet =
                serde_json::from_str(predictions_json).unwrap();
            predictions.dataset_id = material.dataset_id.clone();
            replace_json(&root.join(&material.manifest_path), &manifest);
            replace_json(&root.join(&material.predictions_path), &predictions);
        }

        let status = inspect_pilot_workspace(&root, "2026-07-16").unwrap();
        assert!(
            status.ready_for_provider_shadow,
            "unexpected blockers: {:?}",
            status.blocker_codes
        );
        assert!(status.blocker_codes.is_empty());
        assert_eq!(
            status
                .material_case_counts
                .iter()
                .map(|(_, count)| *count)
                .sum::<usize>(),
            18
        );
        assert!(!status.production_accuracy_claim_allowed);

        fs::write(
            root.join("assets/ordinary_paper/asset-001"),
            b"tampered-real-asset",
        )
        .unwrap();
        let drifted = inspect_pilot_workspace(&root, "2026-07-16").unwrap();
        assert!(!drifted.ready_for_provider_shadow);
        assert!(drifted.blocker_codes.contains(&"DATASET_NOT_READY".into()));
        fs::remove_dir_all(root).unwrap();
    }
}
