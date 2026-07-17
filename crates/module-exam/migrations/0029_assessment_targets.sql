-- M2/M3：作业受众范围。历史作业默认面向全班；单人订正使用显式目标学生。

ALTER TABLE exam_assessments_v2
  ADD COLUMN audience_kind TEXT NOT NULL DEFAULT 'class'
  CHECK(audience_kind IN ('class','explicit'));

CREATE TABLE exam_assessment_targets_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  student_id INTEGER NOT NULL REFERENCES students(id),
  created_by TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at TEXT NOT NULL,
  UNIQUE(assessment_version_id,student_id)
);

CREATE INDEX idx_exam_assessment_targets_student_v2
  ON exam_assessment_targets_v2(student_id,assessment_version_id);

CREATE TRIGGER trg_exam_assessment_explicit_insert_v2
BEFORE INSERT ON exam_assessments_v2
WHEN NEW.audience_kind='explicit' AND NEW.state<>'draft'
BEGIN SELECT RAISE(ABORT,'M2_EXPLICIT_ASSESSMENT_MUST_START_DRAFT'); END;

CREATE TRIGGER trg_exam_assessment_audience_immutable_v2
BEFORE UPDATE OF audience_kind ON exam_assessments_v2
WHEN NEW.audience_kind IS NOT OLD.audience_kind
BEGIN SELECT RAISE(ABORT,'M2_ASSESSMENT_AUDIENCE_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_assessment_target_insert_v2
BEFORE INSERT ON exam_assessment_targets_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_assessment_versions_v2 version
    JOIN exam_assessments_v2 assessment
      ON assessment.id=version.assessment_id
     AND assessment.audience_kind='explicit'
    JOIN students student
      ON student.id=NEW.student_id
     AND student.class_id=assessment.class_id
     AND student.enabled=1
    WHERE version.id=NEW.assessment_version_id
      AND version.state IN ('draft','confirmed')
  ) THEN RAISE(ABORT,'M2_ASSESSMENT_TARGET_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_assessment_target_update_v2
BEFORE UPDATE ON exam_assessment_targets_v2
BEGIN SELECT RAISE(ABORT,'M2_ASSESSMENT_TARGET_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_assessment_target_delete_v2
BEFORE DELETE ON exam_assessment_targets_v2
BEGIN SELECT RAISE(ABORT,'M2_ASSESSMENT_TARGET_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_explicit_assessment_activate_v2
BEFORE UPDATE OF state ON exam_assessments_v2
WHEN NEW.state='active' AND NEW.audience_kind='explicit'
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_assessment_versions_v2 version
    JOIN exam_assessment_targets_v2 target
      ON target.assessment_version_id=version.id
    WHERE version.assessment_id=NEW.id
      AND version.state='confirmed'
  ) THEN RAISE(ABORT,'M2_EXPLICIT_ASSESSMENT_REQUIRES_TARGET') END;
END;

CREATE TRIGGER trg_exam_explicit_attempt_target_v2
BEFORE INSERT ON exam_attempts_v2
WHEN EXISTS (
  SELECT 1
  FROM exam_assessment_versions_v2 version
  JOIN exam_assessments_v2 assessment
    ON assessment.id=version.assessment_id
   AND assessment.audience_kind='explicit'
  WHERE version.id=NEW.assessment_version_id
)
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM exam_assessment_targets_v2 target
    WHERE target.assessment_version_id=NEW.assessment_version_id
      AND target.student_id=NEW.student_id
  ) THEN RAISE(ABORT,'M2_EXPLICIT_ATTEMPT_TARGET_MISMATCH') END;
END;
