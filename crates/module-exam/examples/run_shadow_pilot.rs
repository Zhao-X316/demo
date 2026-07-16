use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use module_exam::material_golden::{MaterialGoldenManifest, MaterialGoldenPredictionSet};
use module_exam::pilot_data_gate::PilotDataGateManifest;
use module_exam::pilot_data_rights::PilotDataRightsEvidenceBundle;
use module_exam::shadow_pilot::{
    evaluate_shadow_pilot_session, write_shadow_pilot_result_once, ShadowPilotMaterialInput,
    ShadowPilotSessionRequest,
};
use serde::de::DeserializeOwned;
use suite_core::domain::hashing;

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

fn read_json<T: DeserializeOwned>(path: &Path, label: &str) -> Result<T, String> {
    serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("无法读取 {label} {}：{error}", path.display()))?,
    )
    .map_err(|error| format!("{label} JSON 非法：{error}"))
}

fn material_input(
    manifest_arg: &str,
    predictions_arg: &str,
) -> Result<ShadowPilotMaterialInput, String> {
    let manifest_path = PathBuf::from(argument(manifest_arg)?);
    let predictions_path = PathBuf::from(argument(predictions_arg)?);
    Ok(ShadowPilotMaterialInput {
        manifest: read_json::<MaterialGoldenManifest>(&manifest_path, manifest_arg)?,
        predictions: read_json::<MaterialGoldenPredictionSet>(&predictions_path, predictions_arg)?,
    })
}

fn run() -> Result<(), String> {
    let inputs = vec![
        material_input("--ordinary-manifest", "--ordinary-predictions")?,
        material_input("--answer-sheet-manifest", "--answer-sheet-predictions")?,
        material_input("--dictation-manifest", "--dictation-predictions")?,
    ];
    let gate_path = optional_argument("--gate").map(PathBuf::from);
    let rights_path = optional_argument("--rights-evidence").map(PathBuf::from);
    if gate_path.is_some() != rights_path.is_some() {
        return Err("--gate 与 --rights-evidence 必须同时提供".into());
    }
    let gate = gate_path
        .as_deref()
        .map(|path| read_json::<PilotDataGateManifest>(path, "--gate"))
        .transpose()?;
    let rights_evidence = rights_path
        .as_deref()
        .map(|path| read_json::<PilotDataRightsEvidenceBundle>(path, "--rights-evidence"))
        .transpose()?;
    let request = ShadowPilotSessionRequest {
        session_id: argument("--session-id")?,
        evaluated_on: argument("--as-of")?,
        started_at: argument("--started-at")?,
        predictions_generated_at: argument("--predictions-generated-at")?,
        provider_ref_sha256: hashing::sha256_hex(argument("--provider-ref")?.as_bytes()),
        model_ref_sha256: hashing::sha256_hex(argument("--model-ref")?.as_bytes()),
        provider_config_sha256: hashing::sha256_hex(
            argument("--provider-config-version")?.as_bytes(),
        ),
    };
    let completed_at = optional_argument("--completed-at")
        .unwrap_or_else(|| Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true));
    let candidate = evaluate_shadow_pilot_session(
        &request,
        inputs,
        gate.as_ref(),
        rights_evidence.as_ref(),
        &completed_at,
    )
    .map_err(|error| error.to_string())?;
    let output = PathBuf::from(argument("--output")?);
    let stored =
        write_shadow_pilot_result_once(&output, &candidate).map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&stored)
            .map_err(|error| format!("结果序列化失败：{error}"))?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
