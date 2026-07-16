-- M2-B3a3：答题卡填空/简答题机器建议与老师终审来源。
-- 填空题只做已确认答案的确定性精确匹配；简答题在独立 AI 评分接入前保持 unscored。
-- suggestion 不等于成绩，只有老师显式接受/修正才写 exam_grade_decisions_v2。

CREATE TABLE exam_subjective_grade_suggestions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  transcription_revision_id INTEGER NOT NULL UNIQUE
    REFERENCES exam_subjective_transcription_revisions_v2(id),
  attempt_id INTEGER NOT NULL REFERENCES exam_attempts_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  machine_grade_ai_run_id INTEGER REFERENCES ai_runs(id),
  outcome TEXT NOT NULL CHECK(outcome IN ('correct','incorrect','partial','unscored')),
  suggested_score REAL,
  result_json TEXT NOT NULL,
  confidence REAL CHECK(confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
  batch_eligible INTEGER NOT NULL DEFAULT 0 CHECK(batch_eligible IN (0,1)),
  exclusion_reason TEXT,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  CHECK((outcome='unscored' AND suggested_score IS NULL)
        OR (outcome<>'unscored' AND suggested_score IS NOT NULL AND suggested_score >= 0.0)),
  CHECK(COALESCE(json_valid(result_json),0)=1),
  CHECK(COALESCE(json_type(result_json),0)='object'),
  CHECK(COALESCE(json_type(result_json,'$.schema_version'),0)='integer'),
  CHECK((batch_eligible=1 AND outcome='correct' AND exclusion_reason IS NULL)
        OR batch_eligible=0)
);
CREATE UNIQUE INDEX idx_exam_subjective_suggestion_active_v2
  ON exam_subjective_grade_suggestions_v2(attempt_id,assessment_item_id)
  WHERE state='active';

CREATE TABLE exam_grade_decision_subjective_sources_v2 (
  grade_decision_id INTEGER PRIMARY KEY REFERENCES exam_grade_decisions_v2(id),
  suggestion_id INTEGER NOT NULL REFERENCES exam_subjective_grade_suggestions_v2(id),
  transcription_revision_id INTEGER NOT NULL
    REFERENCES exam_subjective_transcription_revisions_v2(id),
  review_mode TEXT NOT NULL CHECK(review_mode IN ('single','teacher_corrected')),
  reviewed_by TEXT NOT NULL CHECK(length(trim(reviewed_by)) > 0),
  created_at TEXT NOT NULL
);

CREATE TRIGGER trg_exam_subjective_suggestion_scope_insert_v2
BEFORE INSERT ON exam_subjective_grade_suggestions_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_subjective_transcription_revisions_v2 t
    JOIN exam_attempts_v2 attempt
      ON attempt.id=t.attempt_id AND attempt.state<>'voided'
    JOIN exam_assessment_items_v2 item
      ON item.id=t.assessment_item_id
     AND item.assessment_version_id=attempt.assessment_version_id
     AND item.state='active'
    JOIN k1_question_versions question
      ON question.id=item.question_version_id AND question.state='published'
    JOIN k1_answer_key_versions answer
      ON answer.id=item.answer_key_version_id AND answer.state='confirmed'
    JOIN k1_rubric_versions rubric
      ON rubric.id=item.rubric_version_id AND rubric.state='confirmed'
    WHERE t.id=NEW.transcription_revision_id AND t.state='active'
      AND t.attempt_id=NEW.attempt_id
      AND t.assessment_item_id=NEW.assessment_item_id
      AND question.question_type=t.question_type
      AND question.question_type IN ('fill_blank','short_answer')
      AND answer.id=NEW.answer_key_version_id
      AND rubric.id=NEW.rubric_version_id
  ) THEN RAISE(ABORT,'M2_SUBJECTIVE_SUGGESTION_SCOPE_MISMATCH') END;
  SELECT CASE WHEN NEW.machine_grade_ai_run_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM ai_runs run
    WHERE run.id=NEW.machine_grade_ai_run_id
      AND run.run_type='answer_grade' AND run.status='succeeded'
  ) THEN RAISE(ABORT,'M2_SUBJECTIVE_GRADE_RUN_INVALID') END;
END;

CREATE TRIGGER trg_exam_subjective_suggestion_score_insert_v2
BEFORE INSERT ON exam_subjective_grade_suggestions_v2
WHEN NEW.suggested_score IS NOT NULL
BEGIN
  SELECT CASE WHEN NEW.suggested_score > (
    SELECT item.score FROM exam_assessment_items_v2 item
    WHERE item.id=NEW.assessment_item_id
  ) + 0.000001
  THEN RAISE(ABORT,'M2_SUBJECTIVE_SCORE_EXCEEDS_ITEM') END;
END;

CREATE TRIGGER trg_exam_subjective_source_scope_insert_v2
BEFORE INSERT ON exam_grade_decision_subjective_sources_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_grade_decisions_v2 decision
    JOIN exam_subjective_grade_suggestions_v2 suggestion
      ON suggestion.id=NEW.suggestion_id
    JOIN exam_subjective_transcription_revisions_v2 transcription
      ON transcription.id=NEW.transcription_revision_id
     AND transcription.id=suggestion.transcription_revision_id
    WHERE decision.id=NEW.grade_decision_id AND decision.state='active'
      AND suggestion.state='active' AND transcription.state='active'
      AND decision.attempt_id=suggestion.attempt_id
      AND decision.assessment_item_id=suggestion.assessment_item_id
  ) THEN RAISE(ABORT,'M2_SUBJECTIVE_GRADE_SOURCE_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_subjective_suggestion_content_guard_v2
BEFORE UPDATE ON exam_subjective_grade_suggestions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.transcription_revision_id IS NOT OLD.transcription_revision_id
  OR NEW.attempt_id IS NOT OLD.attempt_id
  OR NEW.assessment_item_id IS NOT OLD.assessment_item_id
  OR NEW.answer_key_version_id IS NOT OLD.answer_key_version_id
  OR NEW.rubric_version_id IS NOT OLD.rubric_version_id
  OR NEW.machine_grade_ai_run_id IS NOT OLD.machine_grade_ai_run_id
  OR NEW.outcome IS NOT OLD.outcome
  OR NEW.suggested_score IS NOT OLD.suggested_score
  OR NEW.result_json IS NOT OLD.result_json
  OR NEW.confidence IS NOT OLD.confidence
  OR NEW.batch_eligible IS NOT OLD.batch_eligible
  OR NEW.exclusion_reason IS NOT OLD.exclusion_reason
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_SUBJECTIVE_SUGGESTION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_subjective_suggestion_delete_v2
BEFORE DELETE ON exam_subjective_grade_suggestions_v2
BEGIN SELECT RAISE(ABORT,'M2_SUBJECTIVE_SUGGESTION_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_subjective_source_update_v2
BEFORE UPDATE ON exam_grade_decision_subjective_sources_v2
BEGIN SELECT RAISE(ABORT,'M2_SUBJECTIVE_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_subjective_source_delete_v2
BEFORE DELETE ON exam_grade_decision_subjective_sources_v2
BEGIN SELECT RAISE(ABORT,'M2_SUBJECTIVE_REVIEW_IMMUTABLE'); END;
