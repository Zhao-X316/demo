//! 火山方舟篇幅受控简答题逐评分点适配器。
//!
//! 请求只包含题干、学生 OCR/老师校正文本、已确认参考答案和 rubric；不包含学生身份、
//! 班级或整页图片。provider 返回的每个得分证据还会由 module-exam 再做原文子串校验。

use std::time::Duration;

use module_exam::short_answer_grading::{
    ShortAnswerGradeErrorCode, ShortAnswerGradeFailure, ShortAnswerGradeOutput,
    ShortAnswerGradeRequest, ShortAnswerGradeState, ShortAnswerGrader,
    ShortAnswerGraderDescriptor, ShortAnswerPointResult, SHORT_ANSWER_GRADE_SCHEMA_VERSION,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::secrets::VolcanoCreds;
use crate::vlm::{ARK_URL, DEFAULT_MODEL};

const MODEL_VERSION: &str = "ark-chat-completions-v3";
const CONFIG_VERSION: &str = "short-answer-rubric-point-text-v1";
const RULE_VERSION: &str = "literal-evidence-teacher-review-v1";

pub struct ArkShortAnswerGrader {
    api_key: String,
    model: String,
    endpoint: String,
}

impl ArkShortAnswerGrader {
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

fn failure(
    code: ShortAnswerGradeErrorCode,
    message: &str,
    retryable: bool,
) -> ShortAnswerGradeFailure {
    ShortAnswerGradeFailure {
        schema_version: SHORT_ANSWER_GRADE_SCHEMA_VERSION,
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
    state: ShortAnswerGradeState,
    suggested_score: f64,
    point_results: Vec<ShortAnswerPointResult>,
    confidence: f64,
    #[serde(default)]
    issue_codes: Vec<String>,
}

fn build_prompt(request: &ShortAnswerGradeRequest) -> String {
    let rubric_points =
        serde_json::to_string(&request.rubric_points).unwrap_or_else(|_| "[]".into());
    format!(
        "你是历史学科简答题的逐评分点证据整理器，只给老师评分建议，不能确认成绩。\n\
         题干：{}\n\
         学生答案原文：{}\n\
         已确认参考答案：{}\n\
         已确认评分点：{}\n\
         只输出一个 JSON 对象，不要 markdown：state 为 ready|needs_review；\n\
         suggested_score 为逐点评分之和；point_results 必须逐一覆盖全部评分点，每项只能含\n\
         rubric_point_id、stable_id、status、suggested_score、evidence_snippets、reason、confidence。\n\
         status 只能是 covered|partial|missing|contradicted|uncertain。covered/partial 必须引用\n\
         学生答案中逐字存在的非空 evidence_snippets；missing 必须 0 分且无证据；contradicted\n\
         必须引用学生原文且 0 分；uncertain 不得给分。不能把参考答案或你补写的文字当学生证据。\n\
         有 contradicted/uncertain 或任何歧义时 state 必须 needs_review 并填写 issue_codes；\n\
         confidence 为 0~1。不得新增评分点，不得超出单点评分上限。",
        request.question_stem,
        request.student_answer,
        request.reference_answer,
        rubric_points
    )
}

impl ShortAnswerGrader for ArkShortAnswerGrader {
    fn descriptor(&self) -> ShortAnswerGraderDescriptor {
        ShortAnswerGraderDescriptor {
            provider: "volcengine_ark".into(),
            model_name: self.model.clone(),
            model_version: MODEL_VERSION.into(),
            config_version: CONFIG_VERSION.into(),
            rule_version: RULE_VERSION.into(),
        }
    }

    fn grade(
        &self,
        request: &ShortAnswerGradeRequest,
    ) -> Result<ShortAnswerGradeOutput, ShortAnswerGradeFailure> {
        request.validate().map_err(|_| {
            failure(
                ShortAnswerGradeErrorCode::ScopeChanged,
                "简答题评分范围或版本已变化，请刷新后重试",
                false,
            )
        })?;
        if self.api_key.trim().is_empty() {
            return Err(failure(
                ShortAnswerGradeErrorCode::ProviderUnavailable,
                "未配置 AI 服务，简答题已保留等待老师判定",
                true,
            ));
        }
        let body = json!({
            "model": self.model,
            "messages": [{"role":"user","content":build_prompt(request)}],
            "temperature": 0.0
        });
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(90))
            .build()
            .map_err(|_| {
                failure(
                    ShortAnswerGradeErrorCode::Internal,
                    "简答题评分客户端初始化失败",
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
                        ShortAnswerGradeErrorCode::Timeout,
                        "简答题评分超时，可稍后重试",
                        true,
                    )
                } else {
                    failure(
                        ShortAnswerGradeErrorCode::ProviderUnavailable,
                        "简答题评分服务暂不可用",
                        true,
                    )
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(if status.as_u16() == 429 {
                failure(
                    ShortAnswerGradeErrorCode::RateLimited,
                    "简答题评分请求过多，可稍后重试",
                    true,
                )
            } else {
                failure(
                    ShortAnswerGradeErrorCode::ProviderUnavailable,
                    "简答题评分服务返回异常",
                    status.is_server_error(),
                )
            });
        }
        let envelope: Value = response.json().map_err(|_| {
            failure(
                ShortAnswerGradeErrorCode::InvalidOutput,
                "简答题评分结果格式无效",
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
                    ShortAnswerGradeErrorCode::InvalidOutput,
                    "简答题评分未返回内容",
                    false,
                )
            })?;
        let payload: Payload = serde_json::from_str(strip_json_fence(raw)).map_err(|_| {
            failure(
                ShortAnswerGradeErrorCode::InvalidOutput,
                "简答题评分内容无法解析",
                false,
            )
        })?;
        let output = ShortAnswerGradeOutput {
            schema_version: SHORT_ANSWER_GRADE_SCHEMA_VERSION,
            transcription_revision_id: request.transcription_revision_id,
            assessment_item_id: request.assessment_item_id,
            answer_key_version_id: request.answer_key_version_id,
            rubric_version_id: request.rubric_version_id,
            input_hash: request.input_hash().map_err(|_| {
                failure(
                    ShortAnswerGradeErrorCode::Internal,
                    "简答题评分输入版本失败",
                    false,
                )
            })?,
            descriptor: self.descriptor(),
            state: payload.state,
            suggested_score: payload.suggested_score,
            point_results: payload.point_results,
            confidence: payload.confidence,
            issue_codes: payload.issue_codes,
        };
        output.validate_against(request).map_err(|_| {
            failure(
                ShortAnswerGradeErrorCode::InvalidOutput,
                "简答题评分结果未通过证据校验，已交给老师判定",
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

    use module_exam::short_answer_grading::ShortAnswerRubricPointSpec;

    use super::*;

    fn serve_once(body: &str) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let body = body.to_string();
        let (sender, receiver) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 32768];
            let read = stream.read(&mut buffer).unwrap();
            let _ = sender.send(String::from_utf8_lossy(&buffer[..read]).into());
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        (format!("http://{address}"), receiver)
    }

    fn request() -> ShortAnswerGradeRequest {
        ShortAnswerGradeRequest {
            transcription_revision_id: 7,
            attempt_id: 8,
            assessment_item_id: 9,
            answer_region_revision_id: 10,
            crop_artifact_id: 11,
            question_stem: "概括洋务运动失败的原因".into(),
            student_answer: "只学习技术，没有改变封建制度".into(),
            reference_answer: "只学技术而不改变制度".into(),
            answer_key_version_id: 12,
            rubric_version_id: 13,
            max_score: 2.0,
            rubric_points: vec![ShortAnswerRubricPointSpec {
                rubric_point_id: 14,
                public_id: "rp-14".into(),
                stable_id: "institution".into(),
                order_index: 0,
                canonical_text: "没有改变封建制度".into(),
                allowed_paraphrases: vec![],
                required_concepts: vec!["制度".into()],
                contradiction_rules: vec![],
                max_score: 2.0,
            }],
        }
    }

    #[test]
    fn provider_sends_only_bounded_teaching_and_answer_evidence() {
        let request = request();
        let payload = json!({
            "state":"ready",
            "suggested_score":2.0,
            "point_results":[{
                "rubric_point_id":14,
                "stable_id":"institution",
                "status":"covered",
                "suggested_score":2.0,
                "evidence_snippets":["没有改变封建制度"],
                "reason":"明确覆盖",
                "confidence":0.98
            }],
            "confidence":0.98,
            "issue_codes":[]
        });
        let envelope = json!({"choices":[{"message":{"content":payload.to_string()}}]});
        let (endpoint, captured) = serve_once(&envelope.to_string());
        let grader = ArkShortAnswerGrader::for_test(endpoint);
        let output = grader.grade(&request).unwrap();
        assert_eq!(output.suggested_score, 2.0);
        let http = captured.recv().unwrap();
        assert!(http.contains("没有改变封建制度"));
        assert!(http.contains("逐字存在"));
        assert!(!http.contains("小林"));
        assert!(!http.contains("八年级一班"));
        assert!(!http.contains("student_name"));
    }

    #[test]
    fn provider_rejects_evidence_not_present_in_student_answer() {
        let request = request();
        let payload = json!({
            "state":"ready",
            "suggested_score":2.0,
            "point_results":[{
                "rubric_point_id":14,
                "stable_id":"institution",
                "status":"covered",
                "suggested_score":2.0,
                "evidence_snippets":["学生没有写出的额外原因"],
                "reason":"伪造证据",
                "confidence":0.98
            }],
            "confidence":0.98,
            "issue_codes":[]
        });
        let envelope = json!({"choices":[{"message":{"content":payload.to_string()}}]});
        let (endpoint, _) = serve_once(&envelope.to_string());
        let error = ArkShortAnswerGrader::for_test(endpoint)
            .grade(&request)
            .unwrap_err();
        assert_eq!(error.code, ShortAnswerGradeErrorCode::InvalidOutput);
    }
}
