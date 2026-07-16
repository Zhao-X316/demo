//! 篇幅受控简答题的 provider-neutral 逐评分点评分合同。
//!
//! 模型只能根据当前题目、学生转写、参考答案与已冻结评分点生成建议。每个得分点必须
//! 引用学生答案中的逐字证据；本合同不写老师分数、不发布成绩，也不允许批量确认。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};

pub const SHORT_ANSWER_GRADE_SCHEMA_VERSION: i64 = 1;

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("简答题{field}不能为空")))
    } else {
        Ok(())
    }
}

fn unit(value: f64, field: &str) -> CoreResult<()> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!("简答题{field}必须位于 0~1")))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortAnswerGraderDescriptor {
    pub provider: String,
    pub model_name: String,
    pub model_version: String,
    pub config_version: String,
    pub rule_version: String,
}

impl ShortAnswerGraderDescriptor {
    pub fn validate(&self) -> CoreResult<()> {
        for (value, field) in [
            (&self.provider, "provider"),
            (&self.model_name, "model_name"),
            (&self.model_version, "model_version"),
            (&self.config_version, "config_version"),
            (&self.rule_version, "rule_version"),
        ] {
            required(value, field)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortAnswerRubricPointSpec {
    pub rubric_point_id: i64,
    pub public_id: String,
    pub stable_id: String,
    pub order_index: i64,
    pub canonical_text: String,
    pub allowed_paraphrases: Vec<String>,
    pub required_concepts: Vec<String>,
    pub contradiction_rules: Vec<Value>,
    pub max_score: f64,
}

impl ShortAnswerRubricPointSpec {
    fn validate(&self) -> CoreResult<()> {
        if self.rubric_point_id <= 0 || self.order_index < 0 {
            return Err(CoreError::Invalid("简答题评分点 id 或顺序非法".into()));
        }
        required(&self.public_id, "评分点 public_id")?;
        required(&self.stable_id, "评分点 stable_id")?;
        required(&self.canonical_text, "评分点标准表述")?;
        if !self.max_score.is_finite() || self.max_score <= 0.0 {
            return Err(CoreError::Invalid("简答题评分点分值必须为正数".into()));
        }
        if self
            .contradiction_rules
            .iter()
            .any(|rule| !rule.is_object())
        {
            return Err(CoreError::Invalid("简答题矛盾规则必须是对象".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortAnswerGradeRequest {
    pub transcription_revision_id: i64,
    pub attempt_id: i64,
    pub assessment_item_id: i64,
    pub answer_region_revision_id: i64,
    pub crop_artifact_id: i64,
    pub question_stem: String,
    pub student_answer: String,
    pub reference_answer: String,
    pub answer_key_version_id: i64,
    pub rubric_version_id: i64,
    pub max_score: f64,
    pub rubric_points: Vec<ShortAnswerRubricPointSpec>,
}

impl ShortAnswerGradeRequest {
    pub fn validate(&self) -> CoreResult<()> {
        if self.transcription_revision_id <= 0
            || self.attempt_id <= 0
            || self.assessment_item_id <= 0
            || self.answer_region_revision_id <= 0
            || self.crop_artifact_id <= 0
            || self.answer_key_version_id <= 0
            || self.rubric_version_id <= 0
        {
            return Err(CoreError::Invalid("简答题评分作用域 id 必须为正数".into()));
        }
        required(&self.question_stem, "题干")?;
        required(&self.student_answer, "学生答案")?;
        required(&self.reference_answer, "参考答案")?;
        if !self.max_score.is_finite() || self.max_score <= 0.0 {
            return Err(CoreError::Invalid("简答题总分必须为正数".into()));
        }
        if self.rubric_points.is_empty() {
            return Err(CoreError::Invalid("简答题至少需要一个评分点".into()));
        }
        let mut ids = BTreeSet::new();
        let mut stable_ids = BTreeSet::new();
        let mut orders = BTreeSet::new();
        let mut point_total = 0.0;
        for point in &self.rubric_points {
            point.validate()?;
            if !ids.insert(point.rubric_point_id)
                || !stable_ids.insert(point.stable_id.trim().to_string())
                || !orders.insert(point.order_index)
            {
                return Err(CoreError::Invalid("简答题评分点 id 或顺序重复".into()));
            }
            point_total += point.max_score;
        }
        if (point_total - self.max_score).abs() > 0.000_001 {
            return Err(CoreError::Invalid(
                "简答题评分点分值之和必须等于题目总分".into(),
            ));
        }
        Ok(())
    }

    pub fn input_hash(&self) -> CoreResult<String> {
        self.validate()?;
        let value = serde_json::json!({
            "schema_version": SHORT_ANSWER_GRADE_SCHEMA_VERSION,
            "transcription_revision_id": self.transcription_revision_id,
            "attempt_id": self.attempt_id,
            "assessment_item_id": self.assessment_item_id,
            "answer_region_revision_id": self.answer_region_revision_id,
            "crop_artifact_id": self.crop_artifact_id,
            "question_stem": self.question_stem,
            "student_answer": self.student_answer,
            "reference_answer": self.reference_answer,
            "answer_key_version_id": self.answer_key_version_id,
            "rubric_version_id": self.rubric_version_id,
            "max_score": self.max_score,
            "rubric_points": self.rubric_points,
        });
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| CoreError::Parse(format!("简答题评分输入序列化失败：{error}")))?;
        Ok(hashing::sha256_hex(&bytes))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortAnswerPointStatus {
    Covered,
    Partial,
    Missing,
    Contradicted,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortAnswerPointResult {
    pub rubric_point_id: i64,
    pub stable_id: String,
    pub status: ShortAnswerPointStatus,
    pub suggested_score: f64,
    #[serde(default)]
    pub evidence_snippets: Vec<String>,
    pub reason: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortAnswerGradeState {
    Ready,
    NeedsReview,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortAnswerGradeOutput {
    pub schema_version: i64,
    pub transcription_revision_id: i64,
    pub assessment_item_id: i64,
    pub answer_key_version_id: i64,
    pub rubric_version_id: i64,
    pub input_hash: String,
    pub descriptor: ShortAnswerGraderDescriptor,
    pub state: ShortAnswerGradeState,
    pub suggested_score: f64,
    pub point_results: Vec<ShortAnswerPointResult>,
    pub confidence: f64,
    #[serde(default)]
    pub issue_codes: Vec<String>,
}

impl ShortAnswerGradeOutput {
    pub fn validate_against(&self, request: &ShortAnswerGradeRequest) -> CoreResult<()> {
        request.validate()?;
        self.descriptor.validate()?;
        if self.schema_version != SHORT_ANSWER_GRADE_SCHEMA_VERSION
            || self.transcription_revision_id != request.transcription_revision_id
            || self.assessment_item_id != request.assessment_item_id
            || self.answer_key_version_id != request.answer_key_version_id
            || self.rubric_version_id != request.rubric_version_id
            || self.input_hash != request.input_hash()?
        {
            return Err(CoreError::Invalid(
                "简答题评分输出与当前输入或版本不一致".into(),
            ));
        }
        if !self.suggested_score.is_finite()
            || self.suggested_score < 0.0
            || self.suggested_score > request.max_score
        {
            return Err(CoreError::Invalid("简答题建议总分越界".into()));
        }
        unit(self.confidence, "总置信度")?;

        let expected = request
            .rubric_points
            .iter()
            .map(|point| (point.rubric_point_id, point))
            .collect::<BTreeMap<_, _>>();
        let mut actual = BTreeSet::new();
        let mut total = 0.0;
        let mut forces_review = false;
        for result in &self.point_results {
            let point = expected.get(&result.rubric_point_id).ok_or_else(|| {
                CoreError::Invalid("简答题评分输出包含当前 rubric 以外的评分点".into())
            })?;
            if !actual.insert(result.rubric_point_id)
                || result.stable_id.trim() != point.stable_id.trim()
            {
                return Err(CoreError::Invalid(
                    "简答题评分点重复或 stable_id 不一致".into(),
                ));
            }
            required(&result.reason, "评分点说明")?;
            unit(result.confidence, "评分点置信度")?;
            if !result.suggested_score.is_finite()
                || result.suggested_score < 0.0
                || result.suggested_score > point.max_score
            {
                return Err(CoreError::Invalid("简答题评分点建议分越界".into()));
            }

            for snippet in &result.evidence_snippets {
                let snippet = snippet.trim();
                if snippet.is_empty() || !request.student_answer.contains(snippet) {
                    return Err(CoreError::Invalid(
                        "简答题得分证据必须逐字存在于学生答案中".into(),
                    ));
                }
            }
            match result.status {
                ShortAnswerPointStatus::Covered | ShortAnswerPointStatus::Partial => {
                    if result.evidence_snippets.is_empty() || result.suggested_score <= 0.0 {
                        return Err(CoreError::Invalid(
                            "已覆盖或部分覆盖的评分点必须有原文证据和正分".into(),
                        ));
                    }
                }
                ShortAnswerPointStatus::Missing => {
                    if !result.evidence_snippets.is_empty() || result.suggested_score != 0.0 {
                        return Err(CoreError::Invalid("遗漏评分点不能带证据或得分".into()));
                    }
                }
                ShortAnswerPointStatus::Contradicted => {
                    if result.evidence_snippets.is_empty() || result.suggested_score != 0.0 {
                        return Err(CoreError::Invalid(
                            "矛盾评分点必须有原文证据且不得得分".into(),
                        ));
                    }
                    forces_review = true;
                }
                ShortAnswerPointStatus::Uncertain => {
                    if result.suggested_score != 0.0 {
                        return Err(CoreError::Invalid("不确定评分点不得直接建议得分".into()));
                    }
                    forces_review = true;
                }
            }
            total += result.suggested_score;
        }
        if actual != expected.keys().copied().collect::<BTreeSet<_>>() {
            return Err(CoreError::Invalid(
                "简答题评分必须逐一返回当前 rubric 的全部评分点".into(),
            ));
        }
        if (total - self.suggested_score).abs() > 0.000_001 {
            return Err(CoreError::Invalid(
                "简答题建议总分必须等于逐点评分之和".into(),
            ));
        }
        if forces_review && self.state != ShortAnswerGradeState::NeedsReview {
            return Err(CoreError::Invalid(
                "存在矛盾或不确定评分点时必须进入老师复核".into(),
            ));
        }
        if self.state == ShortAnswerGradeState::Ready && !self.issue_codes.is_empty() {
            return Err(CoreError::Invalid("ready 简答题评分不能携带问题码".into()));
        }
        if self.state == ShortAnswerGradeState::NeedsReview && self.issue_codes.is_empty() {
            return Err(CoreError::Invalid("待复核简答题评分必须说明问题码".into()));
        }
        Ok(())
    }

    pub fn to_json_against(&self, request: &ShortAnswerGradeRequest) -> CoreResult<String> {
        self.validate_against(request)?;
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("简答题评分输出序列化失败：{error}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortAnswerGradeErrorCode {
    ProviderUnavailable,
    RateLimited,
    Timeout,
    InvalidOutput,
    ScopeChanged,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShortAnswerGradeFailure {
    pub schema_version: i64,
    pub code: ShortAnswerGradeErrorCode,
    pub safe_message: String,
    pub retryable: bool,
}

impl ShortAnswerGradeFailure {
    pub fn to_json(&self) -> CoreResult<String> {
        serde_json::to_string(self)
            .map_err(|error| CoreError::Parse(format!("简答题评分失败信息序列化失败：{error}")))
    }
}

pub trait ShortAnswerGrader {
    fn descriptor(&self) -> ShortAnswerGraderDescriptor;

    fn grade(
        &self,
        request: &ShortAnswerGradeRequest,
    ) -> Result<ShortAnswerGradeOutput, ShortAnswerGradeFailure>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ShortAnswerGradeRequest {
        ShortAnswerGradeRequest {
            transcription_revision_id: 11,
            attempt_id: 12,
            assessment_item_id: 13,
            answer_region_revision_id: 14,
            crop_artifact_id: 17,
            question_stem: "概括洋务运动失败的原因。".into(),
            student_answer: "只学习西方技术，没有改变封建制度；官员管理腐败。".into(),
            reference_answer: "只学习技术而不改变制度，内部管理腐败。".into(),
            answer_key_version_id: 15,
            rubric_version_id: 16,
            max_score: 4.0,
            rubric_points: vec![
                ShortAnswerRubricPointSpec {
                    rubric_point_id: 1,
                    public_id: "rp-1".into(),
                    stable_id: "institutional-limit".into(),
                    order_index: 0,
                    canonical_text: "只学习西方技术，没有改变封建制度".into(),
                    allowed_paraphrases: vec!["只学技术不改制度".into()],
                    required_concepts: vec!["技术".into(), "制度".into()],
                    contradiction_rules: vec![],
                    max_score: 2.0,
                },
                ShortAnswerRubricPointSpec {
                    rubric_point_id: 2,
                    public_id: "rp-2".into(),
                    stable_id: "corruption".into(),
                    order_index: 1,
                    canonical_text: "洋务派内部管理腐败".into(),
                    allowed_paraphrases: vec![],
                    required_concepts: vec!["腐败".into()],
                    contradiction_rules: vec![],
                    max_score: 2.0,
                },
            ],
        }
    }

    fn descriptor() -> ShortAnswerGraderDescriptor {
        ShortAnswerGraderDescriptor {
            provider: "test".into(),
            model_name: "grader".into(),
            model_version: "v1".into(),
            config_version: "cfg-1".into(),
            rule_version: "rule-1".into(),
        }
    }

    fn valid_output(request: &ShortAnswerGradeRequest) -> ShortAnswerGradeOutput {
        ShortAnswerGradeOutput {
            schema_version: SHORT_ANSWER_GRADE_SCHEMA_VERSION,
            transcription_revision_id: request.transcription_revision_id,
            assessment_item_id: request.assessment_item_id,
            answer_key_version_id: request.answer_key_version_id,
            rubric_version_id: request.rubric_version_id,
            input_hash: request.input_hash().unwrap(),
            descriptor: descriptor(),
            state: ShortAnswerGradeState::Ready,
            suggested_score: 3.0,
            point_results: vec![
                ShortAnswerPointResult {
                    rubric_point_id: 1,
                    stable_id: "institutional-limit".into(),
                    status: ShortAnswerPointStatus::Covered,
                    suggested_score: 2.0,
                    evidence_snippets: vec!["只学习西方技术，没有改变封建制度".into()],
                    reason: "完整覆盖制度局限".into(),
                    confidence: 0.98,
                },
                ShortAnswerPointResult {
                    rubric_point_id: 2,
                    stable_id: "corruption".into(),
                    status: ShortAnswerPointStatus::Partial,
                    suggested_score: 1.0,
                    evidence_snippets: vec!["官员管理腐败".into()],
                    reason: "表达较简略".into(),
                    confidence: 0.86,
                },
            ],
            confidence: 0.91,
            issue_codes: vec![],
        }
    }

    #[test]
    fn accepts_point_level_scores_with_literal_student_evidence() {
        let request = request();
        valid_output(&request).validate_against(&request).unwrap();
    }

    #[test]
    fn rejects_hallucinated_evidence_snippet() {
        let request = request();
        let mut output = valid_output(&request);
        output.point_results[0].evidence_snippets = vec!["学生没有写过这句话".into()];
        assert!(output.validate_against(&request).is_err());
    }

    #[test]
    fn rejects_missing_rubric_point() {
        let request = request();
        let mut output = valid_output(&request);
        output.point_results.pop();
        output.suggested_score = 2.0;
        assert!(output.validate_against(&request).is_err());
    }

    #[test]
    fn contradiction_requires_literal_evidence_zero_score_and_review() {
        let request = request();
        let mut output = valid_output(&request);
        output.point_results[1].status = ShortAnswerPointStatus::Contradicted;
        output.point_results[1].suggested_score = 0.0;
        output.suggested_score = 2.0;
        assert!(output.validate_against(&request).is_err());

        output.state = ShortAnswerGradeState::NeedsReview;
        output.issue_codes = vec!["FACT_CONTRADICTION".into()];
        output.validate_against(&request).unwrap();
    }

    #[test]
    fn uncertain_point_cannot_receive_positive_score() {
        let request = request();
        let mut output = valid_output(&request);
        output.point_results[1].status = ShortAnswerPointStatus::Uncertain;
        output.point_results[1].suggested_score = 1.0;
        output.state = ShortAnswerGradeState::NeedsReview;
        output.issue_codes = vec!["UNCERTAIN_POINT".into()];
        assert!(output.validate_against(&request).is_err());
    }
}
