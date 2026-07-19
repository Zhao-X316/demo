-- K1 L2→L3：AI 知识/能力链接建议草稿与老师复核。
-- AI 输出只追加为草稿；正式 link set 与质量晋级只由老师确认事务创建。

CREATE TABLE k1_link_suggestion_drafts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  knowledge_map_id INTEGER NOT NULL REFERENCES k1_knowledge_maps(id),
  ai_run_id INTEGER NOT NULL UNIQUE REFERENCES ai_runs(id),
  input_hash TEXT NOT NULL CHECK(length(input_hash) = 64),
  suggestion_json TEXT NOT NULL,
  suggestion_state TEXT NOT NULL
    CHECK(suggestion_state IN ('ready','needs_review','blocked')),
  confidence REAL NOT NULL CHECK(confidence >= 0 AND confidence <= 1),
  issue_codes_json TEXT NOT NULL,
  content_hash TEXT NOT NULL CHECK(length(content_hash) = 64),
  created_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(suggestion_json), 0) = 1),
  CHECK(COALESCE(json_type(suggestion_json) = 'object', 0)),
  CHECK(COALESCE(json_type(suggestion_json, '$.schemaVersion') = 'integer', 0)),
  CHECK(COALESCE(json_valid(issue_codes_json), 0) = 1),
  CHECK(COALESCE(json_type(issue_codes_json) = 'array', 0))
);
CREATE INDEX idx_k1_link_suggestion_scope
  ON k1_link_suggestion_drafts(question_version_id, knowledge_map_id, id DESC);

CREATE TABLE k1_link_reviews (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE,
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  question_version_id INTEGER NOT NULL UNIQUE REFERENCES k1_question_versions(id),
  knowledge_map_id INTEGER NOT NULL REFERENCES k1_knowledge_maps(id),
  suggestion_draft_id INTEGER REFERENCES k1_link_suggestion_drafts(id),
  confirmed_links_json TEXT NOT NULL,
  result_link_set_id INTEGER NOT NULL UNIQUE REFERENCES k1_link_sets(id),
  result_quality TEXT NOT NULL CHECK(result_quality = 'L3'),
  reviewed_by TEXT NOT NULL,
  note TEXT,
  reviewed_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(confirmed_links_json), 0) = 1),
  CHECK(COALESCE(json_type(confirmed_links_json) = 'array', 0))
);

CREATE TRIGGER trg_k1_link_suggestion_immutable_update
BEFORE UPDATE ON k1_link_suggestion_drafts
BEGIN SELECT RAISE(ABORT, 'K1_LINK_SUGGESTION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_link_suggestion_immutable_delete
BEFORE DELETE ON k1_link_suggestion_drafts
BEGIN SELECT RAISE(ABORT, 'K1_LINK_SUGGESTION_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_link_review_immutable_update
BEFORE UPDATE ON k1_link_reviews
BEGIN SELECT RAISE(ABORT, 'K1_LINK_REVIEW_IMMUTABLE'); END;
CREATE TRIGGER trg_k1_link_review_immutable_delete
BEFORE DELETE ON k1_link_reviews
BEGIN SELECT RAISE(ABORT, 'K1_LINK_REVIEW_IMMUTABLE'); END;
