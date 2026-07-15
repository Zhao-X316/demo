//! 火山方舟固定卷客观题视觉识别适配器。
//!
//! 这里只把一个已经确认的答案区域裁剪转换为结构化 OMR 输出。学生匹配、页面
//! 配准、题区确认、评分建议、老师终审和成绩发布仍由 M2 既有状态机负责。

use std::collections::BTreeSet;
use std::time::Duration;

use base64::Engine;
use module_exam::objective_recognition::{
    ObjectiveRecognitionErrorCode, ObjectiveRecognitionFailure, ObjectiveRecognitionOutput,
    ObjectiveRecognitionRequest, ObjectiveRecognitionState, ObjectiveRecognizedAnswer,
    ObjectiveRecognizer, ObjectiveRecognizerDescriptor, OBJECTIVE_RECOGNITION_SCHEMA_VERSION,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::secrets::VolcanoCreds;
use crate::vlm::{ARK_URL, DEFAULT_MODEL};

const MODEL_VERSION: &str = "ark-chat-completions-v3";
const CONFIG_VERSION: &str = "objective-crop-v1";
const RULE_VERSION: &str = "objective-vision-json-v1";

pub struct ArkObjectiveRecognizer {
    api_key: String,
    model: String,
    endpoint: String,
}

impl ArkObjectiveRecognizer {
    pub fn from_creds(creds: &VolcanoCreds) -> Self {
        Self {
            api_key: creds.ark_api_key.clone(),
            model: if creds.ark_model.trim().is_empty() {
                DEFAULT_MODEL.to_string()
            } else {
                creds.ark_model.trim().to_string()
            },
            endpoint: ARK_URL.to_string(),
        }
    }

    #[cfg(test)]
    fn for_test(endpoint: String) -> Self {
        Self {
            api_key: "test-only-key".into(),
            model: "fixture-vision".into(),
            endpoint,
        }
    }
}

impl ObjectiveRecognizer for ArkObjectiveRecognizer {
    fn descriptor(&self) -> ObjectiveRecognizerDescriptor {
        ObjectiveRecognizerDescriptor {
            provider: "volcengine_ark".into(),
            model_name: self.model.clone(),
            model_version: MODEL_VERSION.into(),
            config_version: CONFIG_VERSION.into(),
            rule_version: RULE_VERSION.into(),
        }
    }

    fn recognize(
        &self,
        request: &ObjectiveRecognitionRequest<'_>,
    ) -> Result<ObjectiveRecognitionOutput, ObjectiveRecognitionFailure> {
        request.validate().map_err(|_| {
            failure(
                ObjectiveRecognitionErrorCode::TemplateMismatch,
                "客观题识别输入与已确认模板不一致",
                false,
            )
        })?;
        if self.api_key.trim().is_empty() {
            return Err(failure(
                ObjectiveRecognitionErrorCode::ProviderUnavailable,
                "未配置豆包视觉服务，已保留题区等待老师处理",
                true,
            ));
        }
        if !matches!(
            request.mime_type.trim().to_ascii_lowercase().as_str(),
            "image/jpeg" | "image/png" | "image/webp" | "image/bmp" | "image/gif"
        ) {
            return Err(failure(
                ObjectiveRecognitionErrorCode::UnsupportedMedia,
                "当前题区裁剪不是受支持的图片格式",
                false,
            ));
        }

        let input_hash = request.input_hash().map_err(|_| {
            failure(
                ObjectiveRecognitionErrorCode::TemplateMismatch,
                "客观题识别输入无法建立稳定版本",
                false,
            )
        })?;
        let data_url = format!(
            "data:{};base64,{}",
            request.mime_type.trim().to_ascii_lowercase(),
            base64::engine::general_purpose::STANDARD.encode(request.image_bytes)
        );
        let prompt = build_prompt(request);
        let body = json!({
            "model": self.model,
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": prompt },
                    { "type": "image_url", "image_url": { "url": data_url } }
                ]
            }],
            "temperature": 0.0
        });
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|_| {
                failure(
                    ObjectiveRecognitionErrorCode::Internal,
                    "客观题识别客户端初始化失败",
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
                        ObjectiveRecognitionErrorCode::Timeout,
                        "客观题识别服务超时，可稍后重试",
                        true,
                    )
                } else {
                    failure(
                        ObjectiveRecognitionErrorCode::ProviderUnavailable,
                        "客观题识别服务暂时不可用，可稍后重试",
                        true,
                    )
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(if status.as_u16() == 429 {
                failure(
                    ObjectiveRecognitionErrorCode::RateLimited,
                    "客观题识别请求过于频繁，可稍后重试",
                    true,
                )
            } else {
                failure(
                    ObjectiveRecognitionErrorCode::ProviderUnavailable,
                    "客观题识别服务返回异常，已转入老师复核",
                    status.is_server_error(),
                )
            });
        }
        let envelope: Value = response.json().map_err(|_| {
            failure(
                ObjectiveRecognitionErrorCode::InvalidOutput,
                "客观题识别结果格式无效，已转入老师复核",
                false,
            )
        })?;
        let content = envelope
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                failure(
                    ObjectiveRecognitionErrorCode::InvalidOutput,
                    "客观题识别结果缺少结构化内容，已转入老师复核",
                    false,
                )
            })?;
        parse_content(request, &input_hash, self.descriptor(), content)
    }
}

fn build_prompt(request: &ObjectiveRecognitionRequest<'_>) -> String {
    let cells = serde_json::to_string(request.cells).unwrap_or_else(|_| "[]".into());
    format!(
        "你只分析这张已裁剪的客观题答题区，不推断学生身份、题干或标准答案。\n\
题型：{}。模板版本：{}。归一化答题格：{}。\n\
严格只返回一个 JSON 对象，不要 Markdown：\n\
{{\"state\":\"recognized|blank|altered|low_confidence\",\"selected_labels\":[\"A\"],\"selected\":null,\"selected_values\":[],\"confidence\":0.98,\"issue_codes\":[]}}\n\
规则：单选/多选只使用 selected_labels；判断题只使用 selected=true/false；判断题发生双选时 state=altered 且 selected_values=[true,false]；空白时不返回答案；涂改或冲突必须 state=altered 并给 issue_codes；置信度低于 0.95 必须 state=low_confidence；标签只能来自给定答题格。",
        request.question_type.as_str(),
        request.template_version,
        cells
    )
}

#[derive(Deserialize)]
struct ProviderAnswer {
    state: String,
    #[serde(default)]
    selected_labels: Vec<String>,
    selected: Option<bool>,
    #[serde(default)]
    selected_values: Vec<bool>,
    confidence: Option<f64>,
    #[serde(default)]
    issue_codes: Vec<String>,
}

fn parse_content(
    request: &ObjectiveRecognitionRequest<'_>,
    input_hash: &str,
    descriptor: ObjectiveRecognizerDescriptor,
    content: &str,
) -> Result<ObjectiveRecognitionOutput, ObjectiveRecognitionFailure> {
    let raw = strip_json_fence(content);
    let parsed: ProviderAnswer = serde_json::from_str(raw).map_err(|_| {
        failure(
            ObjectiveRecognitionErrorCode::InvalidOutput,
            "客观题识别结果不是有效 JSON，已转入老师复核",
            false,
        )
    })?;
    let result_state = match parsed.state.trim() {
        "recognized" => ObjectiveRecognitionState::Recognized,
        "blank" => ObjectiveRecognitionState::Blank,
        "altered" => ObjectiveRecognitionState::Altered,
        "low_confidence" => ObjectiveRecognitionState::LowConfidence,
        _ => {
            return Err(failure(
                ObjectiveRecognitionErrorCode::InvalidOutput,
                "客观题识别状态无效，已转入老师复核",
                false,
            ))
        }
    };
    let allowed = request
        .cells
        .iter()
        .map(|cell| cell.label.trim().to_ascii_uppercase())
        .collect::<BTreeSet<_>>();
    let labels = parsed
        .selected_labels
        .iter()
        .map(|label| label.trim().to_ascii_uppercase())
        .collect::<Vec<_>>();
    if labels.iter().any(|label| !allowed.contains(label)) {
        return Err(failure(
            ObjectiveRecognitionErrorCode::InvalidOutput,
            "客观题识别返回了模板外选项，已转入老师复核",
            false,
        ));
    }
    let answer = match result_state {
        ObjectiveRecognitionState::Blank => None,
        _ => match request.question_type {
            module_exam::objective_recognition::ObjectiveQuestionType::Single
            | module_exam::objective_recognition::ObjectiveQuestionType::Multiple => {
                Some(ObjectiveRecognizedAnswer::SelectedLabels {
                    selected_labels: labels,
                })
            }
            module_exam::objective_recognition::ObjectiveQuestionType::TrueFalse => {
                if result_state == ObjectiveRecognitionState::Altered
                    && !parsed.selected_values.is_empty()
                {
                    Some(ObjectiveRecognizedAnswer::AmbiguousTrueFalse {
                        selected_values: parsed.selected_values,
                    })
                } else {
                    parsed
                        .selected
                        .map(|selected| ObjectiveRecognizedAnswer::TrueFalse { selected })
                }
            }
        },
    };
    let output = ObjectiveRecognitionOutput {
        schema_version: OBJECTIVE_RECOGNITION_SCHEMA_VERSION,
        answer_region_revision_id: request.answer_region_revision_id,
        input_artifact_id: request.input_artifact_id,
        input_artifact_sha256: request.input_artifact_sha256.trim().to_ascii_lowercase(),
        input_hash: input_hash.into(),
        question_type: request.question_type,
        template_version: request.template_version.trim().into(),
        descriptor,
        result_state,
        answer,
        confidence: parsed.confidence,
        issue_codes: parsed.issue_codes,
        measurements: Vec::new(),
    };
    output.validate().map_err(|_| {
        failure(
            ObjectiveRecognitionErrorCode::InvalidOutput,
            "客观题识别结果未通过安全校验，已转入老师复核",
            false,
        )
    })?;
    Ok(output)
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

fn failure(
    code: ObjectiveRecognitionErrorCode,
    safe_message: &str,
    retryable: bool,
) -> ObjectiveRecognitionFailure {
    ObjectiveRecognitionFailure {
        schema_version: OBJECTIVE_RECOGNITION_SCHEMA_VERSION,
        code,
        safe_message: safe_message.into(),
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use module_exam::objective_recognition::{
        ObjectiveMarkCell, ObjectiveQuestionType, ObjectiveRecognitionRequest,
    };
    use suite_core::domain::hashing;

    use super::*;

    fn request<'a>(
        bytes: &'a [u8],
        hash: &'a str,
        cells: &'a [ObjectiveMarkCell],
    ) -> ObjectiveRecognitionRequest<'a> {
        ObjectiveRecognitionRequest {
            answer_region_revision_id: 17,
            input_artifact_id: 29,
            input_artifact_sha256: hash,
            mime_type: "image/jpeg",
            image_bytes: bytes,
            question_type: ObjectiveQuestionType::Single,
            template_version: "fixture-v1",
            cells,
        }
    }

    fn serve_once(status: &str, body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 16_384];
            let _ = stream.read(&mut buffer).unwrap();
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        format!("http://{address}")
    }

    fn cells() -> Vec<ObjectiveMarkCell> {
        vec![
            ObjectiveMarkCell {
                label: "A".into(),
                x: 0.0,
                y: 0.0,
                width: 0.4,
                height: 1.0,
            },
            ObjectiveMarkCell {
                label: "B".into(),
                x: 0.6,
                y: 0.0,
                width: 0.4,
                height: 1.0,
            },
        ]
    }

    #[test]
    fn parses_strict_success_without_persisting_raw_response() {
        let provider_content = r#"{"state":"recognized","selected_labels":["B"],"selected":null,"selected_values":[],"confidence":0.98,"issue_codes":[]}"#;
        let body = json!({"choices":[{"message":{"content":provider_content}}]}).to_string();
        let recognizer = ArkObjectiveRecognizer::for_test(serve_once("200 OK", body));
        let bytes = b"fixture-image";
        let hash = hashing::sha256_hex(bytes);
        let cells = cells();
        let output = recognizer
            .recognize(&request(bytes, &hash, &cells))
            .unwrap();
        assert_eq!(output.result_state, ObjectiveRecognitionState::Recognized);
        assert_eq!(output.confidence, Some(0.98));
        assert_eq!(
            output.answer,
            Some(ObjectiveRecognizedAnswer::SelectedLabels {
                selected_labels: vec!["B".into()]
            })
        );
    }

    #[test]
    fn rejects_model_label_outside_confirmed_cells() {
        let provider_content =
            r#"{"state":"recognized","selected_labels":["C"],"confidence":0.99}"#;
        let body = json!({"choices":[{"message":{"content":provider_content}}]}).to_string();
        let recognizer = ArkObjectiveRecognizer::for_test(serve_once("200 OK", body));
        let bytes = b"fixture-image";
        let hash = hashing::sha256_hex(bytes);
        let cells = cells();
        let failure = recognizer
            .recognize(&request(bytes, &hash, &cells))
            .unwrap_err();
        assert_eq!(failure.code, ObjectiveRecognitionErrorCode::InvalidOutput);
        assert!(!failure.safe_message.contains("C"));
    }

    #[test]
    fn http_error_is_redacted_and_classified() {
        let recognizer = ArkObjectiveRecognizer::for_test(serve_once(
            "429 Too Many Requests",
            r#"{"error":"Bearer secret-value"}"#.into(),
        ));
        let bytes = b"fixture-image";
        let hash = hashing::sha256_hex(bytes);
        let cells = cells();
        let failure = recognizer
            .recognize(&request(bytes, &hash, &cells))
            .unwrap_err();
        assert_eq!(failure.code, ObjectiveRecognitionErrorCode::RateLimited);
        assert!(failure.retryable);
        failure.validate().unwrap();
        assert!(!failure.safe_message.contains("secret-value"));
    }
}
