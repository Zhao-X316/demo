use std::env;
use std::fs;
use std::path::PathBuf;

use module_exam::teacher_shadow::{
    evaluate_teacher_shadow, write_teacher_shadow_report_once, TeacherShadowObservationSet,
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

fn run() -> Result<(), String> {
    let observations_path = PathBuf::from(argument("--observations")?);
    let observations: TeacherShadowObservationSet =
        serde_json::from_slice(&fs::read(&observations_path).map_err(|error| {
            format!(
                "无法读取老师影子观察 {}：{error}",
                observations_path.display()
            )
        })?)
        .map_err(|error| format!("老师影子观察 JSON 非法：{error}"))?;
    let report = evaluate_teacher_shadow(&observations, &argument("--generated-at")?)
        .map_err(|error| error.to_string())?;
    let output = PathBuf::from(argument("--output")?);
    let stored =
        write_teacher_shadow_report_once(&output, &report).map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&stored)
            .map_err(|error| format!("老师影子报告序列化失败：{error}"))?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
