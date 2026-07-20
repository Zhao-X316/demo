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

/// WebView 只接收非敏感字段和掩码，永不拿到可用 token/key。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MaskedVolcanoCreds {
    pub app_id: String,
    pub access_token_mask: Option<String>,
    pub secret_mask: Option<String>,
    pub cluster: String,
    pub ark_api_key_mask: Option<String>,
    pub ark_model: String,
}

fn mask(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    if value.chars().count() <= 4 {
        return Some("••••".to_string());
    }
    let suffix: String = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    Some(format!("••••{suffix}"))
}

pub fn masked(creds: &VolcanoCreds) -> MaskedVolcanoCreds {
    MaskedVolcanoCreds {
        app_id: creds.app_id.clone(),
        access_token_mask: mask(&creds.access_token),
        secret_mask: mask(&creds.secret),
        cluster: creds.cluster.clone(),
        ark_api_key_mask: mask(&creds.ark_api_key),
        ark_model: creds.ark_model.clone(),
    }
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
    serde_json::from_str(&s).map_err(|err| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("secrets.json 格式无效，已拒绝覆盖: {err}"),
        )
    })
}

pub fn save(dir: &Path, creds: &VolcanoCreds) -> std::io::Result<()> {
    let p = secrets_path(dir);
    let s = serde_json::to_string_pretty(creds).map_err(std::io::Error::other)?;
    std::fs::write(&p, s)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

/// 设置页空的敏感字段表示“不修改”；防止掩码读取后保存时清空既有 token。
pub fn merge_and_save(dir: &Path, incoming: &VolcanoCreds) -> std::io::Result<()> {
    let mut current = load(dir)?;
    current.app_id.clone_from(&incoming.app_id);
    current.cluster.clone_from(&incoming.cluster);
    if !incoming.access_token.is_empty() {
        current.access_token.clone_from(&incoming.access_token);
    }
    if !incoming.secret.is_empty() {
        current.secret.clone_from(&incoming.secret);
    }
    if !incoming.ark_api_key.is_empty() {
        current.ark_api_key.clone_from(&incoming.ark_api_key);
    }
    if !incoming.ark_model.is_empty() {
        current.ark_model.clone_from(&incoming.ark_model);
    }
    save(dir, &current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masked_view_never_returns_raw_secrets_and_blank_update_preserves_them() {
        let dir = std::env::temp_dir().join(format!("jiaofu-secrets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let original = VolcanoCreds {
            app_id: "app-1".into(),
            access_token: "token-123456".into(),
            secret: "secret-abcdef".into(),
            cluster: "cluster-a".into(),
            ark_api_key: "ark-987654".into(),
            ark_model: "model-a".into(),
        };
        save(&dir, &original).unwrap();

        let view = masked(&load(&dir).unwrap());
        let encoded = serde_json::to_string(&view).unwrap();
        assert!(!encoded.contains("token-123456"));
        assert!(!encoded.contains("secret-abcdef"));
        assert!(!encoded.contains("ark-987654"));
        assert_eq!(view.access_token_mask.as_deref(), Some("••••3456"));
        assert_eq!(mask("abc").as_deref(), Some("••••"));
        assert!(!mask("abc").unwrap().contains("abc"));

        merge_and_save(
            &dir,
            &VolcanoCreds {
                app_id: "app-2".into(),
                cluster: "cluster-b".into(),
                ..VolcanoCreds::default()
            },
        )
        .unwrap();
        let preserved = load(&dir).unwrap();
        assert_eq!(preserved.app_id, "app-2");
        assert_eq!(preserved.cluster, "cluster-b");
        assert_eq!(preserved.access_token, original.access_token);
        assert_eq!(preserved.secret, original.secret);
        assert_eq!(preserved.ark_api_key, original.ark_api_key);
        assert_eq!(preserved.ark_model, original.ark_model);

        merge_and_save(
            &dir,
            &VolcanoCreds {
                app_id: "app-2".into(),
                access_token: "replacement".into(),
                cluster: "cluster-b".into(),
                ..VolcanoCreds::default()
            },
        )
        .unwrap();
        assert_eq!(load(&dir).unwrap().access_token, "replacement");
    }

    #[test]
    fn malformed_secret_file_is_not_silently_overwritten() {
        let dir =
            std::env::temp_dir().join(format!("jiaofu-secrets-invalid-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secrets.json");
        std::fs::write(&path, "{not-json").unwrap();

        assert!(merge_and_save(&dir, &VolcanoCreds::default()).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "{not-json");
    }
}
