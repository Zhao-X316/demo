//! 背诵模块可调配置：持久化到 `app_settings`（非敏感），并可转成评分用的 `ScoreCfg`。
//!
//! 火山 ASR 凭据属敏感信息，**不**放这里（见外壳 secrets.json）。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use suite_core::db::repo::settings;
use suite_core::domain::accuracy::AccuracyCfg;
use suite_core::domain::normalize::NormalizeCfg;
use suite_core::error::{CoreError, CoreResult};

use crate::domain::fluency::FluencyCfg;
use crate::service::scoring::ScoreCfg;

const KEY: &str = "recitation.config";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecitationConfig {
    /// 正确率通过门槛（百分制）。
    pub accuracy_threshold: f64,
    /// 正确率是否用拼音兜底（同音也算对）。
    pub use_pinyin: bool,
    /// 拼音比较是否忽略声调。
    pub ignore_tone: bool,
    /// 归一化时是否剔除语气词。
    pub remove_fillers: bool,
    /// 理想语速（字/秒），用于熟练度评分。
    pub ideal_cps: f64,
    /// 熟练度 A 档下限。
    pub quality_a_min: f64,
    /// 熟练度 B 档下限。
    pub quality_b_min: f64,
    /// 未通过后补背相对今天的天数偏移（1 = 次日）。
    pub makeup_offset_days: i64,
}

impl Default for RecitationConfig {
    fn default() -> Self {
        let a = AccuracyCfg::default();
        let f = FluencyCfg::default();
        Self {
            accuracy_threshold: a.threshold,
            use_pinyin: a.use_pinyin,
            ignore_tone: a.ignore_tone,
            remove_fillers: NormalizeCfg::default().remove_fillers,
            ideal_cps: f.ideal_cps,
            quality_a_min: f.a_min,
            quality_b_min: f.b_min,
            makeup_offset_days: 1,
        }
    }
}

impl RecitationConfig {
    /// 读取持久化配置；无记录或解析失败则回落默认。
    pub fn load(conn: &Connection) -> CoreResult<Self> {
        match settings::get(conn, KEY)? {
            Some(s) => Ok(serde_json::from_str(&s).unwrap_or_default()),
            None => Ok(Self::default()),
        }
    }

    pub fn save(&self, conn: &Connection) -> CoreResult<()> {
        let s = serde_json::to_string(self).map_err(|e| CoreError::Invalid(e.to_string()))?;
        settings::set(conn, KEY, &s)
    }

    /// 转成评分编排所需的 `ScoreCfg`。
    pub fn to_score_cfg(&self) -> ScoreCfg {
        let accuracy = AccuracyCfg {
            threshold: self.accuracy_threshold,
            use_pinyin: self.use_pinyin,
            ignore_tone: self.ignore_tone,
        };
        let fluency = FluencyCfg {
            ideal_cps: self.ideal_cps,
            a_min: self.quality_a_min,
            b_min: self.quality_b_min,
            ..FluencyCfg::default()
        };
        let normalize = NormalizeCfg { remove_fillers: self.remove_fillers, ..NormalizeCfg::default() };
        ScoreCfg { normalize, accuracy, fluency, makeup_offset_days: self.makeup_offset_days }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    #[test]
    fn default_roundtrip_and_override() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();

        // 无记录 → 默认
        let c = RecitationConfig::load(&conn).unwrap();
        assert_eq!(c.accuracy_threshold, 95.0);
        assert!(c.use_pinyin);

        // 改后保存 → 再读回
        let mut c2 = c.clone();
        c2.accuracy_threshold = 90.0;
        c2.makeup_offset_days = 2;
        c2.save(&conn).unwrap();
        let back = RecitationConfig::load(&conn).unwrap();
        assert_eq!(back.accuracy_threshold, 90.0);
        assert_eq!(back.makeup_offset_days, 2);

        // 转 ScoreCfg 生效
        let sc = back.to_score_cfg();
        assert_eq!(sc.accuracy.threshold, 90.0);
        assert_eq!(sc.makeup_offset_days, 2);
    }
}
