//! 火山方舟(Ark) 豆包视觉大模型 HTTP 客户端：Chat Completions(含图片)。
//!
//! 用于改作业模块的"题目预分析"和"作答识别"。鉴权用方舟 API Key(Bearer)，
//! 与 ASR 那套(App Key/Access Key)不同。模型/接入点 ID 可在设置里覆盖。

use base64::Engine;
use serde_json::{json, Value};

use crate::secrets::VolcanoCreds;

pub(crate) const ARK_URL: &str = "https://ark.cn-beijing.volces.com/api/v3/chat/completions";
pub(crate) const DEFAULT_MODEL: &str = "doubao-1.5-vision-pro";

fn mime_of(path: &str) -> &'static str {
    match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "gif" => "image/gif",
        _ => "image/jpeg",
    }
}

/// 给定一张图片 + 文本提示，调用豆包视觉模型，返回模型文本内容。
pub async fn chat_vision(
    creds: &VolcanoCreds,
    image_path: &str,
    prompt: &str,
) -> Result<String, String> {
    if creds.ark_api_key.is_empty() {
        return Err("未配置方舟 API Key：请在「设置」填写豆包视觉大模型 API Key".into());
    }
    let bytes = std::fs::read(image_path).map_err(|e| format!("读取图片失败: {e}"))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let data_url = format!("data:{};base64,{}", mime_of(image_path), b64);
    let model = if creds.ark_model.is_empty() { DEFAULT_MODEL } else { creds.ark_model.as_str() };

    let body = json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "text", "text": prompt },
                { "type": "image_url", "image_url": { "url": data_url } }
            ]
        }],
        "temperature": 0.1
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(ARK_URL)
        .header("Authorization", format!("Bearer {}", creds.ark_api_key))
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("方舟请求失败: {err}"))?;

    let http = resp.status();
    let raw = resp.text().await.map_err(|err| format!("读取响应失败: {err}"))?;
    if !http.is_success() {
        return Err(format!("方舟 HTTP {http}: {}", truncate(&raw, 500)));
    }

    let v: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("解析响应失败: {err}; 原文: {}", truncate(&raw, 500)))?;
    let content = v
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    if content.is_empty() {
        return Err(format!("方舟返回为空: {}", truncate(&raw, 500)));
    }
    Ok(content)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}
