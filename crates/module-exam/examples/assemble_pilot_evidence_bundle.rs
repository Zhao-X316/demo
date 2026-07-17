use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use module_exam::pilot_data_gate::PilotDataGateManifest;
use module_exam::pilot_data_rights::PilotDataRightsEvidenceBundle;
use module_exam::pilot_evidence_bundle::{
    assemble_pilot_evidence_bundle, write_pilot_evidence_bundle_once,
};
use module_exam::shadow_pilot::ShadowPilotSessionResult;
use module_exam::teacher_shadow::TeacherShadowReport;
use serde::de::DeserializeOwned;

fn argument(name: &str) -> Result<String, String> {
    let mut args = env::args().skip(1);
    while let Some(value) = args.next() {
        if value == name {
            return args.next().ok_or_else(|| format!("{name} 缺少值"));
        }
    }
    Err(format!("缺少 {name}"))
}

fn read_json<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T, String> {
    serde_json::from_slice(
        &fs::read(path)
            .map_err(|error| format!("无法读取 {label} {} ：{error}", path.display()))?,
    )
    .map_err(|error| format!("{label} JSON 非法：{error}"))
}

fn run() -> Result<(), String> {
    let gate_path = PathBuf::from(argument("--gate")?);
    let rights_path = PathBuf::from(argument("--rights-evidence")?);
    let shadow_path = PathBuf::from(argument("--shadow-result")?);
    let teacher_path = PathBuf::from(argument("--teacher-report")?);
    let gate: PilotDataGateManifest = read_json(&gate_path, "--gate")?;
    let rights: PilotDataRightsEvidenceBundle = read_json(&rights_path, "--rights-evidence")?;
    let shadow: ShadowPilotSessionResult = read_json(&shadow_path, "--shadow-result")?;
    let teacher: TeacherShadowReport = read_json(&teacher_path, "--teacher-report")?;
    let bundle = assemble_pilot_evidence_bundle(
        &gate,
        &rights,
        &shadow,
        &teacher,
        &argument("--assembled-at")?,
    )
    .map_err(|error| error.to_string())?;
    let output = PathBuf::from(argument("--output")?);
    let stored =
        write_pilot_evidence_bundle_once(&output, &bundle).map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&stored)
            .map_err(|error| format!("试点总验收包序列化失败：{error}"))?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
