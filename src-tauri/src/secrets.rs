//! 火山 ASR 凭据：存本地 secrets.json（Unix 下 0600），**绝不**进数据库或仓库。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VolcanoCreds {
    #[serde(default)]
    pub app_id: String,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub secret: String,
    /// 资源/集群标识（火山不同识别套餐需要，可留空）。
    #[serde(default)]
    pub cluster: String,
    /// 火山方舟(Ark) 视觉大模型 API Key（改作业模块用，与 ASR 凭据不同）。
    #[serde(default)]
    pub ark_api_key: String,
    /// 方舟接入点/模型 ID（如 ep-xxx 或 doubao-1.5-vision-pro），留空用默认。
    #[serde(default)]
    pub ark_model: String,
}

fn secrets_path(dir: &Path) -> PathBuf {
    dir.join("secrets.json")
}

pub fn load(dir: &Path) -> std::io::Result<VolcanoCreds> {
    let p = secrets_path(dir);
    if !p.exists() {
        return Ok(VolcanoCreds::default());
    }
    let s = std::fs::read_to_string(p)?;
    Ok(serde_json::from_str(&s).unwrap_or_default())
}

pub fn save(dir: &Path, creds: &VolcanoCreds) -> std::io::Result<()> {
    let p = secrets_path(dir);
    let s = serde_json::to_string_pretty(creds)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(&p, s)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
