//! 熟练度（记忆质量）：语速 / 停顿率 / 语气词 → 0~100 → 分级 A/B/C。
//! 仅在正确率通过后用于梯度复习排程。依赖 ASR 词级时间戳；无时间戳则降级。

use suite_core::domain::scheduler::ReviewQuality;
use suite_core::ports::RecognizedWord;

/// 记忆质量分级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    A, // 优
    B, // 良
    C, // 中
}

impl Quality {
    pub fn as_str(&self) -> &'static str {
        match self {
            Quality::A => "A",
            Quality::B => "B",
            Quality::C => "C",
        }
    }
    /// 映射到通用复习质量，供 core 调度器使用。
    pub fn to_review(self) -> ReviewQuality {
        match self {
            Quality::A => ReviewQuality::Good,
            Quality::B => ReviewQuality::Ok,
            Quality::C => ReviewQuality::Hard,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FluencyCfg {
    pub fillers: Vec<String>,
    pub ideal_cps: f64, // 理想语速（字/秒）
    pub a_min: f64,     // A 档下限
    pub b_min: f64,     // B 档下限
}

impl Default for FluencyCfg {
    fn default() -> Self {
        Self {
            fillers: ["嗯", "啊", "呃", "那个", "这个"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            ideal_cps: 4.0,
            a_min: 85.0,
            b_min: 70.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FluencyResult {
    pub fluency: f64,
    pub quality: Quality,
    pub cps: f64,
    pub pause_ratio: f64,
    pub filler_count: u32,
    pub confidence: f64,
}

pub fn evaluate(
    text: &str,
    words: &[RecognizedWord],
    duration_ms: u64,
    cfg: &FluencyCfg,
) -> FluencyResult {
    let dur_s = (duration_ms as f64 / 1000.0).max(0.001);
    let n_chars = text.chars().filter(|c| !c.is_whitespace()).count() as f64;
    let cps = n_chars / dur_s;

    // 停顿率：相邻词间隙累加 / 总时长
    let mut gap_ms = 0u64;
    for w in words.windows(2) {
        if w[1].start_ms > w[0].end_ms {
            gap_ms += w[1].start_ms - w[0].end_ms;
        }
    }
    let pause_ratio = (gap_ms as f64 / duration_ms.max(1) as f64).min(1.0);

    // 语气词计数
    let filler_count: u32 = cfg
        .fillers
        .iter()
        .map(|f| text.matches(f.as_str()).count() as u32)
        .sum();

    // 惩罚项
    let speed_pen = ((cfg.ideal_cps - cps).abs() / cfg.ideal_cps).min(1.0) * 30.0;
    let pause_pen = pause_ratio * 40.0;
    let filler_pen = (filler_count as f64 * 5.0).min(30.0);
    let fluency = (100.0 - speed_pen - pause_pen - filler_pen)
        .max(0.0)
        .round();

    let quality = if fluency >= cfg.a_min {
        Quality::A
    } else if fluency >= cfg.b_min {
        Quality::B
    } else {
        Quality::C
    };

    // 无词级时间戳时，停顿不可信 → 降低置信度
    let confidence = if words.is_empty() { 0.4 } else { 0.9 };

    FluencyResult {
        fluency,
        quality,
        cps,
        pause_ratio,
        filler_count,
        confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, s: u64, e: u64) -> RecognizedWord {
        RecognizedWord {
            text: text.into(),
            start_ms: s,
            end_ms: e,
        }
    }

    #[test]
    fn fluent_recite_scores_high() {
        // 20 字 / 5 秒 = 4 cps（理想），无停顿无语气词
        let words: Vec<_> = (0..20).map(|i| w("字", i * 250, i * 250 + 250)).collect();
        let r = evaluate(&"字".repeat(20), &words, 5000, &FluencyCfg::default());
        assert!(r.fluency >= 85.0);
        assert_eq!(r.quality, Quality::A);
        assert!(r.confidence > 0.8);
    }

    #[test]
    fn pauses_and_fillers_lower_quality() {
        // 大量停顿 + 语气词
        let words = vec![w("嗯", 0, 200), w("那个", 2000, 2500)];
        let r = evaluate("嗯那个床前", &words, 6000, &FluencyCfg::default());
        assert!(r.filler_count >= 2);
        assert!(r.pause_ratio > 0.0);
        assert!(r.fluency < 85.0);
    }

    #[test]
    fn no_timestamps_lowers_confidence() {
        let r = evaluate("床前明月光", &[], 2000, &FluencyCfg::default());
        assert_eq!(r.confidence, 0.4);
        assert_eq!(r.pause_ratio, 0.0);
    }

    #[test]
    fn quality_maps_to_review() {
        assert_eq!(Quality::A.to_review(), ReviewQuality::Good);
        assert_eq!(Quality::B.to_review(), ReviewQuality::Ok);
        assert_eq!(Quality::C.to_review(), ReviewQuality::Hard);
    }
}
