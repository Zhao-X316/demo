use std::env;
use std::fs;
use std::path::PathBuf;

use module_recitation::pilot_metrics::{
    evaluate_real_recitation_pilot, evaluate_recitation_pilot, write_recitation_pilot_report_once,
    RecitationPilotAuthorization, RecitationPilotObservationSet,
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

fn optional_argument(name: &str) -> Result<Option<String>, String> {
    let mut args = env::args().skip(1);
    while let Some(value) = args.next() {
        if value == name {
            return args
                .next()
                .map(Some)
                .ok_or_else(|| format!("{name} 缺少值"));
        }
    }
    Ok(None)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &PathBuf, label: &str) -> Result<T, String> {
    serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("无法读取 {label} {}：{error}", path.display()))?,
    )
    .map_err(|error| format!("{label} JSON 非法：{error}"))
}

fn run() -> Result<(), String> {
    let observations_path = PathBuf::from(argument("--observations")?);
    let observations: RecitationPilotObservationSet =
        read_json(&observations_path, "背诵试点观察")?;
    let generated_at = argument("--generated-at")?;
    let evaluated_on = argument("--evaluated-on")?;
    let report = if let Some(path) = optional_argument("--authorization")? {
        let authorization_path = PathBuf::from(path);
        let authorization: RecitationPilotAuthorization =
            read_json(&authorization_path, "背诵试点授权")?;
        evaluate_real_recitation_pilot(&observations, &authorization, &generated_at, &evaluated_on)
    } else {
        evaluate_recitation_pilot(&observations, &generated_at, &evaluated_on)
    }
    .map_err(|error| error.to_string())?;
    let output = PathBuf::from(argument("--output")?);
    let stored =
        write_recitation_pilot_report_once(&output, &report).map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&stored)
            .map_err(|error| format!("背诵试点报告序列化失败：{error}"))?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
