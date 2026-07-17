-- M2-B3c5：老师把一次已判满分的填空写法显式加入未来答案版本。
-- 本次评分 revision、当前作业版本、历史发布与学习证据均保持不变；这里只追加
-- 新的 K1 答案版本、新的 assessment version 和不可变提升账本。

CREATE TABLE exam_accepted_answer_promotions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  grade_decision_id INTEGER NOT NULL UNIQUE REFERENCES exam_grade_decisions_v2(id),
  suggestion_id INTEGER NOT NULL REFERENCES exam_subjective_grade_suggestions_v2(id),
  transcription_revision_id INTEGER NOT NULL
    REFERENCES exam_subjective_transcription_revisions_v2(id),
  assessment_id INTEGER NOT NULL REFERENCES exam_assessments_v2(id),
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  source_assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  source_assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  source_answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  base_assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  base_assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  base_answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  adopted_assessment_version_id INTEGER NOT NULL UNIQUE REFERENCES exam_assessment_versions_v2(id),
  adopted_assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  adopted_answer_key_version_id INTEGER NOT NULL UNIQUE REFERENCES k1_answer_key_versions(id),
  answer_slot_stable_id TEXT NOT NULL CHECK(length(trim(answer_slot_stable_id)) > 0),
  accepted_text TEXT NOT NULL CHECK(length(trim(accepted_text)) > 0),
  normalized_text TEXT NOT NULL CHECK(length(trim(normalized_text)) > 0),
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by)) > 0),
  created_at TEXT NOT NULL,
  UNIQUE(assessment_id, question_version_id, normalized_text)
);

CREATE TRIGGER trg_exam_accepted_answer_promotion_scope_insert_v2
BEFORE INSERT ON exam_accepted_answer_promotions_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_grade_decisions_v2 decision
    JOIN exam_grade_decision_subjective_sources_v2 decision_source
      ON decision_source.grade_decision_id=decision.id
    JOIN exam_subjective_grade_suggestions_v2 suggestion
      ON suggestion.id=decision_source.suggestion_id
    JOIN exam_subjective_transcription_revisions_v2 transcription
      ON transcription.id=decision_source.transcription_revision_id
     AND transcription.id=suggestion.transcription_revision_id
    JOIN exam_attempts_v2 attempt ON attempt.id=decision.attempt_id
    JOIN exam_assessment_items_v2 source_item
      ON source_item.id=decision.assessment_item_id
     AND source_item.assessment_version_id=attempt.assessment_version_id
    JOIN exam_assessment_versions_v2 source_version
      ON source_version.id=source_item.assessment_version_id
    JOIN k1_question_versions question
      ON question.id=source_item.question_version_id AND question.question_type='fill_blank'
    WHERE decision.id=NEW.grade_decision_id AND decision.state='active'
      AND decision.confirmation_level='teacher_corrected'
      AND ABS(decision.teacher_score-source_item.score) <= 0.000001
      AND suggestion.id=NEW.suggestion_id AND suggestion.state='active'
      AND transcription.id=NEW.transcription_revision_id AND transcription.state='active'
      AND transcription.result_state='recognized'
      AND source_version.assessment_id=NEW.assessment_id
      AND source_version.id=NEW.source_assessment_version_id
      AND source_item.id=NEW.source_assessment_item_id
      AND source_item.question_version_id=NEW.question_version_id
      AND source_item.answer_key_version_id=NEW.source_answer_key_version_id
  ) THEN RAISE(ABORT,'M2_ACCEPTED_ANSWER_SOURCE_SCOPE_MISMATCH') END;

  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_assessment_versions_v2 base_version
    JOIN exam_assessment_items_v2 base_item
      ON base_item.assessment_version_id=base_version.id
    JOIN k1_answer_key_versions base_answer
      ON base_answer.id=base_item.answer_key_version_id
    JOIN exam_assessment_versions_v2 adopted_version
      ON adopted_version.id=NEW.adopted_assessment_version_id
     AND adopted_version.assessment_id=base_version.assessment_id
     AND adopted_version.supersedes_version_id=base_version.id
    JOIN exam_assessment_items_v2 adopted_item
      ON adopted_item.id=NEW.adopted_assessment_item_id
     AND adopted_item.assessment_version_id=adopted_version.id
    JOIN k1_answer_key_versions adopted_answer
      ON adopted_answer.id=NEW.adopted_answer_key_version_id
     AND adopted_answer.id=adopted_item.answer_key_version_id
     AND adopted_answer.question_version_id=base_answer.question_version_id
     AND adopted_answer.supersedes_answer_key_id=base_answer.id
    WHERE base_version.id=NEW.base_assessment_version_id
      AND base_version.assessment_id=NEW.assessment_id
      AND base_item.id=NEW.base_assessment_item_id
      AND base_item.question_version_id=NEW.question_version_id
      AND base_answer.id=NEW.base_answer_key_version_id
      AND adopted_item.question_version_id=NEW.question_version_id
  ) THEN RAISE(ABORT,'M2_ACCEPTED_ANSWER_ADOPTION_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_accepted_answer_promotion_update_v2
BEFORE UPDATE ON exam_accepted_answer_promotions_v2
BEGIN SELECT RAISE(ABORT,'M2_ACCEPTED_ANSWER_PROMOTION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_accepted_answer_promotion_delete_v2
BEFORE DELETE ON exam_accepted_answer_promotions_v2
BEGIN SELECT RAISE(ABORT,'M2_ACCEPTED_ANSWER_PROMOTION_IMMUTABLE'); END;
