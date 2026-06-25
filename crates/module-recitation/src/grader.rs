//! 背诵评分编排：归一化 → 正确率门控（core）+ 熟练度（本模块）。
//! 两套体系并列输出，不合并为综合分。实现 core 的 `Grader`。

use suite_core::domain::accuracy::{self, AccuracyCfg};
use suite_core::domain::normalize::{self, NormalizeCfg};
use suite_core::error::CoreResult;
use suite_core::ports::{GradeResult, Grader, RecognizedWord};

use crate::domain::fluency::{self, FluencyCfg};

pub struct RecitationGradeInput<'a> {
    pub answer_text: &'a str,
    pub asr_text: &'a str,
    pub words: &'a [RecognizedWord],
    pub duration_ms: u64,
    pub normalize_cfg: &'a NormalizeCfg,
    pub accuracy_cfg: &'a AccuracyCfg,
    pub fluency_cfg: &'a FluencyCfg,
}

pub struct RecitationGrader;

impl Grader for RecitationGrader {
    type Input<'a> = RecitationGradeInput<'a>;

    fn grade(&self, input: Self::Input<'_>) -> CoreResult<GradeResult> {
        let ans = normalize::normalize(input.answer_text, input.normalize_cfg);
        let asr = normalize::normalize(input.asr_text, input.normalize_cfg);

        let acc = accuracy::evaluate(&ans, &asr, input.accuracy_cfg);
        let flu = fluency::evaluate(input.asr_text, input.words, input.duration_ms, input.fluency_cfg);

        let metrics = serde_json::json!({
            "coverage": acc.coverage,
            "edit_sim": acc.edit_sim,
            "used_pinyin": acc.used_pinyin,
            "cps": flu.cps,
            "pause_ratio": flu.pause_ratio,
            "filler_count": flu.filler_count,
        });

        let note = format!(
            "正确率 {}%（{}），熟练度 {}={}",
            acc.accuracy as i64,
            if acc.pass { "通过" } else { "未达标→次日补背" },
            flu.fluency as i64,
            flu.quality.as_str()
        );

        Ok(GradeResult {
            primary_score: acc.accuracy,
            pass: acc.pass,
            secondary_score: Some(flu.fluency),
            quality: Some(flu.quality.as_str().to_string()),
            confidence: flu.confidence,
            metrics_json: metrics.to_string(),
            note,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_recite_passes_and_reports_quality() {
        let words: Vec<_> = (0..20)
            .map(|i| RecognizedWord { text: "字".into(), start_ms: i * 250, end_ms: i * 250 + 250 })
            .collect();
        let answer = "字".repeat(20);
        let input = RecitationGradeInput {
            answer_text: &answer,
            asr_text: &answer,
            words: &words,
            duration_ms: 5000,
            normalize_cfg: &NormalizeCfg::default(),
            accuracy_cfg: &AccuracyCfg::default(),
            fluency_cfg: &FluencyCfg::default(),
        };
        let r = RecitationGrader.grade(input).unwrap();
        assert_eq!(r.primary_score, 100.0);
        assert!(r.pass);
        assert_eq!(r.quality.as_deref(), Some("A"));
    }

    #[test]
    fn under_threshold_fails() {
        let input = RecitationGradeInput {
            answer_text: "床前明月光疑是地上霜",
            asr_text: "床前明月光",
            words: &[],
            duration_ms: 3000,
            normalize_cfg: &NormalizeCfg::default(),
            accuracy_cfg: &AccuracyCfg::default(),
            fluency_cfg: &FluencyCfg::default(),
        };
        let r = RecitationGrader.grade(input).unwrap();
        assert!(!r.pass);
    }
}
