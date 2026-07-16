use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use module_exam::pilot_data_rights::{
    perform_pilot_data_rights, PilotDataRightsAction, PilotDataRightsMode, PilotDataRightsOutcome,
    PilotDataRightsRequest, PilotDataRightsScopeKind, PilotDatasetManifest,
    PILOT_BATCH_DELETE_PROCEDURE_VERSION, PILOT_BATCH_EXPORT_PROCEDURE_VERSION,
    PILOT_STUDENT_DELETE_PROCEDURE_VERSION, PILOT_STUDENT_EXPORT_PROCEDURE_VERSION,
};

fn argument(name: &str) -> Result<String, String> {
    let mut args = env::args().skip(1);
    while let Some(value) = args.next() {
        if value == name {
            return args.next().ok_or_else(|| format!("{name} 缺少值"));
        }
    }
    Err(format!("缺少 {name}"))
}

fn optional_argument(name: &str) -> Option<String> {
    let mut args = env::args().skip(1);
    while let Some(value) = args.next() {
        if value == name {
            return args.next();
        }
    }
    None
}

fn flag(name: &str) -> bool {
    env::args().skip(1).any(|value| value == name)
}

fn parse_scope(value: &str) -> Result<PilotDataRightsScopeKind, String> {
    match value {
        "student" => Ok(PilotDataRightsScopeKind::Student),
        "batch" => Ok(PilotDataRightsScopeKind::Batch),
        _ => Err("--scope 只能是 student 或 batch".into()),
    }
}

fn parse_action(value: &str) -> Result<PilotDataRightsAction, String> {
    match value {
        "export" => Ok(PilotDataRightsAction::Export),
        "delete" => Ok(PilotDataRightsAction::Delete),
        _ => Err("--action 只能是 export 或 delete".into()),
    }
}

fn parse_mode(value: &str) -> Result<PilotDataRightsMode, String> {
    match value {
        "dry-run" => Ok(PilotDataRightsMode::DryRun),
        "execute" => Ok(PilotDataRightsMode::Execute),
        _ => Err("--mode 只能是 dry-run 或 execute".into()),
    }
}

fn procedure_version(
    scope: PilotDataRightsScopeKind,
    action: PilotDataRightsAction,
) -> &'static str {
    match (scope, action) {
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

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "回执路径缺少父目录".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建回执目录：{error}"))?;
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    std::io::Write::write_all(
        &mut options
            .open(path)
            .map_err(|error| format!("回执已存在或无法创建 {}：{error}", path.display()))?,
        bytes,
    )
    .map_err(|error| format!("无法写入回执：{error}"))
}

fn run() -> Result<(), String> {
    let dataset_root = PathBuf::from(argument("--dataset-root")?);
    let manifest_path = PathBuf::from(argument("--manifest")?);
    let receipt_path = PathBuf::from(argument("--receipt")?);
    if receipt_path.exists() {
        return Err(format!(
            "回执路径已存在，拒绝覆盖：{}",
            receipt_path.display()
        ));
    }
    let scope_kind = parse_scope(&argument("--scope")?)?;
    let action = parse_action(&argument("--action")?)?;
    let mode = parse_mode(&argument("--mode")?)?;
    let requested_procedure = optional_argument("--procedure-version")
        .unwrap_or_else(|| procedure_version(scope_kind, action).to_owned());
    let request = PilotDataRightsRequest {
        gate_id: argument("--gate-id")?,
        action,
        mode,
        scope_kind,
        scope_ref_sha256: argument("--scope-ref-sha256")?,
        procedure_version: requested_procedure,
        idempotency_key: argument("--idempotency-key")?,
        requested_at: argument("--requested-at")?,
    };
    let mut manifest: PilotDatasetManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| format!("无法读取清单 {}：{error}", manifest_path.display()))?,
    )
    .map_err(|error| format!("数据集清单 JSON 非法：{error}"))?;
    let output_root = optional_argument("--output-root").map(PathBuf::from);
    let receipt = perform_pilot_data_rights(
        &dataset_root,
        &manifest_path,
        &mut manifest,
        &request,
        output_root.as_deref(),
        flag("--confirm-delete"),
    )
    .map_err(|error| error.to_string())?;
    let bytes =
        serde_json::to_vec_pretty(&receipt).map_err(|error| format!("回执序列化失败：{error}"))?;
    write_new(&receipt_path, &bytes)?;
    println!(
        "{}",
        String::from_utf8(bytes).map_err(|_| "回执不是 UTF-8".to_owned())?
    );
    if receipt.payload.outcome == PilotDataRightsOutcome::Blocked {
        return Err(format!(
            "数据权利请求被阻断：{}",
            receipt.payload.blocker_codes.join(",")
        ));
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
