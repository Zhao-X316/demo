-- T6：客观题观察、确定性评分建议和严格批量终审。
-- OMR/fixture 只能产生 observation + suggestion；老师确认后才写 grade decision，
-- 单题确认和批量确认都不会自动发布成绩。

CREATE TABLE exam_objective_observation_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  idempotency_key TEXT NOT NULL UNIQUE CHECK(length(trim(idempotency_key)) > 0),
  input_hash TEXT NOT NULL CHECK(length(input_hash) = 64),
  attempt_id INTEGER NOT NULL REFERENCES exam_attempts_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  answer_region_revision_id INTEGER NOT NULL REFERENCES exam_answer_region_revisions_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  source_kind TEXT NOT NULL CHECK(source_kind IN ('fixed_fixture','omr')),
  question_type TEXT NOT NULL CHECK(question_type IN ('single','multiple','true_false')),
  result_state TEXT NOT NULL CHECK(result_state IN (
    'recognized','blank','altered','low_confidence','failed'
  )),
  observed_answer_json TEXT,
  confidence REAL CHECK(confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
  alteration_detected INTEGER NOT NULL DEFAULT 0 CHECK(alteration_detected IN (0,1)),
  ai_run_id INTEGER REFERENCES ai_runs(id),
  failure_meta_json TEXT,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  UNIQUE(attempt_id, assessment_item_id, revision),
  CHECK(observed_answer_json IS NULL OR (
    COALESCE(json_valid(observed_answer_json), 0) = 1
    AND COALESCE(json_type(observed_answer_json) = 'object', 0)
    AND COALESCE(json_type(observed_answer_json, '$.schema_version') = 'integer', 0)
  )),
  CHECK(failure_meta_json IS NULL OR (
    COALESCE(json_valid(failure_meta_json), 0) = 1
    AND COALESCE(json_type(failure_meta_json) = 'object', 0)
    AND COALESCE(json_type(failure_meta_json, '$.schema_version') = 'integer', 0)
  )),
  CHECK((result_state IN ('recognized','altered','low_confidence')
         AND observed_answer_json IS NOT NULL AND confidence IS NOT NULL)
        OR result_state NOT IN ('recognized','altered','low_confidence')),
  CHECK((result_state='failed' AND failure_meta_json IS NOT NULL)
        OR (result_state<>'failed' AND failure_meta_json IS NULL)),
  CHECK((result_state='altered' AND alteration_detected=1)
        OR (result_state<>'altered' AND alteration_detected=0)),
  CHECK((source_kind='fixed_fixture' AND ai_run_id IS NULL)
        OR source_kind='omr')
);
CREATE UNIQUE INDEX idx_exam_objective_observation_active_v2
  ON exam_objective_observation_revisions_v2(attempt_id, assessment_item_id)
  WHERE state='active';

CREATE TABLE exam_objective_grade_suggestions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  observation_revision_id INTEGER NOT NULL UNIQUE
    REFERENCES exam_objective_observation_revisions_v2(id),
  attempt_id INTEGER NOT NULL REFERENCES exam_attempts_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  outcome TEXT NOT NULL CHECK(outcome IN ('correct','incorrect','unscored')),
  suggested_score REAL,
  result_json TEXT NOT NULL,
  batch_eligible INTEGER NOT NULL CHECK(batch_eligible IN (0,1)),
  exclusion_reason TEXT,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  CHECK((outcome IN ('correct','incorrect') AND suggested_score IS NOT NULL
         AND suggested_score >= 0.0)
        OR (outcome='unscored' AND suggested_score IS NULL)),
  CHECK(COALESCE(json_valid(result_json), 0) = 1),
  CHECK(COALESCE(json_type(result_json) = 'object', 0)),
  CHECK(COALESCE(json_type(result_json, '$.schema_version') = 'integer', 0)),
  CHECK((batch_eligible=1 AND outcome IN ('correct','incorrect') AND exclusion_reason IS NULL)
        OR batch_eligible=0)
);
CREATE UNIQUE INDEX idx_exam_objective_suggestion_active_v2
  ON exam_objective_grade_suggestions_v2(attempt_id, assessment_item_id)
  WHERE state='active';

CREATE TABLE exam_objective_review_batches_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  idempotency_key TEXT NOT NULL UNIQUE CHECK(length(trim(idempotency_key)) > 0),
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  confidence_threshold REAL NOT NULL CHECK(
    confidence_threshold >= 0.95 AND confidence_threshold <= 1.0
  ),
  requested_count INTEGER NOT NULL CHECK(requested_count > 0),
  confirmed_count INTEGER NOT NULL CHECK(confirmed_count >= 0),
  excluded_count INTEGER NOT NULL CHECK(excluded_count >= 0),
  state TEXT NOT NULL CHECK(state='completed'),
  created_by TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at TEXT NOT NULL,
  CHECK(requested_count = confirmed_count + excluded_count)
);

CREATE TABLE exam_objective_review_batch_items_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  batch_id INTEGER NOT NULL REFERENCES exam_objective_review_batches_v2(id),
  suggestion_id INTEGER NOT NULL REFERENCES exam_objective_grade_suggestions_v2(id),
  outcome TEXT NOT NULL CHECK(outcome IN ('confirmed','excluded')),
  reason_code TEXT,
  grade_decision_id INTEGER REFERENCES exam_grade_decisions_v2(id),
  created_at TEXT NOT NULL,
  UNIQUE(batch_id, suggestion_id),
  CHECK((outcome='confirmed' AND grade_decision_id IS NOT NULL AND reason_code IS NULL)
        OR (outcome='excluded' AND grade_decision_id IS NULL
            AND reason_code IS NOT NULL AND length(trim(reason_code)) > 0))
);

CREATE TABLE exam_grade_decision_objective_sources_v2 (
  grade_decision_id INTEGER PRIMARY KEY REFERENCES exam_grade_decisions_v2(id),
  suggestion_id INTEGER NOT NULL REFERENCES exam_objective_grade_suggestions_v2(id),
  review_mode TEXT NOT NULL CHECK(review_mode IN ('single','strict_batch')),
  batch_id INTEGER REFERENCES exam_objective_review_batches_v2(id),
  reviewed_by TEXT NOT NULL CHECK(length(trim(reviewed_by)) > 0),
  created_at TEXT NOT NULL,
  CHECK((review_mode='single' AND batch_id IS NULL)
        OR (review_mode='strict_batch' AND batch_id IS NOT NULL))
);

CREATE TRIGGER trg_exam_objective_observation_content_guard_v2
BEFORE UPDATE ON exam_objective_observation_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.idempotency_key IS NOT OLD.idempotency_key
  OR NEW.input_hash IS NOT OLD.input_hash
  OR NEW.attempt_id IS NOT OLD.attempt_id
  OR NEW.assessment_item_id IS NOT OLD.assessment_item_id
  OR NEW.answer_region_revision_id IS NOT OLD.answer_region_revision_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.source_kind IS NOT OLD.source_kind
  OR NEW.question_type IS NOT OLD.question_type
  OR NEW.result_state IS NOT OLD.result_state
  OR NEW.observed_answer_json IS NOT OLD.observed_answer_json
  OR NEW.confidence IS NOT OLD.confidence
  OR NEW.alteration_detected IS NOT OLD.alteration_detected
  OR NEW.ai_run_id IS NOT OLD.ai_run_id
  OR NEW.failure_meta_json IS NOT OLD.failure_meta_json
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_OBJECTIVE_OBSERVATION_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_objective_observation_delete_v2
BEFORE DELETE ON exam_objective_observation_revisions_v2
BEGIN SELECT RAISE(ABORT, 'M2_OBJECTIVE_OBSERVATION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_objective_suggestion_content_guard_v2
BEFORE UPDATE ON exam_objective_grade_suggestions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.observation_revision_id IS NOT OLD.observation_revision_id
  OR NEW.attempt_id IS NOT OLD.attempt_id
  OR NEW.assessment_item_id IS NOT OLD.assessment_item_id
  OR NEW.answer_key_version_id IS NOT OLD.answer_key_version_id
  OR NEW.outcome IS NOT OLD.outcome
  OR NEW.suggested_score IS NOT OLD.suggested_score
  OR NEW.result_json IS NOT OLD.result_json
  OR NEW.batch_eligible IS NOT OLD.batch_eligible
  OR NEW.exclusion_reason IS NOT OLD.exclusion_reason
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_OBJECTIVE_SUGGESTION_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_objective_suggestion_delete_v2
BEFORE DELETE ON exam_objective_grade_suggestions_v2
BEGIN SELECT RAISE(ABORT, 'M2_OBJECTIVE_SUGGESTION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_objective_batch_update_v2
BEFORE UPDATE ON exam_objective_review_batches_v2
BEGIN SELECT RAISE(ABORT, 'M2_OBJECTIVE_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_objective_batch_delete_v2
BEFORE DELETE ON exam_objective_review_batches_v2
BEGIN SELECT RAISE(ABORT, 'M2_OBJECTIVE_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_objective_batch_item_update_v2
BEFORE UPDATE ON exam_objective_review_batch_items_v2
BEGIN SELECT RAISE(ABORT, 'M2_OBJECTIVE_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_objective_batch_item_delete_v2
BEFORE DELETE ON exam_objective_review_batch_items_v2
BEGIN SELECT RAISE(ABORT, 'M2_OBJECTIVE_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_objective_source_update_v2
BEFORE UPDATE ON exam_grade_decision_objective_sources_v2
BEGIN SELECT RAISE(ABORT, 'M2_OBJECTIVE_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_objective_source_delete_v2
BEFORE DELETE ON exam_grade_decision_objective_sources_v2
BEGIN SELECT RAISE(ABORT, 'M2_OBJECTIVE_REVIEW_IMMUTABLE'); END;
