//! 火山方舟固定默写模板与手写 OCR 适配器。
//!
//! 模板请求只定位题号/行栏；OCR 请求只携带学生裁剪，不携带标准答案。

use std::time::Duration;

use base64::Engine;
use module_exam::dictation::DictationRecognitionState;
use module_exam::dictation_recognition::{
    DictationErrorCode, DictationFailure, DictationOcrOutput, DictationOcrRecognizer,
    DictationOcrRequest, DictationRecognizerDescriptor, DictationRegionProposal,
    DictationTemplateOutput, DictationTemplateRecognizer, DictationTemplateRequest,
    DictationTemplateState, DICTATION_OCR_SCHEMA_VERSION, DICTATION_TEMPLATE_SCHEMA_VERSION,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::secrets::VolcanoCreds;
use crate::vlm::{ARK_URL, DEFAULT_MODEL};

const MODEL_VERSION: &str = "ark-chat-completions-v3";

pub struct ArkDictationTemplateRecognizer {
    api_key: String,
    model: String,
    endpoint: String,
}

pub struct ArkDictationOcrRecognizer {
    api_key: String,
    model: String,
    endpoint: String,
}

/// 答题卡填空/简答题区只读手写 OCR；请求结构与默写一致，但提示词不假设材料是默写。
pub struct ArkHandwritingOcrRecognizer {
    api_key: String,
    model: String,
    endpoint: String,
}

fn model(creds: &VolcanoCreds) -> String {
    if creds.ark_model.trim().is_empty() {
        DEFAULT_MODEL.into()
    } else {
        creds.ark_model.trim().into()
    }
}

impl ArkDictationTemplateRecognizer {
    pub fn from_creds(creds: &VolcanoCreds) -> Self {
        Self {
            api_key: creds.ark_api_key.clone(),
            model: model(creds),
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

impl ArkDictationOcrRecognizer {
    pub fn from_creds(creds: &VolcanoCreds) -> Self {
        Self {
            api_key: creds.ark_api_key.clone(),
            model: model(creds),
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

impl ArkHandwritingOcrRecognizer {
    pub fn from_creds(creds: &VolcanoCreds) -> Self {
        Self {
            api_key: creds.ark_api_key.clone(),
            model: model(creds),
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

fn descriptor(model: &str, config: &str, rule: &str) -> DictationRecognizerDescriptor {
    DictationRecognizerDescriptor {
        provider: "volcengine_ark".into(),
        model_name: model.into(),
        model_version: MODEL_VERSION.into(),
        config_version: config.into(),
        rule_version: rule.into(),
    }
}

fn failure(code: DictationErrorCode, message: &str, retryable: bool) -> DictationFailure {
    DictationFailure {
        schema_version: DICTATION_OCR_SCHEMA_VERSION,
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

fn request_json(
    endpoint: &str,
    api_key: &str,
    model: &str,
    prompt: String,
    mime_type: &str,
    image_bytes: &[u8],
) -> Result<Value, DictationFailure> {
    if api_key.trim().is_empty() {
        return Err(failure(
            DictationErrorCode::ProviderUnavailable,
            "未配置视觉服务，默写图片已保留等待老师处理",
            true,
        ));
    }
    let data_url = format!(
        "data:{};base64,{}",
        mime_type.trim().to_ascii_lowercase(),
        base64::engine::general_purpose::STANDARD.encode(image_bytes)
    );
    let body = json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": prompt},
                {"type": "image_url", "image_url": {"url": data_url}}
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
                DictationErrorCode::Internal,
                "默写识别客户端初始化失败",
                true,
            )
        })?;
    let response = client
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .map_err(|error| {
            if error.is_timeout() {
                failure(
                    DictationErrorCode::Timeout,
                    "默写识别超时，可稍后重试",
                    true,
                )
            } else {
                failure(
                    DictationErrorCode::ProviderUnavailable,
                    "默写识别服务暂时不可用，可稍后重试",
                    true,
                )
            }
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(if status.as_u16() == 429 {
            failure(
                DictationErrorCode::RateLimited,
                "默写识别请求过多，可稍后重试",
                true,
            )
        } else {
            failure(
                DictationErrorCode::ProviderUnavailable,
                "默写识别服务返回异常，已转入老师复核",
                status.is_server_error(),
            )
        });
    }
    let envelope: Value = response.json().map_err(|_| {
        failure(
            DictationErrorCode::InvalidOutput,
            "默写识别结果格式无效，已转入老师复核",
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
                DictationErrorCode::InvalidOutput,
                "默写识别未返回结构化内容，已转入老师复核",
                false,
            )
        })?;
    serde_json::from_str(strip_json_fence(content)).map_err(|_| {
        failure(
            DictationErrorCode::InvalidOutput,
            "默写识别内容无法解析，已转入老师复核",
            false,
        )
    })
}

#[derive(Debug, Deserialize)]
struct TemplatePayload {
    state: DictationTemplateState,
    canvas_width: u32,
    canvas_height: u32,
    regions: Vec<DictationRegionProposal>,
    confidence: f64,
    #[serde(default)]
    issue_codes: Vec<String>,
}

impl DictationTemplateRecognizer for ArkDictationTemplateRecognizer {
    fn descriptor(&self) -> DictationRecognizerDescriptor {
        descriptor(
            &self.model,
            "fixed-dictation-template-v1",
            "fixed-dictation-regions-json-v1",
        )
    }

    fn recognize(
        &self,
        request: &DictationTemplateRequest<'_>,
    ) -> Result<DictationTemplateOutput, DictationFailure> {
        request.validate().map_err(|_| {
            failure(
                DictationErrorCode::TemplateMismatch,
                "默写空白页与当前作业不一致",
                false,
            )
        })?;
        let items = serde_json::to_string(request.items).unwrap_or_else(|_| "[]".into());
        let prompt = format!(
            "你是固定格式默写空白页结构分析器，只定位每个题号/行/栏对应的书写区域，不读答案、不判分。\n\
             当前页码：{}；当前题目清单：{}。\n\
             只输出 JSON：{{state,canvas_width,canvas_height,regions,confidence,issue_codes}}。\n\
             state 为 ready|needs_review|blocked；regions 每项为 {{assessment_item_id,region_index,bbox:{{x,y,width,height}},mapping_confidence}}，bbox 按整页 0~1 归一化。\n\
             不得新增或遗漏题目；无法稳定区分题号、行栏或书写区时必须 needs_review/blocked。只有全部映射置信度和总置信度均不低于 0.95 且无问题时才能 ready。",
            request.page_no, items
        );
        let value = request_json(
            &self.endpoint,
            &self.api_key,
            &self.model,
            prompt,
            request.mime_type,
            request.image_bytes,
        )?;
        let payload: TemplatePayload = serde_json::from_value(value).map_err(|_| {
            failure(
                DictationErrorCode::InvalidOutput,
                "默写模板字段不完整，已转入老师复核",
                false,
            )
        })?;
        let output = DictationTemplateOutput {
            schema_version: DICTATION_TEMPLATE_SCHEMA_VERSION,
            assessment_version_id: request.assessment_version_id,
            page_no: request.page_no,
            blank_artifact_id: request.blank_artifact_id,
            blank_artifact_sha256: request.blank_artifact_sha256.trim().to_ascii_lowercase(),
            input_hash: request.input_hash().map_err(|_| {
                failure(
                    DictationErrorCode::TemplateMismatch,
                    "默写模板输入无法版本化",
                    false,
                )
            })?,
            template_version: request.template_version.trim().into(),
            descriptor: self.descriptor(),
            state: payload.state,
            canvas_width: payload.canvas_width,
            canvas_height: payload.canvas_height,
            regions: payload.regions,
            confidence: payload.confidence,
            issue_codes: payload.issue_codes,
        };
        output.validate_against(request).map_err(|_| {
            failure(
                DictationErrorCode::InvalidOutput,
                "默写模板未通过安全校验，已转入老师复核",
                false,
            )
        })?;
        Ok(output)
    }
}

#[derive(Debug, Deserialize)]
struct OcrPayload {
    state: DictationRecognitionState,
    raw_text: Option<String>,
    normalized_text: Option<String>,
    confidence: Option<f64>,
    #[serde(default)]
    issue_codes: Vec<String>,
}

impl DictationOcrRecognizer for ArkDictationOcrRecognizer {
    fn descriptor(&self) -> DictationRecognizerDescriptor {
        descriptor(
            &self.model,
            "fixed-dictation-ocr-v1",
            "raw-handwriting-only-json-v1",
        )
    }

    fn recognize(
        &self,
        request: &DictationOcrRequest<'_>,
    ) -> Result<DictationOcrOutput, DictationFailure> {
        request.validate().map_err(|_| {
            failure(
                DictationErrorCode::TemplateMismatch,
                "默写裁剪与当前题区不一致",
                false,
            )
        })?;
        let prompt = "你是中文历史默写手写 OCR。只读取图片中学生实际写下的文字，不猜标准答案，不纠正史实、错别字、人名、地名、年份或条约名。只输出 JSON：{state,raw_text,normalized_text,confidence,issue_codes}。state 为 recognized|not_written|unreadable|ambiguous_final；recognized 时原样保留 raw_text，normalized_text 只允许整理空白和标点，不能把疑似错字改成正确知识；其余状态三个文本/置信度字段必须为 null。涂改后无法确定最终答案时用 ambiguous_final。".to_string();
        let value = request_json(
            &self.endpoint,
            &self.api_key,
            &self.model,
            prompt,
            request.mime_type,
            request.image_bytes,
        )?;
        let payload: OcrPayload = serde_json::from_value(value).map_err(|_| {
            failure(
                DictationErrorCode::InvalidOutput,
                "默写 OCR 字段不完整，已转入老师复核",
                false,
            )
        })?;
        let output = DictationOcrOutput {
            schema_version: DICTATION_OCR_SCHEMA_VERSION,
            answer_region_revision_id: request.answer_region_revision_id,
            crop_artifact_id: request.crop_artifact_id,
            crop_artifact_sha256: request.crop_artifact_sha256.trim().to_ascii_lowercase(),
            input_hash: request.input_hash().map_err(|_| {
                failure(
                    DictationErrorCode::TemplateMismatch,
                    "默写 OCR 输入无法版本化",
                    false,
                )
            })?,
            descriptor: self.descriptor(),
            state: payload.state,
            raw_text: payload.raw_text,
            normalized_text: payload.normalized_text,
            confidence: payload.confidence,
            issue_codes: payload.issue_codes,
        };
        output.validate_against(request).map_err(|_| {
            failure(
                DictationErrorCode::InvalidOutput,
                "默写 OCR 未通过安全校验，已转入老师复核",
                false,
            )
        })?;
        Ok(output)
    }
}

impl DictationOcrRecognizer for ArkHandwritingOcrRecognizer {
    fn descriptor(&self) -> DictationRecognizerDescriptor {
        descriptor(
            &self.model,
            "answer-sheet-handwriting-ocr-v1",
            "raw-handwriting-only-json-v1",
        )
    }

    fn recognize(
        &self,
        request: &DictationOcrRequest<'_>,
    ) -> Result<DictationOcrOutput, DictationFailure> {
        request.validate().map_err(|_| {
            failure(
                DictationErrorCode::TemplateMismatch,
                "主观题裁剪与当前题区不一致",
                false,
            )
        })?;
        let prompt = "你是中文历史试卷手写 OCR。只读取图片中学生实际写下的文字，不看也不猜标准答案，不判分，不纠正史实、错别字、人名、地名、年份或条约名。只输出 JSON：{state,raw_text,normalized_text,confidence,issue_codes}。state 为 recognized|not_written|unreadable|ambiguous_final；recognized 时原样保留 raw_text，normalized_text 只允许整理空白和标点，不能把疑似错字改成正确知识；其余状态三个文本/置信度字段必须为 null。涂改后无法确定最终答案时用 ambiguous_final。".to_string();
        let value = request_json(
            &self.endpoint,
            &self.api_key,
            &self.model,
            prompt,
            request.mime_type,
            request.image_bytes,
        )?;
        let payload: OcrPayload = serde_json::from_value(value).map_err(|_| {
            failure(
                DictationErrorCode::InvalidOutput,
                "手写 OCR 字段不完整，已转入老师复核",
                false,
            )
        })?;
        let output = DictationOcrOutput {
            schema_version: DICTATION_OCR_SCHEMA_VERSION,
            answer_region_revision_id: request.answer_region_revision_id,
            crop_artifact_id: request.crop_artifact_id,
            crop_artifact_sha256: request.crop_artifact_sha256.trim().to_ascii_lowercase(),
            input_hash: request.input_hash().map_err(|_| {
                failure(
                    DictationErrorCode::TemplateMismatch,
                    "手写 OCR 输入无法版本化",
                    false,
                )
            })?,
            descriptor: self.descriptor(),
            state: payload.state,
            raw_text: payload.raw_text,
            normalized_text: payload.normalized_text,
            confidence: payload.confidence,
            issue_codes: payload.issue_codes,
        };
        output.validate_against(request).map_err(|_| {
            failure(
                DictationErrorCode::InvalidOutput,
                "手写 OCR 未通过安全校验，已转入老师复核",
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
    use std::sync::mpsc;
    use std::thread;

    use module_exam::dictation_recognition::{DictationItemSpec, DictationQuestionType};
    use suite_core::domain::hashing;

    use super::*;

    fn serve_once(body: String) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 32_768];
            let read = stream.read(&mut request).unwrap();
            let _ = sender.send(String::from_utf8_lossy(&request[..read]).into_owned());
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{address}"), receiver)
    }

    #[test]
    fn template_adapter_accepts_only_complete_high_confidence_mapping() {
        let bytes = b"fixed-dictation-blank";
        let hash = hashing::sha256_hex(bytes);
        let items = [DictationItemSpec {
            assessment_item_id: 11,
            order_index: 0,
            question_type: DictationQuestionType::FillBlank,
        }];
        let payload = json!({
            "state": "ready",
            "canvas_width": 1200,
            "canvas_height": 1800,
            "regions": [{
                "assessment_item_id": 11,
                "region_index": 0,
                "bbox": {"x": 0.1, "y": 0.2, "width": 0.8, "height": 0.1},
                "mapping_confidence": 0.99
            }],
            "confidence": 0.99,
            "issue_codes": []
        });
        let envelope = json!({"choices":[{"message":{"content":payload.to_string()}}]});
        let (endpoint, _) = serve_once(envelope.to_string());
        let recognizer = ArkDictationTemplateRecognizer::for_test(endpoint);
        let request = DictationTemplateRequest {
            assessment_version_id: 7,
            page_no: 1,
            blank_artifact_id: 9,
            blank_artifact_sha256: &hash,
            mime_type: "image/jpeg",
            image_bytes: bytes,
            template_version: "dictation-v1",
            items: &items,
        };
        let output = recognizer.recognize(&request).unwrap();
        assert_eq!(output.state, DictationTemplateState::Ready);
        assert_eq!(output.regions.len(), 1);
    }

    #[test]
    fn ocr_adapter_preserves_wrong_text_and_request_has_no_answer() {
        let bytes = b"student-wrote-1840";
        let hash = hashing::sha256_hex(bytes);
        let payload = json!({
            "state": "recognized",
            "raw_text": "1840年",
            "normalized_text": "1840年",
            "confidence": 0.98,
            "issue_codes": []
        });
        let envelope = json!({"choices":[{"message":{"content":payload.to_string()}}]});
        let (endpoint, received) = serve_once(envelope.to_string());
        let recognizer = ArkDictationOcrRecognizer::for_test(endpoint);
        let request = DictationOcrRequest {
            answer_region_revision_id: 8,
            crop_artifact_id: 9,
            crop_artifact_sha256: &hash,
            mime_type: "image/jpeg",
            image_bytes: bytes,
        };
        let output = recognizer.recognize(&request).unwrap();
        assert_eq!(output.raw_text.as_deref(), Some("1840年"));
        assert_eq!(output.normalized_text.as_deref(), Some("1840年"));

        let sent = received.recv().unwrap();
        assert!(!sent.contains("1842"));
        assert!(sent.contains("不猜标准答案"));
    }

    #[test]
    fn answer_sheet_handwriting_adapter_is_raw_ocr_not_grading() {
        let bytes = b"student-short-answer";
        let hash = hashing::sha256_hex(bytes);
        let payload = json!({
            "state": "recognized",
            "raw_text": "因为只学技术没改制度",
            "normalized_text": "因为只学技术没改制度",
            "confidence": 0.96,
            "issue_codes": []
        });
        let envelope = json!({"choices":[{"message":{"content":payload.to_string()}}]});
        let (endpoint, received) = serve_once(envelope.to_string());
        let recognizer = ArkHandwritingOcrRecognizer::for_test(endpoint);
        let request = DictationOcrRequest {
            answer_region_revision_id: 18,
            crop_artifact_id: 19,
            crop_artifact_sha256: &hash,
            mime_type: "image/jpeg",
            image_bytes: bytes,
        };
        let output = recognizer.recognize(&request).unwrap();
        assert_eq!(output.raw_text.as_deref(), Some("因为只学技术没改制度"));
        let sent = received.recv().unwrap();
        assert!(sent.contains("不看也不猜标准答案"));
        assert!(sent.contains("不判分"));
    }
}
