//! 间隔重复编排（通用，背诵复习/错题复习共用）。
//!
//! - [`record_pass`]：一次"通过"后，按记忆质量推进卡片并写入下次到期日。
//! - [`record_lapse`]：一次"未达标"后，卡片脱档回到起点（后续走补背）。
//!
//! 日期通过参数传入（便于测试与时区控制），不在此处读系统时钟。

use chrono::{Duration, NaiveDate};
use rusqlite::Connection;

use crate::db::repo::memory_cards::{self, CardUpdate};
use crate::domain::scheduler::{ReviewQuality, ScheduleOutcome};
use crate::error::CoreResult;
use crate::models::{CardState, ModuleKey};
use crate::ports::Scheduler;

pub struct ReviewRef<'a> {
    pub module: ModuleKey,
    pub student_id: i64,
    pub ref_type: &'a str,
    pub ref_id: i64,
}

/// 通过一次：推进阶梯，写入新的 due_date，返回排程结果。
pub fn record_pass(
    conn: &Connection,
    r: &ReviewRef<'_>,
    quality: ReviewQuality,
    today: NaiveDate,
    scheduler: &dyn Scheduler,
) -> CoreResult<ScheduleOutcome> {
    let existing = memory_cards::get(conn, r.module, r.student_id, r.ref_type, r.ref_id)?;
    let first = existing
        .as_ref()
        .map(|card| card.state == CardState::Lapsed)
        .unwrap_or(true);
    let prev_stage = existing.as_ref().map(|card| card.stage).unwrap_or(0);

    let out = scheduler.next(prev_stage, quality, first);
    let due = today + Duration::days(out.interval_days as i64);

    memory_cards::upsert_after_pass(
        conn,
        &CardUpdate {
            module: r.module,
            student_id: r.student_id,
            ref_type: r.ref_type,
            ref_id: r.ref_id,
            state: CardState::Review,
            stage: out.stage,
            interval_days: out.interval_days,
            quality: quality_str(quality),
            due_date: &due.format("%Y-%m-%d").to_string(),
        },
    )?;
    Ok(out)
}

/// 未达标一次：脱档回到起点（state=lapsed, stage=0），due 置当日，等待补背。
pub fn record_lapse(conn: &Connection, r: &ReviewRef<'_>, today: NaiveDate) -> CoreResult<()> {
    memory_cards::upsert_after_lapse(
        conn,
        &CardUpdate {
            module: r.module,
            student_id: r.student_id,
            ref_type: r.ref_type,
            ref_id: r.ref_id,
            state: CardState::Lapsed,
            stage: 0,
            interval_days: 0,
            quality: "C",
            due_date: &today.format("%Y-%m-%d").to_string(),
        },
    )
}

fn quality_str(q: ReviewQuality) -> &'static str {
    match q {
        ReviewQuality::Good => "A",
        ReviewQuality::Ok => "B",
        ReviewQuality::Hard => "C",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::students::{upsert as upsert_student, StudentInput};
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use crate::domain::scheduler::LadderScheduler;

    fn setup() -> (Connection, i64) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let s = upsert_student(
            &conn,
            &StudentInput { student_no: "2023001", name: "张三", class_id: None, enabled: true },
        )
        .unwrap();
        (conn, s.id)
    }

    #[test]
    fn first_pass_good_schedules_two_days_out() {
        let (conn, sid) = setup();
        let sched = LadderScheduler::default();
        let r = ReviewRef { module: ModuleKey::Recitation, student_id: sid, ref_type: "content", ref_id: 7 };
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();

        let out = record_pass(&conn, &r, ReviewQuality::Good, today, &sched).unwrap();
        assert_eq!(out.stage, 1);
        assert_eq!(out.interval_days, 2);

        let card = memory_cards::get(&conn, ModuleKey::Recitation, sid, "content", 7)
            .unwrap()
            .unwrap();
        assert_eq!(card.due_date.as_deref(), Some("2026-06-27"));
        assert_eq!(card.reps, 1);
    }

    #[test]
    fn second_pass_good_advances_two_stages() {
        let (conn, sid) = setup();
        let sched = LadderScheduler::default();
        let r = ReviewRef { module: ModuleKey::Recitation, student_id: sid, ref_type: "content", ref_id: 7 };
        let day1 = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        record_pass(&conn, &r, ReviewQuality::Good, day1, &sched).unwrap(); // stage 1

        let day2 = NaiveDate::from_ymd_opt(2026, 6, 27).unwrap();
        let out = record_pass(&conn, &r, ReviewQuality::Good, day2, &sched).unwrap();
        assert_eq!(out.stage, 3); // 1 + 2
        assert_eq!(out.interval_days, 7);
        let card = memory_cards::get(&conn, ModuleKey::Recitation, sid, "content", 7)
            .unwrap()
            .unwrap();
        assert_eq!(card.due_date.as_deref(), Some("2026-07-04")); // 06-27 + 7
        assert_eq!(card.reps, 2);
    }

    #[test]
    fn lapse_marks_lapsed_and_resets() {
        let (conn, sid) = setup();
        let r = ReviewRef { module: ModuleKey::Recitation, student_id: sid, ref_type: "content", ref_id: 7 };
        let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        record_lapse(&conn, &r, today).unwrap();
        let card = memory_cards::get(&conn, ModuleKey::Recitation, sid, "content", 7)
            .unwrap()
            .unwrap();
        assert_eq!(card.stage, 0);
        assert_eq!(card.due_date.as_deref(), Some("2026-06-25"));
        assert_eq!(card.reps, 0);
        assert_eq!(card.lapses, 1);
    }

    #[test]
    fn good_pass_after_lapse_restarts_from_first_stage_and_keeps_lapse_count() {
        let (conn, sid) = setup();
        let sched = LadderScheduler::default();
        let r = ReviewRef { module: ModuleKey::Recitation, student_id: sid, ref_type: "content", ref_id: 7 };
        let failed_on = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
        record_lapse(&conn, &r, failed_on).unwrap();

        let passed_on = NaiveDate::from_ymd_opt(2026, 6, 26).unwrap();
        let out = record_pass(&conn, &r, ReviewQuality::Good, passed_on, &sched).unwrap();
        assert_eq!(out.stage, 1);
        assert_eq!(out.interval_days, 2);

        let card = memory_cards::get(&conn, ModuleKey::Recitation, sid, "content", 7)
            .unwrap()
            .unwrap();
        assert_eq!(card.state, CardState::Review);
        assert_eq!(card.stage, 1);
        assert_eq!(card.interval_days, 2);
        assert_eq!(card.due_date.as_deref(), Some("2026-06-28"));
        assert_eq!(card.reps, 1);
        assert_eq!(card.lapses, 1);
    }
}
