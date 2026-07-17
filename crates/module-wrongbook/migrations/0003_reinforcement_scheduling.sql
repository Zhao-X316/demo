-- M3-3：老师确认后的跨日期巩固复测。
-- 建议预览不落库；确认后原子冻结来源错误、目标学生、K1 版本、策略、到期日、作业和 core task。

CREATE TABLE wb_reinforcement_assignments (
  id                          INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id                   TEXT NOT NULL UNIQUE,
  class_id                    INTEGER NOT NULL REFERENCES classes(id),
  student_id                  INTEGER NOT NULL REFERENCES students(id),
  question_version_id         INTEGER NOT NULL REFERENCES k1_question_versions(id),
  source_grade_decision_id    INTEGER NOT NULL UNIQUE REFERENCES exam_grade_decisions_v2(id),
  source_publication_id       INTEGER NOT NULL REFERENCES exam_grade_publications_v2(id),
  strategy                    TEXT NOT NULL CHECK(strategy IN ('same_question_recheck')),
  priority                    TEXT NOT NULL CHECK(priority IN ('normal','high')),
  schedule_policy_version_id  INTEGER NOT NULL REFERENCES schedule_policy_versions(id),
  due_date                    TEXT NOT NULL CHECK(
    due_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
    AND date(due_date)=due_date
  ),
  assessment_id               INTEGER NOT NULL UNIQUE REFERENCES exam_assessments_v2(id),
  assessment_version_id       INTEGER NOT NULL UNIQUE REFERENCES exam_assessment_versions_v2(id),
  task_id                     INTEGER NOT NULL UNIQUE REFERENCES tasks(id),
  created_by                  TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at                  TEXT NOT NULL
);

CREATE INDEX idx_wb_reinforcement_target
  ON wb_reinforcement_assignments(class_id,student_id,due_date);

CREATE TRIGGER trg_wb_reinforcement_insert
BEFORE INSERT ON wb_reinforcement_assignments
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_grade_decisions_v2 source_decision
    JOIN exam_attempts_v2 source_attempt
      ON source_attempt.id=source_decision.attempt_id
     AND source_attempt.student_id=NEW.student_id
    JOIN exam_grade_publications_v2 source_publication
      ON source_publication.id=source_attempt.active_publication_id
     AND source_publication.id=NEW.source_publication_id
     AND source_publication.state='published'
    JOIN exam_grade_publication_items_v2 source_publication_item
      ON source_publication_item.publication_id=source_publication.id
     AND source_publication_item.attempt_id=source_attempt.id
    JOIN exam_grade_publication_decisions_v2 source_publication_decision
      ON source_publication_decision.publication_item_id=source_publication_item.id
     AND source_publication_decision.grade_decision_id=source_decision.id
    JOIN exam_assessment_items_v2 source_item
      ON source_item.id=source_decision.assessment_item_id
     AND source_item.question_version_id=NEW.question_version_id
    JOIN exam_assessment_versions_v2 source_version
      ON source_version.id=source_attempt.assessment_version_id
    JOIN exam_assessments_v2 source_assessment
      ON source_assessment.id=source_version.assessment_id
     AND source_assessment.class_id=NEW.class_id
    WHERE source_decision.id=NEW.source_grade_decision_id
      AND source_decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
      AND source_decision.teacher_score < source_item.score - 0.000001
  ) THEN RAISE(ABORT,'WB_REINFORCEMENT_SOURCE_MISMATCH') END;

  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_assessments_v2 assessment
    JOIN exam_assessment_versions_v2 version
      ON version.assessment_id=assessment.id
     AND version.id=NEW.assessment_version_id
     AND version.state='confirmed'
     AND version.due_date=NEW.due_date
     AND version.schedule_policy_version_id=NEW.schedule_policy_version_id
    JOIN exam_assessment_items_v2 item
      ON item.assessment_version_id=version.id
     AND item.question_version_id=NEW.question_version_id
     AND item.state='active'
    JOIN exam_assessment_targets_v2 target
      ON target.assessment_version_id=version.id
     AND target.student_id=NEW.student_id
    WHERE assessment.id=NEW.assessment_id
      AND assessment.class_id=NEW.class_id
      AND assessment.assessment_context='homework'
      AND assessment.evidence_policy='include_low_weight'
      AND assessment.audience_kind='explicit'
      AND assessment.state='active'
      AND 1=(SELECT COUNT(*) FROM exam_assessment_items_v2 only_item
             WHERE only_item.assessment_version_id=version.id AND only_item.state='active')
  ) THEN RAISE(ABORT,'WB_REINFORCEMENT_ASSESSMENT_MISMATCH') END;

  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM tasks task
    WHERE task.id=NEW.task_id
      AND task.module='wrongbook'
      AND task.student_id=NEW.student_id
      AND task.ref_type='assessment_version'
      AND task.ref_id=NEW.assessment_version_id
      AND task.kind='review'
      AND task.due_date=NEW.due_date
      AND task.status='open'
  ) THEN RAISE(ABORT,'WB_REINFORCEMENT_TASK_MISMATCH') END;
END;

CREATE TRIGGER trg_wb_reinforcement_update
BEFORE UPDATE ON wb_reinforcement_assignments
BEGIN SELECT RAISE(ABORT,'WB_REINFORCEMENT_IMMUTABLE'); END;

CREATE TRIGGER trg_wb_reinforcement_delete
BEFORE DELETE ON wb_reinforcement_assignments
BEGIN SELECT RAISE(ABORT,'WB_REINFORCEMENT_IMMUTABLE'); END;

CREATE TRIGGER trg_wb_reinforcement_attempt_kind
BEFORE INSERT ON exam_attempts_v2
WHEN EXISTS (
  SELECT 1 FROM wb_reinforcement_assignments assignment
  WHERE assignment.assessment_version_id=NEW.assessment_version_id
)
BEGIN
  SELECT CASE WHEN NEW.attempt_kind NOT IN ('first','retry')
    THEN RAISE(ABORT,'WB_REINFORCEMENT_ATTEMPT_KIND_MISMATCH') END;
END;

CREATE TRIGGER trg_wb_reinforcement_attempt_started
AFTER INSERT ON exam_attempts_v2
WHEN EXISTS (
  SELECT 1 FROM wb_reinforcement_assignments assignment
  WHERE assignment.assessment_version_id=NEW.assessment_version_id
    AND assignment.student_id=NEW.student_id
)
BEGIN
  UPDATE tasks SET status='submitted',updated_at=datetime('now')
  WHERE id=(SELECT task_id FROM wb_reinforcement_assignments
            WHERE assessment_version_id=NEW.assessment_version_id
              AND student_id=NEW.student_id);
END;

CREATE TRIGGER trg_wb_reinforcement_attempt_published
AFTER UPDATE OF state ON exam_attempts_v2
WHEN NEW.state='published' AND OLD.state IS NOT NEW.state
  AND EXISTS (
    SELECT 1 FROM wb_reinforcement_assignments assignment
    WHERE assignment.assessment_version_id=NEW.assessment_version_id
      AND assignment.student_id=NEW.student_id
  )
BEGIN
  UPDATE tasks SET status='closed',updated_at=datetime('now')
  WHERE id=(SELECT task_id FROM wb_reinforcement_assignments
            WHERE assessment_version_id=NEW.assessment_version_id
              AND student_id=NEW.student_id);
END;

CREATE TRIGGER trg_wb_reinforcement_attempt_voided
AFTER UPDATE OF state ON exam_attempts_v2
WHEN NEW.state='voided' AND OLD.state IS NOT NEW.state
  AND EXISTS (
    SELECT 1 FROM wb_reinforcement_assignments assignment
    WHERE assignment.assessment_version_id=NEW.assessment_version_id
      AND assignment.student_id=NEW.student_id
  )
  AND NOT EXISTS (
    SELECT 1 FROM exam_attempts_v2 other
    WHERE other.assessment_version_id=NEW.assessment_version_id
      AND other.student_id=NEW.student_id
      AND other.state<>'voided'
  )
BEGIN
  UPDATE tasks SET status='open',updated_at=datetime('now')
  WHERE id=(SELECT task_id FROM wb_reinforcement_assignments
            WHERE assessment_version_id=NEW.assessment_version_id
              AND student_id=NEW.student_id);
END;
