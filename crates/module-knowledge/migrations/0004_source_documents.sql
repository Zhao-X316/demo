-- K1-4 独立空白卷 / 电子题目文件来源。
--
-- 这一层只保存干净教学内容的来源、一次不可变提取结果及老师处理结论。
-- 未经老师确认的机器草稿不会进入 k1_questions；确认后也只创建 L0 草稿，
-- 不伪造答案、rubric、知识链接或可批改作业。

CREATE TABLE k1_source_documents (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  idempotency_key TEXT NOT NULL UNIQUE,
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  owner_scope TEXT NOT NULL CHECK(owner_scope = 'personal'),
  owner_id TEXT NOT NULL CHECK(length(trim(owner_id)) > 0),
  source_artifact_id INTEGER NOT NULL REFERENCES artifacts(id),
  source_type TEXT NOT NULL CHECK(source_type IN ('blank_paper','source_document')),
  source_format TEXT NOT NULL CHECK(source_format IN ('jpeg','pdf','text','docx','xlsx')),
  source_hash TEXT NOT NULL CHECK(length(source_hash) = 64),
  page_count INTEGER NOT NULL CHECK(page_count BETWEEN 1 AND 200),
  extraction_version TEXT NOT NULL CHECK(length(trim(extraction_version)) > 0),
  created_by TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at TEXT NOT NULL,
  UNIQUE(owner_scope, owner_id, source_artifact_id, source_type, extraction_version)
);

CREATE INDEX idx_k1_source_documents_owner
  ON k1_source_documents(owner_scope, owner_id, created_at DESC, id DESC);

CREATE TABLE k1_source_extraction_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  source_document_id INTEGER NOT NULL REFERENCES k1_source_documents(id),
  ai_run_id INTEGER NOT NULL UNIQUE REFERENCES ai_runs(id),
  output_hash TEXT NOT NULL CHECK(length(output_hash) = 64),
  extraction_state TEXT NOT NULL
    CHECK(extraction_state IN ('ready','needs_review','blocked')),
  confidence REAL NOT NULL CHECK(confidence BETWEEN 0.0 AND 1.0),
  privacy_json TEXT NOT NULL,
  issue_codes_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(source_document_id, output_hash)
);

CREATE INDEX idx_k1_source_extraction_runs_document
  ON k1_source_extraction_runs(source_document_id, id DESC);

CREATE TABLE k1_source_question_drafts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  extraction_run_id INTEGER NOT NULL REFERENCES k1_source_extraction_runs(id),
  order_index INTEGER NOT NULL CHECK(order_index > 0),
  question_no TEXT,
  question_type TEXT NOT NULL
    CHECK(question_type IN ('single','multiple','true_false','fill_blank','short_answer')),
  stem TEXT NOT NULL CHECK(length(trim(stem)) > 0),
  material_text TEXT,
  max_score REAL NOT NULL CHECK(max_score > 0.0),
  options_json TEXT NOT NULL,
  source_anchor_json TEXT NOT NULL,
  confidence REAL NOT NULL CHECK(confidence BETWEEN 0.0 AND 1.0),
  content_hash TEXT NOT NULL CHECK(length(content_hash) = 64),
  created_at TEXT NOT NULL,
  UNIQUE(extraction_run_id, order_index)
);

CREATE INDEX idx_k1_source_question_drafts_run
  ON k1_source_question_drafts(extraction_run_id, order_index, id);

CREATE TABLE k1_source_question_reviews (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE,
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  source_draft_id INTEGER NOT NULL UNIQUE REFERENCES k1_source_question_drafts(id),
  action TEXT NOT NULL CHECK(action IN ('accept','discard')),
  corrected_payload_json TEXT,
  result_kind TEXT NOT NULL
    CHECK(result_kind IN ('exact_reused','draft_created','discarded')),
  result_question_version_id INTEGER REFERENCES k1_question_versions(id),
  reviewed_by TEXT NOT NULL CHECK(length(trim(reviewed_by)) > 0),
  note TEXT,
  reviewed_at TEXT NOT NULL,
  CHECK(
    (action = 'discard' AND result_kind = 'discarded'
      AND result_question_version_id IS NULL)
    OR
    (action = 'accept' AND result_kind IN ('exact_reused','draft_created')
      AND result_question_version_id IS NOT NULL)
  )
);

CREATE INDEX idx_k1_source_question_reviews_result
  ON k1_source_question_reviews(result_question_version_id);

CREATE TRIGGER trg_k1_source_documents_update_guard
BEFORE UPDATE ON k1_source_documents
BEGIN
  SELECT RAISE(ABORT, 'K1_SOURCE_DOCUMENT_IMMUTABLE');
END;

CREATE TRIGGER trg_k1_source_documents_delete_guard
BEFORE DELETE ON k1_source_documents
BEGIN
  SELECT RAISE(ABORT, 'K1_SOURCE_DOCUMENT_DELETE_BLOCKED');
END;

CREATE TRIGGER trg_k1_source_extraction_runs_update_guard
BEFORE UPDATE ON k1_source_extraction_runs
BEGIN
  SELECT RAISE(ABORT, 'K1_SOURCE_EXTRACTION_RUN_IMMUTABLE');
END;

CREATE TRIGGER trg_k1_source_extraction_runs_delete_guard
BEFORE DELETE ON k1_source_extraction_runs
BEGIN
  SELECT RAISE(ABORT, 'K1_SOURCE_EXTRACTION_RUN_DELETE_BLOCKED');
END;

CREATE TRIGGER trg_k1_source_question_drafts_update_guard
BEFORE UPDATE ON k1_source_question_drafts
BEGIN
  SELECT RAISE(ABORT, 'K1_SOURCE_QUESTION_DRAFT_IMMUTABLE');
END;

CREATE TRIGGER trg_k1_source_question_drafts_delete_guard
BEFORE DELETE ON k1_source_question_drafts
BEGIN
  SELECT RAISE(ABORT, 'K1_SOURCE_QUESTION_DRAFT_DELETE_BLOCKED');
END;

CREATE TRIGGER trg_k1_source_question_reviews_update_guard
BEFORE UPDATE ON k1_source_question_reviews
BEGIN
  SELECT RAISE(ABORT, 'K1_SOURCE_QUESTION_REVIEW_IMMUTABLE');
END;

CREATE TRIGGER trg_k1_source_question_reviews_delete_guard
BEFORE DELETE ON k1_source_question_reviews
BEGIN
  SELECT RAISE(ABORT, 'K1_SOURCE_QUESTION_REVIEW_DELETE_BLOCKED');
END;
