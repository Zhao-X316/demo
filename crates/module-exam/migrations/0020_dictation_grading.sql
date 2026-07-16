-- M2-C0：固定默写老师终审来源、严格批量和正式评分桥接。
-- grade decision/publication/learning_evidence 继续复用 B0；本迁移只保存默写证据来源。

CREATE TABLE exam_dictation_review_batches_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  idempotency_key TEXT NOT NULL UNIQUE CHECK(length(trim(idempotency_key)) > 0),
  request_hash TEXT NOT NULL CHECK(length(request_hash)=64),
  requested_count INTEGER NOT NULL CHECK(requested_count > 0),
  confirmed_count INTEGER NOT NULL CHECK(confirmed_count >= 0),
  excluded_count INTEGER NOT NULL CHECK(excluded_count >= 0),
  state TEXT NOT NULL CHECK(state='completed'),
  created_by TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at TEXT NOT NULL,
  CHECK(requested_count=confirmed_count+excluded_count)
);

CREATE TABLE exam_dictation_review_batch_items_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  batch_id INTEGER NOT NULL REFERENCES exam_dictation_review_batches_v2(id),
  transcription_revision_id INTEGER NOT NULL REFERENCES exam_dictation_transcription_revisions_v2(id),
  outcome TEXT NOT NULL CHECK(outcome IN ('confirmed','excluded')),
  reason_code TEXT,
  grade_decision_id INTEGER REFERENCES exam_grade_decisions_v2(id),
  created_at TEXT NOT NULL,
  UNIQUE(batch_id,transcription_revision_id),
  CHECK((outcome='confirmed' AND grade_decision_id IS NOT NULL AND reason_code IS NULL)
        OR (outcome='excluded' AND grade_decision_id IS NULL
            AND reason_code IS NOT NULL AND length(trim(reason_code)) > 0))
);

CREATE TABLE exam_grade_decision_dictation_sources_v2 (
  grade_decision_id INTEGER PRIMARY KEY REFERENCES exam_grade_decisions_v2(id),
  transcription_revision_id INTEGER NOT NULL REFERENCES exam_dictation_transcription_revisions_v2(id),
  point_observation_id INTEGER NOT NULL REFERENCES exam_dictation_point_observations_v2(id),
  review_mode TEXT NOT NULL CHECK(review_mode IN ('single','strict_batch')),
  batch_id INTEGER REFERENCES exam_dictation_review_batches_v2(id),
  teacher_evidence_text TEXT,
  reviewed_by TEXT NOT NULL CHECK(length(trim(reviewed_by)) > 0),
  created_at TEXT NOT NULL,
  CHECK((review_mode='single' AND batch_id IS NULL)
        OR (review_mode='strict_batch' AND batch_id IS NOT NULL)),
  CHECK(teacher_evidence_text IS NULL OR length(trim(teacher_evidence_text)) > 0)
);

CREATE TRIGGER trg_exam_dictation_source_scope_insert_v2
BEFORE INSERT ON exam_grade_decision_dictation_sources_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_grade_decisions_v2 d
    JOIN exam_dictation_transcription_revisions_v2 t
      ON t.id=NEW.transcription_revision_id
    JOIN exam_dictation_point_observations_v2 o
      ON o.id=NEW.point_observation_id
     AND o.transcription_revision_id=t.id
    WHERE d.id=NEW.grade_decision_id
      AND d.attempt_id=t.attempt_id
      AND d.assessment_item_id=t.assessment_item_id
      AND d.state='active'
      AND t.state='active'
  ) THEN RAISE(ABORT,'M2_DICTATION_GRADE_SOURCE_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_dictation_review_batch_update_v2
BEFORE UPDATE ON exam_dictation_review_batches_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_dictation_review_batch_delete_v2
BEFORE DELETE ON exam_dictation_review_batches_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_dictation_review_item_update_v2
BEFORE UPDATE ON exam_dictation_review_batch_items_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_dictation_review_item_delete_v2
BEFORE DELETE ON exam_dictation_review_batch_items_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_dictation_grade_source_update_v2
BEFORE UPDATE ON exam_grade_decision_dictation_sources_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_dictation_grade_source_delete_v2
BEFORE DELETE ON exam_grade_decision_dictation_sources_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_REVIEW_IMMUTABLE'); END;
