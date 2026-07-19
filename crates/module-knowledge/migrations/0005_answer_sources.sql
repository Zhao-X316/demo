-- K1-5 独立答案资料与题目来源匹配。
--
-- 答案图片/PDF/文本/Office 文件必须绑定一个已经过老师题面确认的 K1 来源文档。
-- AI 输出只落匹配草稿；缺题也落 missing 草稿。老师确认后才创建不可变答案/rubric
-- 版本并通过既有质量事件晋级 L1/L2，不创建作业、成绩或学习证据。

CREATE TABLE k1_answer_source_documents (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  idempotency_key TEXT NOT NULL UNIQUE,
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  owner_scope TEXT NOT NULL CHECK(owner_scope = 'personal'),
  owner_id TEXT NOT NULL CHECK(length(trim(owner_id)) > 0),
  question_source_document_id INTEGER NOT NULL REFERENCES k1_source_documents(id),
  source_artifact_id INTEGER NOT NULL REFERENCES artifacts(id),
  source_format TEXT NOT NULL CHECK(source_format IN ('jpeg','pdf','text','docx','xlsx')),
  source_hash TEXT NOT NULL CHECK(length(source_hash) = 64),
  page_count INTEGER NOT NULL CHECK(page_count BETWEEN 1 AND 200),
  extraction_version TEXT NOT NULL CHECK(length(trim(extraction_version)) > 0),
  created_by TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at TEXT NOT NULL,
  UNIQUE(
    owner_scope, owner_id, question_source_document_id, source_artifact_id, extraction_version
  )
);

CREATE INDEX idx_k1_answer_source_documents_owner
  ON k1_answer_source_documents(owner_scope, owner_id, created_at DESC, id DESC);

CREATE TABLE k1_answer_source_extraction_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  answer_source_document_id INTEGER NOT NULL REFERENCES k1_answer_source_documents(id),
  ai_run_id INTEGER NOT NULL UNIQUE REFERENCES ai_runs(id),
  output_hash TEXT NOT NULL CHECK(length(output_hash) = 64),
  extraction_state TEXT NOT NULL CHECK(extraction_state IN ('ready','needs_review','blocked')),
  confidence REAL NOT NULL CHECK(confidence BETWEEN 0.0 AND 1.0),
  issue_codes_json TEXT NOT NULL,
  target_count INTEGER NOT NULL CHECK(target_count > 0),
  matched_count INTEGER NOT NULL CHECK(matched_count BETWEEN 0 AND target_count),
  created_at TEXT NOT NULL,
  UNIQUE(answer_source_document_id, output_hash)
);

CREATE INDEX idx_k1_answer_source_runs_document
  ON k1_answer_source_extraction_runs(answer_source_document_id, id DESC);

CREATE TABLE k1_answer_match_drafts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  extraction_run_id INTEGER NOT NULL REFERENCES k1_answer_source_extraction_runs(id),
  source_question_draft_id INTEGER NOT NULL REFERENCES k1_source_question_drafts(id),
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  order_index INTEGER NOT NULL CHECK(order_index > 0),
  question_no TEXT,
  candidate_state TEXT NOT NULL CHECK(candidate_state IN ('ready','needs_review','missing')),
  answer_json TEXT,
  source_anchor_json TEXT,
  confidence REAL NOT NULL CHECK(confidence BETWEEN 0.0 AND 1.0),
  content_hash TEXT NOT NULL CHECK(length(content_hash) = 64),
  created_at TEXT NOT NULL,
  UNIQUE(extraction_run_id, question_version_id),
  CHECK(
    (candidate_state = 'missing' AND answer_json IS NULL AND source_anchor_json IS NULL)
    OR
    (candidate_state IN ('ready','needs_review')
      AND COALESCE(json_valid(answer_json), 0) = 1
      AND COALESCE(json_type(answer_json) = 'object', 0)
      AND COALESCE(json_type(answer_json, '$.schema_version') = 'integer', 0)
      AND COALESCE(json_valid(source_anchor_json), 0) = 1
      AND COALESCE(json_type(source_anchor_json) = 'object', 0)
      AND COALESCE(json_type(source_anchor_json, '$.schema_version') = 'integer', 0))
  )
);

CREATE INDEX idx_k1_answer_match_drafts_run
  ON k1_answer_match_drafts(extraction_run_id, order_index, id);

CREATE TABLE k1_answer_match_reviews (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE,
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  answer_match_draft_id INTEGER NOT NULL UNIQUE REFERENCES k1_answer_match_drafts(id),
  corrected_answer_json TEXT NOT NULL,
  result_answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  result_rubric_version_id INTEGER REFERENCES k1_rubric_versions(id),
  result_quality TEXT NOT NULL CHECK(result_quality IN ('L1','L2')),
  reviewed_by TEXT NOT NULL CHECK(length(trim(reviewed_by)) > 0),
  note TEXT,
  reviewed_at TEXT NOT NULL,
  CHECK(
    COALESCE(json_valid(corrected_answer_json), 0) = 1
    AND COALESCE(json_type(corrected_answer_json) = 'object', 0)
    AND COALESCE(json_type(corrected_answer_json, '$.schema_version') = 'integer', 0)
  )
);

CREATE INDEX idx_k1_answer_match_reviews_result
  ON k1_answer_match_reviews(result_answer_key_version_id, result_rubric_version_id);

CREATE TRIGGER trg_k1_answer_source_documents_update_guard
BEFORE UPDATE ON k1_answer_source_documents
BEGIN
  SELECT RAISE(ABORT, 'K1_ANSWER_SOURCE_DOCUMENT_IMMUTABLE');
END;

CREATE TRIGGER trg_k1_answer_source_documents_delete_guard
BEFORE DELETE ON k1_answer_source_documents
BEGIN
  SELECT RAISE(ABORT, 'K1_ANSWER_SOURCE_DOCUMENT_DELETE_BLOCKED');
END;

CREATE TRIGGER trg_k1_answer_source_runs_update_guard
BEFORE UPDATE ON k1_answer_source_extraction_runs
BEGIN
  SELECT RAISE(ABORT, 'K1_ANSWER_SOURCE_RUN_IMMUTABLE');
END;

CREATE TRIGGER trg_k1_answer_source_runs_delete_guard
BEFORE DELETE ON k1_answer_source_extraction_runs
BEGIN
  SELECT RAISE(ABORT, 'K1_ANSWER_SOURCE_RUN_DELETE_BLOCKED');
END;

CREATE TRIGGER trg_k1_answer_match_drafts_update_guard
BEFORE UPDATE ON k1_answer_match_drafts
BEGIN
  SELECT RAISE(ABORT, 'K1_ANSWER_MATCH_DRAFT_IMMUTABLE');
END;

CREATE TRIGGER trg_k1_answer_match_drafts_delete_guard
BEFORE DELETE ON k1_answer_match_drafts
BEGIN
  SELECT RAISE(ABORT, 'K1_ANSWER_MATCH_DRAFT_DELETE_BLOCKED');
END;

CREATE TRIGGER trg_k1_answer_match_reviews_update_guard
BEFORE UPDATE ON k1_answer_match_reviews
BEGIN
  SELECT RAISE(ABORT, 'K1_ANSWER_MATCH_REVIEW_IMMUTABLE');
END;

CREATE TRIGGER trg_k1_answer_match_reviews_delete_guard
BEFORE DELETE ON k1_answer_match_reviews
BEGIN
  SELECT RAISE(ABORT, 'K1_ANSWER_MATCH_REVIEW_DELETE_BLOCKED');
END;
