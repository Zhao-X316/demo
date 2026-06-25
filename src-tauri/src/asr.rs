//! 火山「大模型录音文件识别·极速版(flash)」HTTP 客户端：一次请求同步返回。
//!
//! 鉴权头取自设置页保存的 secrets.json。不同套餐的 Resource-Id 可在设置「Cluster」里覆盖。
//! 由于本机无法联网测试，字段解析做了防御式处理；若返回结构与预期不符，错误里会带原文片段便于排查。

use base64::Engine;
use serde_json::{json, Value};

use suite_core::ports::RecognizedWord;

use crate::secrets::VolcanoCreds;

const FLASH_URL: &str = "https://openspeech.bytedance.com/api/v3/auc/bigmodel/recognize/flash";
const DEFAULT_RESOURCE: &str = "volc.bigasr.auc_turbo";

pub struct AsrOutput {
    pub text: String,
    pub words: Vec<RecognizedWord>,
    pub duration_ms: u64,
}

pub async fn recognize(
    creds: &VolcanoCreds,
    audio_path: &str,
    request_id: &str,
) -> Result<AsrOutput, String> {
    if creds.app_id.is_empty() || creds.access_token.is_empty() {
        return Err("未配置火山凭据：请在「设置」填写 App ID / Access Token".into());
    }

    let bytes = std::fs::read(audio_path).map_err(|e| format!("读取音频失败: {e}"))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let format = std::path::Path::new(audio_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("wav")
        .to_lowercase();
    let resource = if creds.cluster.is_empty() { DEFAULT_RESOURCE } else { creds.cluster.as_str() };

    let body = json!({
        "user": { "uid": "jiaofu-suite" },
        "audio": { "format": format, "data": b64 },
        "request": { "model_name": "bigmodel", "enable_punc": true, "enable_itn": true, "show_utterances": true }
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(FLASH_URL)
        .header("X-Api-App-Key", &creds.app_id)
        .header("X-Api-Access-Key", &creds.access_token)
        .header("X-Api-Resource-Id", resource)
        .header("X-Api-Request-Id", request_id)
        .header("X-Api-Sequence", "-1")
        .json(&body)
        .send()
        .await
        .map_err(|err| format!("火山请求失败: {err}"))?;

    let http = resp.status();
    let api_code = resp
        .headers()
        .get("X-Api-Status-Code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let raw = resp.text().await.map_err(|err| format!("读取响应失败: {err}"))?;

    if !http.is_success() {
        return Err(format!("火山 HTTP {http} (status_code={api_code}): {}", truncate(&raw, 500)));
    }

    let v: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("解析响应失败: {err}; 原文: {}", truncate(&raw, 500)))?;
    let result = v.get("result").cloned().unwrap_or(Value::Null);

    let text = result
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    let mut words = Vec::new();
    let mut max_end = 0u64;
    if let Some(utts) = result.get("utterances").and_then(|u| u.as_array()) {
        for utt in utts {
            if let Some(ws) = utt.get("words").and_then(|w| w.as_array()) {
                for w in ws {
                    let t = w.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string();
                    let s = w.get("start_time").and_then(|x| x.as_u64()).unwrap_or(0);
                    let e = w.get("end_time").and_then(|x| x.as_u64()).unwrap_or(s);
                    max_end = max_end.max(e);
                    if !t.is_empty() {
                        words.push(RecognizedWord { text: t, start_ms: s, end_ms: e });
                    }
                }
            }
        }
    }

    let duration_ms = result
        .get("audio_info")
        .and_then(|a| a.get("duration"))
        .and_then(|d| d.as_u64())
        .unwrap_or(max_end);

    if text.is_empty() && words.is_empty() {
        return Err(format!("火山返回为空 (status_code={api_code}): {}", truncate(&raw, 500)));
    }

    Ok(AsrOutput { text, words, duration_ms })
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}
