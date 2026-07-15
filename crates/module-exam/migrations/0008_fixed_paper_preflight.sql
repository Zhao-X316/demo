-- M2-B3a：固定试卷原始资料、答案权威候选和三路预检快照。
-- 继续复用 exam_ingest_batches/pages、页面匹配/配准/题区和 T6 客观题账本；
-- 本迁移不创建第二套页面身份或评分状态机。

CREATE TABLE exam_fixed_input_documents_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  ingest_batch_id INTEGER NOT NULL REFERENCES exam_ingest_batches_v2(id),
  source_artifact_id INTEGER NOT NULL REFERENCES artifacts(id),
  document_role TEXT NOT NULL CHECK(document_role IN ('student_work','answer_source')),
  source_format TEXT NOT NULL CHECK(source_format IN ('jpeg','pdf','text')),
  import_index INTEGER NOT NULL CHECK(import_index >= 0),
  page_count INTEGER NOT NULL CHECK(page_count > 0),
  idempotency_key TEXT NOT NULL CHECK(length(trim(idempotency_key)) > 0),
  state TEXT NOT NULL DEFAULT 'registered' CHECK(state IN ('registered','expanded','voided')),
  created_by TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(ingest_batch_id, idempotency_key),
  UNIQUE(ingest_batch_id, source_artifact_id, document_role),
  UNIQUE(ingest_batch_id, document_role, import_index),
  CHECK(document_role='answer_source' OR source_format IN ('jpeg','pdf')),
  CHECK(source_format<>'jpeg' OR page_count=1)
);

CREATE TABLE exam_answer_authority_candidates_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  ingest_batch_id INTEGER NOT NULL REFERENCES exam_ingest_batches_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  source_kind TEXT NOT NULL CHECK(source_kind IN (
    'selected_k1','uploaded_official','manual_confirmed','ai_draft'
  )),
  source_priority INTEGER NOT NULL CHECK(source_priority BETWEEN 2 AND 5),
  answer_key_version_id INTEGER REFERENCES k1_answer_key_versions(id),
  source_artifact_id INTEGER REFERENCES artifacts(id),
  candidate_answer_json TEXT NOT NULL,
  answer_fingerprint TEXT NOT NULL CHECK(length(answer_fingerprint)=64),
  source_anchor_json TEXT NOT NULL,
  teacher_confirmed INTEGER NOT NULL CHECK(teacher_confirmed IN (0,1)),
  idempotency_key TEXT NOT NULL CHECK(length(trim(idempotency_key)) > 0),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','rejected','voided')),
  created_by_type TEXT NOT NULL CHECK(created_by_type IN ('system','teacher')),
  created_by TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(ingest_batch_id, assessment_item_id, idempotency_key),
  CHECK(COALESCE(json_valid(candidate_answer_json), 0)=1),
  CHECK(COALESCE(json_type(candidate_answer_json), 0)='object'),
  CHECK(COALESCE(json_type(candidate_answer_json, '$.schema_version'), 0)='integer'),
  CHECK(COALESCE(json_valid(source_anchor_json), 0)=1),
  CHECK(COALESCE(json_type(source_anchor_json), 0)='object'),
  CHECK(COALESCE(json_type(source_anchor_json, '$.schema_version'), 0)='integer'),
  CHECK(
    (source_kind='selected_k1' AND source_priority=2
      AND teacher_confirmed=1 AND answer_key_version_id IS NOT NULL)
    OR (source_kind='uploaded_official' AND source_priority=3
      AND ((teacher_confirmed=0) OR answer_key_version_id IS NOT NULL)
      AND source_artifact_id IS NOT NULL)
    OR (source_kind='manual_confirmed' AND source_priority=4
      AND teacher_confirmed=1 AND answer_key_version_id IS NOT NULL)
    OR (source_kind='ai_draft' AND source_priority=5
      AND teacher_confirmed=0 AND answer_key_version_id IS NULL)
  ),
  CHECK((created_by_type='teacher' AND created_by IS NOT NULL
         AND length(trim(created_by)) > 0)
        OR (created_by_type='system' AND created_by IS NULL)),
  CHECK((source_kind='ai_draft' AND created_by_type='system')
        OR (source_kind<>'ai_draft' AND created_by_type='teacher'))
);
CREATE INDEX idx_exam_answer_authority_active_v2
  ON exam_answer_authority_candidates_v2(
    ingest_batch_id, assessment_item_id, source_priority, state
  );

CREATE TABLE exam_fixed_preflight_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  ingest_batch_id INTEGER NOT NULL REFERENCES exam_ingest_batches_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  snapshot_hash TEXT NOT NULL CHECK(length(snapshot_hash)=64),
  expected_pages_per_attempt INTEGER NOT NULL CHECK(expected_pages_per_attempt > 0),
  route TEXT NOT NULL CHECK(route IN (
    'ready_for_batch_confirm','review_required','blocked'
  )),
  target_count INTEGER NOT NULL CHECK(target_count >= 0),
  ready_count INTEGER NOT NULL CHECK(ready_count >= 0),
  review_count INTEGER NOT NULL CHECK(review_count >= 0),
  blocked_count INTEGER NOT NULL CHECK(blocked_count >= 0),
  completed_count INTEGER NOT NULL CHECK(completed_count >= 0),
  reason_codes_json TEXT NOT NULL,
  answer_authority_json TEXT NOT NULL,
  grouping_json TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_by_type TEXT NOT NULL CHECK(created_by_type IN ('system','teacher')),
  created_by TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(ingest_batch_id, revision),
  CHECK(target_count=ready_count+review_count+blocked_count+completed_count),
  CHECK(COALESCE(json_valid(reason_codes_json), 0)=1),
  CHECK(COALESCE(json_type(reason_codes_json), 0)='object'),
  CHECK(COALESCE(json_type(reason_codes_json, '$.schema_version'), 0)='integer'),
  CHECK(COALESCE(json_valid(answer_authority_json), 0)=1),
  CHECK(COALESCE(json_type(answer_authority_json), 0)='object'),
  CHECK(COALESCE(json_type(answer_authority_json, '$.schema_version'), 0)='integer'),
  CHECK(COALESCE(json_valid(grouping_json), 0)=1),
  CHECK(COALESCE(json_type(grouping_json), 0)='object'),
  CHECK(COALESCE(json_type(grouping_json, '$.schema_version'), 0)='integer'),
  CHECK((created_by_type='teacher' AND created_by IS NOT NULL
         AND length(trim(created_by)) > 0)
        OR (created_by_type='system' AND created_by IS NULL))
);
CREATE UNIQUE INDEX idx_exam_fixed_preflight_active_v2
  ON exam_fixed_preflight_revisions_v2(ingest_batch_id) WHERE state='active';

-- 原始资料与答案候选内容不可覆盖；只允许显式改变生命周期。
CREATE TRIGGER trg_exam_fixed_input_content_guard_v2
BEFORE UPDATE ON exam_fixed_input_documents_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.ingest_batch_id IS NOT OLD.ingest_batch_id
  OR NEW.source_artifact_id IS NOT OLD.source_artifact_id
  OR NEW.document_role IS NOT OLD.document_role
  OR NEW.source_format IS NOT OLD.source_format
  OR NEW.import_index IS NOT OLD.import_index
  OR NEW.page_count IS NOT OLD.page_count
  OR NEW.idempotency_key IS NOT OLD.idempotency_key
  OR NEW.created_by_type IS NOT OLD.created_by_type
  OR NEW.created_by IS NOT OLD.created_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_FIXED_INPUT_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_fixed_input_delete_v2
BEFORE DELETE ON exam_fixed_input_documents_v2
BEGIN SELECT RAISE(ABORT, 'M2_FIXED_INPUT_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_answer_authority_content_guard_v2
BEFORE UPDATE ON exam_answer_authority_candidates_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.ingest_batch_id IS NOT OLD.ingest_batch_id
  OR NEW.assessment_item_id IS NOT OLD.assessment_item_id
  OR NEW.source_kind IS NOT OLD.source_kind
  OR NEW.source_priority IS NOT OLD.source_priority
  OR NEW.answer_key_version_id IS NOT OLD.answer_key_version_id
  OR NEW.source_artifact_id IS NOT OLD.source_artifact_id
  OR NEW.candidate_answer_json IS NOT OLD.candidate_answer_json
  OR NEW.answer_fingerprint IS NOT OLD.answer_fingerprint
  OR NEW.source_anchor_json IS NOT OLD.source_anchor_json
  OR NEW.teacher_confirmed IS NOT OLD.teacher_confirmed
  OR NEW.idempotency_key IS NOT OLD.idempotency_key
  OR NEW.created_by IS NOT OLD.created_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_ANSWER_AUTHORITY_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_answer_authority_delete_v2
BEFORE DELETE ON exam_answer_authority_candidates_v2
BEGIN SELECT RAISE(ABORT, 'M2_ANSWER_AUTHORITY_IMMUTABLE'); END;

-- 预检是可复现快照；旧 revision 只能 active -> superseded/voided。
CREATE TRIGGER trg_exam_fixed_preflight_content_guard_v2
BEFORE UPDATE ON exam_fixed_preflight_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.ingest_batch_id IS NOT OLD.ingest_batch_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.snapshot_hash IS NOT OLD.snapshot_hash
  OR NEW.expected_pages_per_attempt IS NOT OLD.expected_pages_per_attempt
  OR NEW.route IS NOT OLD.route
  OR NEW.target_count IS NOT OLD.target_count
  OR NEW.ready_count IS NOT OLD.ready_count
  OR NEW.review_count IS NOT OLD.review_count
  OR NEW.blocked_count IS NOT OLD.blocked_count
  OR NEW.completed_count IS NOT OLD.completed_count
  OR NEW.reason_codes_json IS NOT OLD.reason_codes_json
  OR NEW.answer_authority_json IS NOT OLD.answer_authority_json
  OR NEW.grouping_json IS NOT OLD.grouping_json
  OR NEW.created_by_type IS NOT OLD.created_by_type
  OR NEW.created_by IS NOT OLD.created_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_FIXED_PREFLIGHT_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_fixed_preflight_state_guard_v2
BEFORE UPDATE OF state ON exam_fixed_preflight_revisions_v2
WHEN NOT (
  OLD.state='active' AND NEW.state IN ('superseded','voided')
  OR OLD.state=NEW.state
)
BEGIN SELECT RAISE(ABORT, 'M2_FIXED_PREFLIGHT_STATE_INVALID'); END;
CREATE TRIGGER trg_exam_fixed_preflight_delete_v2
BEFORE DELETE ON exam_fixed_preflight_revisions_v2
BEGIN SELECT RAISE(ABORT, 'M2_FIXED_PREFLIGHT_IMMUTABLE'); END;
