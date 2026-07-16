-- M2-B3a4：答题卡篇幅受控简答题逐评分点 AI 建议。
-- AI 输出是追加式分析，不覆盖 OCR、不直接写老师分数；每次老师终审仍引用 0023 的
-- suggestion，并通过 companion source 固定本次实际采用的 answer_grade run。

CREATE TABLE exam_short_answer_grade_analyses_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  transcription_revision_id INTEGER NOT NULL
    REFERENCES exam_subjective_transcription_revisions_v2(id),
  suggestion_id INTEGER NOT NULL REFERENCES exam_subjective_grade_suggestions_v2(id),
  attempt_id INTEGER NOT NULL REFERENCES exam_attempts_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  machine_grade_ai_run_id INTEGER NOT NULL UNIQUE REFERENCES ai_runs(id),
  outcome TEXT NOT NULL CHECK(outcome IN ('correct','incorrect','partial')),
  suggested_score REAL NOT NULL CHECK(suggested_score >= 0.0),
  result_json TEXT NOT NULL,
  confidence REAL NOT NULL CHECK(confidence >= 0.0 AND confidence <= 1.0),
  exclusion_reason TEXT NOT NULL CHECK(length(trim(exclusion_reason)) > 0),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(result_json),0)=1),
  CHECK(COALESCE(json_type(result_json),0)='object'),
  CHECK(COALESCE(json_type(result_json,'$.schema_version'),0)='integer'),
  CHECK(COALESCE(json_type(result_json,'$.point_results'),0)='array')
);
CREATE UNIQUE INDEX idx_exam_short_answer_analysis_active_v2
  ON exam_short_answer_grade_analyses_v2(transcription_revision_id)
  WHERE state='active';

CREATE TABLE exam_grade_decision_short_answer_sources_v2 (
  grade_decision_id INTEGER PRIMARY KEY REFERENCES exam_grade_decisions_v2(id),
  analysis_id INTEGER NOT NULL REFERENCES exam_short_answer_grade_analyses_v2(id),
  machine_grade_ai_run_id INTEGER NOT NULL REFERENCES ai_runs(id),
  reviewed_by TEXT NOT NULL CHECK(length(trim(reviewed_by)) > 0),
  created_at TEXT NOT NULL
);

CREATE TRIGGER trg_exam_short_answer_analysis_scope_insert_v2
BEFORE INSERT ON exam_short_answer_grade_analyses_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_subjective_transcription_revisions_v2 t
    JOIN exam_subjective_grade_suggestions_v2 suggestion
      ON suggestion.id=NEW.suggestion_id
     AND suggestion.transcription_revision_id=t.id
     AND suggestion.state='active'
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
    JOIN ai_runs run ON run.id=NEW.machine_grade_ai_run_id
    WHERE t.id=NEW.transcription_revision_id AND t.state='active'
      AND t.question_type='short_answer' AND t.result_state='recognized'
      AND t.attempt_id=NEW.attempt_id
      AND t.assessment_item_id=NEW.assessment_item_id
      AND suggestion.attempt_id=NEW.attempt_id
      AND suggestion.assessment_item_id=NEW.assessment_item_id
      AND suggestion.answer_key_version_id=NEW.answer_key_version_id
      AND suggestion.rubric_version_id=NEW.rubric_version_id
      AND answer.id=NEW.answer_key_version_id
      AND rubric.id=NEW.rubric_version_id
      AND question.question_type='short_answer'
      AND run.run_type='answer_grade' AND run.source_module='exam'
      AND run.business_ref_type='subjective_transcription_revision'
      AND run.business_ref_id=CAST(t.id AS TEXT)
      AND run.status='succeeded'
      AND NEW.result_json=run.output_json
      AND COALESCE(json_extract(run.output_json,'$.input_hash'),'')=run.input_hash
      AND COALESCE(json_extract(run.output_json,'$.transcription_revision_id'),-1)=t.id
      AND COALESCE(json_extract(run.output_json,'$.assessment_item_id'),-1)=item.id
      AND COALESCE(json_extract(run.output_json,'$.answer_key_version_id'),-1)=answer.id
      AND COALESCE(json_extract(run.output_json,'$.rubric_version_id'),-1)=rubric.id
      AND COALESCE(json_extract(run.output_json,'$.descriptor.provider'),'')=run.provider
      AND COALESCE(json_extract(run.output_json,'$.descriptor.model_name'),'')=run.model_name
      AND COALESCE(json_extract(run.output_json,'$.descriptor.model_version'),'')=run.model_version
      AND COALESCE(json_extract(run.output_json,'$.descriptor.config_version'),'')=run.config_version
      AND COALESCE(json_extract(run.output_json,'$.descriptor.rule_version'),'')=run.prompt_or_rule_version
      AND ABS(COALESCE(json_extract(run.output_json,'$.suggested_score'),-1)-NEW.suggested_score)
          <= 0.000001
      AND ABS(COALESCE(json_extract(run.output_json,'$.confidence'),-1)-NEW.confidence)
          <= 0.000001
  ) THEN RAISE(ABORT,'M2_SHORT_ANSWER_ANALYSIS_SCOPE_MISMATCH') END;
  SELECT CASE WHEN NEW.suggested_score > (
    SELECT score FROM exam_assessment_items_v2 WHERE id=NEW.assessment_item_id
  ) + 0.000001
  THEN RAISE(ABORT,'M2_SHORT_ANSWER_SCORE_EXCEEDS_ITEM') END;
END;

CREATE TRIGGER trg_exam_short_answer_source_scope_insert_v2
BEFORE INSERT ON exam_grade_decision_short_answer_sources_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_grade_decisions_v2 decision
    JOIN exam_short_answer_grade_analyses_v2 analysis
      ON analysis.id=NEW.analysis_id AND analysis.state='active'
    JOIN exam_grade_decision_subjective_sources_v2 source
      ON source.grade_decision_id=decision.id
     AND source.suggestion_id=analysis.suggestion_id
     AND source.transcription_revision_id=analysis.transcription_revision_id
    WHERE decision.id=NEW.grade_decision_id AND decision.state='active'
      AND decision.attempt_id=analysis.attempt_id
      AND decision.assessment_item_id=analysis.assessment_item_id
      AND analysis.machine_grade_ai_run_id=NEW.machine_grade_ai_run_id
      AND decision.machine_grade_ai_run_id=analysis.machine_grade_ai_run_id
  ) THEN RAISE(ABORT,'M2_SHORT_ANSWER_REVIEW_SOURCE_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_short_answer_analysis_content_guard_v2
BEFORE UPDATE ON exam_short_answer_grade_analyses_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.transcription_revision_id IS NOT OLD.transcription_revision_id
  OR NEW.suggestion_id IS NOT OLD.suggestion_id
  OR NEW.attempt_id IS NOT OLD.attempt_id
  OR NEW.assessment_item_id IS NOT OLD.assessment_item_id
  OR NEW.answer_key_version_id IS NOT OLD.answer_key_version_id
  OR NEW.rubric_version_id IS NOT OLD.rubric_version_id
  OR NEW.machine_grade_ai_run_id IS NOT OLD.machine_grade_ai_run_id
  OR NEW.outcome IS NOT OLD.outcome
  OR NEW.suggested_score IS NOT OLD.suggested_score
  OR NEW.result_json IS NOT OLD.result_json
  OR NEW.confidence IS NOT OLD.confidence
  OR NEW.exclusion_reason IS NOT OLD.exclusion_reason
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_SHORT_ANSWER_ANALYSIS_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_short_answer_analysis_delete_v2
BEFORE DELETE ON exam_short_answer_grade_analyses_v2
BEGIN SELECT RAISE(ABORT,'M2_SHORT_ANSWER_ANALYSIS_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_short_answer_source_update_v2
BEFORE UPDATE ON exam_grade_decision_short_answer_sources_v2
BEGIN SELECT RAISE(ABORT,'M2_SHORT_ANSWER_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_short_answer_source_delete_v2
BEFORE DELETE ON exam_grade_decision_short_answer_sources_v2
BEGIN SELECT RAISE(ABORT,'M2_SHORT_ANSWER_REVIEW_IMMUTABLE'); END;
