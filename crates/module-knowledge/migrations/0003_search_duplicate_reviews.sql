-- K1-3 结构化检索与疑似重复人工归类。
-- FTS 只保存可重建的检索投影；老师结论采用 append-only revision。

CREATE VIRTUAL TABLE k1_question_search USING fts5(
  question_version_id UNINDEXED,
  stem,
  material_text,
  options_text,
  tokenize = 'unicode61'
);

INSERT INTO k1_question_search(question_version_id, stem, material_text, options_text)
SELECT
  version.id,
  version.stem,
  COALESCE(version.material_text, ''),
  COALESCE((
    SELECT group_concat(option.label || ' ' || option.content, ' ')
    FROM k1_question_options option
    WHERE option.question_version_id = version.id
    ORDER BY option.order_index, option.id
  ), '')
FROM k1_question_versions version;

CREATE TRIGGER trg_k1_question_search_version_insert
AFTER INSERT ON k1_question_versions
BEGIN
  INSERT INTO k1_question_search(question_version_id, stem, material_text, options_text)
  VALUES (NEW.id, NEW.stem, COALESCE(NEW.material_text, ''), '');
END;

CREATE TRIGGER trg_k1_question_search_option_insert
AFTER INSERT ON k1_question_options
BEGIN
  UPDATE k1_question_search
  SET options_text = COALESCE((
    SELECT group_concat(option.label || ' ' || option.content, ' ')
    FROM k1_question_options option
    WHERE option.question_version_id = NEW.question_version_id
    ORDER BY option.order_index, option.id
  ), '')
  WHERE question_version_id = NEW.question_version_id;
END;

CREATE TABLE k1_duplicate_review_decisions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE,
  payload_hash TEXT NOT NULL CHECK(length(payload_hash) = 64),
  left_question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  right_question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  match_kind TEXT NOT NULL CHECK(match_kind IN ('exact','similar')),
  similarity_millis INTEGER NOT NULL
    CHECK(similarity_millis BETWEEN 0 AND 1000),
  decision TEXT NOT NULL CHECK(decision IN ('independent','same_family')),
  note TEXT,
  decided_by TEXT NOT NULL CHECK(length(trim(decided_by)) > 0),
  decided_at TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded')),
  created_at TEXT NOT NULL,
  CHECK(left_question_version_id < right_question_version_id),
  UNIQUE(left_question_version_id, right_question_version_id, revision)
);

CREATE UNIQUE INDEX idx_k1_duplicate_review_active
  ON k1_duplicate_review_decisions(left_question_version_id, right_question_version_id)
  WHERE state = 'active';

CREATE INDEX idx_k1_duplicate_review_versions
  ON k1_duplicate_review_decisions(left_question_version_id, right_question_version_id, revision);

CREATE TRIGGER trg_k1_duplicate_review_update_guard
BEFORE UPDATE ON k1_duplicate_review_decisions
WHEN NOT (
  OLD.state = 'active'
  AND NEW.state = 'superseded'
  AND NEW.public_id IS OLD.public_id
  AND NEW.request_key IS OLD.request_key
  AND NEW.payload_hash IS OLD.payload_hash
  AND NEW.left_question_version_id IS OLD.left_question_version_id
  AND NEW.right_question_version_id IS OLD.right_question_version_id
  AND NEW.revision IS OLD.revision
  AND NEW.match_kind IS OLD.match_kind
  AND NEW.similarity_millis IS OLD.similarity_millis
  AND NEW.decision IS OLD.decision
  AND NEW.note IS OLD.note
  AND NEW.decided_by IS OLD.decided_by
  AND NEW.decided_at IS OLD.decided_at
  AND NEW.created_at IS OLD.created_at
)
BEGIN
  SELECT RAISE(ABORT, 'K1_DUPLICATE_REVIEW_IMMUTABLE');
END;

CREATE TRIGGER trg_k1_duplicate_review_delete_guard
BEFORE DELETE ON k1_duplicate_review_decisions
BEGIN
  SELECT RAISE(ABORT, 'K1_DUPLICATE_REVIEW_DELETE_BLOCKED');
END;
