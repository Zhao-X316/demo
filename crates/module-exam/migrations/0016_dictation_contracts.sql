-- M2-C0：固定格式默写的版本化模板、策略、识别文本和逐点评价观察。
-- 本迁移只保存机器证据与老师修正文本，不创建 grade decision、publication 或 learning evidence。

CREATE TABLE exam_dictation_template_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  template_version TEXT NOT NULL CHECK(length(trim(template_version)) > 0),
  page_no INTEGER NOT NULL CHECK(page_no > 0),
  template_json TEXT NOT NULL,
  source_ai_run_id INTEGER REFERENCES ai_runs(id),
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by)) > 0),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  UNIQUE(assessment_version_id,page_no,revision),
  CHECK(COALESCE(json_valid(template_json),0)=1),
  CHECK(COALESCE(json_type(template_json),0)='object'),
  CHECK(COALESCE(json_type(template_json,'$.schema_version'),0)='integer'),
  CHECK(COALESCE(json_type(template_json,'$.regions'),0)='array')
);

CREATE UNIQUE INDEX idx_exam_dictation_template_active_v2
  ON exam_dictation_template_revisions_v2(assessment_version_id,page_no)
  WHERE state='active';

CREATE TABLE exam_dictation_policy_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  policy_hash TEXT NOT NULL CHECK(length(policy_hash)=64),
  policy_json TEXT NOT NULL,
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by)) > 0),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  UNIQUE(assessment_item_id,revision),
  CHECK(COALESCE(json_valid(policy_json),0)=1),
  CHECK(COALESCE(json_type(policy_json),0)='object'),
  CHECK(COALESCE(json_type(policy_json,'$.schema_version'),0)='integer'),
  CHECK(COALESCE(json_type(policy_json,'$.typo_policy'),0)='text'),
  CHECK(COALESCE(json_type(policy_json,'$.ordered_sequence'),0) IN ('true','false','integer'))
);

CREATE UNIQUE INDEX idx_exam_dictation_policy_active_v2
  ON exam_dictation_policy_revisions_v2(assessment_item_id)
  WHERE state='active';

CREATE TRIGGER trg_exam_dictation_policy_scope_insert_v2
BEFORE INSERT ON exam_dictation_policy_revisions_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM exam_assessment_items_v2 i
    WHERE i.id=NEW.assessment_item_id
      AND i.answer_key_version_id=NEW.answer_key_version_id
      AND i.rubric_version_id=NEW.rubric_version_id
      AND i.state='active'
  ) THEN RAISE(ABORT,'M2_DICTATION_POLICY_VERSION_MISMATCH') END;
END;

CREATE TABLE exam_dictation_transcription_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  attempt_id INTEGER NOT NULL REFERENCES exam_attempts_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  answer_region_revision_id INTEGER NOT NULL REFERENCES exam_answer_region_revisions_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  source_ai_run_id INTEGER REFERENCES ai_runs(id),
  result_state TEXT NOT NULL CHECK(result_state IN (
    'recognized','not_written','unreadable','recognize_failed','ambiguous_final'
  )),
  raw_ocr_text TEXT,
  normalized_text TEXT,
  teacher_corrected_text TEXT,
  confidence REAL CHECK(confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
  failure_meta_json TEXT,
  corrected_by TEXT,
  corrected_at TEXT,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  UNIQUE(attempt_id,assessment_item_id,answer_region_revision_id,revision),
  CHECK(failure_meta_json IS NULL OR (
    COALESCE(json_valid(failure_meta_json),0)=1
    AND COALESCE(json_type(failure_meta_json),0)='object'
    AND COALESCE(json_type(failure_meta_json,'$.schema_version'),0)='integer'
  )),
  CHECK((result_state='recognized' AND raw_ocr_text IS NOT NULL AND normalized_text IS NOT NULL
         AND confidence IS NOT NULL AND failure_meta_json IS NULL)
        OR (result_state='recognize_failed' AND raw_ocr_text IS NULL AND normalized_text IS NULL
            AND confidence IS NULL AND failure_meta_json IS NOT NULL)
        OR result_state IN ('not_written','unreadable','ambiguous_final')),
  CHECK((teacher_corrected_text IS NOT NULL AND corrected_by IS NOT NULL
         AND length(trim(corrected_by)) > 0 AND corrected_at IS NOT NULL)
        OR (teacher_corrected_text IS NULL AND corrected_by IS NULL AND corrected_at IS NULL))
);

CREATE UNIQUE INDEX idx_exam_dictation_transcription_active_v2
  ON exam_dictation_transcription_revisions_v2(
    attempt_id,assessment_item_id,answer_region_revision_id
  ) WHERE state='active';

CREATE TRIGGER trg_exam_dictation_transcription_scope_insert_v2
BEFORE INSERT ON exam_dictation_transcription_revisions_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_answer_region_revisions_v2 r
    JOIN exam_ingest_pages_v2 pg ON pg.id=r.page_id
    JOIN exam_ingest_batches_v2 b ON b.id=pg.batch_id
    JOIN exam_page_match_revisions_v2 m
      ON m.page_id=r.page_id
     AND m.state='active'
     AND m.decision='teacher_confirmed'
    JOIN exam_attempts_v2 a ON a.id=NEW.attempt_id
    JOIN exam_assessment_items_v2 i ON i.id=NEW.assessment_item_id
    WHERE r.id=NEW.answer_region_revision_id
      AND r.assessment_item_id=NEW.assessment_item_id
      AND r.state='active'
      AND r.decision='teacher_confirmed'
      AND m.attempt_id=NEW.attempt_id
      AND a.assessment_version_id=i.assessment_version_id
      AND b.assessment_version_id=a.assessment_version_id
  ) THEN RAISE(ABORT,'M2_DICTATION_TRANSCRIPTION_SCOPE_MISMATCH') END;
END;

CREATE TABLE exam_dictation_point_observations_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  transcription_revision_id INTEGER NOT NULL REFERENCES exam_dictation_transcription_revisions_v2(id),
  policy_revision_id INTEGER NOT NULL REFERENCES exam_dictation_policy_revisions_v2(id),
  rubric_point_id INTEGER NOT NULL REFERENCES k1_rubric_points(id),
  result TEXT NOT NULL CHECK(result IN (
    'not_written','unreadable','recognize_failed','exact','accepted_variant',
    'partial','missing','contradicted','extra_wrong','ambiguous_final','needs_review'
  )),
  evidence_text TEXT,
  rationale TEXT NOT NULL CHECK(length(trim(rationale)) > 0),
  suggested_score REAL CHECK(suggested_score IS NULL OR suggested_score >= 0.0),
  confidence REAL CHECK(confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
  created_at TEXT NOT NULL,
  UNIQUE(transcription_revision_id,rubric_point_id)
);

CREATE TRIGGER trg_exam_dictation_point_scope_insert_v2
BEFORE INSERT ON exam_dictation_point_observations_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_dictation_transcription_revisions_v2 t
    JOIN exam_dictation_policy_revisions_v2 p ON p.id=NEW.policy_revision_id
    JOIN exam_assessment_items_v2 i ON i.id=t.assessment_item_id
    JOIN k1_rubric_points rp ON rp.id=NEW.rubric_point_id
    WHERE t.id=NEW.transcription_revision_id
      AND t.assessment_item_id=p.assessment_item_id
      AND t.state='active'
      AND p.state='active'
      AND i.rubric_version_id=p.rubric_version_id
      AND rp.rubric_version_id=p.rubric_version_id
  ) THEN RAISE(ABORT,'M2_DICTATION_POINT_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_dictation_template_content_guard_v2
BEFORE UPDATE ON exam_dictation_template_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.assessment_version_id IS NOT OLD.assessment_version_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.template_version IS NOT OLD.template_version
  OR NEW.page_no IS NOT OLD.page_no
  OR NEW.template_json IS NOT OLD.template_json
  OR NEW.source_ai_run_id IS NOT OLD.source_ai_run_id
  OR NEW.confirmed_by IS NOT OLD.confirmed_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_TEMPLATE_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_dictation_policy_content_guard_v2
BEFORE UPDATE ON exam_dictation_policy_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.assessment_item_id IS NOT OLD.assessment_item_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.answer_key_version_id IS NOT OLD.answer_key_version_id
  OR NEW.rubric_version_id IS NOT OLD.rubric_version_id
  OR NEW.policy_hash IS NOT OLD.policy_hash
  OR NEW.policy_json IS NOT OLD.policy_json
  OR NEW.confirmed_by IS NOT OLD.confirmed_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_POLICY_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_dictation_transcription_content_guard_v2
BEFORE UPDATE ON exam_dictation_transcription_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.attempt_id IS NOT OLD.attempt_id
  OR NEW.assessment_item_id IS NOT OLD.assessment_item_id
  OR NEW.answer_region_revision_id IS NOT OLD.answer_region_revision_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.source_ai_run_id IS NOT OLD.source_ai_run_id
  OR NEW.result_state IS NOT OLD.result_state
  OR NEW.raw_ocr_text IS NOT OLD.raw_ocr_text
  OR NEW.normalized_text IS NOT OLD.normalized_text
  OR NEW.teacher_corrected_text IS NOT OLD.teacher_corrected_text
  OR NEW.confidence IS NOT OLD.confidence
  OR NEW.failure_meta_json IS NOT OLD.failure_meta_json
  OR NEW.corrected_by IS NOT OLD.corrected_by
  OR NEW.corrected_at IS NOT OLD.corrected_at
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_TRANSCRIPTION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_dictation_template_delete_v2
BEFORE DELETE ON exam_dictation_template_revisions_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_TEMPLATE_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_dictation_policy_delete_v2
BEFORE DELETE ON exam_dictation_policy_revisions_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_POLICY_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_dictation_transcription_delete_v2
BEFORE DELETE ON exam_dictation_transcription_revisions_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_TRANSCRIPTION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_dictation_point_update_v2
BEFORE UPDATE ON exam_dictation_point_observations_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_POINT_OBSERVATION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_dictation_point_delete_v2
BEFORE DELETE ON exam_dictation_point_observations_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_POINT_OBSERVATION_IMMUTABLE'); END;
