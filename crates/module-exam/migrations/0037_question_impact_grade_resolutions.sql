-- M2.5-3c：老师逐条处理版本影响 case，显式创建新 grade revision。
--
-- resolution 冻结老师在目标 answer/rubric/link 下确认的分数和逐项证据。它不会自动
-- 创建 publication；旧 publication 及其正式 learning evidence 要等老师再次发布时
-- 才在同一事务中切换。

CREATE TABLE exam_question_version_review_resolutions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE CHECK(length(trim(request_key)) > 0),
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  review_case_id INTEGER NOT NULL UNIQUE
    REFERENCES exam_question_version_review_cases_v2(id),
  grade_decision_id INTEGER NOT NULL UNIQUE REFERENCES exam_grade_decisions_v2(id),
  expected_source_snapshot_hash TEXT NOT NULL CHECK(length(expected_source_snapshot_hash) = 64),
  component_results_json TEXT NOT NULL,
  teacher_note TEXT NOT NULL CHECK(length(trim(teacher_note)) > 0),
  resolved_by TEXT NOT NULL CHECK(length(trim(resolved_by)) > 0),
  resolved_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(component_results_json), 0) = 1),
  CHECK(COALESCE(json_type(component_results_json), 0) = 'object'),
  CHECK(COALESCE(json_type(component_results_json, '$.schema_version'), 0) = 'integer'),
  CHECK(COALESCE(json_type(component_results_json, '$.components'), 0) = 'array')
);

CREATE TRIGGER trg_exam_question_review_resolution_scope_insert_v2
BEFORE INSERT ON exam_question_version_review_resolutions_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_question_version_review_cases_v2 review_case
    JOIN exam_question_version_impact_tasks_v2 task
      ON task.id=review_case.impact_task_id
    JOIN exam_question_version_impact_plans_v2 plan
      ON plan.id=task.impact_plan_id
    JOIN exam_attempts_v2 attempt
      ON attempt.id=review_case.attempt_id
    JOIN exam_assessment_items_v2 item
      ON item.id=review_case.assessment_item_id
     AND item.assessment_version_id=attempt.assessment_version_id
    JOIN k1_question_versions question
      ON question.id=item.question_version_id
    JOIN exam_grade_decisions_v2 decision
      ON decision.id=NEW.grade_decision_id
     AND decision.attempt_id=attempt.id
     AND decision.assessment_item_id=item.id
     AND decision.state='active'
     AND decision.confirmation_level='teacher_corrected'
     AND decision.machine_grade_ai_run_id IS NULL
     AND decision.decided_by=NEW.resolved_by
     AND decision.teacher_note=NEW.teacher_note
    WHERE review_case.id=NEW.review_case_id
      AND review_case.state='open'
      AND plan.planned_by=NEW.resolved_by
      AND review_case.source_snapshot_hash=NEW.expected_source_snapshot_hash
      AND COALESCE(json_extract(decision.point_results_json, '$.review_case_public_id'),'')
          =review_case.public_id
      AND COALESCE(json_extract(decision.point_results_json, '$.target_answer_key_version_id'),0)
          =review_case.target_answer_key_version_id
      AND COALESCE(json_extract(decision.point_results_json, '$.target_rubric_version_id'),0)
          =review_case.target_rubric_version_id
      AND COALESCE(json_extract(decision.point_results_json, '$.target_link_set_id'),0)
          =review_case.target_link_set_id
      AND decision.revision=COALESCE(review_case.source_grade_decision_revision,0)+1
      AND (
        (review_case.case_kind='unpublished_recalculation'
         AND attempt.active_publication_id IS NULL)
        OR
        (review_case.case_kind='published_review'
         AND attempt.active_publication_id=review_case.publication_id
         AND EXISTS (
           SELECT 1
           FROM exam_grade_publications_v2 publication
           JOIN exam_grade_publication_items_v2 publication_item
             ON publication_item.publication_id=publication.id
            AND publication_item.attempt_id=attempt.id
           JOIN exam_grade_publication_decisions_v2 publication_decision
             ON publication_decision.publication_item_id=publication_item.id
            AND publication_decision.attempt_id=attempt.id
            AND publication_decision.grade_decision_id=review_case.source_grade_decision_id
           WHERE publication.id=review_case.publication_id
             AND publication.state='published'
         ))
      )
      AND (
        (question.question_type IN ('single','multiple','true_false')
         AND json_array_length(NEW.component_results_json,'$.components')=0)
        OR
        (question.question_type='fill_blank'
         AND json_array_length(NEW.component_results_json,'$.components')=(
           SELECT COUNT(*) FROM k1_answer_slots source
           WHERE source.answer_key_version_id=review_case.target_answer_key_version_id
         )
         AND ABS((
           SELECT COALESCE(SUM(source.max_score),0)
           FROM k1_answer_slots source
           WHERE source.answer_key_version_id=review_case.target_answer_key_version_id
         )-item.score)<=0.000001
         AND NOT EXISTS (
           SELECT 1 FROM json_each(NEW.component_results_json,'$.components') component
           WHERE COALESCE(json_extract(component.value,'$.source_type'),'')<>'answer_slot'
              OR NOT EXISTS (
                SELECT 1 FROM k1_answer_slots source
                WHERE source.answer_key_version_id=review_case.target_answer_key_version_id
                  AND source.public_id=json_extract(component.value,'$.source_public_id')
                  AND source.stable_id=json_extract(component.value,'$.stable_id')
                  AND source.order_index=json_extract(component.value,'$.order_index')
                  AND ABS(source.max_score-json_extract(component.value,'$.max_score'))<=0.000001
              )
         ))
        OR
        (question.question_type='short_answer'
         AND json_array_length(NEW.component_results_json,'$.components')=(
           SELECT COUNT(*) FROM k1_rubric_points source
           WHERE source.rubric_version_id=review_case.target_rubric_version_id
         )
         AND ABS((
           SELECT COALESCE(SUM(source.max_score),0)
           FROM k1_rubric_points source
           WHERE source.rubric_version_id=review_case.target_rubric_version_id
         )-item.score)<=0.000001
         AND NOT EXISTS (
           SELECT 1 FROM json_each(NEW.component_results_json,'$.components') component
           WHERE COALESCE(json_extract(component.value,'$.source_type'),'')<>'rubric_point'
              OR NOT EXISTS (
                SELECT 1 FROM k1_rubric_points source
                WHERE source.rubric_version_id=review_case.target_rubric_version_id
                  AND source.public_id=json_extract(component.value,'$.source_public_id')
                  AND source.stable_id=json_extract(component.value,'$.stable_id')
                  AND source.order_index=json_extract(component.value,'$.order_index')
                  AND ABS(source.max_score-json_extract(component.value,'$.max_score'))<=0.000001
              )
         ))
      )
      AND (
        SELECT COUNT(DISTINCT json_extract(component.value,'$.source_public_id'))
        FROM json_each(NEW.component_results_json,'$.components') component
      )=json_array_length(NEW.component_results_json,'$.components')
      AND NOT EXISTS (
        SELECT 1 FROM json_each(NEW.component_results_json,'$.components') component
        WHERE COALESCE(json_type(component.value,'$.teacher_score'),'')
              NOT IN ('integer','real')
           OR json_extract(component.value,'$.teacher_score')<0
           OR json_extract(component.value,'$.teacher_score')
              >json_extract(component.value,'$.max_score')+0.000001
           OR COALESCE(json_extract(component.value,'$.result_status'),'')
              NOT IN ('correct','partial','incorrect')
           OR (
             json_extract(component.value,'$.result_status')='correct'
             AND ABS(json_extract(component.value,'$.teacher_score')
                     -json_extract(component.value,'$.max_score'))>0.000001
           )
           OR (
             json_extract(component.value,'$.result_status')='partial'
             AND (
               json_extract(component.value,'$.teacher_score')<=0.000001
               OR json_extract(component.value,'$.teacher_score')
                  >=json_extract(component.value,'$.max_score')-0.000001
             )
           )
           OR (
             json_extract(component.value,'$.result_status')='incorrect'
             AND json_extract(component.value,'$.teacher_score')>0.000001
           )
           OR (
             json_extract(component.value,'$.teacher_score')>0.000001
             AND length(trim(COALESCE(json_extract(component.value,'$.evidence_text'),'')))=0
           )
      )
      AND ABS(
        COALESCE((
          SELECT SUM(json_extract(component.value,'$.teacher_score'))
          FROM json_each(NEW.component_results_json,'$.components') component
        ),decision.teacher_score)
        -decision.teacher_score
      )<=0.000001
  ) THEN RAISE(ABORT,'K1_IMPACT_REVIEW_RESOLUTION_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_question_review_resolution_update_v2
BEFORE UPDATE ON exam_question_version_review_resolutions_v2
BEGIN SELECT RAISE(ABORT,'K1_IMPACT_REVIEW_RESOLUTION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_question_review_resolution_delete_v2
BEFORE DELETE ON exam_question_version_review_resolutions_v2
BEGIN SELECT RAISE(ABORT,'K1_IMPACT_REVIEW_RESOLUTION_IMMUTABLE'); END;
