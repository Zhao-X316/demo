-- M2.5-3b：消费已冻结的版本影响 task，建立不可变重评准备/已发布复核 case。
--
-- case 只冻结当前老师评分/发布快照与目标 answer/rubric/link 版本。它不会更新
-- assessment item、attempt、grade decision、publication 或 learning evidence。

CREATE TABLE exam_question_version_review_cases_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  impact_task_id INTEGER NOT NULL UNIQUE
    REFERENCES exam_question_version_impact_tasks_v2(id),
  case_kind TEXT NOT NULL CHECK(case_kind IN (
    'unpublished_recalculation','published_review'
  )),
  attempt_id INTEGER NOT NULL REFERENCES exam_attempts_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  publication_id INTEGER REFERENCES exam_grade_publications_v2(id),
  source_grade_decision_id INTEGER REFERENCES exam_grade_decisions_v2(id),
  source_grade_decision_revision INTEGER,
  source_teacher_score REAL,
  source_point_results_json TEXT,
  source_snapshot_hash TEXT NOT NULL CHECK(length(source_snapshot_hash) = 64),
  target_answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  target_rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  target_link_set_id INTEGER NOT NULL REFERENCES k1_link_sets(id),
  prepared_by TEXT NOT NULL CHECK(length(trim(prepared_by)) > 0),
  prepared_at TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'open' CHECK(state = 'open'),
  CHECK(
    (source_grade_decision_id IS NULL
     AND source_grade_decision_revision IS NULL
     AND source_teacher_score IS NULL
     AND source_point_results_json IS NULL)
    OR
    (source_grade_decision_id IS NOT NULL
     AND source_grade_decision_revision IS NOT NULL
     AND source_teacher_score IS NOT NULL
     AND source_point_results_json IS NOT NULL
     AND source_grade_decision_revision > 0
     AND source_teacher_score >= 0
     AND COALESCE(json_valid(source_point_results_json), 0) = 1
     AND COALESCE(json_type(source_point_results_json), 0) = 'object')
  ),
  CHECK((case_kind='published_review' AND publication_id IS NOT NULL
         AND source_grade_decision_id IS NOT NULL)
        OR (case_kind='unpublished_recalculation' AND publication_id IS NULL))
);

CREATE INDEX idx_exam_question_review_case_scope_v2
  ON exam_question_version_review_cases_v2(case_kind,state,prepared_at,id);

CREATE TRIGGER trg_exam_question_review_case_scope_insert_v2
BEFORE INSERT ON exam_question_version_review_cases_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_question_version_impact_tasks_v2 task
    JOIN exam_question_version_impact_plans_v2 plan ON plan.id=task.impact_plan_id
    JOIN exam_attempts_v2 attempt
      ON attempt.id=NEW.attempt_id
     AND attempt.id=task.attempt_id
    JOIN exam_assessment_items_v2 item
      ON item.id=NEW.assessment_item_id
     AND item.id=task.assessment_item_id
     AND item.assessment_version_id=attempt.assessment_version_id
     AND item.question_version_id=plan.question_version_id
    WHERE task.id=NEW.impact_task_id
      AND plan.planned_by=NEW.prepared_by
      AND NEW.target_answer_key_version_id=task.target_answer_key_version_id
      AND NEW.target_rubric_version_id=task.target_rubric_version_id
      AND NEW.target_link_set_id=task.target_link_set_id
      AND (
        (NEW.case_kind='unpublished_recalculation'
         AND task.task_kind='recalculate_unpublished'
         AND NEW.publication_id IS NULL
         AND attempt.active_publication_id IS NULL
         AND attempt.state<>'voided'
         AND (
           (NEW.source_grade_decision_id IS NULL AND NOT EXISTS (
             SELECT 1 FROM exam_grade_decisions_v2 current_decision
             WHERE current_decision.attempt_id=attempt.id
               AND current_decision.assessment_item_id=item.id
               AND current_decision.state='active'
           ))
           OR
           EXISTS (
             SELECT 1 FROM exam_grade_decisions_v2 current_decision
             WHERE current_decision.id=NEW.source_grade_decision_id
               AND current_decision.attempt_id=attempt.id
               AND current_decision.assessment_item_id=item.id
               AND current_decision.revision=NEW.source_grade_decision_revision
               AND current_decision.teacher_score=NEW.source_teacher_score
               AND current_decision.point_results_json=NEW.source_point_results_json
               AND current_decision.state='active'
           )
         ))
        OR
        (NEW.case_kind='published_review'
         AND task.task_kind='review_published'
         AND NEW.publication_id=task.publication_id
         AND attempt.active_publication_id=NEW.publication_id
         AND EXISTS (
           SELECT 1
           FROM exam_grade_publications_v2 publication
           JOIN exam_grade_publication_items_v2 publication_item
             ON publication_item.publication_id=publication.id
            AND publication_item.attempt_id=attempt.id
           JOIN exam_grade_publication_decisions_v2 publication_decision
             ON publication_decision.publication_item_id=publication_item.id
            AND publication_decision.attempt_id=attempt.id
           JOIN exam_grade_decisions_v2 published_decision
             ON published_decision.id=publication_decision.grade_decision_id
            AND published_decision.id=NEW.source_grade_decision_id
            AND published_decision.assessment_item_id=item.id
            AND published_decision.revision=NEW.source_grade_decision_revision
            AND published_decision.teacher_score=NEW.source_teacher_score
            AND published_decision.point_results_json=NEW.source_point_results_json
           WHERE publication.id=NEW.publication_id
             AND publication.state='published'
         ))
      )
  ) THEN RAISE(ABORT,'K1_IMPACT_REVIEW_CASE_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_question_review_case_update_v2
BEFORE UPDATE ON exam_question_version_review_cases_v2
BEGIN SELECT RAISE(ABORT,'K1_IMPACT_REVIEW_CASE_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_question_review_case_delete_v2
BEFORE DELETE ON exam_question_version_review_cases_v2
BEGIN SELECT RAISE(ABORT,'K1_IMPACT_REVIEW_CASE_IMMUTABLE'); END;
