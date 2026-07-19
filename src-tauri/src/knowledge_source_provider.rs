//! 火山方舟独立题目来源提取适配器。
//!
//! 输入只允许老师主动上传、已按 teaching_content 归档的空白卷或电子题目文件。
//! 模型不接触学生作答，也不生成答案；输出必须通过 module-knowledge 的严格契约校验。

use std::time::Duration;

use base64::Engine;
use module_knowledge::source_import::{
    validate_output, SourceExtractionInput, SourceExtractionOutput, SourceQuestionRecognizer,
};
use serde_json::{json, Value};
use suite_core::error::{CoreError, CoreResult};

use crate::secrets::VolcanoCreds;
use crate::vlm::{ARK_URL, DEFAULT_MODEL};

const MODEL_VERSION: &str = "ark-chat-completions-v3";
const CONFIG_VERSION: &str = "k1-source-jpeg-pdf-text-office-v1";
const RULE_VERSION: &str = "k1-clean-question-source-no-answer-v1";

pub struct ArkSourceQuestionRecognizer {
    api_key: String,
    model: String,
    endpoint: String,
}

impl ArkSourceQuestionRecognizer {
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

fn strip_json_fence(content: &str) -> &str {
    let content = content.trim();
    let content = content
        .strip_prefix("```json")
        .or_else(|| content.strip_prefix("```"))
        .unwrap_or(content)
        .trim();
    content.strip_suffix("```").unwrap_or(content).trim()
}

fn prompt(input: &SourceExtractionInput) -> String {
    format!(
        "你是历史教师私有题库的题面整理器。输入是老师主动上传的空白卷或电子题目文件，\
         只提取文件中明确存在的题目，不找答案、不猜答案、不评分、不发布题目。\n\
         来源类型：{}；格式：{}；总页数：{}。\n\
         本机提取文字如下（图片/PDF 时可为空）：\n{}\n\
         只输出一个 JSON 对象，不要 Markdown。必须完整符合：\n\
         {{\"schema_version\":1,\"state\":\"ready|needs_review|blocked\",\
         \"confidence\":0~1,\"privacy\":{{\"schema_version\":1,\"sanitized\":bool,\
         \"contains_student_identity\":bool,\"contains_student_answer\":bool,\
         \"contains_teacher_mark\":bool,\"contains_score\":bool}},\
         \"issue_codes\":[string],\"drafts\":[...]}}。\n\
         每个 draft 必须含 order_index(从1开始)、question_no(可空)、\
         question_type(single|multiple|true_false|fill_blank|short_answer)、stem、\
         material_text(可空)、max_score(资料没写时按1)、options、source_anchor、confidence。\
         选择题 options 每项含 label、content、order_index(从1开始)，其他题型 options=[]。\
         source_anchor 必须含 schema_version=1、真实 page_no；视觉页可写归一化 region=[x1,y1,x2,y2]，\
         文字来源可写 line_start/line_end。不能确定题号、题型、题干边界或分值时进入 needs_review。\
         若发现学生姓名/学号/手写作答/教师批注/分数，必须 state=blocked、drafts=[] 并在 privacy 标明。\
         只有隐私检查通过、所有题完整、总置信度和逐题置信度都不少于0.95且 issue_codes 为空时才可 ready。",
        input.source_type,
        input.source_format,
        input.page_count,
        input.extracted_text.as_deref().unwrap_or_default()
    )
}

impl SourceQuestionRecognizer for ArkSourceQuestionRecognizer {
    fn provider(&self) -> &str {
        "volcengine_ark"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn model_version(&self) -> &str {
        MODEL_VERSION
    }

    fn config_version(&self) -> &str {
        CONFIG_VERSION
    }

    fn prompt_or_rule_version(&self) -> &str {
        RULE_VERSION
    }

    fn recognize(&self, input: &SourceExtractionInput) -> CoreResult<SourceExtractionOutput> {
        module_knowledge::source_import::validate_input(input)?;
        if self.api_key.trim().is_empty() {
            return Err(CoreError::Invalid(
                "未配置视觉服务，题目来源已保留，可配置后重试".into(),
            ));
        }
        let mut content = vec![json!({"type":"text","text":prompt(input)})];
        for page in &input.visual_pages {
            content.push(json!({
                "type": "text",
                "text": format!("题目来源第 {} 页", page.page_no)
            }));
            content.push(json!({
                "type": "image_url",
                "image_url": {
                    "url": format!(
                        "data:{};base64,{}",
                        page.mime_type,
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
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|_| CoreError::Invalid("题目提取客户端初始化失败".into()))?;
        let response = client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    CoreError::Invalid("题目提取超时，可稍后重试".into())
                } else {
                    CoreError::Invalid("题目提取服务暂不可用".into())
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(CoreError::Invalid(if status.as_u16() == 429 {
                "题目提取请求过多，可稍后重试".into()
            } else {
                "题目提取服务返回异常".into()
            }));
        }
        let envelope: Value = response
            .json()
            .map_err(|_| CoreError::Invalid("题目提取响应无法解析".into()))?;
        let raw = envelope
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Invalid("题目提取未返回内容".into()))?;
        let output: SourceExtractionOutput = serde_json::from_str(strip_json_fence(raw))
            .map_err(|_| CoreError::Invalid("题目提取结果格式无效".into()))?;
        validate_output(input, &output)?;
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use module_knowledge::source_import::SourcePrivacyResult;

    use super::*;

    fn input() -> SourceExtractionInput {
        SourceExtractionInput {
            schema_version: 1,
            source_document_public_id: "doc-1".into(),
            source_type: "source_document".into(),
            source_format: "text".into(),
            source_hash: "a".repeat(64),
            page_count: 1,
            extracted_text: Some("1. 洋务运动前期的口号是什么？".into()),
            visual_pages: Vec::new(),
        }
    }

    fn response(output: &SourceExtractionOutput) -> String {
        serde_json::json!({
            "choices": [{
                "message": {"content": serde_json::to_string(output).unwrap()}
            }]
        })
        .to_string()
    }

    fn serve_once(body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 16384];
            let _ = stream.read(&mut request);
            let reply = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(reply.as_bytes()).unwrap();
        });
        format!("http://{address}")
    }

    #[test]
    fn adapter_accepts_contract_valid_output() {
        let output = SourceExtractionOutput {
            schema_version: 1,
            state: "ready".into(),
            confidence: 0.99,
            privacy: SourcePrivacyResult {
                schema_version: 1,
                sanitized: true,
                contains_student_identity: false,
                contains_student_answer: false,
                contains_teacher_mark: false,
                contains_score: false,
            },
            issue_codes: Vec::new(),
            drafts: vec![module_knowledge::source_import::SourceQuestionDraft {
                order_index: 1,
                question_no: Some("1".into()),
                question_type: "short_answer".into(),
                stem: "洋务运动前期的口号是什么？".into(),
                material_text: None,
                max_score: 1.0,
                options: Vec::new(),
                source_anchor: module_knowledge::source_import::SourceAnchorDraft {
                    schema_version: 1,
                    page_no: 1,
                    region: None,
                    line_start: Some(1),
                    line_end: Some(1),
                },
                confidence: 0.99,
            }],
        };
        let recognizer = ArkSourceQuestionRecognizer::for_test(serve_once(response(&output)));
        assert_eq!(recognizer.recognize(&input()).unwrap(), output);
    }

    #[test]
    fn adapter_rejects_provider_privacy_leak() {
        let output = SourceExtractionOutput {
            schema_version: 1,
            state: "blocked".into(),
            confidence: 0.9,
            privacy: SourcePrivacyResult {
                schema_version: 1,
                sanitized: false,
                contains_student_identity: true,
                contains_student_answer: false,
                contains_teacher_mark: false,
                contains_score: false,
            },
            issue_codes: vec!["STUDENT_IDENTITY".into()],
            drafts: vec![module_knowledge::source_import::SourceQuestionDraft {
                order_index: 1,
                question_no: None,
                question_type: "short_answer".into(),
                stem: "不应泄露".into(),
                material_text: None,
                max_score: 1.0,
                options: Vec::new(),
                source_anchor: module_knowledge::source_import::SourceAnchorDraft {
                    schema_version: 1,
                    page_no: 1,
                    region: None,
                    line_start: None,
                    line_end: None,
                },
                confidence: 0.9,
            }],
        };
        let recognizer = ArkSourceQuestionRecognizer::for_test(serve_once(response(&output)));
        let error = recognizer.recognize(&input()).unwrap_err().to_string();
        assert!(error.contains("不得返回题干"));
    }
}
