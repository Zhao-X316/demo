use std::env;
use std::fs;
use std::path::PathBuf;

use module_exam::material_golden::{
    evaluate_material_calibration, MaterialGoldenManifest, MaterialGoldenPredictionSet,
};

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
    let report = evaluate_material_calibration(&manifest, &predictions)
        .map_err(|error| error.to_string())?;
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
