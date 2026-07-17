use std::env;
use std::path::PathBuf;

use module_exam::pilot_workspace::inspect_pilot_workspace;

fn argument(name: &str) -> Result<String, String> {
    let mut args = env::args().skip(1);
    while let Some(value) = args.next() {
        if value == name {
            return args.next().ok_or_else(|| format!("{name} 缺少值"));
        }
    }
    Err(format!("缺少 {name}"))
}

fn run() -> Result<bool, String> {
    let root = PathBuf::from(argument("--workspace")?);
    let status =
        inspect_pilot_workspace(&root, &argument("--as-of")?).map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&status)
            .map_err(|error| format!("工作区状态序列化失败：{error}"))?
    );
    Ok(status.ready_for_provider_shadow)
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
