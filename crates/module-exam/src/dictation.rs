//! 固定格式默写的安全比较合同。
//!
//! OCR/图像模型只提供转写候选。标准答案只用于比较，不得反向改写原始 OCR；
//! 不是精确答案或老师确认的可接受变体时，只进入复核，不在这里猜测错字或给分。

use serde::{Deserialize, Serialize};
use suite_core::error::{CoreError, CoreResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationRecognitionState {
    Recognized,
    NotWritten,
    Unreadable,
    RecognizeFailed,
    AmbiguousFinal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DictationPointResult {
    NotWritten,
    Unreadable,
    RecognizeFailed,
    Exact,
    AcceptedVariant,
    Partial,
    Missing,
    Contradicted,
    ExtraWrong,
    AmbiguousFinal,
    NeedsReview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictationPointRule {
    pub stable_id: String,
    pub canonical_text: String,
    #[serde(default)]
    pub accepted_variants: Vec<String>,
}

impl DictationPointRule {
    pub fn validate(&self) -> CoreResult<()> {
        if self.stable_id.trim().is_empty() {
            return Err(CoreError::Invalid("默写评分点 stable_id 不能为空".into()));
        }
        let canonical = normalize_for_comparison(&self.canonical_text);
        if canonical.is_empty() {
            return Err(CoreError::Invalid("默写标准内容不能为空".into()));
        }
        if self
            .accepted_variants
            .iter()
            .any(|variant| normalize_for_comparison(variant).is_empty())
        {
            return Err(CoreError::Invalid("默写可接受变体不能为空".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DictationPointComparison {
    pub result: DictationPointResult,
    pub compared_text: Option<String>,
    pub requires_teacher_review: bool,
    pub suggested_score: Option<f64>,
}

/// 只做可复现的精确比较。
///
/// `teacher_corrected_text` 优先于规范化 OCR，但原始 OCR 必须由调用方单独保留。
/// 未命中标准内容或显式可接受变体时返回 `needs_review`，不自动判错或给 0 分。
pub fn compare_dictation_point(
    recognition_state: DictationRecognitionState,
    normalized_ocr_text: Option<&str>,
    teacher_corrected_text: Option<&str>,
    rule: &DictationPointRule,
    max_score: f64,
) -> CoreResult<DictationPointComparison> {
    rule.validate()?;
    if !max_score.is_finite() || max_score <= 0.0 {
        return Err(CoreError::Invalid("默写评分点分值必须大于 0".into()));
    }

    let special = match recognition_state {
        DictationRecognitionState::NotWritten => Some(DictationPointResult::NotWritten),
        DictationRecognitionState::Unreadable => Some(DictationPointResult::Unreadable),
        DictationRecognitionState::RecognizeFailed => Some(DictationPointResult::RecognizeFailed),
        DictationRecognitionState::AmbiguousFinal => Some(DictationPointResult::AmbiguousFinal),
        DictationRecognitionState::Recognized => None,
    };
    if let Some(result) = special {
        return Ok(DictationPointComparison {
            result,
            compared_text: None,
            requires_teacher_review: true,
            suggested_score: None,
        });
    }

    let source = teacher_corrected_text.or(normalized_ocr_text).unwrap_or("");
    let compared = normalize_for_comparison(source);
    if compared.is_empty() {
        return Ok(DictationPointComparison {
            result: DictationPointResult::NeedsReview,
            compared_text: Some(compared),
            requires_teacher_review: true,
            suggested_score: None,
        });
    }
    if compared == normalize_for_comparison(&rule.canonical_text) {
        return Ok(DictationPointComparison {
            result: DictationPointResult::Exact,
            compared_text: Some(compared),
            requires_teacher_review: false,
            suggested_score: Some(max_score),
        });
    }
    if rule
        .accepted_variants
        .iter()
        .any(|variant| normalize_for_comparison(variant) == compared)
    {
        return Ok(DictationPointComparison {
            result: DictationPointResult::AcceptedVariant,
            compared_text: Some(compared),
            requires_teacher_review: false,
            suggested_score: Some(max_score),
        });
    }

    Ok(DictationPointComparison {
        result: DictationPointResult::NeedsReview,
        compared_text: Some(compared),
        requires_teacher_review: true,
        suggested_score: None,
    })
}

fn normalize_for_comparison(value: &str) -> String {
    value.split_whitespace().collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule() -> DictationPointRule {
        DictationPointRule {
            stable_id: "treaty-year".into(),
            canonical_text: "1842年".into(),
            accepted_variants: vec!["一八四二年".into(), "1842 年".into()],
        }
    }

    #[test]
    fn exact_and_teacher_confirmed_variants_are_reproducible() {
        let exact = compare_dictation_point(
            DictationRecognitionState::Recognized,
            Some("1842 年"),
            None,
            &rule(),
            2.0,
        )
        .unwrap();
        assert_eq!(exact.result, DictationPointResult::Exact);
        assert_eq!(exact.suggested_score, Some(2.0));

        let variant = compare_dictation_point(
            DictationRecognitionState::Recognized,
            Some("一八四二年"),
            None,
            &rule(),
            2.0,
        )
        .unwrap();
        assert_eq!(variant.result, DictationPointResult::AcceptedVariant);
        assert!(!variant.requires_teacher_review);
    }

    #[test]
    fn standard_answer_never_repairs_an_unmatched_ocr_result() {
        let result = compare_dictation_point(
            DictationRecognitionState::Recognized,
            Some("1840年"),
            None,
            &rule(),
            2.0,
        )
        .unwrap();
        assert_eq!(result.result, DictationPointResult::NeedsReview);
        assert_eq!(result.compared_text.as_deref(), Some("1840年"));
        assert_eq!(result.suggested_score, None);
    }

    #[test]
    fn teacher_correction_is_compared_without_overwriting_raw_ocr() {
        let result = compare_dictation_point(
            DictationRecognitionState::Recognized,
            Some("1840年"),
            Some("1842年"),
            &rule(),
            2.0,
        )
        .unwrap();
        assert_eq!(result.result, DictationPointResult::Exact);
        assert_eq!(result.compared_text.as_deref(), Some("1842年"));
    }

    #[test]
    fn absence_and_recognition_failures_never_receive_machine_scores() {
        for state in [
            DictationRecognitionState::NotWritten,
            DictationRecognitionState::Unreadable,
            DictationRecognitionState::RecognizeFailed,
            DictationRecognitionState::AmbiguousFinal,
        ] {
            let result = compare_dictation_point(state, None, None, &rule(), 2.0).unwrap();
            assert!(result.requires_teacher_review);
            assert_eq!(result.suggested_score, None);
        }
    }
}
