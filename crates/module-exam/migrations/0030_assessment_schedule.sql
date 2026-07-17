-- M2/M3：作业版本冻结校历到期日和对应共享排程策略版本。
-- 历史作业保持 NULL，不按 created_at 猜测到期日。

ALTER TABLE exam_assessment_versions_v2
  ADD COLUMN due_date TEXT CHECK(
    due_date IS NULL OR (
      due_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
      AND date(due_date)=due_date
    )
  );

ALTER TABLE exam_assessment_versions_v2
  ADD COLUMN schedule_policy_version_id INTEGER
  REFERENCES schedule_policy_versions(id);

CREATE TRIGGER trg_exam_assessment_schedule_pair_insert_v2
BEFORE INSERT ON exam_assessment_versions_v2
WHEN (NEW.due_date IS NULL) <> (NEW.schedule_policy_version_id IS NULL)
BEGIN SELECT RAISE(ABORT,'M2_ASSESSMENT_SCHEDULE_PAIR_REQUIRED'); END;

CREATE TRIGGER trg_exam_assessment_schedule_pair_update_v2
BEFORE UPDATE ON exam_assessment_versions_v2
WHEN (NEW.due_date IS NULL) <> (NEW.schedule_policy_version_id IS NULL)
BEGIN SELECT RAISE(ABORT,'M2_ASSESSMENT_SCHEDULE_PAIR_REQUIRED'); END;

CREATE TRIGGER trg_exam_assessment_schedule_immutable_v2
BEFORE UPDATE OF due_date,schedule_policy_version_id ON exam_assessment_versions_v2
WHEN NEW.due_date IS NOT OLD.due_date
  OR NEW.schedule_policy_version_id IS NOT OLD.schedule_policy_version_id
BEGIN SELECT RAISE(ABORT,'M2_ASSESSMENT_SCHEDULE_IMMUTABLE'); END;
