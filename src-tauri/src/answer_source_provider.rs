//! 火山方舟答案图片/PDF/文本/Office 结构化适配器。
//!
//! 模型只看到老师答案资料与题目清单，不看到学生答案或当前 K1 标准答案。

use std::time::Duration;

use base64::Engine;
use module_exam::answer_source_recognition::{
    AnswerSourceEntry, AnswerSourceErrorCode, AnswerSourceFailure, AnswerSourceRecognitionOutput,
    AnswerSourceRecognitionRequest, AnswerSourceRecognizer, AnswerSourceRecognizerDescriptor,
    AnswerSourceState, ANSWER_SOURCE_SCHEMA_VERSION,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::secrets::VolcanoCreds;
use crate::vlm::{ARK_URL, DEFAULT_MODEL};

const MODEL_VERSION: &str = "ark-chat-completions-v3";
const CONFIG_VERSION: &str = "answer-source-image-text-pdf-office-v3";
const RULE_VERSION: &str = "answer-source-structured-json-source-anchor-v3";

pub struct ArkAnswerSourceRecognizer {
    api_key: String,
    model: String,
    endpoint: String,
}

impl ArkAnswerSourceRecognizer {
    pub fn from_creds(creds: &VolcanoCreds) -> Self {
        Self {
            api_key: creds.ark_api_key.clone(),
            model: if creds.ark_model.trim().is_empty() {
                DEFAULT_MODEL.into()
            } else {
                creds.ark_model.trim().into()
            },
            endpoint: ARK_URL.into(),
        }
    }

    #[cfg(test)]
    fn for_test(endpoint: String) -> Self {
        Self {
            api_key: "test-key".into(),
            model: "test-model".into(),
            endpoint,
        }
    }
}

fn failure(code: AnswerSourceErrorCode, message: &str, retryable: bool) -> AnswerSourceFailure {
    AnswerSourceFailure {
        schema_version: ANSWER_SOURCE_SCHEMA_VERSION,
        code,
        safe_message: message.into(),
        retryable,
    }
}

fn strip_json_fence(content: &str) -> &str {
    let content = content.trim();
    let content = content
        .strip_prefix("```json")
        .or_else(|| content.strip_prefix("```"))
        .unwrap_or(content)
        .trim();
    content.strip_suffix("```").unwrap_or(content).trim()
}

#[derive(Debug, Deserialize)]
struct Payload {
    state: AnswerSourceState,
    entries: Vec<AnswerSourceEntry>,
    confidence: f64,
    #[serde(default)]
    issue_codes: Vec<String>,
}

fn build_prompt(request: &AnswerSourceRecognitionRequest<'_>) -> String {
    let items = serde_json::to_string(request.items).unwrap_or_else(|_| "[]".into());
    let source_text = request.source_text.unwrap_or_default();
    format!(
        "你是教师上传答案资料的结构化整理器，只提取资料中明确写出的答案，不判学生作答。\n\
         当前题目清单：{items}。\n\
         文本答案资料（图片/PDF 时为空；Word/Excel 已在本机安全提取）：{source_text}\n\
         只输出一个 JSON 对象，不要 markdown：state 为 ready|needs_review|blocked；\n\
         entries 为 [{{assessment_item_id,answer_json,source_anchor,confidence}}]；\n\
         confidence 为 0~1；issue_codes 为字符串数组。\n\
         answer_json 必须带 schema_version=1：单选/多选使用 correct_labels 字符串数组；\n\
         判断题使用 correct 布尔值；填空使用 slots 数组，每项含 order_index 与 canonical_answers；\n\
         简答使用 reference_answer 与 rubric_points 数组。source_anchor 必须带 schema_version=1，\n\
         图片/PDF 写真实的 page（从1开始）和 region_hint，文本/Word/Excel 写 line/quote。不得新增清单外题目，不得根据题干猜答案，\n\
         不得利用学生多数答案；资料没写清、缺题或题号无法绑定时必须 needs_review/blocked。\n\
         只有逐题覆盖完整且每项与总置信度均不低于0.95时才能 ready。"
    )
}

impl AnswerSourceRecognizer for ArkAnswerSourceRecognizer {
    fn descriptor(&self) -> AnswerSourceRecognizerDescriptor {
        AnswerSourceRecognizerDescriptor {
            provider: "volcengine_ark".into(),
            model_name: self.model.clone(),
            model_version: MODEL_VERSION.into(),
            config_version: CONFIG_VERSION.into(),
            rule_version: RULE_VERSION.into(),
        }
    }

    fn recognize(
        &self,
        request: &AnswerSourceRecognitionRequest<'_>,
    ) -> Result<AnswerSourceRecognitionOutput, AnswerSourceFailure> {
        request.validate().map_err(|_| {
            failure(
                AnswerSourceErrorCode::UnsupportedFormat,
                "答案资料格式当前无法安全结构化",
                false,
            )
        })?;
        if self.api_key.trim().is_empty() {
            return Err(failure(
                AnswerSourceErrorCode::ProviderUnavailable,
                "未配置视觉服务，答案资料已保留等待整理",
                true,
            ));
        }
        let mut content = vec![json!({"type":"text","text":build_prompt(request)})];
        for page in request.visual_pages {
            content.push(json!({
                "type":"text",
                "text":format!("答案资料第 {} 页", page.page_no)
            }));
            content.push(json!({
                "type":"image_url",
                "image_url":{
                    "url":format!(
                        "data:{};base64,{}",
                        page.mime_type.trim().to_ascii_lowercase(),
                        base64::engine::general_purpose::STANDARD.encode(&page.bytes)
                    )
                }
            }));
        }
        let body = json!({
            "model": self.model,
            "messages": [{"role":"user","content":content}],
            "temperature": 0.0
        });
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(|_| {
                failure(
                    AnswerSourceErrorCode::Internal,
                    "答案结构化客户端初始化失败",
                    true,
                )
            })?;
        let response = client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    failure(
                        AnswerSourceErrorCode::Timeout,
                        "答案结构化超时，可稍后重试",
                        true,
                    )
                } else {
                    failure(
                        AnswerSourceErrorCode::ProviderUnavailable,
                        "答案结构化服务暂不可用",
                        true,
                    )
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(if status.as_u16() == 429 {
                failure(
                    AnswerSourceErrorCode::RateLimited,
                    "答案结构化请求过多，可稍后重试",
                    true,
                )
            } else {
                failure(
                    AnswerSourceErrorCode::ProviderUnavailable,
                    "答案结构化服务返回异常",
                    status.is_server_error(),
                )
            });
        }
        let envelope: Value = response.json().map_err(|_| {
            failure(
                AnswerSourceErrorCode::InvalidOutput,
                "答案结构化结果格式无效",
                false,
            )
        })?;
        let raw = envelope
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                failure(
                    AnswerSourceErrorCode::InvalidOutput,
                    "答案结构化未返回内容",
                    false,
                )
            })?;
        let payload: Payload = serde_json::from_str(strip_json_fence(raw)).map_err(|_| {
            failure(
                AnswerSourceErrorCode::InvalidOutput,
                "答案结构化内容无法解析",
                false,
            )
        })?;
        let output = AnswerSourceRecognitionOutput {
            schema_version: ANSWER_SOURCE_SCHEMA_VERSION,
            ingest_batch_id: request.ingest_batch_id,
            source_artifact_id: request.source_artifact_id,
            source_artifact_sha256: request.source_artifact_sha256.trim().to_ascii_lowercase(),
            input_hash: request.input_hash().map_err(|_| {
                failure(
                    AnswerSourceErrorCode::Internal,
                    "答案结构化输入版本失败",
                    false,
                )
            })?,
            descriptor: self.descriptor(),
            state: payload.state,
            entries: payload.entries,
            confidence: payload.confidence,
            issue_codes: payload.issue_codes,
        };
        output.validate_against(request).map_err(|_| {
            failure(
                AnswerSourceErrorCode::InvalidOutput,
                "答案结构化结果未通过安全校验",
                false,
            )
        })?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use module_exam::answer_source_recognition::{
        AnswerSourceItemSpec, AnswerSourceQuestionType, AnswerSourceVisualPage,
    };
    use suite_core::domain::hashing;

    use super::*;

    fn serve_once(body: &str) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_string();
        let (sender, receiver) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 16384];
            let read = stream.read(&mut buffer).unwrap();
            request.extend_from_slice(&buffer[..read]);
            sender
                .send(String::from_utf8_lossy(&request).into())
                .unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{address}"), receiver)
    }

    #[test]
    fn text_provider_extracts_candidate_without_receiving_current_answer_or_student_data() {
        let items = [AnswerSourceItemSpec {
            assessment_item_id: 11,
            order_index: 0,
            question_no: "1".into(),
            question_type: AnswerSourceQuestionType::Single,
            stem: "鸦片战争爆发于哪一年？".into(),
            max_score: 1.0,
        }];
        let request = AnswerSourceRecognitionRequest {
            ingest_batch_id: 7,
            source_artifact_id: 9,
            source_artifact_sha256: &hashing::sha256_hex(b"1.A"),
            source_format: "text",
            mime_type: "text/plain",
            source_bytes: b"1.A",
            source_text: Some("1.A"),
            text_extraction_version: None,
            visualization_version: None,
            visual_pages: &[],
            items: &items,
        };
        let payload = json!({
            "state":"ready",
            "entries":[{
                "assessment_item_id":11,
                "answer_json":{"schema_version":1,"correct_labels":["A"]},
                "source_anchor":{"schema_version":1,"line":1,"quote":"1.A"},
                "confidence":0.99
            }],
            "confidence":0.99,
            "issue_codes":[]
        });
        let envelope = json!({"choices":[{"message":{"content":payload.to_string()}}]});
        let (endpoint, captured) = serve_once(&envelope.to_string());
        let recognizer = ArkAnswerSourceRecognizer::for_test(endpoint);
        let output = recognizer.recognize(&request).unwrap();
        assert_eq!(output.entries.len(), 1);
        let http = captured.recv().unwrap();
        assert!(http.contains("不得根据题干猜答案"));
        assert!(http.contains("1.A"));
        assert!(!http.contains("student_answer"));
        assert!(!http.contains("bound_answer"));
    }

    #[test]
    fn pdf_provider_sends_numbered_visual_pages_and_accepts_bounded_anchors() {
        let items = [AnswerSourceItemSpec {
            assessment_item_id: 11,
            order_index: 0,
            question_no: "1".into(),
            question_type: AnswerSourceQuestionType::Single,
            stem: "鸦片战争爆发于哪一年？".into(),
            max_score: 1.0,
        }];
        let source = b"%PDF-test";
        let pages = vec![
            AnswerSourceVisualPage {
                page_no: 1,
                mime_type: "image/jpeg".into(),
                sha256: hashing::sha256_hex(b"page-one"),
                bytes: b"page-one".to_vec(),
            },
            AnswerSourceVisualPage {
                page_no: 2,
                mime_type: "image/jpeg".into(),
                sha256: hashing::sha256_hex(b"page-two"),
                bytes: b"page-two".to_vec(),
            },
        ];
        let request = AnswerSourceRecognitionRequest {
            ingest_batch_id: 7,
            source_artifact_id: 9,
            source_artifact_sha256: &hashing::sha256_hex(source),
            source_format: "pdf",
            mime_type: "application/pdf",
            source_bytes: source,
            source_text: None,
            text_extraction_version: None,
            visualization_version: Some("fixture-renderer-v1"),
            visual_pages: &pages,
            items: &items,
        };
        let payload = json!({
            "state":"ready",
            "entries":[{
                "assessment_item_id":11,
                "answer_json":{"schema_version":1,"correct_labels":["A"]},
                "source_anchor":{"schema_version":1,"page":2,"region_hint":"题号1"},
                "confidence":0.99
            }],
            "confidence":0.99,
            "issue_codes":[]
        });
        let envelope = json!({"choices":[{"message":{"content":payload.to_string()}}]});
        let (endpoint, captured) = serve_once(&envelope.to_string());
        let recognizer = ArkAnswerSourceRecognizer::for_test(endpoint);
        recognizer.recognize(&request).unwrap();
        let http = captured.recv().unwrap();
        assert!(http.contains("答案资料第 1 页"));
        assert!(http.contains("答案资料第 2 页"));
        assert!(http.contains(&base64::engine::general_purpose::STANDARD.encode(b"page-one")));
        assert!(!http.contains("student_answer"));
        assert!(!http.contains("bound_answer"));
    }
}
