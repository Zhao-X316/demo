use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use module_exam::pilot_data_rights::{build_pilot_data_rights_evidence, PilotDataRightsReceipt};

fn argument(name: &str) -> Result<String, String> {
    let mut args = env::args().skip(1);
    while let Some(value) = args.next() {
        if value == name {
            return args.next().ok_or_else(|| format!("{name} 缺少值"));
        }
    }
    Err(format!("缺少 {name}"))
}

fn repeated_argument(name: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut args = env::args().skip(1);
    while let Some(value) = args.next() {
        if value == name {
            if let Some(value) = args.next() {
                values.push(value);
            }
        }
    }
    values
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "证据路径缺少父目录".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建证据目录：{error}"))?;
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    std::io::Write::write_all(
        &mut options
            .open(path)
            .map_err(|error| format!("证据已存在或无法创建 {}：{error}", path.display()))?,
        bytes,
    )
    .map_err(|error| format!("无法写入证据：{error}"))
}

fn run() -> Result<(), String> {
    let receipt_paths = repeated_argument("--receipt");
    if receipt_paths.len() != 4 {
        return Err("必须提供四个 --receipt dry-run 回执".into());
    }
    let receipts = receipt_paths
        .iter()
        .map(|path| {
            let path = PathBuf::from(path);
            serde_json::from_slice::<PilotDataRightsReceipt>(
                &fs::read(&path)
                    .map_err(|error| format!("无法读取回执 {}：{error}", path.display()))?,
            )
            .map_err(|error| format!("回执 JSON 非法 {}：{error}", path.display()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let evidence = build_pilot_data_rights_evidence(
        argument("--gate-id")?,
        argument("--dataset-id")?,
        argument("--generated-at")?,
        receipts,
    )
    .map_err(|error| error.to_string())?;
    let evidence_sha256 = evidence
        .evidence_sha256()
        .map_err(|error| error.to_string())?;
    let bytes =
        serde_json::to_vec_pretty(&evidence).map_err(|error| format!("证据序列化失败：{error}"))?;
    let output = PathBuf::from(argument("--output")?);
    write_new(&output, &bytes)?;
    println!("{{\"evidence_sha256\":\"{evidence_sha256}\"}}");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
