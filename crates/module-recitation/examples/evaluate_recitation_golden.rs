use std::env;
use std::fs;
use std::path::PathBuf;

use module_recitation::golden::{
    evaluate_real_recitation_golden, evaluate_recitation_golden, RecitationGoldenAuthorization,
    RecitationGoldenManifest, RecitationGoldenPredictionSet,
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

fn optional_argument(name: &str) -> Option<String> {
    let mut args = env::args().skip(1);
    while let Some(value) = args.next() {
        if value == name {
            return args.next();
        }
    }
    None
}

fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf, label: &str) -> Result<T, String> {
    let bytes =
        fs::read(path).map_err(|error| format!("无法读取 {label} {}：{error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{label} JSON 非法：{error}"))
}

fn run() -> Result<(), String> {
    let manifest_path = argument("--manifest")?;
    let predictions_path = argument("--predictions")?;
    let manifest: RecitationGoldenManifest = read_json(&manifest_path, "manifest")?;
    let predictions: RecitationGoldenPredictionSet = read_json(&predictions_path, "predictions")?;
    let report = if manifest.contains_real_data() {
        let authorization_path = PathBuf::from(
            optional_argument("--authorization")
                .ok_or_else(|| "真实黄金集必须提供 --authorization 授权清单".to_owned())?,
        );
        let evaluated_on = optional_argument("--as-of")
            .ok_or_else(|| "真实黄金集必须提供 --as-of YYYY-MM-DD".to_owned())?;
        let authorization: RecitationGoldenAuthorization =
            read_json(&authorization_path, "authorization")?;
        evaluate_real_recitation_golden(&manifest, &predictions, &authorization, &evaluated_on)
            .map_err(|error| error.to_string())?
    } else {
        evaluate_recitation_golden(&manifest, &predictions).map_err(|error| error.to_string())?
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
