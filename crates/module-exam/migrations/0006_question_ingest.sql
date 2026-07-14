-- M2.5-0/1：批改题目自动沉淀。
-- 题目提取任务通过 core outbox 可靠投递；学生作答证据与可复用题目资产严格分离。

CREATE TABLE exam_question_ingest_jobs_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  idempotency_key TEXT NOT NULL UNIQUE CHECK(length(trim(idempotency_key)) > 0),
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  answer_region_revision_id INTEGER NOT NULL REFERENCES exam_answer_region_revisions_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  owner_id TEXT NOT NULL CHECK(length(trim(owner_id)) > 0),
  source_type TEXT NOT NULL CHECK(source_type IN (
    'blank_paper','source_document','student_paper','workbook'
  )),
  extraction_version TEXT NOT NULL CHECK(length(trim(extraction_version)) > 0),
  structured_payload_json TEXT,
  content_hash TEXT CHECK(content_hash IS NULL OR length(content_hash) = 64),
  privacy_status TEXT NOT NULL CHECK(privacy_status IN (
    'reusable_asset','text_only','rejected'
  )),
  reusable_artifact_id INTEGER REFERENCES artifacts(id),
  state TEXT NOT NULL CHECK(state IN (
    'pending','processing','completed','failed'
  )),
  attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts >= 0),
  active_event_id INTEGER REFERENCES outbox_events(id),
  error_meta_json TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  CHECK(
    structured_payload_json IS NULL OR (
      COALESCE(json_valid(structured_payload_json), 0) = 1
      AND COALESCE(json_type(structured_payload_json) = 'object', 0)
      AND COALESCE(json_type(structured_payload_json, '$.schema_version') = 'integer', 0)
    )
  ),
  CHECK(
    error_meta_json IS NULL OR (
      COALESCE(json_valid(error_meta_json), 0) = 1
      AND COALESCE(json_type(error_meta_json) = 'object', 0)
      AND COALESCE(json_type(error_meta_json, '$.schema_version') = 'integer', 0)
    )
  ),
  CHECK(source_type <> 'student_paper' OR reusable_artifact_id IS NULL),
  CHECK((privacy_status='reusable_asset' AND reusable_artifact_id IS NOT NULL)
        OR (privacy_status IN ('text_only','rejected') AND reusable_artifact_id IS NULL)),
  CHECK((privacy_status='rejected' AND state='failed' AND structured_payload_json IS NULL
         AND content_hash IS NULL AND error_meta_json IS NOT NULL)
        OR privacy_status<>'rejected'),
  CHECK((state='pending' AND active_event_id IS NOT NULL)
        OR state IN ('processing','completed','failed'))
);
CREATE INDEX idx_exam_question_ingest_state_v2
  ON exam_question_ingest_jobs_v2(state, id);

CREATE TABLE exam_question_candidates_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  job_id INTEGER NOT NULL UNIQUE REFERENCES exam_question_ingest_jobs_v2(id),
  owner_id TEXT NOT NULL CHECK(length(trim(owner_id)) > 0),
  answer_region_revision_id INTEGER NOT NULL REFERENCES exam_answer_region_revisions_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  source_type TEXT NOT NULL CHECK(source_type IN (
    'blank_paper','source_document','student_paper','workbook'
  )),
  question_type TEXT NOT NULL CHECK(question_type IN (
    'single','multiple','true_false','fill_blank','short_answer'
  )),
  normalized_stem TEXT NOT NULL CHECK(length(trim(normalized_stem)) > 0),
  material_text TEXT,
  max_score REAL NOT NULL CHECK(max_score > 0),
  options_json TEXT NOT NULL,
  content_hash TEXT NOT NULL CHECK(length(content_hash) = 64),
  privacy_status TEXT NOT NULL CHECK(privacy_status IN ('reusable_asset','text_only')),
  reusable_artifact_id INTEGER REFERENCES artifacts(id),
  exact_match_question_version_id INTEGER REFERENCES k1_question_versions(id),
  created_question_version_id INTEGER REFERENCES k1_question_versions(id),
  status TEXT NOT NULL CHECK(status IN (
    'matched','candidate_created','needs_review','discarded'
  )),
  quality_issues_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(options_json), 0) = 1),
  CHECK(COALESCE(json_type(options_json) = 'object', 0)),
  CHECK(COALESCE(json_type(options_json, '$.schema_version') = 'integer', 0)),
  CHECK(COALESCE(json_valid(quality_issues_json), 0) = 1),
  CHECK(COALESCE(json_type(quality_issues_json) = 'object', 0)),
  CHECK(COALESCE(json_type(quality_issues_json, '$.schema_version') = 'integer', 0)),
  CHECK((privacy_status='reusable_asset' AND reusable_artifact_id IS NOT NULL)
        OR (privacy_status='text_only' AND reusable_artifact_id IS NULL)),
  CHECK((status='matched' AND exact_match_question_version_id IS NOT NULL
         AND created_question_version_id IS NULL)
        OR (status='candidate_created' AND exact_match_question_version_id IS NULL
            AND created_question_version_id IS NOT NULL)
        OR status IN ('needs_review','discarded'))
);

CREATE TABLE exam_duplicate_candidates_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  candidate_id INTEGER NOT NULL REFERENCES exam_question_candidates_v2(id),
  target_question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  match_kind TEXT NOT NULL CHECK(match_kind IN ('exact','similar','variant','same_family')),
  score REAL NOT NULL CHECK(score >= 0.0 AND score <= 1.0),
  reason_code TEXT NOT NULL CHECK(length(trim(reason_code)) > 0),
  state TEXT NOT NULL DEFAULT 'open' CHECK(state IN ('open','accepted','rejected','voided')),
  created_at TEXT NOT NULL,
  UNIQUE(candidate_id, target_question_version_id, match_kind)
);

CREATE TABLE exam_assessment_item_question_links_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  candidate_id INTEGER REFERENCES exam_question_candidates_v2(id),
  link_source TEXT NOT NULL CHECK(link_source IN (
    'exact_auto','teacher_selected','candidate_promoted'
  )),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  linked_by TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(assessment_item_id, revision),
  CHECK((link_source='exact_auto' AND linked_by IS NULL)
        OR (link_source<>'exact_auto' AND linked_by IS NOT NULL AND length(trim(linked_by)) > 0))
);
CREATE UNIQUE INDEX idx_exam_item_question_link_active_v2
  ON exam_assessment_item_question_links_v2(assessment_item_id) WHERE state='active';

CREATE TRIGGER trg_exam_item_question_exact_auto_guard_v2
BEFORE INSERT ON exam_assessment_item_question_links_v2
WHEN NEW.link_source='exact_auto'
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM exam_assessment_items_v2 i
    WHERE i.id=NEW.assessment_item_id AND i.question_version_id=NEW.question_version_id
  ) THEN RAISE(ABORT, 'M2_EXACT_AUTO_CANNOT_REBIND_ITEM') END;
END;

-- 身份与提取内容不可覆盖；worker 只推进 job 状态/次数/错误和 active event。
CREATE TRIGGER trg_exam_question_ingest_job_identity_guard_v2
BEFORE UPDATE ON exam_question_ingest_jobs_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.idempotency_key IS NOT OLD.idempotency_key
  OR NEW.request_hash IS NOT OLD.request_hash
  OR NEW.answer_region_revision_id IS NOT OLD.answer_region_revision_id
  OR NEW.assessment_item_id IS NOT OLD.assessment_item_id
  OR NEW.owner_id IS NOT OLD.owner_id
  OR NEW.source_type IS NOT OLD.source_type
  OR NEW.extraction_version IS NOT OLD.extraction_version
  OR NEW.structured_payload_json IS NOT OLD.structured_payload_json
  OR NEW.content_hash IS NOT OLD.content_hash
  OR NEW.privacy_status IS NOT OLD.privacy_status
  OR NEW.reusable_artifact_id IS NOT OLD.reusable_artifact_id
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_QUESTION_INGEST_IDENTITY_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_question_ingest_job_delete_v2
BEFORE DELETE ON exam_question_ingest_jobs_v2
BEGIN SELECT RAISE(ABORT, 'M2_QUESTION_INGEST_RETAINED'); END;

CREATE TRIGGER trg_exam_question_candidate_guard_v2
BEFORE UPDATE ON exam_question_candidates_v2
BEGIN SELECT RAISE(ABORT, 'M2_QUESTION_CANDIDATE_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_question_candidate_delete_v2
BEFORE DELETE ON exam_question_candidates_v2
BEGIN SELECT RAISE(ABORT, 'M2_QUESTION_CANDIDATE_RETAINED'); END;

CREATE TRIGGER trg_exam_duplicate_candidate_content_guard_v2
BEFORE UPDATE ON exam_duplicate_candidates_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.candidate_id IS NOT OLD.candidate_id
  OR NEW.target_question_version_id IS NOT OLD.target_question_version_id
  OR NEW.match_kind IS NOT OLD.match_kind
  OR NEW.score IS NOT OLD.score
  OR NEW.reason_code IS NOT OLD.reason_code
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_DUPLICATE_CANDIDATE_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_duplicate_candidate_delete_v2
BEFORE DELETE ON exam_duplicate_candidates_v2
BEGIN SELECT RAISE(ABORT, 'M2_DUPLICATE_CANDIDATE_RETAINED'); END;

CREATE TRIGGER trg_exam_item_question_link_content_guard_v2
BEFORE UPDATE ON exam_assessment_item_question_links_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.assessment_item_id IS NOT OLD.assessment_item_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.question_version_id IS NOT OLD.question_version_id
  OR NEW.candidate_id IS NOT OLD.candidate_id
  OR NEW.link_source IS NOT OLD.link_source
  OR NEW.linked_by IS NOT OLD.linked_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_ITEM_QUESTION_LINK_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_item_question_link_delete_v2
BEFORE DELETE ON exam_assessment_item_question_links_v2
BEGIN SELECT RAISE(ABORT, 'M2_ITEM_QUESTION_LINK_RETAINED'); END;
