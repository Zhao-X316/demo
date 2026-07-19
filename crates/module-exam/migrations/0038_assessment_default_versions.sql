-- M2.5-3d：未来新作业默认版本的显式选择账本。
--
-- 选择默认版本只影响之后从上传入口创建的 attempt；历史 attempt、评分、
-- 发布和 learning evidence 继续引用原 assessment version，不允许静默重绑。

CREATE TABLE exam_assessment_default_version_selections_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE CHECK(length(trim(request_key)) > 0),
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  assessment_id INTEGER NOT NULL REFERENCES exam_assessments_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  previous_assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  selected_assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  source_impact_plan_id INTEGER NOT NULL REFERENCES exam_question_version_impact_plans_v2(id),
  selected_by TEXT NOT NULL CHECK(length(trim(selected_by)) > 0),
  selected_at TEXT NOT NULL,
  UNIQUE(assessment_id, revision),
  CHECK(previous_assessment_version_id <> selected_assessment_version_id)
);

CREATE INDEX idx_exam_assessment_default_selection_current_v2
  ON exam_assessment_default_version_selections_v2(
    assessment_id,revision DESC,id DESC
  );

CREATE VIEW exam_assessment_current_defaults_v2 AS
SELECT selection.assessment_id,
       selection.selected_assessment_version_id AS assessment_version_id,
       selection.public_id AS selection_public_id,
       selection.revision AS selection_revision,
       selection.selected_by,
       selection.selected_at
FROM exam_assessment_default_version_selections_v2 selection
WHERE selection.revision=(
  SELECT MAX(latest.revision)
  FROM exam_assessment_default_version_selections_v2 latest
  WHERE latest.assessment_id=selection.assessment_id
);

CREATE TRIGGER trg_exam_assessment_default_selection_insert_v2
BEFORE INSERT ON exam_assessment_default_version_selections_v2
BEGIN
  SELECT CASE WHEN NEW.revision <> (
    SELECT COALESCE(MAX(selection.revision),0)+1
    FROM exam_assessment_default_version_selections_v2 selection
    WHERE selection.assessment_id=NEW.assessment_id
  ) THEN RAISE(ABORT,'M2_DEFAULT_SELECTION_REVISION_MISMATCH') END;

  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_assessments_v2 assessment
    JOIN exam_assessment_versions_v2 previous
      ON previous.id=NEW.previous_assessment_version_id
     AND previous.assessment_id=assessment.id
     AND previous.state='confirmed'
    JOIN exam_assessment_versions_v2 selected
      ON selected.id=NEW.selected_assessment_version_id
     AND selected.assessment_id=assessment.id
     AND selected.state='confirmed'
     AND selected.supersedes_version_id=previous.id
     AND selected.revision>previous.revision
    JOIN exam_question_version_impact_plans_v2 plan
      ON plan.id=NEW.source_impact_plan_id
     AND plan.planned_by=NEW.selected_by
    JOIN exam_assessment_items_v2 target_item
      ON target_item.assessment_version_id=selected.id
     AND target_item.question_version_id=plan.question_version_id
     AND target_item.answer_key_version_id=plan.target_answer_key_version_id
     AND target_item.rubric_version_id=plan.target_rubric_version_id
     AND target_item.link_set_id=plan.target_link_set_id
     AND target_item.state='active'
    WHERE assessment.id=NEW.assessment_id
      AND assessment.created_by=NEW.selected_by
  ) THEN RAISE(ABORT,'M2_DEFAULT_SELECTION_SCOPE_MISMATCH') END;

  SELECT CASE WHEN EXISTS (
    SELECT 1 FROM exam_assessment_default_version_selections_v2 current
    WHERE current.assessment_id=NEW.assessment_id
  ) AND NEW.previous_assessment_version_id <> (
    SELECT current.selected_assessment_version_id
    FROM exam_assessment_default_version_selections_v2 current
    WHERE current.assessment_id=NEW.assessment_id
    ORDER BY current.revision DESC,current.id DESC
    LIMIT 1
  ) THEN RAISE(ABORT,'M2_DEFAULT_SELECTION_STALE') END;

  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM exam_assessment_default_version_selections_v2 current
    WHERE current.assessment_id=NEW.assessment_id
  ) AND NEW.previous_assessment_version_id <> (
    SELECT version.id
    FROM exam_assessment_versions_v2 version
    WHERE version.assessment_id=NEW.assessment_id
      AND version.state='confirmed'
      AND version.id<>NEW.selected_assessment_version_id
    ORDER BY version.revision DESC,version.id DESC
    LIMIT 1
  ) THEN RAISE(ABORT,'M2_DEFAULT_SELECTION_STALE') END;
END;

CREATE TRIGGER trg_exam_assessment_default_selection_update_v2
BEFORE UPDATE ON exam_assessment_default_version_selections_v2
BEGIN SELECT RAISE(ABORT,'M2_DEFAULT_SELECTION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_assessment_default_selection_delete_v2
BEFORE DELETE ON exam_assessment_default_version_selections_v2
BEGIN SELECT RAISE(ABORT,'M2_DEFAULT_SELECTION_IMMUTABLE'); END;
