//! 跨模块校历排程策略。
//!
//! M1/M3/M6 共用 `tasks` 作为正式任务；本服务只负责版本化策略、假期和
//! 每名学生每日上限。调用方必须在自己的业务事务中再次计算并写入任务。

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::domain::{ids, time};
use crate::error::{CoreError, CoreResult};

pub const LEARNING_POLICY_KEY: &str = "learning_default";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleHoliday {
    pub calendar_date: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulePolicy {
    pub id: i64,
    pub public_id: String,
    pub policy_key: String,
    pub revision: i64,
    pub timezone: String,
    pub default_delay_days: i64,
    pub daily_limit_per_student: i64,
    pub weekend_policy: String,
    pub holiday_policy: String,
    pub max_shift_days: i64,
    pub state: String,
    pub created_by: String,
    pub created_at: String,
    pub holidays: Vec<ScheduleHoliday>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulePolicyUpdate {
    pub default_delay_days: i64,
    pub daily_limit_per_student: i64,
    pub weekend_policy: String,
    pub holiday_policy: String,
    pub max_shift_days: i64,
    pub holidays: Vec<ScheduleHoliday>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DueDatePlan {
    pub policy_public_id: String,
    pub policy_revision: i64,
    pub earliest_due_date: String,
    pub suggested_due_date: String,
    pub shifted_days: i64,
    pub existing_task_count: i64,
    pub daily_limit_per_student: i64,
}

type SchedulePolicyRow = (
    i64,
    String,
    String,
    i64,
    String,
    i64,
    i64,
    String,
    String,
    i64,
    String,
    String,
    String,
);

fn parse_date(value: &str, label: &str) -> CoreResult<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid(format!("{label}必须是有效的 YYYY-MM-DD 日期")))
}

fn normalize_holidays(values: &[ScheduleHoliday]) -> CoreResult<Vec<ScheduleHoliday>> {
    let mut unique = BTreeMap::new();
    for holiday in values {
        parse_date(&holiday.calendar_date, "假期日期")?;
        let label = holiday.label.trim();
        if label.is_empty() {
            return Err(CoreError::Invalid("假期名称不能为空".into()));
        }
        if unique
            .insert(holiday.calendar_date.clone(), label.to_string())
            .is_some()
        {
            return Err(CoreError::Invalid("同一天不能重复登记假期".into()));
        }
    }
    Ok(unique
        .into_iter()
        .map(|(calendar_date, label)| ScheduleHoliday {
            calendar_date,
            label,
        })
        .collect())
}

fn validate_update(input: &SchedulePolicyUpdate) -> CoreResult<Vec<ScheduleHoliday>> {
    if !(1..=60).contains(&input.default_delay_days) {
        return Err(CoreError::Invalid("默认间隔必须在 1～60 天之间".into()));
    }
    if !(1..=20).contains(&input.daily_limit_per_student) {
        return Err(CoreError::Invalid("每日上限必须在 1～20 项之间".into()));
    }
    if !matches!(input.weekend_policy.as_str(), "allow" | "next_workday") {
        return Err(CoreError::Invalid("周末策略非法".into()));
    }
    if !matches!(input.holiday_policy.as_str(), "allow" | "next_workday") {
        return Err(CoreError::Invalid("假期策略非法".into()));
    }
    if !(1..=120).contains(&input.max_shift_days) {
        return Err(CoreError::Invalid("顺延上限必须在 1～120 天之间".into()));
    }
    normalize_holidays(&input.holidays)
}

fn load_holidays(conn: &Connection, policy_id: i64) -> CoreResult<Vec<ScheduleHoliday>> {
    let mut statement = conn.prepare(
        "SELECT calendar_date,label
         FROM schedule_policy_holidays
         WHERE policy_version_id=?1
         ORDER BY calendar_date",
    )?;
    let rows = statement.query_map([policy_id], |row| {
        Ok(ScheduleHoliday {
            calendar_date: row.get(0)?,
            label: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn row_to_policy(conn: &Connection, row: SchedulePolicyRow) -> CoreResult<SchedulePolicy> {
    Ok(SchedulePolicy {
        id: row.0,
        public_id: row.1,
        policy_key: row.2,
        revision: row.3,
        timezone: row.4,
        default_delay_days: row.5,
        daily_limit_per_student: row.6,
        weekend_policy: row.7,
        holiday_policy: row.8,
        max_shift_days: row.9,
        state: row.10,
        created_by: row.11,
        created_at: row.12,
        holidays: load_holidays(conn, row.0)?,
    })
}

pub fn load_active_policy(conn: &Connection, policy_key: &str) -> CoreResult<SchedulePolicy> {
    let row = conn
        .query_row(
            "SELECT id,public_id,policy_key,revision,timezone,default_delay_days,
                    daily_limit_per_student,weekend_policy,holiday_policy,max_shift_days,
                    state,created_by,created_at
             FROM schedule_policy_versions
             WHERE policy_key=?1 AND state='active'",
            [policy_key],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                    row.get(10)?,
                    row.get(11)?,
                    row.get(12)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("排程策略 {policy_key}")))?;
    row_to_policy(conn, row)
}

pub fn replace_active_policy(
    conn: &mut Connection,
    policy_key: &str,
    input: &SchedulePolicyUpdate,
    created_by: &str,
) -> CoreResult<SchedulePolicy> {
    if created_by.trim().is_empty() {
        return Err(CoreError::Invalid("策略修改人不能为空".into()));
    }
    let holidays = validate_update(input)?;
    let current = load_active_policy(conn, policy_key)?;
    if current.default_delay_days == input.default_delay_days
        && current.daily_limit_per_student == input.daily_limit_per_student
        && current.weekend_policy == input.weekend_policy
        && current.holiday_policy == input.holiday_policy
        && current.max_shift_days == input.max_shift_days
        && current.holidays == holidays
    {
        return Ok(current);
    }

    let transaction = conn.transaction()?;
    transaction.execute(
        "UPDATE schedule_policy_versions SET state='retired'
         WHERE id=?1 AND state='active'",
        [current.id],
    )?;
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    transaction.execute(
        "INSERT INTO schedule_policy_versions
         (public_id,policy_key,revision,timezone,default_delay_days,
          daily_limit_per_student,weekend_policy,holiday_policy,max_shift_days,
          state,created_by,created_at)
         VALUES (?1,?2,?3,'Asia/Shanghai',?4,?5,?6,?7,?8,'active',?9,?10)",
        (
            &public_id,
            policy_key,
            current.revision + 1,
            input.default_delay_days,
            input.daily_limit_per_student,
            input.weekend_policy.as_str(),
            input.holiday_policy.as_str(),
            input.max_shift_days,
            created_by.trim(),
            &created_at,
        ),
    )?;
    let policy_id = transaction.last_insert_rowid();
    for holiday in holidays {
        transaction.execute(
            "INSERT INTO schedule_policy_holidays
             (policy_version_id,calendar_date,label,created_at)
             VALUES (?1,?2,?3,?4)",
            (policy_id, holiday.calendar_date, holiday.label, &created_at),
        )?;
    }
    transaction.commit()?;
    load_active_policy(conn, policy_key)
}

fn is_blocked_date(
    conn: &Connection,
    policy: &SchedulePolicy,
    date: NaiveDate,
) -> CoreResult<bool> {
    if policy.weekend_policy == "next_workday"
        && matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
    {
        return Ok(true);
    }
    if policy.holiday_policy != "next_workday" {
        return Ok(false);
    }
    let blocked: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM schedule_policy_holidays
           WHERE policy_version_id=?1 AND calendar_date=?2
         )",
        (policy.id, date.format("%Y-%m-%d").to_string()),
        |row| row.get(0),
    )?;
    Ok(blocked)
}

pub fn plan_due_date(
    conn: &Connection,
    student_id: i64,
    earliest_due_date: NaiveDate,
    policy: &SchedulePolicy,
) -> CoreResult<DueDatePlan> {
    let eligible: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM students WHERE id=?1 AND enabled=1)",
        [student_id],
        |row| row.get(0),
    )?;
    if !eligible {
        return Err(CoreError::Invalid("排程目标必须是启用学生".into()));
    }
    for shifted_days in 0..=policy.max_shift_days {
        let candidate = earliest_due_date + Duration::days(shifted_days);
        if is_blocked_date(conn, policy, candidate)? {
            continue;
        }
        let date_text = candidate.format("%Y-%m-%d").to_string();
        let existing_task_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tasks
             WHERE student_id=?1 AND due_date=?2
               AND status IN ('open','submitted','reopened')",
            (student_id, &date_text),
            |row| row.get(0),
        )?;
        if existing_task_count < policy.daily_limit_per_student {
            return Ok(DueDatePlan {
                policy_public_id: policy.public_id.clone(),
                policy_revision: policy.revision,
                earliest_due_date: earliest_due_date.format("%Y-%m-%d").to_string(),
                suggested_due_date: date_text,
                shifted_days,
                existing_task_count,
                daily_limit_per_student: policy.daily_limit_per_student,
            });
        }
    }
    Err(CoreError::Invalid(format!(
        "未来 {} 天没有可用任务名额，请调整每日上限或校历",
        policy.max_shift_days
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn setup() -> (Connection, i64) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        conn.execute(
            "INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO students(student_no,name,class_id,enabled)
             VALUES ('01','小林',1,1)",
            [],
        )
        .unwrap();
        let student_id = conn.last_insert_rowid();
        (conn, student_id)
    }

    #[test]
    fn default_policy_skips_weekend_holiday_and_full_days() {
        let (mut conn, student_id) = setup();
        let updated = replace_active_policy(
            &mut conn,
            LEARNING_POLICY_KEY,
            &SchedulePolicyUpdate {
                default_delay_days: 7,
                daily_limit_per_student: 1,
                weekend_policy: "next_workday".into(),
                holiday_policy: "next_workday".into(),
                max_shift_days: 30,
                holidays: vec![ScheduleHoliday {
                    calendar_date: "2026-07-20".into(),
                    label: "校休".into(),
                }],
            },
            "teacher",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tasks(module,student_id,ref_type,ref_id,kind,due_date,status)
             VALUES ('recitation',?1,'content',1,'review','2026-07-21','open')",
            [student_id],
        )
        .unwrap();
        let plan = plan_due_date(
            &conn,
            student_id,
            NaiveDate::from_ymd_opt(2026, 7, 18).unwrap(),
            &updated,
        )
        .unwrap();
        assert_eq!(plan.suggested_due_date, "2026-07-22");
        assert_eq!(plan.shifted_days, 4);
        assert_eq!(plan.existing_task_count, 0);
    }

    #[test]
    fn identical_policy_update_is_noop_and_changes_create_revision() {
        let (mut conn, _) = setup();
        let current = load_active_policy(&conn, LEARNING_POLICY_KEY).unwrap();
        let same = replace_active_policy(
            &mut conn,
            LEARNING_POLICY_KEY,
            &SchedulePolicyUpdate {
                default_delay_days: current.default_delay_days,
                daily_limit_per_student: current.daily_limit_per_student,
                weekend_policy: current.weekend_policy.clone(),
                holiday_policy: current.holiday_policy.clone(),
                max_shift_days: current.max_shift_days,
                holidays: current.holidays.clone(),
            },
            "teacher",
        )
        .unwrap();
        assert_eq!(same.public_id, current.public_id);

        let changed = replace_active_policy(
            &mut conn,
            LEARNING_POLICY_KEY,
            &SchedulePolicyUpdate {
                default_delay_days: 10,
                daily_limit_per_student: 4,
                weekend_policy: "allow".into(),
                holiday_policy: "allow".into(),
                max_shift_days: 30,
                holidays: vec![],
            },
            "teacher",
        )
        .unwrap();
        assert_eq!(changed.revision, current.revision + 1);
        let retired: String = conn
            .query_row(
                "SELECT state FROM schedule_policy_versions WHERE id=?1",
                [current.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retired, "retired");
    }
}
