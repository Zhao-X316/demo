//! 独立空白卷 / 电子题目文件的 provider-neutral 提取契约。
//!
//! 模型只能输出来源定位明确的题目草稿；隐私检查失败时禁止返回题干。
//! 这份契约不包含答案，不会把模型输出直接升级成可批改题目。

use serde::{Deserialize, Serialize};
use suite_core::error::{CoreError, CoreResult};

pub const SOURCE_EXTRACTION_SCHEMA_VERSION: i64 = 1;
pub const SOURCE_EXTRACTION_VERSION: &str = "k1-source-extraction-v1";
pub const MAX_VISUAL_PAGES: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceVisualPage {
    pub page_no: i64,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceExtractionInput {
    pub schema_version: i64,
    pub source_document_public_id: String,
    pub source_type: String,
    pub source_format: String,
    pub source_hash: String,
    pub page_count: i64,
    pub extracted_text: Option<String>,
    pub visual_pages: Vec<SourceVisualPage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceOptionDraft {
    pub label: String,
    pub content: String,
    pub order_index: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceAnchorDraft {
    pub schema_version: i64,
    pub page_no: i64,
    pub region: Option<[f64; 4]>,
    pub line_start: Option<i64>,
    pub line_end: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceQuestionDraft {
    pub order_index: i64,
    pub question_no: Option<String>,
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub max_score: f64,
    pub options: Vec<SourceOptionDraft>,
    pub source_anchor: SourceAnchorDraft,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePrivacyResult {
    pub schema_version: i64,
    pub sanitized: bool,
    pub contains_student_identity: bool,
    pub contains_student_answer: bool,
    pub contains_teacher_mark: bool,
    pub contains_score: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceExtractionOutput {
    pub schema_version: i64,
    pub state: String,
    pub confidence: f64,
    pub privacy: SourcePrivacyResult,
    pub issue_codes: Vec<String>,
    pub drafts: Vec<SourceQuestionDraft>,
}

pub trait SourceQuestionRecognizer {
    fn provider(&self) -> &str;
    fn model_name(&self) -> &str;
    fn model_version(&self) -> &str;
    fn config_version(&self) -> &str;
    fn prompt_or_rule_version(&self) -> &str;
    fn recognize(&self, input: &SourceExtractionInput) -> CoreResult<SourceExtractionOutput>;
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn validate_hash(value: &str, label: &str) -> CoreResult<()> {
    let value = value.trim();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(format!(
            "{label}必须是 64 位十六进制 hash"
        )));
    }
    Ok(())
}

pub fn validate_input(input: &SourceExtractionInput) -> CoreResult<()> {
    if input.schema_version != SOURCE_EXTRACTION_SCHEMA_VERSION {
        return Err(CoreError::Invalid("题目来源提取输入 schema 不兼容".into()));
    }
    required(&input.source_document_public_id, "题目来源文档标识")?;
    if !matches!(
        input.source_type.as_str(),
        "blank_paper" | "source_document"
    ) {
        return Err(CoreError::Invalid("题目来源类型非法".into()));
    }
    if !matches!(
        input.source_format.as_str(),
        "jpeg" | "pdf" | "text" | "docx" | "xlsx"
    ) {
        return Err(CoreError::Invalid("题目来源格式非法".into()));
    }
    validate_hash(&input.source_hash, "题目来源 hash")?;
    if !(1..=200).contains(&input.page_count) {
        return Err(CoreError::Invalid(
            "题目来源页数必须在 1 到 200 之间".into(),
        ));
    }
    if input.visual_pages.len() > MAX_VISUAL_PAGES {
        return Err(CoreError::Invalid(format!(
            "单次视觉提取最多支持 {MAX_VISUAL_PAGES} 页"
        )));
    }
    if input
        .extracted_text
        .as_deref()
        .map_or(true, |text| text.trim().is_empty())
        && input.visual_pages.is_empty()
    {
        return Err(CoreError::Invalid("题目来源没有可提取的文本或页面".into()));
    }
    let mut page_numbers = std::collections::HashSet::new();
    for page in &input.visual_pages {
        if !(1..=input.page_count).contains(&page.page_no) {
            return Err(CoreError::Invalid("视觉页码超出来源文档范围".into()));
        }
        if !matches!(page.mime_type.as_str(), "image/jpeg" | "image/png") {
            return Err(CoreError::Invalid("视觉页只支持 JPEG 或 PNG".into()));
        }
        if page.bytes.is_empty() {
            return Err(CoreError::Invalid("视觉页内容不能为空".into()));
        }
        if !page_numbers.insert(page.page_no) {
            return Err(CoreError::Invalid("视觉页码不能重复".into()));
        }
    }
    Ok(())
}

fn privacy_passed(privacy: &SourcePrivacyResult) -> bool {
    privacy.schema_version == SOURCE_EXTRACTION_SCHEMA_VERSION
        && privacy.sanitized
        && !privacy.contains_student_identity
        && !privacy.contains_student_answer
        && !privacy.contains_teacher_mark
        && !privacy.contains_score
}

pub fn validate_output(
    input: &SourceExtractionInput,
    output: &SourceExtractionOutput,
) -> CoreResult<()> {
    validate_input(input)?;
    if output.schema_version != SOURCE_EXTRACTION_SCHEMA_VERSION {
        return Err(CoreError::Invalid("题目来源提取输出 schema 不兼容".into()));
    }
    if !matches!(output.state.as_str(), "ready" | "needs_review" | "blocked") {
        return Err(CoreError::Invalid("题目来源提取状态非法".into()));
    }
    if !output.confidence.is_finite() || !(0.0..=1.0).contains(&output.confidence) {
        return Err(CoreError::Invalid("题目来源总体置信度非法".into()));
    }
    if output.issue_codes.iter().any(|code| code.trim().is_empty()) {
        return Err(CoreError::Invalid("题目来源问题码不能为空".into()));
    }
    let privacy_ok = privacy_passed(&output.privacy);
    if !privacy_ok {
        if output.state != "blocked" || !output.drafts.is_empty() {
            return Err(CoreError::Invalid(
                "隐私检查未通过时必须阻断且不得返回题干".into(),
            ));
        }
        return Ok(());
    }
    if output.state == "blocked" && !output.drafts.is_empty() {
        return Err(CoreError::Invalid("阻断结果不得携带题目草稿".into()));
    }
    if output.state != "blocked" && output.drafts.is_empty() {
        return Err(CoreError::Invalid("非阻断结果至少需要一道题目草稿".into()));
    }
    let mut orders = std::collections::HashSet::new();
    for draft in &output.drafts {
        if draft.order_index <= 0 || !orders.insert(draft.order_index) {
            return Err(CoreError::Invalid("题目顺序必须为不重复的正整数".into()));
        }
        if !matches!(
            draft.question_type.as_str(),
            "single" | "multiple" | "true_false" | "fill_blank" | "short_answer"
        ) {
            return Err(CoreError::Invalid("题目草稿题型非法".into()));
        }
        required(&draft.stem, "题目草稿题干")?;
        if !draft.max_score.is_finite() || draft.max_score <= 0.0 {
            return Err(CoreError::Invalid("题目草稿分值必须大于 0".into()));
        }
        if !draft.confidence.is_finite() || !(0.0..=1.0).contains(&draft.confidence) {
            return Err(CoreError::Invalid("题目草稿置信度非法".into()));
        }
        if draft.source_anchor.schema_version != SOURCE_EXTRACTION_SCHEMA_VERSION
            || !(1..=input.page_count).contains(&draft.source_anchor.page_no)
        {
            return Err(CoreError::Invalid("题目来源锚点非法".into()));
        }
        if let Some(region) = draft.source_anchor.region {
            if region
                .iter()
                .any(|coordinate| !coordinate.is_finite() || !(0.0..=1.0).contains(coordinate))
                || region[2] <= region[0]
                || region[3] <= region[1]
            {
                return Err(CoreError::Invalid("题目来源区域坐标非法".into()));
            }
        }
        match (draft.source_anchor.line_start, draft.source_anchor.line_end) {
            (Some(start), Some(end)) if start > 0 && end >= start => {}
            (None, None) => {}
            _ => return Err(CoreError::Invalid("题目来源文本行锚点非法".into())),
        }
        if matches!(draft.question_type.as_str(), "single" | "multiple") {
            if draft.options.len() < 2 {
                return Err(CoreError::Invalid("选择题至少需要两个选项".into()));
            }
            let mut labels = std::collections::HashSet::new();
            for option in &draft.options {
                required(&option.label, "选项标签")?;
                required(&option.content, "选项内容")?;
                if option.order_index <= 0
                    || !labels.insert(option.label.trim().to_ascii_uppercase())
                {
                    return Err(CoreError::Invalid("选择题选项标签或顺序非法".into()));
                }
            }
        } else if !draft.options.is_empty() {
            return Err(CoreError::Invalid("非选择题不得携带选项".into()));
        }
    }
    if output.state == "ready"
        && (output.confidence < 0.95
            || output.drafts.iter().any(|draft| draft.confidence < 0.95)
            || !output.issue_codes.is_empty())
    {
        return Err(CoreError::Invalid(
            "存在低置信度或问题码时必须进入 needs_review".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> SourceExtractionInput {
        SourceExtractionInput {
            schema_version: 1,
            source_document_public_id: "source-1".into(),
            source_type: "source_document".into(),
            source_format: "text".into(),
            source_hash: "a".repeat(64),
            page_count: 1,
            extracted_text: Some("1. 洋务运动前期的口号是什么？".into()),
            visual_pages: Vec::new(),
        }
    }

    fn output() -> SourceExtractionOutput {
        SourceExtractionOutput {
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
            drafts: vec![SourceQuestionDraft {
                order_index: 1,
                question_no: Some("1".into()),
                question_type: "short_answer".into(),
                stem: "洋务运动前期的口号是什么？".into(),
                material_text: None,
                max_score: 2.0,
                options: Vec::new(),
                source_anchor: SourceAnchorDraft {
                    schema_version: 1,
                    page_no: 1,
                    region: None,
                    line_start: Some(1),
                    line_end: Some(1),
                },
                confidence: 0.99,
            }],
        }
    }

    #[test]
    fn accepts_clean_traceable_source_output() {
        validate_output(&input(), &output()).unwrap();
    }

    #[test]
    fn privacy_failure_cannot_leak_question_content() {
        let mut output = output();
        output.state = "blocked".into();
        output.privacy.contains_student_answer = true;
        let error = validate_output(&input(), &output).unwrap_err().to_string();
        assert!(error.contains("不得返回题干"));
    }

    #[test]
    fn low_confidence_cannot_claim_ready() {
        let mut output = output();
        output.drafts[0].confidence = 0.7;
        let error = validate_output(&input(), &output).unwrap_err().to_string();
        assert!(error.contains("needs_review"));
    }
}
