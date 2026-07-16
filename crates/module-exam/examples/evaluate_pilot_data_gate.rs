use std::env;
use std::fs;
use std::path::PathBuf;

use module_exam::pilot_data_gate::PilotDataGateManifest;

fn argument(name: &str) -> Result<String, String> {
    let mut args = env::args().skip(1);
    while let Some(value) = args.next() {
        if value == name {
            return args.next().ok_or_else(|| format!("{name} 缺少值"));
        }
    }
    Err(format!("缺少 {name}"))
}

fn run() -> Result<(), String> {
    let manifest_path = PathBuf::from(argument("--manifest")?);
    let evaluated_on = argument("--as-of")?;
    let manifest: PilotDataGateManifest = serde_json::from_slice(
        &fs::read(&manifest_path)
            .map_err(|error| format!("无法读取闸门 {}：{error}", manifest_path.display()))?,
    )
    .map_err(|error| format!("闸门 JSON 非法：{error}"))?;
    let report = manifest
        .evaluate(&evaluated_on)
        .map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|error| format!("报告序列化失败：{error}"))?
    );
    if !report.real_data_allowed {
        return Err("真实学生数据闸门未放行".into());
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
