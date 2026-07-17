//! 真实材料影子试点的按学生/批次导出与删除工具。
//!
//! 数据集清单和导出文件只允许存在于仓库外的本机受限目录。回执只记录不可逆引用、
//! 数量、字节数、hash 和结果码，不记录学生姓名、文件路径、OCR/答案正文或原图内容。

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use suite_core::domain::{hashing, ids};
use suite_core::error::{CoreError, CoreResult};

use crate::pilot_data_gate::PilotDataType;

pub const PILOT_DATASET_SCHEMA_VERSION: i64 = 1;
pub const PILOT_DATA_RIGHTS_RECEIPT_SCHEMA_VERSION: i64 = 1;
pub const PILOT_DATA_RIGHTS_EVIDENCE_SCHEMA_VERSION: i64 = 1;
pub const PILOT_STUDENT_EXPORT_PROCEDURE_VERSION: &str = "pilot-student-export-v1";
pub const PILOT_BATCH_EXPORT_PROCEDURE_VERSION: &str = "pilot-batch-export-v1";
pub const PILOT_STUDENT_DELETE_PROCEDURE_VERSION: &str = "pilot-student-delete-v1";
pub const PILOT_BATCH_DELETE_PROCEDURE_VERSION: &str = "pilot-batch-delete-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotDatasetAssetState {
    Active,
    DeletionPending,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotDatasetAsset {
    pub asset_id: String,
    pub student_ref_sha256: String,
    pub batch_ref_sha256: String,
    pub data_type: PilotDataType,
    pub artifact_sha256: String,
    pub byte_size: u64,
    pub relative_path: String,
    pub state: PilotDatasetAssetState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletion_request_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletion_receipt_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotDatasetManifest {
    pub schema_version: i64,
    pub dataset_id: String,
    pub gate_id: String,
    pub assets: Vec<PilotDatasetAsset>,
    #[serde(default)]
    pub completed_operations: Vec<PilotDataRightsReceipt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotDataRightsScopeKind {
    Student,
    Batch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotDataRightsAction {
    Export,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotDataRightsMode {
    DryRun,
    Execute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotDataRightsOutcome {
    Succeeded,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotDataRightsRequest {
    pub gate_id: String,
    pub action: PilotDataRightsAction,
    pub mode: PilotDataRightsMode,
    pub scope_kind: PilotDataRightsScopeKind,
    pub scope_ref_sha256: String,
    pub procedure_version: String,
    pub idempotency_key: String,
    pub requested_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotDataRightsReceiptPayload {
    pub schema_version: i64,
    pub receipt_id: String,
    pub request_sha256: String,
    pub idempotency_key_sha256: String,
    pub gate_id: String,
    pub dataset_id: String,
    pub action: PilotDataRightsAction,
    pub mode: PilotDataRightsMode,
    pub scope_kind: PilotDataRightsScopeKind,
    pub scope_ref_sha256: String,
    pub procedure_version: String,
    pub dataset_content_sha256_before: String,
    pub dataset_content_sha256_after: String,
    pub outcome: PilotDataRightsOutcome,
    pub selected_asset_count: usize,
    pub verified_asset_count: usize,
    pub already_deleted_count: usize,
    pub total_bytes: u64,
    pub exported_asset_count: usize,
    pub deleted_asset_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_manifest_sha256: Option<String>,
    pub blocker_codes: Vec<String>,
    pub content_excluded: bool,
    pub paths_excluded: bool,
    pub student_identity_excluded: bool,
    pub completed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotDataRightsReceipt {
    pub payload: PilotDataRightsReceiptPayload,
    pub receipt_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotDataRightsEvidenceBundle {
    pub schema_version: i64,
    pub gate_id: String,
    pub dataset_id: String,
    pub generated_at: String,
    pub receipts: Vec<PilotDataRightsReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PilotDatasetContentIdentity<'a> {
    schema_version: i64,
    dataset_id: &'a str,
    gate_id: &'a str,
    assets: Vec<PilotDatasetAssetContentIdentity<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PilotDatasetAssetContentIdentity<'a> {
    asset_id: &'a str,
    student_ref_sha256: &'a str,
    batch_ref_sha256: &'a str,
    data_type: PilotDataType,
    artifact_sha256: &'a str,
    byte_size: u64,
    relative_path: &'a str,
    state: PilotDatasetAssetState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PilotExportAsset<'a> {
    asset_id: &'a str,
    student_ref_sha256: &'a str,
    batch_ref_sha256: &'a str,
    data_type: PilotDataType,
    artifact_sha256: &'a str,
    byte_size: u64,
    bundle_relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PilotExportManifest<'a> {
    schema_version: i64,
    dataset_id: &'a str,
    gate_id: &'a str,
    scope_kind: PilotDataRightsScopeKind,
    scope_ref_sha256: &'a str,
    assets: Vec<PilotExportAsset<'a>>,
}

#[derive(Debug)]
struct Preflight {
    selected_indexes: Vec<usize>,
    verified_asset_count: usize,
    already_deleted_count: usize,
    total_bytes: u64,
    blockers: BTreeSet<String>,
}

#[derive(Debug)]
struct ReceiptExecution {
    after: String,
    exported_asset_count: usize,
    deleted_asset_count: usize,
    export_manifest_sha256: Option<String>,
    blockers: BTreeSet<String>,
}

fn required_text(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Invalid(format!("{field} 不能为空")));
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

fn parse_time(value: &str, field: &str) -> CoreResult<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value.trim())
        .map_err(|_| CoreError::Invalid(format!("{field} 必须是 RFC3339")))
}

fn is_safe_asset_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn safe_relative_path(value: &str) -> CoreResult<PathBuf> {
    let path = Path::new(value);
    if value.trim().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(CoreError::Invalid(
            "试点数据文件必须使用不含跳转的相对路径".into(),
        ));
    }
    Ok(path.to_path_buf())
}

fn expected_procedure(
    scope_kind: PilotDataRightsScopeKind,
    action: PilotDataRightsAction,
) -> &'static str {
    match (scope_kind, action) {
        (PilotDataRightsScopeKind::Student, PilotDataRightsAction::Export) => {
            PILOT_STUDENT_EXPORT_PROCEDURE_VERSION
        }
        (PilotDataRightsScopeKind::Batch, PilotDataRightsAction::Export) => {
            PILOT_BATCH_EXPORT_PROCEDURE_VERSION
        }
        (PilotDataRightsScopeKind::Student, PilotDataRightsAction::Delete) => {
            PILOT_STUDENT_DELETE_PROCEDURE_VERSION
        }
        (PilotDataRightsScopeKind::Batch, PilotDataRightsAction::Delete) => {
            PILOT_BATCH_DELETE_PROCEDURE_VERSION
        }
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> CoreResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::Invalid("输出路径缺少父目录".into()))?;
    fs::create_dir_all(parent).map_err(|error| CoreError::Io(error.to_string()))?;
    let temporary = parent.join(format!(".pilot-write-{}.tmp", ids::new_public_id()));
    let result = (|| -> CoreResult<()> {
        let mut options = fs::OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|error| CoreError::Io(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| CoreError::Io(error.to_string()))?;
        file.sync_all()
            .map_err(|error| CoreError::Io(error.to_string()))?;
        fs::rename(&temporary, path).map_err(|error| CoreError::Io(error.to_string()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn canonical_root(root: &Path) -> CoreResult<PathBuf> {
    let metadata = fs::symlink_metadata(root).map_err(|error| CoreError::Io(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(CoreError::Invalid(
            "试点数据根目录必须是非符号链接目录".into(),
        ));
    }
    fs::canonicalize(root).map_err(|error| CoreError::Io(error.to_string()))
}

fn checked_asset_path(root: &Path, relative_path: &str) -> CoreResult<PathBuf> {
    let relative = safe_relative_path(relative_path)?;
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path).map_err(|error| CoreError::Io(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CoreError::Invalid("试点资产必须是普通文件".into()));
    }
    let canonical = fs::canonicalize(&path).map_err(|error| CoreError::Io(error.to_string()))?;
    if !canonical.starts_with(root) {
        return Err(CoreError::Invalid("试点资产越出受限目录".into()));
    }
    Ok(canonical)
}

impl PilotDatasetManifest {
    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != PILOT_DATASET_SCHEMA_VERSION {
            return Err(CoreError::Invalid("试点数据集 schema 版本不支持".into()));
        }
        required_text(&self.dataset_id, "dataset_id")?;
        required_text(&self.gate_id, "gate_id")?;
        if self.assets.is_empty() {
            return Err(CoreError::Invalid("试点数据集至少包含一个资产".into()));
        }
        let mut asset_ids = BTreeSet::new();
        let mut relative_paths = BTreeSet::new();
        for asset in &self.assets {
            if !is_safe_asset_id(&asset.asset_id) || !asset_ids.insert(asset.asset_id.clone()) {
                return Err(CoreError::Invalid("试点 asset_id 非法或重复".into()));
            }
            normalized_sha256(&asset.student_ref_sha256, "学生引用")?;
            normalized_sha256(&asset.batch_ref_sha256, "批次引用")?;
            normalized_sha256(&asset.artifact_sha256, "资产 hash")?;
            safe_relative_path(&asset.relative_path)?;
            if !relative_paths.insert(asset.relative_path.clone()) {
                return Err(CoreError::Invalid("试点资产相对路径重复".into()));
            }
            match asset.state {
                PilotDatasetAssetState::Active => {
                    if asset.deletion_request_sha256.is_some()
                        || asset.deleted_at.is_some()
                        || asset.deletion_receipt_sha256.is_some()
                    {
                        return Err(CoreError::Invalid("active 资产不能带删除元数据".into()));
                    }
                }
                PilotDatasetAssetState::DeletionPending => {
                    normalized_sha256(
                        asset.deletion_request_sha256.as_deref().ok_or_else(|| {
                            CoreError::Invalid("deletion_pending 缺少 request hash".into())
                        })?,
                        "删除 request hash",
                    )?;
                    if asset.deleted_at.is_some() || asset.deletion_receipt_sha256.is_some() {
                        return Err(CoreError::Invalid(
                            "deletion_pending 不能提前记录完成回执".into(),
                        ));
                    }
                }
                PilotDatasetAssetState::Deleted => {
                    normalized_sha256(
                        asset.deletion_request_sha256.as_deref().ok_or_else(|| {
                            CoreError::Invalid("deleted 资产缺少 request hash".into())
                        })?,
                        "删除 request hash",
                    )?;
                    parse_time(
                        asset
                            .deleted_at
                            .as_deref()
                            .ok_or_else(|| CoreError::Invalid("deleted 资产缺少删除时间".into()))?,
                        "删除时间",
                    )?;
                    normalized_sha256(
                        asset.deletion_receipt_sha256.as_deref().ok_or_else(|| {
                            CoreError::Invalid("deleted 资产缺少 receipt hash".into())
                        })?,
                        "删除 receipt hash",
                    )?;
                }
            }
        }
        let mut request_hashes = BTreeSet::new();
        let mut idempotency_hashes = BTreeSet::new();
        for receipt in &self.completed_operations {
            receipt.validate()?;
            if receipt.payload.dataset_id != self.dataset_id
                || receipt.payload.gate_id != self.gate_id
                || !request_hashes.insert(receipt.payload.request_sha256.clone())
                || !idempotency_hashes.insert(receipt.payload.idempotency_key_sha256.clone())
            {
                return Err(CoreError::Invalid(
                    "试点数据集操作回执身份不一致或重复".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn content_sha256(&self) -> CoreResult<String> {
        self.validate()?;
        let identity = PilotDatasetContentIdentity {
            schema_version: self.schema_version,
            dataset_id: &self.dataset_id,
            gate_id: &self.gate_id,
            assets: self
                .assets
                .iter()
                .map(|asset| PilotDatasetAssetContentIdentity {
                    asset_id: &asset.asset_id,
                    student_ref_sha256: &asset.student_ref_sha256,
                    batch_ref_sha256: &asset.batch_ref_sha256,
                    data_type: asset.data_type,
                    artifact_sha256: &asset.artifact_sha256,
                    byte_size: asset.byte_size,
                    relative_path: &asset.relative_path,
                    state: asset.state,
                })
                .collect(),
        };
        serde_json::to_vec(&identity)
            .map(|bytes| hashing::sha256_hex(&bytes))
            .map_err(|error| CoreError::Config(format!("试点数据集序列化失败：{error}")))
    }

    pub fn write_atomic(&self, path: &Path) -> CoreResult<()> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| CoreError::Config(format!("试点清单序列化失败：{error}")))?;
        atomic_write(path, &bytes)
    }
}

impl PilotDataRightsRequest {
    pub fn validate(&self) -> CoreResult<()> {
        required_text(&self.gate_id, "gate_id")?;
        normalized_sha256(&self.scope_ref_sha256, "作用域引用")?;
        required_text(&self.procedure_version, "流程版本")?;
        required_text(&self.idempotency_key, "幂等键")?;
        parse_time(&self.requested_at, "请求时间")?;
        if self.procedure_version != expected_procedure(self.scope_kind, self.action) {
            return Err(CoreError::Invalid("试点数据权利流程版本不匹配".into()));
        }
        Ok(())
    }

    pub fn request_sha256(&self) -> CoreResult<String> {
        self.validate()?;
        serde_json::to_vec(self)
            .map(|bytes| hashing::sha256_hex(&bytes))
            .map_err(|error| CoreError::Config(format!("数据权利请求序列化失败：{error}")))
    }
}

impl PilotDataRightsReceipt {
    pub fn new(payload: PilotDataRightsReceiptPayload) -> CoreResult<Self> {
        let receipt_sha256 = serde_json::to_vec(&payload)
            .map(|bytes| hashing::sha256_hex(&bytes))
            .map_err(|error| CoreError::Config(format!("回执序列化失败：{error}")))?;
        let receipt = Self {
            payload,
            receipt_sha256,
        };
        receipt.validate()?;
        Ok(receipt)
    }

    pub fn validate(&self) -> CoreResult<()> {
        let payload = &self.payload;
        if payload.schema_version != PILOT_DATA_RIGHTS_RECEIPT_SCHEMA_VERSION {
            return Err(CoreError::Invalid("数据权利回执 schema 版本不支持".into()));
        }
        required_text(&payload.receipt_id, "receipt_id")?;
        required_text(&payload.gate_id, "gate_id")?;
        required_text(&payload.dataset_id, "dataset_id")?;
        normalized_sha256(&payload.request_sha256, "request hash")?;
        normalized_sha256(&payload.idempotency_key_sha256, "幂等键 hash")?;
        normalized_sha256(&payload.scope_ref_sha256, "作用域引用")?;
        normalized_sha256(&payload.dataset_content_sha256_before, "操作前数据集 hash")?;
        normalized_sha256(&payload.dataset_content_sha256_after, "操作后数据集 hash")?;
        if let Some(hash) = payload.export_manifest_sha256.as_deref() {
            normalized_sha256(hash, "导出清单 hash")?;
        }
        parse_time(&payload.completed_at, "回执完成时间")?;
        if payload.procedure_version != expected_procedure(payload.scope_kind, payload.action) {
            return Err(CoreError::Invalid("回执流程版本不匹配".into()));
        }
        if !payload.content_excluded
            || !payload.paths_excluded
            || !payload.student_identity_excluded
        {
            return Err(CoreError::Invalid(
                "回执不得包含正文、路径或学生身份".into(),
            ));
        }
        let blockers = payload.blocker_codes.iter().collect::<BTreeSet<_>>();
        if blockers.len() != payload.blocker_codes.len()
            || payload
                .blocker_codes
                .iter()
                .any(|code| code.trim().is_empty())
        {
            return Err(CoreError::Invalid("回执阻断码为空或重复".into()));
        }
        match payload.outcome {
            PilotDataRightsOutcome::Succeeded if !payload.blocker_codes.is_empty() => {
                return Err(CoreError::Invalid("成功回执不能带阻断码".into()))
            }
            PilotDataRightsOutcome::Blocked if payload.blocker_codes.is_empty() => {
                return Err(CoreError::Invalid("阻断回执必须给出阻断码".into()))
            }
            _ => {}
        }
        if payload.outcome == PilotDataRightsOutcome::Blocked {
            if payload.exported_asset_count != 0
                || payload.deleted_asset_count != 0
                || payload.export_manifest_sha256.is_some()
                || payload.dataset_content_sha256_before != payload.dataset_content_sha256_after
            {
                return Err(CoreError::Invalid(
                    "阻断回执不能声称已导出、删除或改变数据集".into(),
                ));
            }
        } else {
            if payload.selected_asset_count == 0
                || payload.verified_asset_count != payload.selected_asset_count
                || payload.already_deleted_count != 0
            {
                return Err(CoreError::Invalid(
                    "成功回执必须完整验证非空 active 作用域".into(),
                ));
            }
            match (payload.mode, payload.action) {
                (PilotDataRightsMode::DryRun, _) => {
                    if payload.exported_asset_count != 0
                        || payload.deleted_asset_count != 0
                        || payload.export_manifest_sha256.is_some()
                        || payload.dataset_content_sha256_before
                            != payload.dataset_content_sha256_after
                    {
                        return Err(CoreError::Invalid("dry-run 回执不得产生副作用".into()));
                    }
                }
                (PilotDataRightsMode::Execute, PilotDataRightsAction::Export) => {
                    if payload.exported_asset_count != payload.selected_asset_count
                        || payload.deleted_asset_count != 0
                        || payload.export_manifest_sha256.is_none()
                        || payload.dataset_content_sha256_before
                            != payload.dataset_content_sha256_after
                    {
                        return Err(CoreError::Invalid("执行导出回执计数或 hash 非法".into()));
                    }
                }
                (PilotDataRightsMode::Execute, PilotDataRightsAction::Delete) => {
                    if payload.exported_asset_count != 0
                        || payload.deleted_asset_count != payload.selected_asset_count
                        || payload.export_manifest_sha256.is_some()
                        || payload.dataset_content_sha256_before
                            == payload.dataset_content_sha256_after
                    {
                        return Err(CoreError::Invalid("执行删除回执计数或 hash 非法".into()));
                    }
                }
            }
        }
        let expected_hash = serde_json::to_vec(payload)
            .map(|bytes| hashing::sha256_hex(&bytes))
            .map_err(|error| CoreError::Config(format!("回执序列化失败：{error}")))?;
        if self.receipt_sha256 != expected_hash {
            return Err(CoreError::Invalid("数据权利回执 hash 不匹配".into()));
        }
        Ok(())
    }
}

impl PilotDataRightsEvidenceBundle {
    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != PILOT_DATA_RIGHTS_EVIDENCE_SCHEMA_VERSION {
            return Err(CoreError::Invalid("数据权利证据 schema 版本不支持".into()));
        }
        required_text(&self.gate_id, "gate_id")?;
        required_text(&self.dataset_id, "dataset_id")?;
        let generated_at = parse_time(&self.generated_at, "证据生成时间")?;
        let required = BTreeSet::from([
            (
                PilotDataRightsScopeKind::Student,
                PilotDataRightsAction::Export,
            ),
            (
                PilotDataRightsScopeKind::Batch,
                PilotDataRightsAction::Export,
            ),
            (
                PilotDataRightsScopeKind::Student,
                PilotDataRightsAction::Delete,
            ),
            (
                PilotDataRightsScopeKind::Batch,
                PilotDataRightsAction::Delete,
            ),
        ]);
        let mut found = BTreeSet::new();
        let mut request_hashes = BTreeSet::new();
        let mut idempotency_hashes = BTreeSet::new();
        if self.receipts.len() != required.len() {
            return Err(CoreError::Invalid(
                "数据权利证据必须恰好包含四类 dry-run 回执".into(),
            ));
        }
        for receipt in &self.receipts {
            receipt.validate()?;
            let payload = &receipt.payload;
            if payload.gate_id != self.gate_id
                || payload.dataset_id != self.dataset_id
                || payload.mode != PilotDataRightsMode::DryRun
                || payload.outcome != PilotDataRightsOutcome::Succeeded
                || payload.selected_asset_count == 0
                || payload.verified_asset_count != payload.selected_asset_count
                || payload.already_deleted_count != 0
                || payload.exported_asset_count != 0
                || payload.deleted_asset_count != 0
                || payload.export_manifest_sha256.is_some()
                || payload.dataset_content_sha256_before != payload.dataset_content_sha256_after
                || parse_time(&payload.completed_at, "回执完成时间")? > generated_at
                || !request_hashes.insert(payload.request_sha256.clone())
                || !idempotency_hashes.insert(payload.idempotency_key_sha256.clone())
                || !found.insert((payload.scope_kind, payload.action))
            {
                return Err(CoreError::Invalid(
                    "数据权利 dry-run 回执身份、结果或时间不满足闸门".into(),
                ));
            }
        }
        if found != required {
            return Err(CoreError::Invalid("数据权利 dry-run 回执覆盖不完整".into()));
        }
        Ok(())
    }

    pub fn evidence_sha256(&self) -> CoreResult<String> {
        self.validate()?;
        serde_json::to_vec(self)
            .map(|bytes| hashing::sha256_hex(&bytes))
            .map_err(|error| CoreError::Config(format!("数据权利证据序列化失败：{error}")))
    }
}

fn selected(asset: &PilotDatasetAsset, request: &PilotDataRightsRequest) -> bool {
    match request.scope_kind {
        PilotDataRightsScopeKind::Student => asset
            .student_ref_sha256
            .eq_ignore_ascii_case(&request.scope_ref_sha256),
        PilotDataRightsScopeKind::Batch => asset
            .batch_ref_sha256
            .eq_ignore_ascii_case(&request.scope_ref_sha256),
    }
}

fn preflight(
    root: &Path,
    manifest: &PilotDatasetManifest,
    request: &PilotDataRightsRequest,
    request_sha256: &str,
) -> Preflight {
    let mut result = Preflight {
        selected_indexes: Vec::new(),
        verified_asset_count: 0,
        already_deleted_count: 0,
        total_bytes: 0,
        blockers: BTreeSet::new(),
    };
    for (index, asset) in manifest.assets.iter().enumerate() {
        if !selected(asset, request) {
            continue;
        }
        result.selected_indexes.push(index);
        result.total_bytes = result.total_bytes.saturating_add(asset.byte_size);
        match asset.state {
            PilotDatasetAssetState::Deleted => {
                result.already_deleted_count += 1;
                result.blockers.insert("ASSET_ALREADY_DELETED".into());
            }
            PilotDatasetAssetState::DeletionPending => {
                if asset.deletion_request_sha256.as_deref() != Some(request_sha256)
                    || request.action != PilotDataRightsAction::Delete
                    || request.mode != PilotDataRightsMode::Execute
                {
                    result.blockers.insert("ASSET_DELETE_PENDING".into());
                    continue;
                }
                let path = root.join(&asset.relative_path);
                if path.exists() {
                    match checked_asset_path(root, &asset.relative_path) {
                        Ok(path)
                            if fs::metadata(&path).map(|meta| meta.len()).ok()
                                == Some(asset.byte_size)
                                && hashing::sha256_file(&path).ok().as_deref()
                                    == Some(asset.artifact_sha256.as_str()) =>
                        {
                            result.verified_asset_count += 1;
                        }
                        _ => {
                            result.blockers.insert("ASSET_INTEGRITY_MISMATCH".into());
                        }
                    }
                } else {
                    // 上次执行可能已删除文件但尚未提交清单；同一请求可恢复完成。
                    result.verified_asset_count += 1;
                }
            }
            PilotDatasetAssetState::Active => {
                match checked_asset_path(root, &asset.relative_path) {
                    Ok(path) => match fs::metadata(&path) {
                        Ok(metadata)
                            if metadata.len() == asset.byte_size
                                && hashing::sha256_file(&path).ok().as_deref()
                                    == Some(asset.artifact_sha256.as_str()) =>
                        {
                            result.verified_asset_count += 1;
                        }
                        _ => {
                            result.blockers.insert("ASSET_INTEGRITY_MISMATCH".into());
                        }
                    },
                    Err(_) => {
                        result
                            .blockers
                            .insert("ASSET_PATH_UNSAFE_OR_MISSING".into());
                    }
                }
            }
        }
    }
    if result.selected_indexes.is_empty() {
        result.blockers.insert("SCOPE_EMPTY".into());
    }
    result
}

fn make_receipt(
    manifest: &PilotDatasetManifest,
    request: &PilotDataRightsRequest,
    request_sha256: String,
    before: String,
    preflight: &Preflight,
    execution: ReceiptExecution,
) -> CoreResult<PilotDataRightsReceipt> {
    let outcome = if execution.blockers.is_empty() {
        PilotDataRightsOutcome::Succeeded
    } else {
        PilotDataRightsOutcome::Blocked
    };
    let receipt_id = format!("pilot-rights-{}", &request_sha256[..20]);
    PilotDataRightsReceipt::new(PilotDataRightsReceiptPayload {
        schema_version: PILOT_DATA_RIGHTS_RECEIPT_SCHEMA_VERSION,
        receipt_id,
        request_sha256,
        idempotency_key_sha256: hashing::sha256_hex(request.idempotency_key.as_bytes()),
        gate_id: request.gate_id.clone(),
        dataset_id: manifest.dataset_id.clone(),
        action: request.action,
        mode: request.mode,
        scope_kind: request.scope_kind,
        scope_ref_sha256: request.scope_ref_sha256.trim().to_ascii_lowercase(),
        procedure_version: request.procedure_version.clone(),
        dataset_content_sha256_before: before,
        dataset_content_sha256_after: execution.after,
        outcome,
        selected_asset_count: preflight.selected_indexes.len(),
        verified_asset_count: preflight.verified_asset_count,
        already_deleted_count: preflight.already_deleted_count,
        total_bytes: preflight.total_bytes,
        exported_asset_count: execution.exported_asset_count,
        deleted_asset_count: execution.deleted_asset_count,
        export_manifest_sha256: execution.export_manifest_sha256,
        blocker_codes: execution.blockers.into_iter().collect(),
        content_excluded: true,
        paths_excluded: true,
        student_identity_excluded: true,
        completed_at: request.requested_at.clone(),
    })
}

fn write_export(
    root: &Path,
    output_root: &Path,
    manifest: &PilotDatasetManifest,
    request: &PilotDataRightsRequest,
    request_sha256: &str,
    indexes: &[usize],
) -> CoreResult<(usize, String)> {
    fs::create_dir_all(output_root).map_err(|error| CoreError::Io(error.to_string()))?;
    let output_root = canonical_root(output_root)?;
    if output_root.starts_with(root) || root.starts_with(&output_root) {
        return Err(CoreError::Invalid(
            "导出目录必须与受限试点数据目录分离".into(),
        ));
    }
    let final_dir = output_root.join(format!("pilot-export-{}", &request_sha256[..20]));
    let partial_dir = output_root.join(format!(".pilot-export-{}.partial", &request_sha256[..20]));
    if partial_dir.exists() {
        fs::remove_dir_all(&partial_dir).map_err(|error| CoreError::Io(error.to_string()))?;
    }
    let export_assets = indexes
        .iter()
        .map(|index| {
            let asset = &manifest.assets[*index];
            if asset.state != PilotDatasetAssetState::Active {
                return Err(CoreError::Invalid("只允许导出 active 试点资产".into()));
            }
            Ok(PilotExportAsset {
                asset_id: &asset.asset_id,
                student_ref_sha256: &asset.student_ref_sha256,
                batch_ref_sha256: &asset.batch_ref_sha256,
                data_type: asset.data_type,
                artifact_sha256: &asset.artifact_sha256,
                byte_size: asset.byte_size,
                bundle_relative_path: format!("files/{}", asset.asset_id),
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    let export_manifest = PilotExportManifest {
        schema_version: 1,
        dataset_id: &manifest.dataset_id,
        gate_id: &manifest.gate_id,
        scope_kind: request.scope_kind,
        scope_ref_sha256: &request.scope_ref_sha256,
        assets: export_assets,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&export_manifest)
        .map_err(|error| CoreError::Config(format!("导出清单序列化失败：{error}")))?;
    let manifest_sha256 = hashing::sha256_hex(&manifest_bytes);
    if final_dir.exists() {
        let final_root = canonical_root(&final_dir)?;
        let existing_manifest = checked_asset_path(&final_root, "manifest.json")?;
        if fs::read(existing_manifest).map_err(|error| CoreError::Io(error.to_string()))?
            != manifest_bytes
        {
            return Err(CoreError::Invalid("既有导出目录与当前请求不一致".into()));
        }
        for index in indexes {
            let asset = &manifest.assets[*index];
            let relative = format!("files/{}", asset.asset_id);
            let exported = checked_asset_path(&final_root, &relative)?;
            let metadata =
                fs::metadata(&exported).map_err(|error| CoreError::Io(error.to_string()))?;
            if metadata.len() != asset.byte_size
                || hashing::sha256_file(&exported)? != asset.artifact_sha256
            {
                return Err(CoreError::Invalid("既有导出资产完整性校验失败".into()));
            }
        }
        return Ok((indexes.len(), manifest_sha256));
    }
    fs::create_dir_all(partial_dir.join("files"))
        .map_err(|error| CoreError::Io(error.to_string()))?;

    let result = (|| -> CoreResult<(usize, String)> {
        for index in indexes {
            let asset = &manifest.assets[*index];
            let source = checked_asset_path(root, &asset.relative_path)?;
            let bundle_relative_path = format!("files/{}", asset.asset_id);
            let destination = partial_dir.join(&bundle_relative_path);
            fs::copy(&source, &destination).map_err(|error| CoreError::Io(error.to_string()))?;
            if hashing::sha256_file(&destination)? != asset.artifact_sha256 {
                return Err(CoreError::Io("导出副本 hash 校验失败".into()));
            }
        }
        atomic_write(&partial_dir.join("manifest.json"), &manifest_bytes)?;
        fs::rename(&partial_dir, &final_dir).map_err(|error| CoreError::Io(error.to_string()))?;
        Ok((indexes.len(), manifest_sha256.clone()))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&partial_dir);
    }
    result
}

/// 实际检查并执行试点数据权利请求。dry-run 永不修改数据集；execute export 生成隔离导出目录；
/// execute delete 先写 deletion_pending 再删文件，崩溃后可用同一请求恢复。
pub fn perform_pilot_data_rights(
    dataset_root: &Path,
    manifest_path: &Path,
    manifest: &mut PilotDatasetManifest,
    request: &PilotDataRightsRequest,
    output_root: Option<&Path>,
    confirm_delete: bool,
) -> CoreResult<PilotDataRightsReceipt> {
    manifest.validate()?;
    request.validate()?;
    if manifest.gate_id != request.gate_id {
        return Err(CoreError::Invalid("数据集与请求引用了不同闸门".into()));
    }
    let root = canonical_root(dataset_root)?;
    let manifest_metadata =
        fs::symlink_metadata(manifest_path).map_err(|error| CoreError::Io(error.to_string()))?;
    if manifest_metadata.file_type().is_symlink() || !manifest_metadata.is_file() {
        return Err(CoreError::Invalid(
            "数据集清单必须是受限目录内的普通文件".into(),
        ));
    }
    let canonical_manifest =
        fs::canonicalize(manifest_path).map_err(|error| CoreError::Io(error.to_string()))?;
    let manifest_parent = manifest_path
        .parent()
        .ok_or_else(|| CoreError::Invalid("数据集清单缺少父目录".into()))?;
    let manifest_parent =
        fs::canonicalize(manifest_parent).map_err(|error| CoreError::Io(error.to_string()))?;
    if !manifest_parent.starts_with(&root) || !canonical_manifest.starts_with(&root) {
        return Err(CoreError::Invalid("数据集清单必须位于受限根目录内".into()));
    }
    let manifest_on_disk: PilotDatasetManifest = serde_json::from_slice(
        &fs::read(&canonical_manifest).map_err(|error| CoreError::Io(error.to_string()))?,
    )
    .map_err(|error| CoreError::Invalid(format!("磁盘数据集清单 JSON 非法：{error}")))?;
    if &manifest_on_disk != manifest {
        return Err(CoreError::Invalid(
            "内存数据集与磁盘清单不一致，拒绝继续操作".into(),
        ));
    }

    let request_sha256 = request.request_sha256()?;
    let idempotency_key_sha256 = hashing::sha256_hex(request.idempotency_key.as_bytes());
    if let Some(receipt) = manifest
        .completed_operations
        .iter()
        .find(|receipt| receipt.payload.request_sha256 == request_sha256)
    {
        return Ok(receipt.clone());
    }
    if manifest.completed_operations.iter().any(|receipt| {
        receipt.payload.idempotency_key_sha256 == idempotency_key_sha256
            && receipt.payload.request_sha256 != request_sha256
    }) {
        return Err(CoreError::Invalid(
            "同一幂等键不能用于不同数据权利请求".into(),
        ));
    }

    let before = manifest.content_sha256()?;
    let preflight = preflight(&root, manifest, request, &request_sha256);
    if !preflight.blockers.is_empty() {
        return make_receipt(
            manifest,
            request,
            request_sha256,
            before.clone(),
            &preflight,
            ReceiptExecution {
                after: before,
                exported_asset_count: 0,
                deleted_asset_count: 0,
                export_manifest_sha256: None,
                blockers: preflight.blockers.clone(),
            },
        );
    }
    if request.mode == PilotDataRightsMode::DryRun {
        return make_receipt(
            manifest,
            request,
            request_sha256,
            before.clone(),
            &preflight,
            ReceiptExecution {
                after: before,
                exported_asset_count: 0,
                deleted_asset_count: 0,
                export_manifest_sha256: None,
                blockers: BTreeSet::new(),
            },
        );
    }

    match request.action {
        PilotDataRightsAction::Export => {
            let output_root = output_root
                .ok_or_else(|| CoreError::Invalid("执行导出必须提供独立输出目录".into()))?;
            let (count, export_manifest_sha256) = write_export(
                &root,
                output_root,
                manifest,
                request,
                &request_sha256,
                &preflight.selected_indexes,
            )?;
            let receipt = make_receipt(
                manifest,
                request,
                request_sha256,
                before.clone(),
                &preflight,
                ReceiptExecution {
                    after: before,
                    exported_asset_count: count,
                    deleted_asset_count: 0,
                    export_manifest_sha256: Some(export_manifest_sha256),
                    blockers: BTreeSet::new(),
                },
            )?;
            manifest.completed_operations.push(receipt.clone());
            manifest.write_atomic(manifest_path)?;
            Ok(receipt)
        }
        PilotDataRightsAction::Delete => {
            if !confirm_delete {
                return Err(CoreError::Invalid("执行删除必须显式提供确认标志".into()));
            }
            for index in &preflight.selected_indexes {
                let asset = &mut manifest.assets[*index];
                if asset.state == PilotDatasetAssetState::Active {
                    asset.state = PilotDatasetAssetState::DeletionPending;
                    asset.deletion_request_sha256 = Some(request_sha256.clone());
                }
            }
            manifest.write_atomic(manifest_path)?;

            let mut deleted_count = 0;
            for index in &preflight.selected_indexes {
                let asset = &manifest.assets[*index];
                if asset.state == PilotDatasetAssetState::Deleted {
                    continue;
                }
                let path = root.join(safe_relative_path(&asset.relative_path)?);
                if path.exists() {
                    let path = checked_asset_path(&root, &asset.relative_path)?;
                    if hashing::sha256_file(&path)? != asset.artifact_sha256 {
                        return Err(CoreError::Io(
                            "删除前资产 hash 漂移，已保留 pending 以供人工核对".into(),
                        ));
                    }
                    fs::remove_file(&path).map_err(|error| CoreError::Io(error.to_string()))?;
                }
                deleted_count += 1;
            }
            for index in &preflight.selected_indexes {
                let asset = &mut manifest.assets[*index];
                if asset.state != PilotDatasetAssetState::Deleted {
                    asset.state = PilotDatasetAssetState::Deleted;
                    asset.deleted_at = Some(request.requested_at.clone());
                    // 内容身份不包含回执 hash；先用合法占位完成 after hash，再替换为真实回执。
                    asset.deletion_receipt_sha256 = Some("0".repeat(64));
                }
            }
            let after = manifest.content_sha256()?;
            let receipt = make_receipt(
                manifest,
                request,
                request_sha256,
                before,
                &preflight,
                ReceiptExecution {
                    after,
                    exported_asset_count: 0,
                    deleted_asset_count: deleted_count,
                    export_manifest_sha256: None,
                    blockers: BTreeSet::new(),
                },
            )?;
            for index in &preflight.selected_indexes {
                let asset = &mut manifest.assets[*index];
                asset.deletion_receipt_sha256 = Some(receipt.receipt_sha256.clone());
            }
            manifest.completed_operations.push(receipt.clone());
            manifest.write_atomic(manifest_path)?;
            Ok(receipt)
        }
    }
}

pub fn build_pilot_data_rights_evidence(
    gate_id: String,
    dataset_id: String,
    generated_at: String,
    receipts: Vec<PilotDataRightsReceipt>,
) -> CoreResult<PilotDataRightsEvidenceBundle> {
    let bundle = PilotDataRightsEvidenceBundle {
        schema_version: PILOT_DATA_RIGHTS_EVIDENCE_SCHEMA_VERSION,
        gate_id,
        dataset_id,
        generated_at,
        receipts,
    };
    bundle.validate()?;
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
        output: PathBuf,
        manifest_path: PathBuf,
        manifest: PilotDatasetManifest,
        student_one: String,
        batch: String,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
            let _ = fs::remove_dir_all(&self.output);
        }
    }

    fn fixture() -> Fixture {
        let unique = ids::new_public_id();
        let root = std::env::temp_dir().join(format!("jiaofu-pilot-rights-{unique}"));
        let output = std::env::temp_dir().join(format!("jiaofu-pilot-export-{unique}"));
        fs::create_dir_all(root.join("assets")).unwrap();
        let one = b"student-one-page";
        let two = b"student-two-page";
        fs::write(root.join("assets/asset-one"), one).unwrap();
        fs::write(root.join("assets/asset-two"), two).unwrap();
        let student_one = "1".repeat(64);
        let student_two = "2".repeat(64);
        let batch = "b".repeat(64);
        let manifest = PilotDatasetManifest {
            schema_version: 1,
            dataset_id: "pilot-dataset-opaque-001".into(),
            gate_id: "pilot-gate-opaque-001".into(),
            assets: vec![
                PilotDatasetAsset {
                    asset_id: "asset-one".into(),
                    student_ref_sha256: student_one.clone(),
                    batch_ref_sha256: batch.clone(),
                    data_type: PilotDataType::StudentPageImage,
                    artifact_sha256: hashing::sha256_hex(one),
                    byte_size: one.len() as u64,
                    relative_path: "assets/asset-one".into(),
                    state: PilotDatasetAssetState::Active,
                    deletion_request_sha256: None,
                    deleted_at: None,
                    deletion_receipt_sha256: None,
                },
                PilotDatasetAsset {
                    asset_id: "asset-two".into(),
                    student_ref_sha256: student_two,
                    batch_ref_sha256: batch.clone(),
                    data_type: PilotDataType::AnswerRegionImage,
                    artifact_sha256: hashing::sha256_hex(two),
                    byte_size: two.len() as u64,
                    relative_path: "assets/asset-two".into(),
                    state: PilotDatasetAssetState::Active,
                    deletion_request_sha256: None,
                    deleted_at: None,
                    deletion_receipt_sha256: None,
                },
            ],
            completed_operations: Vec::new(),
        };
        let manifest_path = root.join("pilot_dataset.json");
        manifest.write_atomic(&manifest_path).unwrap();
        Fixture {
            root,
            output,
            manifest_path,
            manifest,
            student_one,
            batch,
        }
    }

    fn request(
        scope_kind: PilotDataRightsScopeKind,
        scope_ref_sha256: String,
        action: PilotDataRightsAction,
        mode: PilotDataRightsMode,
        key: &str,
    ) -> PilotDataRightsRequest {
        PilotDataRightsRequest {
            gate_id: "pilot-gate-opaque-001".into(),
            action,
            mode,
            scope_kind,
            scope_ref_sha256,
            procedure_version: expected_procedure(scope_kind, action).into(),
            idempotency_key: key.into(),
            requested_at: "2026-07-15T08:00:00Z".into(),
        }
    }

    #[test]
    fn four_real_dry_runs_create_content_free_evidence_bundle() {
        let mut fixture = fixture();
        let receipts = [
            request(
                PilotDataRightsScopeKind::Student,
                fixture.student_one.clone(),
                PilotDataRightsAction::Export,
                PilotDataRightsMode::DryRun,
                "student-export-dry",
            ),
            request(
                PilotDataRightsScopeKind::Batch,
                fixture.batch.clone(),
                PilotDataRightsAction::Export,
                PilotDataRightsMode::DryRun,
                "batch-export-dry",
            ),
            request(
                PilotDataRightsScopeKind::Student,
                fixture.student_one.clone(),
                PilotDataRightsAction::Delete,
                PilotDataRightsMode::DryRun,
                "student-delete-dry",
            ),
            request(
                PilotDataRightsScopeKind::Batch,
                fixture.batch.clone(),
                PilotDataRightsAction::Delete,
                PilotDataRightsMode::DryRun,
                "batch-delete-dry",
            ),
        ]
        .iter()
        .map(|request| {
            perform_pilot_data_rights(
                &fixture.root,
                &fixture.manifest_path,
                &mut fixture.manifest,
                request,
                None,
                false,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
        let evidence = build_pilot_data_rights_evidence(
            fixture.manifest.gate_id.clone(),
            fixture.manifest.dataset_id.clone(),
            "2026-07-15T09:00:00Z".into(),
            receipts,
        )
        .unwrap();
        assert_eq!(evidence.receipts.len(), 4);
        assert_eq!(evidence.evidence_sha256().unwrap().len(), 64);
        let json = serde_json::to_string(&evidence).unwrap();
        assert!(!json.contains("student-one-page"));
        assert!(!json.contains("assets/"));
        assert!(!json.contains(fixture.root.to_string_lossy().as_ref()));
    }

    #[test]
    fn repository_synthetic_dataset_contract_is_valid() {
        let manifest: PilotDatasetManifest = serde_json::from_str(include_str!(
            "../tests/fixtures/material_golden/pilot_dataset_synthetic_contract_v1.json"
        ))
        .unwrap();
        manifest.validate().unwrap();
        assert_eq!(manifest.assets.len(), 1);
        assert!(manifest.completed_operations.is_empty());
    }

    #[test]
    fn execute_export_copies_verified_files_to_internal_names_and_is_idempotent() {
        let mut fixture = fixture();
        let request = request(
            PilotDataRightsScopeKind::Student,
            fixture.student_one.clone(),
            PilotDataRightsAction::Export,
            PilotDataRightsMode::Execute,
            "student-export-execute",
        );
        let receipt = perform_pilot_data_rights(
            &fixture.root,
            &fixture.manifest_path,
            &mut fixture.manifest,
            &request,
            Some(&fixture.output),
            false,
        )
        .unwrap();
        assert_eq!(receipt.payload.exported_asset_count, 1);
        assert!(receipt.payload.export_manifest_sha256.is_some());
        let export_dir = fixture.output.join(format!(
            "pilot-export-{}",
            &request.request_sha256().unwrap()[..20]
        ));
        assert!(export_dir.join("files/asset-one").is_file());
        assert!(export_dir.join("manifest.json").is_file());

        let repeated = perform_pilot_data_rights(
            &fixture.root,
            &fixture.manifest_path,
            &mut fixture.manifest,
            &request,
            Some(&fixture.output),
            false,
        )
        .unwrap();
        assert_eq!(receipt, repeated);
        assert_eq!(fixture.manifest.completed_operations.len(), 1);
    }

    #[test]
    fn completed_export_directory_recovers_before_receipt_ledger_commit() {
        let mut fixture = fixture();
        let request = request(
            PilotDataRightsScopeKind::Student,
            fixture.student_one.clone(),
            PilotDataRightsAction::Export,
            PilotDataRightsMode::Execute,
            "student-export-recover",
        );
        let request_sha256 = request.request_sha256().unwrap();
        let root = canonical_root(&fixture.root).unwrap();
        let preflight = preflight(&root, &fixture.manifest, &request, &request_sha256);
        let first = write_export(
            &root,
            &fixture.output,
            &fixture.manifest,
            &request,
            &request_sha256,
            &preflight.selected_indexes,
        )
        .unwrap();
        assert!(fixture.manifest.completed_operations.is_empty());

        let receipt = perform_pilot_data_rights(
            &fixture.root,
            &fixture.manifest_path,
            &mut fixture.manifest,
            &request,
            Some(&fixture.output),
            false,
        )
        .unwrap();
        assert_eq!(receipt.payload.exported_asset_count, 1);
        assert_eq!(receipt.payload.export_manifest_sha256, Some(first.1));
        assert_eq!(fixture.manifest.completed_operations.len(), 1);
    }

    #[test]
    fn execute_delete_requires_confirmation_removes_only_scope_and_returns_no_content() {
        let mut fixture = fixture();
        let request = request(
            PilotDataRightsScopeKind::Student,
            fixture.student_one.clone(),
            PilotDataRightsAction::Delete,
            PilotDataRightsMode::Execute,
            "student-delete-execute",
        );
        assert!(perform_pilot_data_rights(
            &fixture.root,
            &fixture.manifest_path,
            &mut fixture.manifest,
            &request,
            None,
            false,
        )
        .is_err());
        assert!(fixture.root.join("assets/asset-one").is_file());

        let receipt = perform_pilot_data_rights(
            &fixture.root,
            &fixture.manifest_path,
            &mut fixture.manifest,
            &request,
            None,
            true,
        )
        .unwrap();
        assert_eq!(receipt.payload.deleted_asset_count, 1);
        assert!(!fixture.root.join("assets/asset-one").exists());
        assert!(fixture.root.join("assets/asset-two").is_file());
        assert_eq!(
            fixture.manifest.assets[0].state,
            PilotDatasetAssetState::Deleted
        );
        let json = serde_json::to_string(&receipt).unwrap();
        assert!(!json.contains("asset-one"));
        assert!(!json.contains("assets/"));

        let repeated = perform_pilot_data_rights(
            &fixture.root,
            &fixture.manifest_path,
            &mut fixture.manifest,
            &request,
            None,
            true,
        )
        .unwrap();
        assert_eq!(receipt, repeated);
    }

    #[test]
    fn interrupted_delete_resumes_only_with_the_same_request() {
        let mut first_fixture = fixture();
        let resume_request = request(
            PilotDataRightsScopeKind::Student,
            first_fixture.student_one.clone(),
            PilotDataRightsAction::Delete,
            PilotDataRightsMode::Execute,
            "student-delete-resume",
        );
        let request_sha256 = resume_request.request_sha256().unwrap();
        first_fixture.manifest.assets[0].state = PilotDatasetAssetState::DeletionPending;
        first_fixture.manifest.assets[0].deletion_request_sha256 = Some(request_sha256);
        first_fixture
            .manifest
            .write_atomic(&first_fixture.manifest_path)
            .unwrap();
        fs::remove_file(first_fixture.root.join("assets/asset-one")).unwrap();

        let receipt = perform_pilot_data_rights(
            &first_fixture.root,
            &first_fixture.manifest_path,
            &mut first_fixture.manifest,
            &resume_request,
            None,
            true,
        )
        .unwrap();
        assert_eq!(receipt.payload.deleted_asset_count, 1);
        assert_eq!(
            first_fixture.manifest.assets[0].state,
            PilotDatasetAssetState::Deleted
        );

        let mut second_fixture = fixture();
        let different_request = request(
            PilotDataRightsScopeKind::Student,
            second_fixture.student_one.clone(),
            PilotDataRightsAction::Delete,
            PilotDataRightsMode::Execute,
            "different-delete-request",
        );
        second_fixture.manifest.assets[0].state = PilotDatasetAssetState::DeletionPending;
        second_fixture.manifest.assets[0].deletion_request_sha256 = Some("f".repeat(64));
        second_fixture
            .manifest
            .write_atomic(&second_fixture.manifest_path)
            .unwrap();
        let blocked = perform_pilot_data_rights(
            &second_fixture.root,
            &second_fixture.manifest_path,
            &mut second_fixture.manifest,
            &different_request,
            None,
            true,
        )
        .unwrap();
        assert_eq!(blocked.payload.outcome, PilotDataRightsOutcome::Blocked);
        assert_eq!(blocked.payload.blocker_codes, vec!["ASSET_DELETE_PENDING"]);
        assert!(second_fixture.root.join("assets/asset-one").is_file());
    }

    #[test]
    fn completed_idempotency_key_cannot_be_reused_for_a_different_request() {
        let mut fixture = fixture();
        let first = request(
            PilotDataRightsScopeKind::Student,
            fixture.student_one.clone(),
            PilotDataRightsAction::Export,
            PilotDataRightsMode::Execute,
            "shared-key",
        );
        perform_pilot_data_rights(
            &fixture.root,
            &fixture.manifest_path,
            &mut fixture.manifest,
            &first,
            Some(&fixture.output),
            false,
        )
        .unwrap();
        let second = request(
            PilotDataRightsScopeKind::Batch,
            fixture.batch.clone(),
            PilotDataRightsAction::Export,
            PilotDataRightsMode::Execute,
            "shared-key",
        );
        assert!(perform_pilot_data_rights(
            &fixture.root,
            &fixture.manifest_path,
            &mut fixture.manifest,
            &second,
            Some(&fixture.output),
            false,
        )
        .is_err());
    }

    #[test]
    fn hash_mismatch_and_unsafe_path_block_without_mutation() {
        let mut fixture = fixture();
        fs::write(fixture.root.join("assets/asset-one"), b"tampered").unwrap();
        let request = request(
            PilotDataRightsScopeKind::Student,
            fixture.student_one.clone(),
            PilotDataRightsAction::Delete,
            PilotDataRightsMode::DryRun,
            "tampered-dry-run",
        );
        let receipt = perform_pilot_data_rights(
            &fixture.root,
            &fixture.manifest_path,
            &mut fixture.manifest,
            &request,
            None,
            false,
        )
        .unwrap();
        assert_eq!(receipt.payload.outcome, PilotDataRightsOutcome::Blocked);
        assert_eq!(
            receipt.payload.blocker_codes,
            vec!["ASSET_INTEGRITY_MISMATCH"]
        );
        assert_eq!(
            fixture.manifest.assets[0].state,
            PilotDatasetAssetState::Active
        );

        fixture.manifest.assets[0].relative_path = "../outside".into();
        assert!(fixture.manifest.validate().is_err());
    }
}
