-- M3-1 老师确认错因。
-- 错因绑定具体已发布评分 revision；机器候选和证据链异常不能直接写入正式统计。

CREATE TABLE wb_error_cause_revisions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  grade_decision_id INTEGER NOT NULL REFERENCES exam_grade_decisions_v2(id),
  publication_id INTEGER NOT NULL REFERENCES exam_grade_publications_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  teacher_note TEXT,
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by)) > 0),
  confirmed_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('active','superseded')),
  UNIQUE(grade_decision_id,revision),
  CHECK(teacher_note IS NULL OR length(trim(teacher_note)) > 0)
);

CREATE UNIQUE INDEX idx_wb_error_cause_active
  ON wb_error_cause_revisions(grade_decision_id)
  WHERE state='active';

CREATE TABLE wb_error_cause_items (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  revision_id INTEGER NOT NULL REFERENCES wb_error_cause_revisions(id),
  cause_code TEXT NOT NULL CHECK(cause_code IN (
    'missing_answer',
    'fact_error',
    'concept_confusion',
    'chronology_error',
    'causal_gap',
    'rubric_omission',
    'contradiction',
    'incomplete_expression',
    'misread_prompt',
    'other'
  )),
  order_index INTEGER NOT NULL CHECK(order_index >= 0),
  created_at TEXT NOT NULL,
  UNIQUE(revision_id,cause_code),
  UNIQUE(revision_id,order_index)
);

CREATE TRIGGER trg_wb_error_cause_scope_insert
BEFORE INSERT ON wb_error_cause_revisions
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_grade_decisions_v2 decision
    JOIN exam_attempts_v2 attempt
      ON attempt.id=decision.attempt_id
     AND attempt.active_publication_id=NEW.publication_id
    JOIN exam_grade_publications_v2 publication
      ON publication.id=NEW.publication_id
     AND publication.state='published'
    JOIN exam_grade_publication_items_v2 publication_item
      ON publication_item.publication_id=publication.id
     AND publication_item.attempt_id=attempt.id
    JOIN exam_grade_publication_decisions_v2 publication_decision
      ON publication_decision.publication_item_id=publication_item.id
     AND publication_decision.grade_decision_id=decision.id
    JOIN exam_assessment_items_v2 assessment_item
      ON assessment_item.id=decision.assessment_item_id
    WHERE decision.id=NEW.grade_decision_id
      AND decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
      AND decision.teacher_score < assessment_item.score - 0.000001
  ) THEN RAISE(ABORT,'M3_ERROR_CAUSE_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_wb_error_cause_content_immutable
BEFORE UPDATE ON wb_error_cause_revisions
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.grade_decision_id IS NOT OLD.grade_decision_id
  OR NEW.publication_id IS NOT OLD.publication_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.teacher_note IS NOT OLD.teacher_note
  OR NEW.confirmed_by IS NOT OLD.confirmed_by
  OR NEW.confirmed_at IS NOT OLD.confirmed_at
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M3_ERROR_CAUSE_IMMUTABLE'); END;

CREATE TRIGGER trg_wb_error_cause_state_guard
BEFORE UPDATE OF state ON wb_error_cause_revisions
WHEN NOT (OLD.state='active' AND NEW.state='superseded')
BEGIN SELECT RAISE(ABORT,'M3_ERROR_CAUSE_STATE_TRANSITION'); END;

CREATE TRIGGER trg_wb_error_cause_delete
BEFORE DELETE ON wb_error_cause_revisions
BEGIN SELECT RAISE(ABORT,'M3_ERROR_CAUSE_IMMUTABLE'); END;

CREATE TRIGGER trg_wb_error_cause_item_update
BEFORE UPDATE ON wb_error_cause_items
BEGIN SELECT RAISE(ABORT,'M3_ERROR_CAUSE_ITEM_IMMUTABLE'); END;

CREATE TRIGGER trg_wb_error_cause_item_delete
BEFORE DELETE ON wb_error_cause_items
BEGIN SELECT RAISE(ABORT,'M3_ERROR_CAUSE_ITEM_IMMUTABLE'); END;
