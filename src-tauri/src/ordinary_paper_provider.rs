//! 火山方舟固定普通试卷整页分析适配器。
//!
//! 只返回页面质量、配准和题区候选。结果必须通过 provider-neutral 合同校验，
//! 不能直接确认页面、题区、分数或发布。

use std::time::Duration;

use base64::Engine;
use module_exam::ordinary_paper_recognition::{
    OrdinaryPaperAlignment, OrdinaryPaperQuality, OrdinaryPaperRecognitionErrorCode,
    OrdinaryPaperRecognitionFailure, OrdinaryPaperRecognitionOutput,
    OrdinaryPaperRecognitionRequest, OrdinaryPaperRecognitionState, OrdinaryPaperRecognizer,
    OrdinaryPaperRecognizerDescriptor, OrdinaryPaperRegionProposal, ORDINARY_PAPER_SCHEMA_VERSION,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::secrets::VolcanoCreds;
use crate::vlm::{ARK_URL, DEFAULT_MODEL};

const MODEL_VERSION: &str = "ark-chat-completions-v3";
const CONFIG_VERSION: &str = "ordinary-paper-page-v2";
const RULE_VERSION: &str = "ordinary-paper-vision-json-v2";

pub struct ArkOrdinaryPaperRecognizer {
    api_key: String,
    model: String,
    endpoint: String,
}

impl ArkOrdinaryPaperRecognizer {
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

impl OrdinaryPaperRecognizer for ArkOrdinaryPaperRecognizer {
    fn descriptor(&self) -> OrdinaryPaperRecognizerDescriptor {
        OrdinaryPaperRecognizerDescriptor {
            provider: "volcengine_ark".into(),
            model_name: self.model.clone(),
            model_version: MODEL_VERSION.into(),
            config_version: CONFIG_VERSION.into(),
            rule_version: RULE_VERSION.into(),
        }
    }

    fn recognize(
        &self,
        request: &OrdinaryPaperRecognitionRequest<'_>,
    ) -> Result<OrdinaryPaperRecognitionOutput, OrdinaryPaperRecognitionFailure> {
        request.validate().map_err(|_| {
            failure(
                OrdinaryPaperRecognitionErrorCode::TemplateMismatch,
                "普通试卷页面与当前作业模板不一致",
                false,
            )
        })?;
        if self.api_key.trim().is_empty() {
            return Err(failure(
                OrdinaryPaperRecognitionErrorCode::ProviderUnavailable,
                "未配置视觉服务，页面已保留等待老师处理",
                true,
            ));
        }

        let input_hash = request.input_hash().map_err(|_| {
            failure(
                OrdinaryPaperRecognitionErrorCode::TemplateMismatch,
                "普通试卷页面无法建立稳定输入版本",
                false,
            )
        })?;
        let data_url = format!(
            "data:{};base64,{}",
            request.mime_type.trim().to_ascii_lowercase(),
            base64::engine::general_purpose::STANDARD.encode(request.image_bytes)
        );
        let body = json!({
            "model": self.model,
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": build_prompt(request) },
                    { "type": "image_url", "image_url": { "url": data_url } }
                ]
            }],
            "temperature": 0.0
        });
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(|_| {
                failure(
                    OrdinaryPaperRecognitionErrorCode::Internal,
                    "普通试卷分析客户端初始化失败",
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
                        OrdinaryPaperRecognitionErrorCode::Timeout,
                        "普通试卷分析超时，可稍后重试",
                        true,
                    )
                } else {
                    failure(
                        OrdinaryPaperRecognitionErrorCode::ProviderUnavailable,
                        "普通试卷分析服务暂时不可用，可稍后重试",
                        true,
                    )
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(if status.as_u16() == 429 {
                failure(
                    OrdinaryPaperRecognitionErrorCode::RateLimited,
                    "普通试卷分析请求过于频繁，可稍后重试",
                    true,
                )
            } else {
                failure(
                    OrdinaryPaperRecognitionErrorCode::ProviderUnavailable,
                    "普通试卷分析服务返回异常，已转入老师复核",
                    status.is_server_error(),
                )
            });
        }
        let envelope: Value = response.json().map_err(|_| {
            failure(
                OrdinaryPaperRecognitionErrorCode::InvalidOutput,
                "普通试卷分析结果格式无效，已转入老师复核",
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
                    OrdinaryPaperRecognitionErrorCode::InvalidOutput,
                    "普通试卷分析未返回结构化内容，已转入老师复核",
                    false,
                )
            })?;
        let parsed: ArkOrdinaryPaperPayload = serde_json::from_str(strip_json_fence(content))
            .map_err(|_| {
                failure(
                    OrdinaryPaperRecognitionErrorCode::InvalidOutput,
                    "普通试卷分析内容无法解析，已转入老师复核",
                    false,
                )
            })?;
        let output = OrdinaryPaperRecognitionOutput {
            schema_version: ORDINARY_PAPER_SCHEMA_VERSION,
            page_id: request.page_id,
            input_artifact_id: request.input_artifact_id,
            input_artifact_sha256: request.input_artifact_sha256.trim().to_ascii_lowercase(),
            input_hash,
            expected_page_no: request.expected_page_no,
            descriptor: self.descriptor(),
            state: parsed.state,
            quality: parsed.quality,
            alignment: parsed.alignment,
            regions: parsed.regions,
            confidence: parsed.confidence,
            issue_codes: parsed.issue_codes,
        };
        output.validate_against(request).map_err(|_| {
            failure(
                OrdinaryPaperRecognitionErrorCode::InvalidOutput,
                "普通试卷分析结果未通过安全校验，已转入老师复核",
                false,
            )
        })?;
        Ok(output)
    }
}

#[derive(Debug, Deserialize)]
struct ArkOrdinaryPaperPayload {
    state: OrdinaryPaperRecognitionState,
    quality: OrdinaryPaperQuality,
    alignment: Option<OrdinaryPaperAlignment>,
    regions: Vec<OrdinaryPaperRegionProposal>,
    confidence: f64,
    #[serde(default)]
    issue_codes: Vec<String>,
}

fn build_prompt(request: &OrdinaryPaperRecognitionRequest<'_>) -> String {
    let items = serde_json::to_string(request.items).unwrap_or_else(|_| "[]".into());
    format!(
        "你是固定普通试卷整页分析器，只做页面质量、配准和题区候选，不判分。\n\
         当前页码：{}；模板版本：{}；当前页题目清单：{}。\n\
         只输出一个 JSON 对象，不要 markdown。字段必须为：\n\
         state: ready|needs_review|blocked；\n\
         quality: {{blur_score,glare_score,brightness_score,perspective_score,rotation_degrees,crop_complete,result,issue_codes}}，result 为 pass|needs_review|reject；\n\
         alignment: null 或 {{template_version,matrix:[9个数],confidence}}；\n\
         regions: [{{assessment_item_id,region_index,bbox:{{x,y,width,height}},mapping_confidence,mark_cells:[{{label,rect:{{x,y,width,height}}}}]}}]；\n\
         confidence: 0到1；issue_codes: 字符串数组。bbox 按整页 0~1 归一化，mark_cells.rect 按各自 bbox 裁图 0~1 归一化；\n\
         不得新增清单外题目；无法可靠定位时必须 needs_review/blocked 并给问题码；\n\
         每个客观题必须给出至少两个答题格，判断题必须且只能给 TRUE/FALSE；\n\
         只有质量通过、配准、全部题区和答题格齐全且各置信度不低于 0.95 时才能 ready。",
        request.expected_page_no,
        request.template_version.trim(),
        items
    )
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
    code: OrdinaryPaperRecognitionErrorCode,
    safe_message: &str,
    retryable: bool,
) -> OrdinaryPaperRecognitionFailure {
    OrdinaryPaperRecognitionFailure {
        schema_version: ORDINARY_PAPER_SCHEMA_VERSION,
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

    use module_exam::ordinary_paper_recognition::{
        OrdinaryPaperItemSpec, OrdinaryPaperQuestionType,
    };
    use suite_core::domain::hashing;

    use super::*;

    fn serve_once(status: &str, body: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_string();
        let body = body.to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 16384];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        format!("http://{address}")
    }

    fn item() -> OrdinaryPaperItemSpec {
        OrdinaryPaperItemSpec {
            assessment_item_id: 11,
            order_index: 0,
            question_type: OrdinaryPaperQuestionType::Single,
        }
    }

    #[test]
    fn parses_ready_page_without_trusting_identity_fields_from_provider() {
        let bytes = b"ordinary-page-image";
        let hash = hashing::sha256_hex(bytes);
        let items = [item()];
        let request = OrdinaryPaperRecognitionRequest {
            page_id: 7,
            input_artifact_id: 9,
            input_artifact_sha256: &hash,
            mime_type: "image/jpeg",
            image_bytes: bytes,
            expected_page_no: 1,
            template_version: "ordinary-v1",
            items: &items,
        };
        let payload = json!({
            "state": "ready",
            "quality": {
                "blur_score": 0.02,
                "glare_score": 0.01,
                "brightness_score": 0.8,
                "perspective_score": 0.99,
                "rotation_degrees": 0.0,
                "crop_complete": true,
                "result": "pass",
                "issue_codes": []
            },
            "alignment": {
                "template_version": "ordinary-v1",
                "matrix": [1,0,0,0,1,0,0,0,1],
                "confidence": 0.99
            },
            "regions": [{
                "assessment_item_id": 11,
                "region_index": 0,
                "bbox": {"x":0.1,"y":0.2,"width":0.8,"height":0.3},
                "mapping_confidence": 0.99,
                "mark_cells": [
                    {"label":"A","rect":{"x":0.05,"y":0.1,"width":0.2,"height":0.3}},
                    {"label":"B","rect":{"x":0.35,"y":0.1,"width":0.2,"height":0.3}}
                ]
            }],
            "confidence": 0.99,
            "issue_codes": []
        });
        let envelope = json!({"choices":[{"message":{"content":payload.to_string()}}]});
        let recognizer =
            ArkOrdinaryPaperRecognizer::for_test(serve_once("200 OK", &envelope.to_string()));
        let output = recognizer.recognize(&request).unwrap();
        assert_eq!(output.page_id, 7);
        assert_eq!(output.input_artifact_id, 9);
        assert_eq!(output.regions.len(), 1);
        assert_eq!(output.descriptor.provider, "volcengine_ark");
    }

    #[test]
    fn rejects_provider_output_with_item_outside_current_page() {
        let bytes = b"ordinary-page-image";
        let hash = hashing::sha256_hex(bytes);
        let items = [item()];
        let request = OrdinaryPaperRecognitionRequest {
            page_id: 7,
            input_artifact_id: 9,
            input_artifact_sha256: &hash,
            mime_type: "image/jpeg",
            image_bytes: bytes,
            expected_page_no: 1,
            template_version: "ordinary-v1",
            items: &items,
        };
        let payload = json!({
            "state": "ready",
            "quality": {"blur_score":0.01,"glare_score":0.01,"brightness_score":0.8,"perspective_score":0.99,"rotation_degrees":0.0,"crop_complete":true,"result":"pass","issue_codes":[]},
            "alignment": {"template_version":"ordinary-v1","matrix":[1,0,0,0,1,0,0,0,1],"confidence":0.99},
            "regions": [{"assessment_item_id":999,"region_index":0,"bbox":{"x":0.1,"y":0.2,"width":0.8,"height":0.3},"mapping_confidence":0.99,"mark_cells":[]}],
            "confidence": 0.99,
            "issue_codes": []
        });
        let envelope = json!({"choices":[{"message":{"content":payload.to_string()}}]});
        let recognizer =
            ArkOrdinaryPaperRecognizer::for_test(serve_once("200 OK", &envelope.to_string()));
        let failure = recognizer.recognize(&request).unwrap_err();
        assert_eq!(
            failure.code,
            OrdinaryPaperRecognitionErrorCode::InvalidOutput
        );
        failure.validate().unwrap();
    }

    #[test]
    fn rate_limit_failure_is_safe_and_retryable() {
        let bytes = b"ordinary-page-image";
        let hash = hashing::sha256_hex(bytes);
        let items = [item()];
        let request = OrdinaryPaperRecognitionRequest {
            page_id: 7,
            input_artifact_id: 9,
            input_artifact_sha256: &hash,
            mime_type: "image/jpeg",
            image_bytes: bytes,
            expected_page_no: 1,
            template_version: "ordinary-v1",
            items: &items,
        };
        let recognizer =
            ArkOrdinaryPaperRecognizer::for_test(serve_once("429 Too Many Requests", "{}"));
        let failure = recognizer.recognize(&request).unwrap_err();
        assert_eq!(failure.code, OrdinaryPaperRecognitionErrorCode::RateLimited);
        assert!(failure.retryable);
        failure.validate().unwrap();
    }
}
