-- K1-5 / M2.5-3：题目版本变更影响计划与复核清单。
--
-- 题目表现统计保持只读；老师确认影响处理时只冻结计划和待办清单。
-- 本迁移不允许直接修改 assessment item、grade decision、publication 或 learning evidence。

CREATE TABLE exam_question_version_impact_plans_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE CHECK(length(trim(request_key)) > 0),
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  target_answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  target_rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  target_link_set_id INTEGER NOT NULL REFERENCES k1_link_sets(id),
  expected_preview_hash TEXT NOT NULL CHECK(length(expected_preview_hash) = 64),
  action TEXT NOT NULL CHECK(action IN (
    'future_only','recalculate_unpublished','review_published'
  )),
  impact_json TEXT NOT NULL,
  planned_by TEXT NOT NULL CHECK(length(trim(planned_by)) > 0),
  planned_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(impact_json), 0) = 1),
  CHECK(COALESCE(json_type(impact_json), 0) = 'object'),
  CHECK(COALESCE(json_type(impact_json, '$.schemaVersion'), 0) = 'integer')
);

CREATE TABLE exam_question_version_impact_tasks_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  impact_plan_id INTEGER NOT NULL REFERENCES exam_question_version_impact_plans_v2(id),
  attempt_id INTEGER NOT NULL REFERENCES exam_attempts_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  publication_id INTEGER REFERENCES exam_grade_publications_v2(id),
  task_kind TEXT NOT NULL CHECK(task_kind IN (
    'recalculate_unpublished','review_published'
  )),
  source_answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  source_rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  source_link_set_id INTEGER NOT NULL REFERENCES k1_link_sets(id),
  target_answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  target_rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  target_link_set_id INTEGER NOT NULL REFERENCES k1_link_sets(id),
  state TEXT NOT NULL DEFAULT 'open' CHECK(state = 'open'),
  created_at TEXT NOT NULL,
  UNIQUE(impact_plan_id, attempt_id, assessment_item_id),
  CHECK((task_kind='review_published' AND publication_id IS NOT NULL)
        OR (task_kind='recalculate_unpublished' AND publication_id IS NULL))
);

CREATE INDEX idx_exam_question_impact_plan_scope_v2
  ON exam_question_version_impact_plans_v2(question_version_id,planned_at DESC,id DESC);
CREATE INDEX idx_exam_question_impact_task_scope_v2
  ON exam_question_version_impact_tasks_v2(task_kind,state,attempt_id,assessment_item_id);

CREATE TRIGGER trg_exam_question_impact_plan_scope_insert_v2
BEFORE INSERT ON exam_question_version_impact_plans_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM k1_question_versions question
    JOIN k1_answer_key_versions answer
      ON answer.id=NEW.target_answer_key_version_id
     AND answer.question_version_id=question.id
     AND answer.state='confirmed'
    JOIN k1_rubric_versions rubric
      ON rubric.id=NEW.target_rubric_version_id
     AND rubric.question_version_id=question.id
     AND rubric.state='confirmed'
    JOIN k1_link_sets links
      ON links.id=NEW.target_link_set_id
     AND links.question_version_id=question.id
     AND links.state='confirmed'
    WHERE question.id=NEW.question_version_id
  ) THEN RAISE(ABORT,'K1_IMPACT_TARGET_VERSION_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_question_impact_task_scope_insert_v2
BEFORE INSERT ON exam_question_version_impact_tasks_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_question_version_impact_plans_v2 plan
    JOIN exam_attempts_v2 attempt ON attempt.id=NEW.attempt_id
    JOIN exam_assessment_items_v2 item
      ON item.id=NEW.assessment_item_id
     AND item.assessment_version_id=attempt.assessment_version_id
     AND item.question_version_id=plan.question_version_id
     AND item.answer_key_version_id=NEW.source_answer_key_version_id
     AND item.rubric_version_id=NEW.source_rubric_version_id
     AND item.link_set_id=NEW.source_link_set_id
    WHERE plan.id=NEW.impact_plan_id
      AND plan.target_answer_key_version_id=NEW.target_answer_key_version_id
      AND plan.target_rubric_version_id=NEW.target_rubric_version_id
      AND plan.target_link_set_id=NEW.target_link_set_id
      AND (
        (NEW.task_kind='recalculate_unpublished'
         AND attempt.active_publication_id IS NULL
         AND attempt.state<>'voided')
        OR
        (NEW.task_kind='review_published'
         AND attempt.active_publication_id=NEW.publication_id
         AND EXISTS (
           SELECT 1 FROM exam_grade_publications_v2 publication
           WHERE publication.id=NEW.publication_id AND publication.state='published'
         ))
      )
  ) THEN RAISE(ABORT,'K1_IMPACT_TASK_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_question_impact_plan_update_v2
BEFORE UPDATE ON exam_question_version_impact_plans_v2
BEGIN SELECT RAISE(ABORT,'K1_IMPACT_PLAN_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_question_impact_plan_delete_v2
BEFORE DELETE ON exam_question_version_impact_plans_v2
BEGIN SELECT RAISE(ABORT,'K1_IMPACT_PLAN_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_question_impact_task_update_v2
BEFORE UPDATE ON exam_question_version_impact_tasks_v2
BEGIN SELECT RAISE(ABORT,'K1_IMPACT_TASK_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_question_impact_task_delete_v2
BEFORE DELETE ON exam_question_version_impact_tasks_v2
BEGIN SELECT RAISE(ABORT,'K1_IMPACT_TASK_IMMUTABLE'); END;
