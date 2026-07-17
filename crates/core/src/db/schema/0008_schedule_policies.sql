-- M1/M3/M6 共用的校历排程策略。
-- 业务任务继续写入 core.tasks；策略只负责冻结“何时到期、每天最多多少项、哪些日期跳过”。

CREATE TABLE schedule_policy_versions (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    public_id                TEXT NOT NULL UNIQUE CHECK(length(trim(public_id)) > 0),
    policy_key               TEXT NOT NULL CHECK(length(trim(policy_key)) > 0),
    revision                 INTEGER NOT NULL CHECK(revision > 0),
    timezone                 TEXT NOT NULL CHECK(timezone = 'Asia/Shanghai'),
    default_delay_days       INTEGER NOT NULL CHECK(default_delay_days BETWEEN 1 AND 60),
    daily_limit_per_student  INTEGER NOT NULL CHECK(daily_limit_per_student BETWEEN 1 AND 20),
    weekend_policy           TEXT NOT NULL CHECK(weekend_policy IN ('allow','next_workday')),
    holiday_policy           TEXT NOT NULL CHECK(holiday_policy IN ('allow','next_workday')),
    max_shift_days           INTEGER NOT NULL CHECK(max_shift_days BETWEEN 1 AND 120),
    state                    TEXT NOT NULL CHECK(state IN ('active','retired')),
    created_by               TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
    created_at               TEXT NOT NULL CHECK(length(trim(created_at)) > 0),
    UNIQUE(policy_key, revision)
);

CREATE UNIQUE INDEX idx_schedule_policy_active
    ON schedule_policy_versions(policy_key) WHERE state='active';

CREATE TABLE schedule_policy_holidays (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    policy_version_id  INTEGER NOT NULL REFERENCES schedule_policy_versions(id),
    calendar_date      TEXT NOT NULL CHECK(
        calendar_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
        AND date(calendar_date)=calendar_date
    ),
    label              TEXT NOT NULL CHECK(length(trim(label)) > 0),
    created_at         TEXT NOT NULL CHECK(length(trim(created_at)) > 0),
    UNIQUE(policy_version_id, calendar_date)
);

CREATE TRIGGER trg_schedule_policy_content_immutable
BEFORE UPDATE ON schedule_policy_versions
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.policy_key IS NOT OLD.policy_key
  OR NEW.revision IS NOT OLD.revision
  OR NEW.timezone IS NOT OLD.timezone
  OR NEW.default_delay_days IS NOT OLD.default_delay_days
  OR NEW.daily_limit_per_student IS NOT OLD.daily_limit_per_student
  OR NEW.weekend_policy IS NOT OLD.weekend_policy
  OR NEW.holiday_policy IS NOT OLD.holiday_policy
  OR NEW.max_shift_days IS NOT OLD.max_shift_days
  OR NEW.created_by IS NOT OLD.created_by
  OR NEW.created_at IS NOT OLD.created_at
  OR NOT (
      NEW.state IS OLD.state
      OR (OLD.state='active' AND NEW.state='retired')
  )
BEGIN SELECT RAISE(ABORT, 'SCHEDULE_POLICY_IMMUTABLE'); END;

CREATE TRIGGER trg_schedule_policy_delete
BEFORE DELETE ON schedule_policy_versions
BEGIN SELECT RAISE(ABORT, 'SCHEDULE_POLICY_IMMUTABLE'); END;

CREATE TRIGGER trg_schedule_holiday_scope
BEFORE INSERT ON schedule_policy_holidays
WHEN NOT EXISTS (
    SELECT 1 FROM schedule_policy_versions
    WHERE id=NEW.policy_version_id AND state='active'
)
BEGIN SELECT RAISE(ABORT, 'SCHEDULE_HOLIDAY_REQUIRES_ACTIVE_POLICY'); END;

CREATE TRIGGER trg_schedule_holiday_update
BEFORE UPDATE ON schedule_policy_holidays
BEGIN SELECT RAISE(ABORT, 'SCHEDULE_HOLIDAY_IMMUTABLE'); END;

CREATE TRIGGER trg_schedule_holiday_delete
BEFORE DELETE ON schedule_policy_holidays
BEGIN SELECT RAISE(ABORT, 'SCHEDULE_HOLIDAY_IMMUTABLE'); END;

INSERT INTO schedule_policy_versions (
    public_id, policy_key, revision, timezone, default_delay_days,
    daily_limit_per_student, weekend_policy, holiday_policy, max_shift_days,
    state, created_by, created_at
) VALUES (
    'learning-default-v1', 'learning_default', 1, 'Asia/Shanghai', 7,
    3, 'next_workday', 'next_workday', 60,
    'active', 'system_default', strftime('%Y-%m-%dT%H:%M:%fZ','now')
);
