-- K1-4b：老师换题、排序与可打印试卷的不可变版本账本。
--
-- 每次确认都从当前蓝图/上一打印版追加一个 confirmed assessment version，
-- 同时冻结题卷和答案卷 HTML。它不选择未来上传默认版本，也不重绑历史作答。

CREATE TABLE exam_blueprint_paper_editions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE CHECK(length(trim(request_key)) > 0),
  request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
  assembly_id INTEGER NOT NULL REFERENCES exam_blueprint_assemblies_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  source_assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  assessment_version_id INTEGER NOT NULL UNIQUE REFERENCES exam_assessment_versions_v2(id),
  supersedes_edition_id INTEGER REFERENCES exam_blueprint_paper_editions_v2(id),
  title TEXT NOT NULL CHECK(length(trim(title)) > 0),
  layout_json TEXT NOT NULL,
  item_set_hash TEXT NOT NULL CHECK(length(item_set_hash) = 64),
  question_html_sha256 TEXT NOT NULL CHECK(length(question_html_sha256) = 64),
  answer_html_sha256 TEXT NOT NULL CHECK(length(answer_html_sha256) = 64),
  question_html TEXT NOT NULL,
  answer_html TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'confirmed' CHECK(state = 'confirmed'),
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by)) > 0),
  confirmed_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(assembly_id, revision),
  CHECK(supersedes_edition_id IS NULL OR supersedes_edition_id <> id),
  CHECK(COALESCE(json_valid(layout_json), 0) = 1),
  CHECK(COALESCE(json_type(layout_json) = 'object', 0)),
  CHECK(COALESCE(json_type(layout_json, '$.schema_version') = 'integer', 0))
);

CREATE TABLE exam_blueprint_paper_items_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  edition_id INTEGER NOT NULL REFERENCES exam_blueprint_paper_editions_v2(id),
  source_slot_order_index INTEGER NOT NULL CHECK(source_slot_order_index >= 0),
  assessment_item_id INTEGER NOT NULL UNIQUE REFERENCES exam_assessment_items_v2(id),
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  link_set_id INTEGER NOT NULL REFERENCES k1_link_sets(id),
  order_index INTEGER NOT NULL CHECK(order_index >= 0),
  score REAL NOT NULL CHECK(score > 0),
  page_break_before INTEGER NOT NULL DEFAULT 0 CHECK(page_break_before IN (0,1)),
  created_at TEXT NOT NULL,
  UNIQUE(edition_id, source_slot_order_index),
  UNIQUE(edition_id, order_index),
  UNIQUE(edition_id, question_version_id)
);

CREATE INDEX idx_exam_blueprint_paper_editions_v2
  ON exam_blueprint_paper_editions_v2(assembly_id,revision DESC,id DESC);

CREATE TRIGGER trg_exam_blueprint_paper_edition_insert_v2
BEFORE INSERT ON exam_blueprint_paper_editions_v2
BEGIN
  SELECT CASE WHEN NEW.revision <> (
    SELECT COALESCE(MAX(edition.revision),0)+1
    FROM exam_blueprint_paper_editions_v2 edition
    WHERE edition.assembly_id=NEW.assembly_id
  ) THEN RAISE(ABORT,'K1_PAPER_EDITION_REVISION_MISMATCH') END;

  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_blueprint_assemblies_v2 assembly
    JOIN exam_assessment_versions_v2 source
      ON source.id=NEW.source_assessment_version_id
     AND source.assessment_id=assembly.assessment_id
     AND source.state='confirmed'
    JOIN exam_assessment_versions_v2 target
      ON target.id=NEW.assessment_version_id
     AND target.assessment_id=assembly.assessment_id
     AND target.state='confirmed'
     AND target.supersedes_version_id=source.id
    WHERE assembly.id=NEW.assembly_id
      AND assembly.state='confirmed'
  ) THEN RAISE(ABORT,'K1_PAPER_ASSESSMENT_VERSION_MISMATCH') END;

  SELECT CASE WHEN NEW.revision=1 AND (
    NEW.supersedes_edition_id IS NOT NULL
    OR NEW.source_assessment_version_id <> (
      SELECT assembly.assessment_version_id
      FROM exam_blueprint_assemblies_v2 assembly
      WHERE assembly.id=NEW.assembly_id
    )
  ) THEN RAISE(ABORT,'K1_PAPER_INITIAL_SOURCE_MISMATCH') END;

  SELECT CASE WHEN NEW.revision>1 AND NOT EXISTS (
    SELECT 1
    FROM exam_blueprint_paper_editions_v2 previous
    WHERE previous.id=NEW.supersedes_edition_id
      AND previous.assembly_id=NEW.assembly_id
      AND previous.revision=NEW.revision-1
      AND previous.assessment_version_id=NEW.source_assessment_version_id
  ) THEN RAISE(ABORT,'K1_PAPER_PREVIOUS_EDITION_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_blueprint_paper_item_insert_v2
BEFORE INSERT ON exam_blueprint_paper_items_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM exam_blueprint_paper_editions_v2 edition
    JOIN exam_blueprint_assemblies_v2 assembly ON assembly.id=edition.assembly_id
    JOIN exam_blueprint_items_v2 source_item
      ON source_item.assembly_id=assembly.id
     AND source_item.order_index=NEW.source_slot_order_index
    JOIN k1_question_versions source_question
      ON source_question.id=source_item.question_version_id
    JOIN k1_question_versions target_question
      ON target_question.id=NEW.question_version_id
     AND target_question.question_type=source_question.question_type
    JOIN exam_assessment_items_v2 assessment_item
      ON assessment_item.id=NEW.assessment_item_id
     AND assessment_item.assessment_version_id=edition.assessment_version_id
     AND assessment_item.question_version_id=NEW.question_version_id
     AND assessment_item.answer_key_version_id=NEW.answer_key_version_id
     AND assessment_item.rubric_version_id=NEW.rubric_version_id
     AND assessment_item.link_set_id=NEW.link_set_id
     AND assessment_item.order_index=NEW.order_index
     AND assessment_item.state='active'
    WHERE edition.id=NEW.edition_id
      AND ABS(source_item.score-NEW.score) < 0.000001
      AND ABS(assessment_item.score-NEW.score) < 0.000001
  ) THEN RAISE(ABORT,'K1_PAPER_ITEM_SLOT_MISMATCH') END;
END;

CREATE TRIGGER trg_exam_blueprint_paper_edition_update_v2
BEFORE UPDATE ON exam_blueprint_paper_editions_v2
BEGIN SELECT RAISE(ABORT,'K1_PAPER_EDITION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_blueprint_paper_edition_delete_v2
BEFORE DELETE ON exam_blueprint_paper_editions_v2
BEGIN SELECT RAISE(ABORT,'K1_PAPER_EDITION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_blueprint_paper_item_update_v2
BEFORE UPDATE ON exam_blueprint_paper_items_v2
BEGIN SELECT RAISE(ABORT,'K1_PAPER_EDITION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_blueprint_paper_item_delete_v2
BEFORE DELETE ON exam_blueprint_paper_items_v2
BEGIN SELECT RAISE(ABORT,'K1_PAPER_EDITION_IMMUTABLE'); END;
