use std::env;
use std::path::PathBuf;

use module_exam::pilot_workspace::{create_pilot_workspace, PilotWorkspaceRequest};

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
    let request = PilotWorkspaceRequest {
        root: PathBuf::from(argument("--output-dir")?),
        starts_on: argument("--starts-on")?,
        ends_on: argument("--ends-on")?,
        purpose: argument("--purpose")?,
        responsible_party_ref_sha256: argument("--responsible-party-ref-sha256")?,
        class_scope_sha256: argument("--class-scope-sha256")?,
        retention_policy_version: argument("--retention-policy-version")
            .unwrap_or_else(|_| "pilot-local-retention-v1".into()),
        notice_version: argument("--notice-version").unwrap_or_else(|_| "pilot-notice-v1".into()),
    };
    let index = create_pilot_workspace(&request).map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&index)
            .map_err(|error| format!("工作区索引序列化失败：{error}"))?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
