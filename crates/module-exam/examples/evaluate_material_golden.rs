use std::env;
use std::fs;
use std::path::PathBuf;

use module_exam::material_golden::{
    evaluate_material_calibration, evaluate_real_material_calibration, MaterialGoldenManifest,
    MaterialGoldenPredictionSet,
};
use module_exam::pilot_data_gate::PilotDataGateManifest;
use module_exam::pilot_data_rights::PilotDataRightsEvidenceBundle;

fn argument(name: &str) -> Result<PathBuf, String> {
    let mut args = env::args().skip(1);
    while let Some(value) = args.next() {
        if value == name {
            return args
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| format!("{name} 缺少路径"));
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

fn run() -> Result<(), String> {
    let manifest_path = argument("--manifest")?;
    let predictions_path = argument("--predictions")?;
    let manifest: MaterialGoldenManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| format!("无法读取 manifest {}：{error}", manifest_path.display()))?,
    )
    .map_err(|error| format!("manifest JSON 非法：{error}"))?;
    let predictions: MaterialGoldenPredictionSet =
        serde_json::from_slice(&fs::read(&predictions_path).map_err(|error| {
            format!(
                "无法读取 predictions {}：{error}",
                predictions_path.display()
            )
        })?)
        .map_err(|error| format!("predictions JSON 非法：{error}"))?;
    let report = if manifest.contains_real_data() {
        let gate_path = PathBuf::from(
            optional_argument("--gate")
                .ok_or_else(|| "真实黄金集必须提供 --gate 闸门清单".to_owned())?,
        );
        let evaluated_on = optional_argument("--as-of")
            .ok_or_else(|| "真实黄金集必须提供 --as-of YYYY-MM-DD".to_owned())?;
        let rights_evidence_path = PathBuf::from(
            optional_argument("--rights-evidence")
                .ok_or_else(|| "真实黄金集必须提供 --rights-evidence 演练证据".to_owned())?,
        );
        let gate: PilotDataGateManifest = serde_json::from_slice(
            &fs::read(&gate_path)
                .map_err(|error| format!("无法读取 gate {}：{error}", gate_path.display()))?,
        )
        .map_err(|error| format!("gate JSON 非法：{error}"))?;
        let rights_evidence: PilotDataRightsEvidenceBundle =
            serde_json::from_slice(&fs::read(&rights_evidence_path).map_err(|error| {
                format!(
                    "无法读取 rights evidence {}：{error}",
                    rights_evidence_path.display()
                )
            })?)
            .map_err(|error| format!("rights evidence JSON 非法：{error}"))?;
        evaluate_real_material_calibration(
            &manifest,
            &predictions,
            &gate,
            &rights_evidence,
            &evaluated_on,
        )
        .map_err(|error| error.to_string())?
    } else {
        evaluate_material_calibration(&manifest, &predictions).map_err(|error| error.to_string())?
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|error| format!("报告序列化失败：{error}"))?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
