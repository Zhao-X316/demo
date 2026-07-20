//! 豆包视觉大模型(方舟/Ark) 题目预分析的**提示词与解析**（纯逻辑，可测）。
//!
//! 网络调用在应用外壳（src-tauri，持有 reqwest + Ark API Key）完成；这里只负责
//! 生成提示词、把模型返回的 JSON 解析成结构。知识点为"建议名"，由教师在知识点树里认领/新建。

use serde::{Deserialize, Serialize};

use suite_core::error::{CoreError, CoreResult};

/// 题目预分析的系统/用户提示词：要求模型只输出严格 JSON。
pub const QUESTION_ANALYSIS_PROMPT: &str = r#"你是中小学试卷题目分析助手。请识别图片中的"单道题目"并输出严格 JSON（不要任何额外文字、不要 markdown 代码块）。字段：
{
  "qtype": "single|multi|judge|fill|subjective",   // 单选|多选|判断|填空|主观
  "stem": "题干完整文本",
  "correct_answer": "客观题正确答案，如 B 或 AC；判断填 对/错；填空填标准答案；主观题留空字符串",
  "knowledge_point": "本题主要考查的知识点名称（简短）",
  "analysis": "整体解析",
  "options": [                                       // 仅选择题/判断题给出；填空/主观题给空数组
    {
      "label": "A",
      "content": "选项文本",
      "is_correct": true,
      "knowledge_point": "该选项考查或迷惑的知识点名称",
      "analysis": "选这一项说明哪个知识点没掌握；正确项说明为什么对"
    }
  ]
}
要求：所有字段都要给；无法判断的文本字段给空字符串，is_correct 无法判断给 false。只输出这个 JSON 对象。"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalyzedOption {
    pub label: String,
    pub content: String,
    #[serde(default)]
    pub is_correct: bool,
    #[serde(default)]
    pub knowledge_point: Option<String>,
    #[serde(default)]
    pub analysis: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalyzedQuestion {
    pub qtype: String,
    pub stem: String,
    #[serde(default)]
    pub correct_answer: Option<String>,
    #[serde(default)]
    pub knowledge_point: Option<String>,
    #[serde(default)]
    pub analysis: Option<String>,
    #[serde(default)]
    pub options: Vec<AnalyzedOption>,
}

/// 去掉模型偶尔包裹的 ```json ... ``` 围栏，并截取首个 `{` 到末个 `}`。
fn extract_json(s: &str) -> &str {
    let t = s.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t).trim();
    match (t.find('{'), t.rfind('}')) {
        (Some(a), Some(b)) if b >= a => &t[a..=b],
        _ => t,
    }
}

/// 解析模型返回内容为题目结构。
pub fn parse_question_analysis(content: &str) -> CoreResult<AnalyzedQuestion> {
    let json = extract_json(content);
    let mut q: AnalyzedQuestion = serde_json::from_str(json).map_err(|e| {
        CoreError::Recognize(format!(
            "VLM 返回无法解析为题目 JSON: {e}; 原文片段: {}",
            truncate(json, 300)
        ))
    })?;
    // 规范化空字符串 → None
    q.correct_answer = q.correct_answer.filter(|s| !s.trim().is_empty());
    q.knowledge_point = q.knowledge_point.filter(|s| !s.trim().is_empty());
    q.analysis = q.analysis.filter(|s| !s.trim().is_empty());
    for o in &mut q.options {
        o.knowledge_point = o.knowledge_point.take().filter(|s| !s.trim().is_empty());
        o.analysis = o.analysis.take().filter(|s| !s.trim().is_empty());
    }
    if q.stem.trim().is_empty() && q.options.is_empty() {
        return Err(CoreError::Recognize(
            "VLM 返回的题目为空（无题干无选项）".into(),
        ));
    }
    Ok(q)
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_json() {
        let raw = r#"{"qtype":"single","stem":"碰撞中正确的是","correct_answer":"B","knowledge_point":"动量守恒","analysis":"内力不改变总动量","options":[
            {"label":"A","content":"动能守恒","is_correct":false,"knowledge_point":"弹性碰撞","analysis":"混淆弹性与非弹性"},
            {"label":"B","content":"动量守恒","is_correct":true,"knowledge_point":"动量守恒","analysis":"正确"}
        ]}"#;
        let q = parse_question_analysis(raw).unwrap();
        assert_eq!(q.qtype, "single");
        assert_eq!(q.correct_answer.as_deref(), Some("B"));
        assert_eq!(q.options.len(), 2);
        assert!(q.options[1].is_correct);
        assert_eq!(q.options[0].knowledge_point.as_deref(), Some("弹性碰撞"));
    }

    #[test]
    fn parse_fenced_and_empty_fields() {
        let raw = "```json\n{\"qtype\":\"fill\",\"stem\":\"水的化学式是____\",\"correct_answer\":\"H2O\",\"knowledge_point\":\"\",\"analysis\":\"\",\"options\":[]}\n```";
        let q = parse_question_analysis(raw).unwrap();
        assert_eq!(q.qtype, "fill");
        assert_eq!(q.correct_answer.as_deref(), Some("H2O"));
        assert_eq!(q.knowledge_point, None); // 空串归一为 None
        assert!(q.options.is_empty());
    }

    #[test]
    fn reject_garbage() {
        assert!(parse_question_analysis("抱歉我看不清图片").is_err());
    }
}
