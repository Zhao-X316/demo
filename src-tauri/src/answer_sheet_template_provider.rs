//! 火山方舟空白答题卡模板候选适配器。
//!
//! 模型只提出四角锚点、题号与格位；输出还要通过 provider-neutral 合同校验，
//! 且必须由老师确认一次后才能用于学生答题卡。

use std::time::Duration;

use base64::Engine;
use module_exam::answer_sheet_recognition::{
    AnswerSheetAnchor, AnswerSheetItemTemplate, AnswerSheetSubjectiveRegionTemplate, LocalOmrPolicy,
};
use module_exam::answer_sheet_template_recognition::{
    AnswerSheetTemplateRecognitionErrorCode, AnswerSheetTemplateRecognitionFailure,
    AnswerSheetTemplateRecognitionOutput, AnswerSheetTemplateRecognitionRequest,
    AnswerSheetTemplateRecognitionState, AnswerSheetTemplateRecognizer,
    AnswerSheetTemplateRecognizerDescriptor, ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::secrets::VolcanoCreds;
use crate::vlm::{ARK_URL, DEFAULT_MODEL};

const MODEL_VERSION: &str = "ark-chat-completions-v3";
const CONFIG_VERSION: &str = "answer-sheet-template-v2";
const RULE_VERSION: &str = "answer-sheet-template-json-v2";

pub struct ArkAnswerSheetTemplateRecognizer {
    api_key: String,
    model: String,
    endpoint: String,
}

impl ArkAnswerSheetTemplateRecognizer {
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

impl AnswerSheetTemplateRecognizer for ArkAnswerSheetTemplateRecognizer {
    fn descriptor(&self) -> AnswerSheetTemplateRecognizerDescriptor {
        AnswerSheetTemplateRecognizerDescriptor {
            provider: "volcengine_ark".into(),
            model_name: self.model.clone(),
            model_version: MODEL_VERSION.into(),
            config_version: CONFIG_VERSION.into(),
            rule_version: RULE_VERSION.into(),
        }
    }

    fn recognize(
        &self,
        request: &AnswerSheetTemplateRecognitionRequest<'_>,
    ) -> Result<AnswerSheetTemplateRecognitionOutput, AnswerSheetTemplateRecognitionFailure> {
        request.validate().map_err(|_| {
            failure(
                AnswerSheetTemplateRecognitionErrorCode::TemplateMismatch,
                "空白答题卡与当前作业题目清单不一致",
                false,
            )
        })?;
        if self.api_key.trim().is_empty() {
            return Err(failure(
                AnswerSheetTemplateRecognitionErrorCode::ProviderUnavailable,
                "未配置视觉服务，空白答题卡已保留等待重试",
                true,
            ));
        }
        let input_hash = request.input_hash().map_err(|_| {
            failure(
                AnswerSheetTemplateRecognitionErrorCode::TemplateMismatch,
                "空白答题卡无法建立稳定输入版本",
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
                    AnswerSheetTemplateRecognitionErrorCode::Internal,
                    "答题卡模板分析客户端初始化失败",
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
                        AnswerSheetTemplateRecognitionErrorCode::Timeout,
                        "答题卡模板分析超时，可稍后重试",
                        true,
                    )
                } else {
                    failure(
                        AnswerSheetTemplateRecognitionErrorCode::ProviderUnavailable,
                        "答题卡模板分析服务暂时不可用，可稍后重试",
                        true,
                    )
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(if status.as_u16() == 429 {
                failure(
                    AnswerSheetTemplateRecognitionErrorCode::RateLimited,
                    "答题卡模板分析请求过于频繁，可稍后重试",
                    true,
                )
            } else {
                failure(
                    AnswerSheetTemplateRecognitionErrorCode::ProviderUnavailable,
                    "答题卡模板分析服务返回异常，已保留空白卡等待重试",
                    status.is_server_error(),
                )
            });
        }
        let envelope: Value = response.json().map_err(|_| {
            failure(
                AnswerSheetTemplateRecognitionErrorCode::InvalidOutput,
                "答题卡模板结果格式无效，已转入老师复核",
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
                    AnswerSheetTemplateRecognitionErrorCode::InvalidOutput,
                    "答题卡模板未返回结构化内容，已转入老师复核",
                    false,
                )
            })?;
        let parsed: ArkAnswerSheetTemplatePayload = serde_json::from_str(strip_json_fence(content))
            .map_err(|_| {
                failure(
                    AnswerSheetTemplateRecognitionErrorCode::InvalidOutput,
                    "答题卡模板内容无法解析，已转入老师复核",
                    false,
                )
            })?;
        let output = AnswerSheetTemplateRecognitionOutput {
            schema_version: ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION,
            assessment_version_id: request.assessment_version_id,
            page_no: request.page_no,
            template_version: request.template_version.trim().into(),
            blank_artifact_id: request.blank_artifact_id,
            blank_artifact_sha256: request.blank_artifact_sha256.trim().to_ascii_lowercase(),
            input_hash,
            descriptor: self.descriptor(),
            state: parsed.state,
            canvas_width: parsed.canvas_width,
            canvas_height: parsed.canvas_height,
            anchors: parsed.anchors,
            items: parsed.items,
            subjective_regions: parsed.subjective_regions,
            policy: parsed.policy,
            confidence: parsed.confidence,
            issue_codes: parsed.issue_codes,
        };
        output.validate_against(request).map_err(|_| {
            failure(
                AnswerSheetTemplateRecognitionErrorCode::InvalidOutput,
                "答题卡模板未通过题号、锚点和格位安全校验，已转入老师复核",
                false,
            )
        })?;
        Ok(output)
    }
}

#[derive(Debug, Deserialize)]
struct ArkAnswerSheetTemplatePayload {
    state: AnswerSheetTemplateRecognitionState,
    canvas_width: u32,
    canvas_height: u32,
    anchors: Vec<AnswerSheetAnchor>,
    items: Vec<AnswerSheetItemTemplate>,
    #[serde(default)]
    subjective_regions: Vec<AnswerSheetSubjectiveRegionTemplate>,
    policy: LocalOmrPolicy,
    confidence: f64,
    #[serde(default)]
    issue_codes: Vec<String>,
}

fn build_prompt(request: &AnswerSheetTemplateRecognitionRequest<'_>) -> String {
    let items = serde_json::to_string(request.items).unwrap_or_else(|_| "[]".into());
    format!(
        "你是固定答题卡空白模板分析器，只定位四角校准锚点、客观题涂点格和填空/简答作答区，不读取学生答案、不判分。\n\
         当前页码：{}；模板版本：{}；当前页题目清单：{}。\n\
         只输出一个 JSON 对象，不要 markdown。字段必须为：\n\
         state: ready|needs_review|blocked；canvas_width、canvas_height 必须是空白原图像素尺寸；\n\
         anchors: 四项，每项 {{key,expected:{{x,y,width,height}},search:{{x,y,width,height}}}}，key 必须且只能是 top_left/top_right/bottom_left/bottom_right；\n\
         expected 是对应黑色定位块在空白原图上的 0~1 区域，search 是学生照片中寻找同一定位块的安全搜索区；\n\
         items: [{{assessment_item_id,region_index,question_type,region:{{x,y,width,height}},cells:[{{label,x,y,width,height}}]}}]；\n\
         question_type 为 single|multiple|true_false；region 按整页 0~1，cells 按各自 region 0~1；判断题标签必须且只能 TRUE/FALSE；\n\
         subjective_regions: [{{assessment_item_id,region_index,question_type,region:{{x,y,width,height}}}}]，question_type 只能为 fill_blank|short_answer；主观区不得出现在 items，也不得生成 cells；\n\
         policy 固定输出 {{blank_max_ratio:0.04,marked_min_ratio:0.14,pixel_delta_threshold:24,cell_inset_ratio:0.18}}；\n\
         confidence: 0到1；issue_codes: 字符串数组。不得新增、遗漏或重复题目；找不到四个可靠黑色定位块、题号或格位时必须 needs_review/blocked；\n\
         只有四角、全部题号和全部格位都清晰且各置信度不低于 0.95 时才能 ready。",
        request.page_no,
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
    code: AnswerSheetTemplateRecognitionErrorCode,
    safe_message: &str,
    retryable: bool,
) -> AnswerSheetTemplateRecognitionFailure {
    AnswerSheetTemplateRecognitionFailure {
        schema_version: ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION,
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

    use image::{DynamicImage, ImageOutputFormat};
    use module_exam::answer_sheet_template_recognition::{
        AnswerSheetTemplateItemSpec, AnswerSheetTemplateQuestionType,
    };
    use suite_core::domain::hashing;

    use super::*;

    fn serve_once(body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 16_384];
            let _ = stream.read(&mut request);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(), body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        format!("http://{address}")
    }

    #[test]
    fn adapter_parses_a_strict_ready_candidate() {
        let mut image = Vec::new();
        DynamicImage::new_rgb8(1200, 1800)
            .write_to(
                &mut std::io::Cursor::new(&mut image),
                ImageOutputFormat::Jpeg(90),
            )
            .unwrap();
        let hash = hashing::sha256_hex(&image);
        let items = vec![AnswerSheetTemplateItemSpec {
            assessment_item_id: 11,
            order_index: 0,
            question_type: AnswerSheetTemplateQuestionType::Single,
        }];
        let payload = json!({
            "state":"ready","canvas_width":1200,"canvas_height":1800,
            "anchors":[
                {"key":"top_left","expected":{"x":0.02,"y":0.02,"width":0.03,"height":0.03},"search":{"x":0.0,"y":0.0,"width":0.1,"height":0.1}},
                {"key":"top_right","expected":{"x":0.95,"y":0.02,"width":0.03,"height":0.03},"search":{"x":0.9,"y":0.0,"width":0.1,"height":0.1}},
                {"key":"bottom_left","expected":{"x":0.02,"y":0.95,"width":0.03,"height":0.03},"search":{"x":0.0,"y":0.9,"width":0.1,"height":0.1}},
                {"key":"bottom_right","expected":{"x":0.95,"y":0.95,"width":0.03,"height":0.03},"search":{"x":0.9,"y":0.9,"width":0.1,"height":0.1}}
            ],
            "items":[{"assessment_item_id":11,"region_index":0,"question_type":"single","region":{"x":0.1,"y":0.2,"width":0.8,"height":0.08},"cells":[{"label":"A","x":0.1,"y":0.1,"width":0.1,"height":0.8},{"label":"B","x":0.3,"y":0.1,"width":0.1,"height":0.8}]}],
            "subjective_regions":[],
            "policy":{"blank_max_ratio":0.04,"marked_min_ratio":0.14,"pixel_delta_threshold":24,"cell_inset_ratio":0.18},
            "confidence":0.99,"issue_codes":[]
        });
        let body = json!({"choices":[{"message":{"content":payload.to_string()}}]}).to_string();
        let recognizer = ArkAnswerSheetTemplateRecognizer::for_test(serve_once(body));
        let request = AnswerSheetTemplateRecognitionRequest {
            assessment_version_id: 7,
            page_no: 1,
            template_version: "sheet-v1",
            blank_artifact_id: 9,
            blank_artifact_sha256: &hash,
            mime_type: "image/jpeg",
            image_bytes: &image,
            items: &items,
        };
        let output = recognizer.recognize(&request).unwrap();
        assert_eq!(output.state, AnswerSheetTemplateRecognitionState::Ready);
        assert_eq!(output.items.len(), 1);
    }
}
