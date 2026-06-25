//! 间隔重复调度（通用）。背诵复习与错题复习共用。
//!
//! 记忆质量分三级，决定在间隔阶梯上前进多少：
//!
//! - Good(优)：跳 2 档；Ok(良)：跳 1 档；Hard(中)：原地。
//!
//! 首次通过：优起步间隔更长（stage=1），其余 stage=0。
//! 正确率未达标走 [`lapse`] 回到起点（次日补背）。SM-2 ease 变体可后续扩展。

/// 复习质量三级。M1 熟练度 A/B/C 映射到此。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewQuality {
    Good,
    Ok,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleOutcome {
    pub stage: i32,
    pub interval_days: i32,
}

/// 默认间隔阶梯（天）。设置页可改。
pub const DEFAULT_LADDER: &[i32] = &[1, 2, 4, 7, 15, 30, 60, 120];

/// 阶梯模式：根据上一阶段与本次质量，给出新阶段与间隔。
pub fn next_with_ladder(
    ladder: &[i32],
    prev_stage: i32,
    quality: ReviewQuality,
    first: bool,
) -> ScheduleOutcome {
    let last = (ladder.len() as i32 - 1).max(0);
    let stage = if first {
        match quality {
            ReviewQuality::Good => 1,
            _ => 0,
        }
    } else {
        let step = match quality {
            ReviewQuality::Good => 2,
            ReviewQuality::Ok => 1,
            ReviewQuality::Hard => 0,
        };
        prev_stage + step
    };
    let stage = stage.clamp(0, last);
    let interval = ladder.get(stage as usize).copied().unwrap_or(1);
    ScheduleOutcome { stage, interval_days: interval }
}

/// 正确率未达标（脱档/遗忘）：回到起点，next 走补背流程。
pub fn lapse() -> ScheduleOutcome {
    ScheduleOutcome { stage: 0, interval_days: DEFAULT_LADDER[0] }
}

/// 默认调度器（阶梯模式），实现 `ports::Scheduler`。
pub struct LadderScheduler {
    pub ladder: Vec<i32>,
}

impl Default for LadderScheduler {
    fn default() -> Self {
        Self { ladder: DEFAULT_LADDER.to_vec() }
    }
}

impl crate::ports::Scheduler for LadderScheduler {
    fn next(&self, prev_stage: i32, quality: ReviewQuality, first: bool) -> ScheduleOutcome {
        next_with_ladder(&self.ladder, prev_stage, quality, first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: &[i32] = DEFAULT_LADDER;

    #[test]
    fn first_good_starts_longer() {
        let o = next_with_ladder(L, 0, ReviewQuality::Good, true);
        assert_eq!(o.stage, 1);
        assert_eq!(o.interval_days, 2);
    }

    #[test]
    fn first_hard_starts_short() {
        let o = next_with_ladder(L, 0, ReviewQuality::Hard, true);
        assert_eq!(o.stage, 0);
        assert_eq!(o.interval_days, 1);
    }

    #[test]
    fn good_jumps_two_ok_one_hard_zero() {
        assert_eq!(next_with_ladder(L, 2, ReviewQuality::Good, false).stage, 4);
        assert_eq!(next_with_ladder(L, 2, ReviewQuality::Ok, false).stage, 3);
        assert_eq!(next_with_ladder(L, 2, ReviewQuality::Hard, false).stage, 2);
    }

    #[test]
    fn caps_at_last_stage() {
        let o = next_with_ladder(L, 7, ReviewQuality::Good, false);
        assert_eq!(o.stage, 7);
        assert_eq!(o.interval_days, 120);
    }
}
