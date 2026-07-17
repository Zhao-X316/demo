-- M3-2：单学生单题订正范围。
-- 建立时只冻结 correction assessment 与目标学生；照片归属确认后才创建 attempt。

CREATE TABLE wb_correction_assignments (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  class_id INTEGER NOT NULL REFERENCES classes(id),
  student_id INTEGER NOT NULL REFERENCES students(id),
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  source_grade_decision_id INTEGER NOT NULL UNIQUE REFERENCES exam_grade_decisions_v2(id),
  source_publication_id INTEGER NOT NULL REFERENCES exam_grade_publications_v2(id),
  assessment_id INTEGER NOT NULL UNIQUE REFERENCES exam_assessments_v2(id),
  assessment_version_id INTEGER NOT NULL UNIQUE REFERENCES exam_assessment_versions_v2(id),
  created_by TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at TEXT NOT NULL
);

CREATE INDEX idx_wb_correction_target
  ON wb_correction_assignments(class_id,student_id,question_version_id);

CREATE TRIGGER trg_wb_correction_assignment_insert
BEFORE INSERT ON wb_correction_assignments
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM students student
    JOIN exam_grade_decisions_v2 decision
      ON decision.id=NEW.source_grade_decision_id
     AND decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
    JOIN exam_attempts_v2 source_attempt
      ON source_attempt.id=decision.attempt_id
     AND source_attempt.student_id=student.id
    JOIN exam_grade_publications_v2 publication
      ON publication.id=NEW.source_publication_id
     AND publication.id=source_attempt.active_publication_id
     AND publication.state='published'
    JOIN exam_grade_publication_items_v2 publication_item
      ON publication_item.publication_id=publication.id
     AND publication_item.attempt_id=source_attempt.id
    JOIN exam_grade_publication_decisions_v2 publication_decision
      ON publication_decision.publication_item_id=publication_item.id
     AND publication_decision.grade_decision_id=decision.id
    JOIN exam_assessment_items_v2 source_item
      ON source_item.id=decision.assessment_item_id
     AND source_item.question_version_id=NEW.question_version_id
     AND decision.teacher_score < source_item.score - 0.000001
    JOIN exam_assessments_v2 correction
      ON correction.id=NEW.assessment_id
     AND correction.class_id=NEW.class_id
     AND correction.assessment_context='correction'
     AND correction.evidence_policy='progress_only'
     AND correction.state='active'
    JOIN exam_assessment_versions_v2 correction_version
      ON correction_version.id=NEW.assessment_version_id
     AND correction_version.assessment_id=correction.id
     AND correction_version.state='confirmed'
    JOIN exam_assessment_items_v2 correction_item
      ON correction_item.assessment_version_id=correction_version.id
     AND correction_item.question_version_id=NEW.question_version_id
     AND correction_item.state='active'
    JOIN exam_assessment_targets_v2 correction_target
      ON correction_target.assessment_version_id=correction_version.id
     AND correction_target.student_id=NEW.student_id
    WHERE student.id=NEW.student_id
      AND student.class_id=NEW.class_id
      AND student.enabled=1
      AND (
        SELECT COUNT(*) FROM exam_assessment_items_v2 only_item
        WHERE only_item.assessment_version_id=correction_version.id
          AND only_item.state='active'
      )=1
  ) THEN RAISE(ABORT,'M3_CORRECTION_SCOPE_MISMATCH') END;

  -- 来源必须仍是同一学生同一题的最新发布作答；后续答对也会使旧错误失效。
  SELECT CASE WHEN EXISTS (
    SELECT 1
    FROM exam_grade_decisions_v2 newer_decision
    JOIN exam_attempts_v2 newer_attempt
      ON newer_attempt.id=newer_decision.attempt_id
     AND newer_attempt.student_id=NEW.student_id
    JOIN exam_grade_publications_v2 newer_publication
      ON newer_publication.id=newer_attempt.active_publication_id
     AND newer_publication.state='published'
    JOIN exam_grade_publication_items_v2 newer_item
      ON newer_item.publication_id=newer_publication.id
     AND newer_item.attempt_id=newer_attempt.id
    JOIN exam_grade_publication_decisions_v2 newer_publication_decision
      ON newer_publication_decision.publication_item_id=newer_item.id
     AND newer_publication_decision.grade_decision_id=newer_decision.id
    JOIN exam_assessment_items_v2 newer_assessment_item
      ON newer_assessment_item.id=newer_decision.assessment_item_id
     AND newer_assessment_item.question_version_id=NEW.question_version_id
    WHERE newer_decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
      AND (
        newer_publication.published_at > (
          SELECT published_at FROM exam_grade_publications_v2
          WHERE id=NEW.source_publication_id
        )
        OR (
          newer_publication.published_at = (
            SELECT published_at FROM exam_grade_publications_v2
            WHERE id=NEW.source_publication_id
          )
          AND newer_decision.id > NEW.source_grade_decision_id
        )
      )
  ) THEN RAISE(ABORT,'M3_CORRECTION_SOURCE_NOT_LATEST') END;
END;

CREATE TRIGGER trg_wb_correction_assignment_update
BEFORE UPDATE ON wb_correction_assignments
BEGIN SELECT RAISE(ABORT,'M3_CORRECTION_ASSIGNMENT_IMMUTABLE'); END;

CREATE TRIGGER trg_wb_correction_assignment_delete
BEFORE DELETE ON wb_correction_assignments
BEGIN SELECT RAISE(ABORT,'M3_CORRECTION_ASSIGNMENT_IMMUTABLE'); END;

-- 只有被冻结的目标学生可以进入该 correction assessment，且类型必须保持 correction。
CREATE TRIGGER trg_wb_correction_attempt_scope
BEFORE INSERT ON exam_attempts_v2
WHEN EXISTS (
  SELECT 1 FROM wb_correction_assignments assignment
  WHERE assignment.assessment_version_id=NEW.assessment_version_id
)
BEGIN
  SELECT CASE WHEN NEW.attempt_kind <> 'correction' OR NOT EXISTS (
    SELECT 1 FROM wb_correction_assignments assignment
    WHERE assignment.assessment_version_id=NEW.assessment_version_id
      AND assignment.student_id=NEW.student_id
  ) THEN RAISE(ABORT,'M3_CORRECTION_ATTEMPT_SCOPE_MISMATCH') END;
END;
