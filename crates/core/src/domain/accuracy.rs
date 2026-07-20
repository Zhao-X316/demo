//! 正确率门控（通用，M1 背诵 / M2 填空 等可复用）。
//!
//! 以"覆盖率"（LCS/答案长度）为主指标，更贴合"背对了百分之多少"的直觉。
//! 开启拼音容错时，分别用汉字、拼音计算覆盖率并取**较高值**：
//! 念对了给分，ASR 同音识别误差不惩罚。

use super::pinyin_util::to_pinyin;
use super::similarity::{coverage, edit_sim};

#[derive(Debug, Clone)]
pub struct AccuracyCfg {
    /// 通过门槛（0~100），默认 95。
    pub threshold: f64,
    /// 是否启用拼音容错，默认开启。
    pub use_pinyin: bool,
    /// 拼音是否忽略声调，默认忽略（更宽松）。
    pub ignore_tone: bool,
}

impl Default for AccuracyCfg {
    fn default() -> Self {
        Self {
            threshold: 95.0,
            use_pinyin: true,
            ignore_tone: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AccuracyResult {
    /// 正确率 0~100（四舍五入）。
    pub accuracy: f64,
    /// 是否 ≥ 门槛。
    pub pass: bool,
    /// 采用的覆盖率（汉字/拼音两路较高者）。
    pub coverage: f64,
    /// 编辑相似度（与所选路径一致），写入 metrics 供参考。
    pub edit_sim: f64,
    /// 本次是否走了拼音路径（即拼音覆盖率更高）。
    pub used_pinyin: bool,
}

/// `answer_norm`/`asr_norm` 应为已归一化文本（见 `normalize`）。
pub fn evaluate(answer_norm: &str, asr_norm: &str, cfg: &AccuracyCfg) -> AccuracyResult {
    let a: Vec<char> = answer_norm.chars().collect();
    let b: Vec<char> = asr_norm.chars().collect();
    if a.is_empty() {
        return AccuracyResult {
            accuracy: 0.0,
            pass: false,
            coverage: 0.0,
            edit_sim: 0.0,
            used_pinyin: false,
        };
    }

    let cov_c = coverage(&a, &b);
    let edit_c = edit_sim(&a, &b);

    let (cov, edit, used_pinyin) = if cfg.use_pinyin {
        let pa: Vec<char> = to_pinyin(answer_norm, cfg.ignore_tone).chars().collect();
        let pb: Vec<char> = to_pinyin(asr_norm, cfg.ignore_tone).chars().collect();
        let cov_p = coverage(&pa, &pb);
        if cov_p > cov_c {
            (cov_p, edit_sim(&pa, &pb), true)
        } else {
            (cov_c, edit_c, false)
        }
    } else {
        (cov_c, edit_c, false)
    };

    let accuracy = (100.0 * cov).round();
    AccuracyResult {
        accuracy,
        pass: accuracy >= cfg.threshold,
        coverage: cov,
        edit_sim: edit,
        used_pinyin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_passes() {
        let r = evaluate("床前明月光", "床前明月光", &AccuracyCfg::default());
        assert_eq!(r.accuracy, 100.0);
        assert!(r.pass);
    }

    #[test]
    fn missing_line_fails_threshold() {
        // 答案两句，只背一句 → 50%，低于 95 门槛
        let r = evaluate(
            "床前明月光疑是地上霜",
            "床前明月光",
            &AccuracyCfg::default(),
        );
        assert!(!r.pass);
        assert!(r.accuracy < 95.0);
    }

    #[test]
    fn pinyin_tolerates_homophone() {
        // ASR 把"霜"识别成同音字"双"——汉字路径会扣分，拼音路径应救回
        let cfg = AccuracyCfg::default();
        let with_pinyin = evaluate("疑是地上霜", "疑是地上双", &cfg);
        let no_pinyin = evaluate(
            "疑是地上霜",
            "疑是地上双",
            &AccuracyCfg {
                use_pinyin: false,
                ..cfg
            },
        );
        assert!(with_pinyin.accuracy > no_pinyin.accuracy);
        assert!(with_pinyin.used_pinyin);
        assert!(with_pinyin.pass); // shuang==shuang，拼音全对 → 100
    }

    #[test]
    fn empty_answer_is_zero() {
        let r = evaluate("", "随便", &AccuracyCfg::default());
        assert_eq!(r.accuracy, 0.0);
        assert!(!r.pass);
    }
}
