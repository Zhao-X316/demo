//! 火山「大模型录音文件识别」HTTP 客户端。
//!
//! 支持两种套餐，按 Resource-Id 自动路由：
//!   - 标准版（默认）：`volc.bigasr.auc` —— 异步 submit + 轮询 query。
//!   - 极速版：`volc.bigasr.auc_turbo` —— 单请求同步返回（flash 端点）。
//!
//! 鉴权头取自设置页保存的 secrets.json。套餐/Resource-Id 可在设置「Cluster」里覆盖：
//! 留空 = 标准版；填 `volc.bigasr.auc_turbo` = 极速版。

use std::time::Duration;

use base64::Engine;
use serde_json::{json, Value};

use suite_core::ports::RecognizedWord;

use crate::secrets::VolcanoCreds;

const FLASH_URL: &str = "https://openspeech.bytedance.com/api/v3/auc/bigmodel/recognize/flash";
const SUBMIT_URL: &str = "https://openspeech.bytedance.com/api/v3/auc/bigmodel/submit";
const QUERY_URL: &str = "https://openspeech.bytedance.com/api/v3/auc/bigmodel/query";
/// 默认走标准版资源；极速版用 `volc.bigasr.auc_turbo`。
const DEFAULT_RESOURCE: &str = "volc.bigasr.auc";
const TURBO_RESOURCE: &str = "volc.bigasr.auc_turbo";
/// 标准版轮询：最多 ~90s（60 次 × 1.5s）。
const POLL_MAX: usize = 60;
const POLL_INTERVAL_MS: u64 = 1500;

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

    if resource == TURBO_RESOURCE {
        recognize_flash(creds, &b64, &format, resource, request_id).await
    } else {
        recognize_standard(creds, &b64, &format, resource, request_id).await
    }
}

/// 提交/识别请求体（两种套餐通用）。
fn build_body(b64: &str, format: &str) -> Value {
    json!({
        "user": { "uid": "jiaofu-suite" },
        "audio": { "format": format, "data": b64 },
        "request": { "model_name": "bigmodel", "enable_punc": true, "enable_itn": true, "show_utterances": true }
    })
}

fn req(
    builder: reqwest::RequestBuilder,
    creds: &VolcanoCreds,
    resource: &str,
    request_id: &str,
) -> reqwest::RequestBuilder {
    builder
        .header("X-Api-App-Key", &creds.app_id)
        .header("X-Api-Access-Key", &creds.access_token)
        .header("X-Api-Resource-Id", resource)
        .header("X-Api-Request-Id", request_id)
        .header("X-Api-Sequence", "-1")
}

/// 火山是境内端点：**绕过系统/环境代理**直连，否则本机代理（mihomo/gost 等）会把
/// 境内请求绕到境外造成 504/超时。同时设超时与浏览器 UA（沿用项目「境内调用绕代理」口径）。
fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(120))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))
}

fn status_code(resp: &reqwest::Response) -> String {
    resp.headers()
        .get("X-Api-Status-Code")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

/// 极速版：单请求同步返回。
async fn recognize_flash(
    creds: &VolcanoCreds,
    b64: &str,
    format: &str,
    resource: &str,
    request_id: &str,
) -> Result<AsrOutput, String> {
    let client = http_client()?;
    let resp = req(client.post(FLASH_URL), creds, resource, request_id)
        .json(&build_body(b64, format))
        .send()
        .await
        .map_err(|err| format!("火山请求失败: {err}"))?;

    let http = resp.status();
    let api_code = status_code(&resp);
    let raw = resp.text().await.map_err(|err| format!("读取响应失败: {err}"))?;
    if !http.is_success() {
        return Err(format!("火山 HTTP {http} (status_code={api_code}): {}", truncate(&raw, 500)));
    }
    let v: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("解析响应失败: {err}; 原文: {}", truncate(&raw, 500)))?;
    parse_result(&v, &api_code, &raw)
}

/// 标准版：submit 提交任务 → 轮询 query 直到完成。
async fn recognize_standard(
    creds: &VolcanoCreds,
    b64: &str,
    format: &str,
    resource: &str,
    request_id: &str,
) -> Result<AsrOutput, String> {
    let client = http_client()?;

    // —— 1. 提交任务 ——
    let resp = req(client.post(SUBMIT_URL), creds, resource, request_id)
        .json(&build_body(b64, format))
        .send()
        .await
        .map_err(|err| format!("火山 submit 请求失败: {err}"))?;
    let http = resp.status();
    let code = status_code(&resp);
    let raw = resp.text().await.map_err(|err| format!("读取 submit 响应失败: {err}"))?;
    if !http.is_success() || (!code.is_empty() && code != "20000000") {
        return Err(format!("火山 submit 失败 HTTP {http} (status_code={code}): {}", truncate(&raw, 500)));
    }

    // —— 2. 轮询查询（同一 request_id 作为任务号）——
    for _ in 0..POLL_MAX {
        tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
        let resp = req(client.post(QUERY_URL), creds, resource, request_id)
            .json(&json!({}))
            .send()
            .await
            .map_err(|err| format!("火山 query 请求失败: {err}"))?;
        let code = status_code(&resp);
        let raw = resp.text().await.map_err(|err| format!("读取 query 响应失败: {err}"))?;

        if code == "20000000" {
            // 完成
            let v: Value = serde_json::from_str(&raw)
                .map_err(|err| format!("解析 query 响应失败: {err}; 原文: {}", truncate(&raw, 500)))?;
            return parse_result(&v, &code, &raw);
        } else if code == "20000001" || code == "20000002" || code.is_empty() {
            // 处理中 / 排队中 / 头暂缺 → 继续轮询
            continue;
        } else {
            return Err(format!("火山 query 失败 (status_code={code}): {}", truncate(&raw, 500)));
        }
    }
    Err(format!("火山识别超时：轮询 {POLL_MAX} 次仍未返回结果（标准版任务可能较慢，可重试）"))
}

/// 解析识别结果（标准版/极速版结构一致：result.text + result.utterances；
/// 时长可能在顶层 audio_info 或 result.audio_info）。
fn parse_result(v: &Value, api_code: &str, raw: &str) -> Result<AsrOutput, String> {
    let result = v.get("result").cloned().unwrap_or(Value::Null);
    let text = result.get("text").and_then(|t| t.as_str()).unwrap_or("").to_string();

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

    let duration_ms = v
        .get("audio_info")
        .and_then(|a| a.get("duration"))
        .and_then(|d| d.as_u64())
        .or_else(|| {
            result
                .get("audio_info")
                .and_then(|a| a.get("duration"))
                .and_then(|d| d.as_u64())
        })
        .unwrap_or(max_end);

    if text.is_empty() && words.is_empty() {
        return Err(format!("火山返回为空 (status_code={api_code}): {}", truncate(raw, 500)));
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
