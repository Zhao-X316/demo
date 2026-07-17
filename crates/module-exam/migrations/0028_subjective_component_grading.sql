-- M2-B3a5：主观题老师逐槽/逐评分点终审账本。
-- 整题 grade decision 仍是唯一当前成绩；本表只保存该 revision 的可解释组成，
-- 供发布后按已确认 answer_slot / rubric_point 形成正式学习证据。

CREATE TABLE exam_grade_decision_subjective_components_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  grade_decision_id INTEGER NOT NULL REFERENCES exam_grade_decisions_v2(id),
  source_type TEXT NOT NULL CHECK(source_type IN ('answer_slot','rubric_point')),
  source_public_id TEXT NOT NULL CHECK(length(trim(source_public_id)) > 0),
  stable_id TEXT NOT NULL CHECK(length(trim(stable_id)) > 0),
  order_index INTEGER NOT NULL CHECK(order_index >= 0),
  teacher_score REAL NOT NULL CHECK(teacher_score >= 0.0),
  max_score REAL NOT NULL CHECK(max_score > 0.0),
  result_status TEXT NOT NULL CHECK(result_status IN ('correct','partial','incorrect')),
  evidence_text TEXT,
  teacher_note TEXT,
  created_at TEXT NOT NULL,
  CHECK(teacher_score <= max_score + 0.000001),
  CHECK((teacher_score <= 0.000001 AND result_status='incorrect')
        OR (teacher_score >= max_score - 0.000001 AND result_status='correct')
        OR (teacher_score > 0.000001 AND teacher_score < max_score - 0.000001
            AND result_status='partial')),
  CHECK(evidence_text IS NULL OR length(trim(evidence_text)) > 0),
  CHECK(teacher_note IS NULL OR length(trim(teacher_note)) > 0),
  UNIQUE(grade_decision_id,source_type,source_public_id)
);

CREATE TRIGGER trg_exam_subjective_component_scope_insert_v2
BEFORE INSERT ON exam_grade_decision_subjective_components_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_grade_decisions_v2 decision
    JOIN exam_grade_decision_subjective_sources_v2 decision_source
      ON decision_source.grade_decision_id=decision.id
    JOIN exam_subjective_grade_suggestions_v2 suggestion
      ON suggestion.id=decision_source.suggestion_id
    JOIN exam_assessment_items_v2 item
      ON item.id=decision.assessment_item_id
    JOIN k1_question_versions question
      ON question.id=item.question_version_id
    WHERE decision.id=NEW.grade_decision_id
      AND decision.state='active'
      AND decision.confirmation_level='teacher_corrected'
      AND decision.attempt_id=suggestion.attempt_id
      AND decision.assessment_item_id=suggestion.assessment_item_id
      AND (
        (NEW.source_type='answer_slot' AND question.question_type='fill_blank'
         AND EXISTS (
           SELECT 1 FROM k1_answer_slots source
           WHERE source.public_id=NEW.source_public_id
             AND source.answer_key_version_id=item.answer_key_version_id
             AND source.stable_id=NEW.stable_id
             AND source.order_index=NEW.order_index
             AND ABS(source.max_score-NEW.max_score) <= 0.000001
         ))
        OR
        (NEW.source_type='rubric_point' AND question.question_type='short_answer'
         AND EXISTS (
           SELECT 1 FROM k1_rubric_points source
           WHERE source.public_id=NEW.source_public_id
             AND source.rubric_version_id=item.rubric_version_id
             AND source.stable_id=NEW.stable_id
             AND source.order_index=NEW.order_index
             AND ABS(source.max_score-NEW.max_score) <= 0.000001
         ))
      )
  ) THEN RAISE(ABORT,'M2_SUBJECTIVE_COMPONENT_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_subjective_component_update_v2
BEFORE UPDATE ON exam_grade_decision_subjective_components_v2
BEGIN SELECT RAISE(ABORT,'M2_SUBJECTIVE_COMPONENT_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_subjective_component_delete_v2
BEFORE DELETE ON exam_grade_decision_subjective_components_v2
BEGIN SELECT RAISE(ABORT,'M2_SUBJECTIVE_COMPONENT_IMMUTABLE'); END;
