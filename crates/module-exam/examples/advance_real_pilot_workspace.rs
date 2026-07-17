use std::env;
use std::path::PathBuf;

use chrono::{SecondsFormat, Utc};
use module_exam::pilot_workspace_run::{
    advance_pilot_workspace, PilotWorkspaceAdvanceRequest, PilotWorkspaceShadowMetadata,
};
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

fn shadow_metadata() -> Result<Option<PilotWorkspaceShadowMetadata>, String> {
    let started_at = optional_argument("--started-at");
    let predictions_generated_at = optional_argument("--predictions-generated-at");
    let completed_at = optional_argument("--completed-at");
    let provider_ref = optional_argument("--provider-ref");
    let model_ref = optional_argument("--model-ref");
    let provider_config = optional_argument("--provider-config-version");
    let present = [
        started_at.is_some(),
        predictions_generated_at.is_some(),
        completed_at.is_some(),
        provider_ref.is_some(),
        model_ref.is_some(),
        provider_config.is_some(),
    ];
    if present.iter().all(|value| !value) {
        return Ok(None);
    }
    if !present.iter().all(|value| *value) {
        return Err(
            "首次运行机器影子评估时，开始/预测/完成时间及 provider/model/config 必须同时提供"
                .into(),
        );
    }
    Ok(Some(PilotWorkspaceShadowMetadata {
        started_at: started_at.unwrap_or_default(),
        predictions_generated_at: predictions_generated_at.unwrap_or_default(),
        completed_at: completed_at.unwrap_or_default(),
        provider_ref_sha256: hashing::sha256_hex(provider_ref.unwrap_or_default().as_bytes()),
        model_ref_sha256: hashing::sha256_hex(model_ref.unwrap_or_default().as_bytes()),
        provider_config_sha256: hashing::sha256_hex(provider_config.unwrap_or_default().as_bytes()),
    }))
}

fn run() -> Result<bool, String> {
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let status = advance_pilot_workspace(&PilotWorkspaceAdvanceRequest {
        root: PathBuf::from(argument("--workspace")?),
        evaluated_on: argument("--as-of")?,
        shadow_metadata: shadow_metadata()?,
        teacher_report_generated_at: optional_argument("--teacher-report-generated-at")
            .unwrap_or_else(|| now.clone()),
        evidence_assembled_at: optional_argument("--assembled-at").unwrap_or(now),
    })
    .map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&status)
            .map_err(|error| format!("试点工作区推进状态序列化失败：{error}"))?
    );
    Ok(status.threshold_review_ready)
}

fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(3),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }
}
