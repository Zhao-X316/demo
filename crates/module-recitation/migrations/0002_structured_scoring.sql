-- M1.1 第一批：背诵答案、评分点、ASR 与逐点评分的不可变证据底座。
-- 本迁移不改变 M1.0 verdict / decision_effects / 复习排程语义：
-- 机器分析仍只是建议，老师终审仍由既有 human_decide 事务生效。

PRAGMA foreign_keys = ON;

-- 老库只能证明 rec_contents 当前保存的答案，不能猜测已经被覆盖的旧文本。
CREATE TABLE rec_answer_versions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  content_id INTEGER NOT NULL REFERENCES rec_contents(id) ON DELETE CASCADE,
  answer_version INTEGER NOT NULL CHECK(answer_version > 0),
  answer_text TEXT NOT NULL CHECK(length(trim(answer_text)) > 0),
  provenance TEXT NOT NULL
    CHECK(provenance IN ('legacy_current_only','system_versioned')),
  supersedes_answer_version_id INTEGER
    REFERENCES rec_answer_versions(id) ON DELETE RESTRICT,
  created_at TEXT NOT NULL,
  UNIQUE(content_id, answer_version),
  CHECK(supersedes_answer_version_id IS NULL OR supersedes_answer_version_id <> id)
);
CREATE INDEX idx_rec_answer_versions_content
  ON rec_answer_versions(content_id, answer_version DESC);

INSERT INTO rec_answer_versions
  (public_id,content_id,answer_version,answer_text,provenance,created_at)
SELECT 'rec-answer-' || id || '-v' || answer_version,
       id,answer_version,answer_text,'legacy_current_only',updated_at
FROM rec_contents;

-- 迁移后的新内容自动登记 v1；答案变化必须严格 +1，并追加新版本。
CREATE TRIGGER trg_rec_content_answer_insert
AFTER INSERT ON rec_contents
BEGIN
  INSERT INTO rec_answer_versions
    (public_id,content_id,answer_version,answer_text,provenance,created_at)
  VALUES (
    'rec-answer-' || NEW.id || '-v' || NEW.answer_version,
    NEW.id,NEW.answer_version,NEW.answer_text,'system_versioned',NEW.updated_at
  );
END;

CREATE TRIGGER trg_rec_content_answer_update_guard
BEFORE UPDATE OF answer_text,answer_version ON rec_contents
WHEN (NEW.answer_text IS NOT OLD.answer_text
      AND NEW.answer_version <> OLD.answer_version + 1)
  OR (NEW.answer_text IS OLD.answer_text
      AND NEW.answer_version <> OLD.answer_version)
BEGIN SELECT RAISE(ABORT,'M1_ANSWER_VERSION_STEP_INVALID'); END;

CREATE TRIGGER trg_rec_content_answer_update
AFTER UPDATE OF answer_text,answer_version ON rec_contents
WHEN NEW.answer_text IS NOT OLD.answer_text
BEGIN
  INSERT INTO rec_answer_versions
    (public_id,content_id,answer_version,answer_text,provenance,
     supersedes_answer_version_id,created_at)
  VALUES (
    'rec-answer-' || NEW.id || '-v' || NEW.answer_version,
    NEW.id,NEW.answer_version,NEW.answer_text,'system_versioned',
    (SELECT id FROM rec_answer_versions
     WHERE content_id=NEW.id AND answer_version=OLD.answer_version),
    NEW.updated_at
  );
END;

CREATE TRIGGER trg_rec_answer_version_immutable_update
BEFORE UPDATE ON rec_answer_versions
BEGIN SELECT RAISE(ABORT,'M1_ANSWER_VERSION_IMMUTABLE'); END;

-- 一份答案版本可以有多个 rubric 草稿，但同一时刻最多一个 confirmed。
CREATE TABLE rec_rubric_versions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  answer_version_id INTEGER NOT NULL
    REFERENCES rec_answer_versions(id) ON DELETE CASCADE,
  revision INTEGER NOT NULL CHECK(revision > 0),
  definition_hash TEXT NOT NULL
    CHECK(length(definition_hash)=64
      AND lower(definition_hash)=definition_hash
      AND definition_hash NOT GLOB '*[^0-9a-f]*'),
  status TEXT NOT NULL DEFAULT 'draft'
    CHECK(status IN ('draft','confirmed','retired')),
  generated_by_ai_run_id INTEGER REFERENCES ai_runs(id) ON DELETE RESTRICT,
  supersedes_rubric_version_id INTEGER
    REFERENCES rec_rubric_versions(id) ON DELETE RESTRICT,
  created_by TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at TEXT NOT NULL,
  confirmed_by TEXT,
  confirmed_at TEXT,
  UNIQUE(answer_version_id, revision),
  UNIQUE(answer_version_id, definition_hash),
  CHECK(supersedes_rubric_version_id IS NULL OR supersedes_rubric_version_id <> id),
  CHECK((status='confirmed' AND confirmed_by IS NOT NULL AND confirmed_at IS NOT NULL)
        OR status<>'confirmed')
);
CREATE UNIQUE INDEX idx_rec_rubric_confirmed
  ON rec_rubric_versions(answer_version_id) WHERE status='confirmed';
CREATE INDEX idx_rec_rubric_answer
  ON rec_rubric_versions(answer_version_id, revision DESC);

CREATE TABLE rec_rubric_points (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  stable_key TEXT NOT NULL CHECK(length(trim(stable_key)) > 0),
  rubric_version_id INTEGER NOT NULL
    REFERENCES rec_rubric_versions(id) ON DELETE CASCADE,
  canonical_text TEXT NOT NULL CHECK(length(trim(canonical_text)) > 0),
  required_entities_json TEXT NOT NULL,
  allowed_paraphrases_json TEXT NOT NULL,
  contradiction_rules_json TEXT NOT NULL,
  required INTEGER NOT NULL CHECK(required IN (0,1)),
  weight REAL NOT NULL CHECK(weight > 0.0),
  order_index INTEGER NOT NULL CHECK(order_index >= 0),
  knowledge_node_id INTEGER REFERENCES k1_knowledge_nodes(id) ON DELETE RESTRICT,
  knowledge_link_state TEXT NOT NULL DEFAULT 'none'
    CHECK(knowledge_link_state IN ('none','suggested','confirmed')),
  verified_by TEXT,
  verified_at TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(rubric_version_id, stable_key),
  UNIQUE(rubric_version_id, order_index),
  CHECK(COALESCE(json_valid(required_entities_json),0)=1),
  CHECK(COALESCE(json_type(required_entities_json),0)='object'),
  CHECK(COALESCE(json_extract(required_entities_json,'$.schema_version'),0)=1),
  CHECK(COALESCE(json_type(required_entities_json,'$.items'),0)='array'),
  CHECK(COALESCE(json_valid(allowed_paraphrases_json),0)=1),
  CHECK(COALESCE(json_type(allowed_paraphrases_json),0)='object'),
  CHECK(COALESCE(json_extract(allowed_paraphrases_json,'$.schema_version'),0)=1),
  CHECK(COALESCE(json_type(allowed_paraphrases_json,'$.items'),0)='array'),
  CHECK(COALESCE(json_valid(contradiction_rules_json),0)=1),
  CHECK(COALESCE(json_type(contradiction_rules_json),0)='object'),
  CHECK(COALESCE(json_extract(contradiction_rules_json,'$.schema_version'),0)=1),
  CHECK(COALESCE(json_type(contradiction_rules_json,'$.items'),0)='array'),
  CHECK((knowledge_link_state='none' AND knowledge_node_id IS NULL
         AND verified_by IS NULL AND verified_at IS NULL)
        OR (knowledge_link_state='suggested' AND knowledge_node_id IS NOT NULL
            AND verified_by IS NULL AND verified_at IS NULL)
        OR (knowledge_link_state='confirmed' AND knowledge_node_id IS NOT NULL
            AND verified_by IS NOT NULL AND verified_at IS NOT NULL))
);
CREATE INDEX idx_rec_rubric_points_version
  ON rec_rubric_points(rubric_version_id, order_index);

CREATE TRIGGER trg_rec_rubric_ai_scope_insert
BEFORE INSERT ON rec_rubric_versions
WHEN NEW.generated_by_ai_run_id IS NOT NULL
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM ai_runs run
    WHERE run.id=NEW.generated_by_ai_run_id
      AND run.run_type='recitation_rubric'
      AND run.source_module='recitation'
      AND run.business_ref_type='recitation_answer_version'
      AND run.business_ref_id=CAST(NEW.answer_version_id AS TEXT)
      AND run.status='succeeded'
      AND run.output_hash IS NOT NULL
      AND COALESCE(json_extract(run.output_json,'$.answer_version_id'),-1)
          =NEW.answer_version_id
      AND COALESCE(json_extract(run.output_json,'$.definition_hash'),'')
          =NEW.definition_hash
  ) THEN RAISE(ABORT,'M1_RUBRIC_AI_RUN_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_rec_rubric_fact_immutable
BEFORE UPDATE ON rec_rubric_versions
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.answer_version_id IS NOT OLD.answer_version_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.definition_hash IS NOT OLD.definition_hash
  OR NEW.generated_by_ai_run_id IS NOT OLD.generated_by_ai_run_id
  OR NEW.supersedes_rubric_version_id IS NOT OLD.supersedes_rubric_version_id
  OR NEW.created_by IS NOT OLD.created_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M1_RUBRIC_VERSION_IMMUTABLE'); END;

CREATE TRIGGER trg_rec_rubric_state_transition
BEFORE UPDATE OF status,confirmed_by,confirmed_at ON rec_rubric_versions
WHEN NOT (
  (OLD.status='draft' AND NEW.status='confirmed'
   AND OLD.confirmed_by IS NULL AND OLD.confirmed_at IS NULL
   AND NEW.confirmed_by IS NOT NULL AND NEW.confirmed_at IS NOT NULL)
  OR
  (OLD.status='draft' AND NEW.status='retired'
   AND NEW.confirmed_by IS OLD.confirmed_by AND NEW.confirmed_at IS OLD.confirmed_at)
  OR
  (OLD.status='confirmed' AND NEW.status='retired'
   AND NEW.confirmed_by IS OLD.confirmed_by AND NEW.confirmed_at IS OLD.confirmed_at)
)
BEGIN SELECT RAISE(ABORT,'M1_RUBRIC_STATE_TRANSITION_INVALID'); END;

CREATE TRIGGER trg_rec_rubric_point_immutable_update
BEFORE UPDATE ON rec_rubric_points
BEGIN SELECT RAISE(ABORT,'M1_RUBRIC_POINT_IMMUTABLE'); END;

-- ASR 原文、规范化文本和词级时间段分开保存；旧 submissions.recognized_text
-- 继续作为 M1.0 兼容投影，不由本表反向覆盖。
CREATE TABLE rec_transcripts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  submission_id INTEGER NOT NULL REFERENCES submissions(id) ON DELETE RESTRICT,
  asr_ai_run_id INTEGER NOT NULL UNIQUE REFERENCES ai_runs(id) ON DELETE RESTRICT,
  raw_transcript TEXT NOT NULL,
  normalized_transcript TEXT NOT NULL,
  normalization_version TEXT NOT NULL CHECK(length(trim(normalization_version)) > 0),
  word_segments_json TEXT NOT NULL,
  duration_ms INTEGER NOT NULL CHECK(duration_ms >= 0),
  output_hash TEXT NOT NULL
    CHECK(length(output_hash)=64
      AND lower(output_hash)=output_hash
      AND output_hash NOT GLOB '*[^0-9a-f]*'),
  state TEXT NOT NULL DEFAULT 'active'
    CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(word_segments_json),0)=1),
  CHECK(COALESCE(json_type(word_segments_json),0)='array')
);
CREATE UNIQUE INDEX idx_rec_transcript_active
  ON rec_transcripts(submission_id) WHERE state='active';
CREATE INDEX idx_rec_transcript_submission
  ON rec_transcripts(submission_id, id DESC);

CREATE TRIGGER trg_rec_transcript_scope_insert
BEFORE INSERT ON rec_transcripts
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM submissions submission
    JOIN ai_runs run ON run.id=NEW.asr_ai_run_id
    WHERE submission.id=NEW.submission_id
      AND submission.module='recitation'
      AND run.run_type='asr'
      AND run.source_module='recitation'
      AND run.business_ref_type='submission'
      AND run.business_ref_id=CAST(submission.id AS TEXT)
      AND run.status='succeeded'
      AND run.output_hash=NEW.output_hash
      AND COALESCE(json_extract(run.output_json,'$.submission_id'),-1)=submission.id
      AND COALESCE(json_extract(run.output_json,'$.raw_transcript'),'')=NEW.raw_transcript
      AND COALESCE(json_extract(run.output_json,'$.normalized_transcript'),'')=NEW.normalized_transcript
      AND COALESCE(json_extract(run.output_json,'$.normalization_version'),'')=NEW.normalization_version
      AND COALESCE(json_extract(run.output_json,'$.duration_ms'),-1)=NEW.duration_ms
      AND json_extract(run.output_json,'$.word_segments')=NEW.word_segments_json
  ) THEN RAISE(ABORT,'M1_TRANSCRIPT_AI_RUN_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_rec_transcript_content_immutable
BEFORE UPDATE ON rec_transcripts
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.submission_id IS NOT OLD.submission_id
  OR NEW.asr_ai_run_id IS NOT OLD.asr_ai_run_id
  OR NEW.raw_transcript IS NOT OLD.raw_transcript
  OR NEW.normalized_transcript IS NOT OLD.normalized_transcript
  OR NEW.normalization_version IS NOT OLD.normalization_version
  OR NEW.word_segments_json IS NOT OLD.word_segments_json
  OR NEW.duration_ms IS NOT OLD.duration_ms
  OR NEW.output_hash IS NOT OLD.output_hash
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M1_TRANSCRIPT_IMMUTABLE'); END;

CREATE TRIGGER trg_rec_transcript_state_transition
BEFORE UPDATE OF state ON rec_transcripts
WHEN NOT (OLD.state='active' AND NEW.state IN ('superseded','voided'))
BEGIN SELECT RAISE(ABORT,'M1_TRANSCRIPT_STATE_TRANSITION_INVALID'); END;

CREATE TRIGGER trg_rec_transcript_delete
BEFORE DELETE ON rec_transcripts
BEGIN SELECT RAISE(ABORT,'M1_TRANSCRIPT_IMMUTABLE'); END;

-- 一次 score run 固定引用一次 ASR 快照和一版 rubric。
CREATE TABLE rec_score_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  submission_id INTEGER NOT NULL REFERENCES submissions(id) ON DELETE RESTRICT,
  transcript_id INTEGER NOT NULL REFERENCES rec_transcripts(id) ON DELETE RESTRICT,
  rubric_version_id INTEGER NOT NULL
    REFERENCES rec_rubric_versions(id) ON DELETE RESTRICT,
  score_ai_run_id INTEGER NOT NULL UNIQUE REFERENCES ai_runs(id) ON DELETE RESTRICT,
  overall_suggestion TEXT NOT NULL
    CHECK(overall_suggestion IN ('pass','fail','unable_to_score')),
  accuracy_json TEXT NOT NULL,
  fluency_json TEXT NOT NULL,
  confidence REAL NOT NULL CHECK(confidence >= 0.0 AND confidence <= 1.0),
  output_hash TEXT NOT NULL
    CHECK(length(output_hash)=64
      AND lower(output_hash)=output_hash
      AND output_hash NOT GLOB '*[^0-9a-f]*'),
  state TEXT NOT NULL DEFAULT 'active'
    CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(accuracy_json),0)=1),
  CHECK(COALESCE(json_type(accuracy_json),0)='object'),
  CHECK(COALESCE(json_extract(accuracy_json,'$.schema_version'),0)=1),
  CHECK(COALESCE(json_valid(fluency_json),0)=1),
  CHECK(COALESCE(json_type(fluency_json),0)='object'),
  CHECK(COALESCE(json_extract(fluency_json,'$.schema_version'),0)=1)
);
CREATE UNIQUE INDEX idx_rec_score_active
  ON rec_score_runs(submission_id) WHERE state='active';
CREATE INDEX idx_rec_score_submission
  ON rec_score_runs(submission_id, id DESC);

CREATE TABLE rec_point_results (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  score_run_id INTEGER NOT NULL REFERENCES rec_score_runs(id) ON DELETE RESTRICT,
  rubric_point_id INTEGER NOT NULL REFERENCES rec_rubric_points(id) ON DELETE RESTRICT,
  machine_state TEXT NOT NULL
    CHECK(machine_state IN ('covered','partial','omitted','contradiction','uncertain')),
  confidence REAL NOT NULL CHECK(confidence >= 0.0 AND confidence <= 1.0),
  evidence_spans_json TEXT NOT NULL,
  reason TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(score_run_id, rubric_point_id),
  CHECK(COALESCE(json_valid(evidence_spans_json),0)=1),
  CHECK(COALESCE(json_type(evidence_spans_json),0)='array')
);
CREATE INDEX idx_rec_point_result_score
  ON rec_point_results(score_run_id);

CREATE TRIGGER trg_rec_score_scope_insert
BEFORE INSERT ON rec_score_runs
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM rec_transcripts transcript
    JOIN rec_rubric_versions rubric ON rubric.id=NEW.rubric_version_id
    JOIN ai_runs run ON run.id=NEW.score_ai_run_id
    WHERE transcript.id=NEW.transcript_id
      AND transcript.submission_id=NEW.submission_id
      AND transcript.state='active'
      AND rubric.status='confirmed'
      AND run.run_type='recitation_score'
      AND run.source_module='recitation'
      AND run.business_ref_type='recitation_transcript'
      AND run.business_ref_id=CAST(transcript.id AS TEXT)
      AND run.status='succeeded'
      AND run.output_hash=NEW.output_hash
      AND COALESCE(json_extract(run.output_json,'$.submission_id'),-1)=NEW.submission_id
      AND COALESCE(json_extract(run.output_json,'$.transcript_id'),-1)=NEW.transcript_id
      AND COALESCE(json_extract(run.output_json,'$.rubric_version_id'),-1)=NEW.rubric_version_id
      AND COALESCE(json_extract(run.output_json,'$.overall_suggestion'),'')=NEW.overall_suggestion
      AND json_extract(run.output_json,'$.accuracy')=NEW.accuracy_json
      AND json_extract(run.output_json,'$.fluency')=NEW.fluency_json
      AND ABS(COALESCE(json_extract(run.output_json,'$.confidence'),-1)-NEW.confidence)
          <= 0.000001
  ) THEN RAISE(ABORT,'M1_SCORE_AI_RUN_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_rec_point_result_scope_insert
BEFORE INSERT ON rec_point_results
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM rec_score_runs score
    JOIN rec_rubric_points point
      ON point.id=NEW.rubric_point_id
     AND point.rubric_version_id=score.rubric_version_id
    JOIN ai_runs run ON run.id=score.score_ai_run_id
    JOIN json_each(run.output_json,'$.point_results') result
    WHERE score.id=NEW.score_run_id
      AND COALESCE(json_extract(result.value,'$.rubric_point_id'),-1)=point.id
      AND COALESCE(json_extract(result.value,'$.machine_state'),'')=NEW.machine_state
      AND ABS(COALESCE(json_extract(result.value,'$.confidence'),-1)-NEW.confidence)
          <= 0.000001
      AND json_extract(result.value,'$.evidence_spans')=NEW.evidence_spans_json
      AND COALESCE(json_extract(result.value,'$.reason'),'')=NEW.reason
  ) THEN RAISE(ABORT,'M1_POINT_RESULT_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_rec_score_content_immutable
BEFORE UPDATE ON rec_score_runs
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.submission_id IS NOT OLD.submission_id
  OR NEW.transcript_id IS NOT OLD.transcript_id
  OR NEW.rubric_version_id IS NOT OLD.rubric_version_id
  OR NEW.score_ai_run_id IS NOT OLD.score_ai_run_id
  OR NEW.overall_suggestion IS NOT OLD.overall_suggestion
  OR NEW.accuracy_json IS NOT OLD.accuracy_json
  OR NEW.fluency_json IS NOT OLD.fluency_json
  OR NEW.confidence IS NOT OLD.confidence
  OR NEW.output_hash IS NOT OLD.output_hash
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M1_SCORE_RUN_IMMUTABLE'); END;

CREATE TRIGGER trg_rec_score_state_transition
BEFORE UPDATE OF state ON rec_score_runs
WHEN NOT (OLD.state='active' AND NEW.state IN ('superseded','voided'))
BEGIN SELECT RAISE(ABORT,'M1_SCORE_STATE_TRANSITION_INVALID'); END;

CREATE TRIGGER trg_rec_score_delete
BEFORE DELETE ON rec_score_runs
BEGIN SELECT RAISE(ABORT,'M1_SCORE_RUN_IMMUTABLE'); END;
CREATE TRIGGER trg_rec_point_result_update
BEFORE UPDATE ON rec_point_results
BEGIN SELECT RAISE(ABORT,'M1_POINT_RESULT_IMMUTABLE'); END;
CREATE TRIGGER trg_rec_point_result_delete
BEFORE DELETE ON rec_point_results
BEGIN SELECT RAISE(ABORT,'M1_POINT_RESULT_IMMUTABLE'); END;
