-- M2-B0：作业/版本/item/attempt/评分 revision/发布 revision 兼容底座。
-- 不迁移、不删除、不猜测旧 exam_student_answers 的作业、页面或发布归属。

CREATE TABLE exam_assessments_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  title TEXT NOT NULL CHECK(length(trim(title)) > 0),
  class_id INTEGER NOT NULL REFERENCES classes(id),
  assessment_context TEXT NOT NULL CHECK(assessment_context IN (
    'classwork','homework','quiz','exam','open_book','correction','demo'
  )),
  evidence_policy TEXT NOT NULL CHECK(evidence_policy IN (
    'include','exclude','include_low_weight','progress_only'
  )),
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','active','archived')),
  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE exam_assessment_versions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  assessment_id INTEGER NOT NULL REFERENCES exam_assessments_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  item_set_hash TEXT CHECK(item_set_hash IS NULL OR length(item_set_hash) = 64),
  template_version TEXT,
  state TEXT NOT NULL DEFAULT 'draft' CHECK(state IN ('draft','confirmed','retired')),
  supersedes_version_id INTEGER REFERENCES exam_assessment_versions_v2(id),
  created_at TEXT NOT NULL,
  confirmed_by TEXT,
  confirmed_at TEXT,
  UNIQUE(assessment_id, revision),
  CHECK(supersedes_version_id IS NULL OR supersedes_version_id <> id),
  CHECK((state='confirmed' AND item_set_hash IS NOT NULL
         AND confirmed_by IS NOT NULL AND confirmed_at IS NOT NULL)
        OR state <> 'confirmed')
);

CREATE TABLE exam_assessment_items_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  question_version_id INTEGER NOT NULL REFERENCES k1_question_versions(id),
  answer_key_version_id INTEGER NOT NULL REFERENCES k1_answer_key_versions(id),
  rubric_version_id INTEGER NOT NULL REFERENCES k1_rubric_versions(id),
  link_set_id INTEGER NOT NULL REFERENCES k1_link_sets(id),
  order_index INTEGER NOT NULL CHECK(order_index >= 0),
  score REAL NOT NULL CHECK(score > 0),
  option_order_json TEXT,
  presentation_snapshot_json TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','voided')),
  created_at TEXT NOT NULL,
  UNIQUE(assessment_version_id, order_index),
  UNIQUE(assessment_version_id, question_version_id),
  CHECK(option_order_json IS NULL OR (
    COALESCE(json_valid(option_order_json), 0) = 1
    AND COALESCE(json_type(option_order_json, '$.schema_version') = 'integer', 0)
  )),
  CHECK(COALESCE(json_valid(presentation_snapshot_json), 0) = 1),
  CHECK(COALESCE(json_type(presentation_snapshot_json) = 'object', 0)),
  CHECK(COALESCE(json_type(presentation_snapshot_json, '$.schema_version') = 'integer', 0))
);

CREATE TABLE exam_attempts_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  student_id INTEGER NOT NULL REFERENCES students(id),
  attempt_no INTEGER NOT NULL DEFAULT 1 CHECK(attempt_no > 0),
  source_kind TEXT NOT NULL CHECK(source_kind IN ('manual','image')),
  attempt_kind TEXT NOT NULL DEFAULT 'first' CHECK(attempt_kind IN ('first','correction','retry')),
  state TEXT NOT NULL DEFAULT 'grading' CHECK(state IN (
    'ingesting','grading','ready_to_publish','published','voided'
  )),
  active_publication_id INTEGER REFERENCES exam_grade_publications_v2(id),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(assessment_version_id, student_id, attempt_no)
);

CREATE TABLE exam_grade_decisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  attempt_id INTEGER NOT NULL REFERENCES exam_attempts_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  machine_grade_ai_run_id INTEGER REFERENCES ai_runs(id),
  teacher_score REAL NOT NULL CHECK(teacher_score >= 0),
  point_results_json TEXT NOT NULL,
  teacher_note TEXT,
  confirmation_level TEXT NOT NULL CHECK(confirmation_level IN (
    'teacher_accepted','teacher_corrected'
  )),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  decided_by TEXT NOT NULL,
  decided_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(attempt_id, assessment_item_id, revision),
  CHECK(COALESCE(json_valid(point_results_json), 0) = 1),
  CHECK(COALESCE(json_type(point_results_json) = 'object', 0)),
  CHECK(COALESCE(json_type(point_results_json, '$.schema_version') = 'integer', 0))
);
CREATE UNIQUE INDEX idx_exam_grade_decision_active_v2
  ON exam_grade_decisions_v2(attempt_id, assessment_item_id) WHERE state='active';

CREATE TABLE exam_grade_publications_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  state TEXT NOT NULL CHECK(state IN ('draft','published','superseded','withdrawn')),
  published_by TEXT,
  published_at TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(assessment_version_id, revision),
  CHECK((state='published' AND published_by IS NOT NULL AND published_at IS NOT NULL)
        OR state <> 'published')
);

CREATE TABLE exam_grade_publication_items_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  publication_id INTEGER NOT NULL REFERENCES exam_grade_publications_v2(id),
  attempt_id INTEGER NOT NULL REFERENCES exam_attempts_v2(id),
  grade_decision_set_hash TEXT NOT NULL CHECK(length(grade_decision_set_hash) = 64),
  total_score REAL NOT NULL CHECK(total_score >= 0),
  created_at TEXT NOT NULL,
  UNIQUE(publication_id, attempt_id)
);

CREATE TABLE exam_grade_publication_decisions_v2 (
  publication_item_id INTEGER NOT NULL REFERENCES exam_grade_publication_items_v2(id),
  attempt_id INTEGER NOT NULL REFERENCES exam_attempts_v2(id),
  grade_decision_id INTEGER NOT NULL REFERENCES exam_grade_decisions_v2(id),
  created_at TEXT NOT NULL,
  PRIMARY KEY(publication_item_id, grade_decision_id),
  UNIQUE(publication_item_id, attempt_id, grade_decision_id)
);

-- 只保存明确核对过的 legacy 关系；迁移不自动插入旧作答映射。
CREATE TABLE exam_legacy_answer_mappings_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  legacy_answer_id INTEGER NOT NULL REFERENCES exam_student_answers(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  attempt_id INTEGER NOT NULL REFERENCES exam_attempts_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','reverted')),
  verified_by TEXT NOT NULL,
  verified_at TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(legacy_answer_id, revision)
);
CREATE UNIQUE INDEX idx_exam_legacy_answer_active_v2
  ON exam_legacy_answer_mappings_v2(legacy_answer_id) WHERE state='active';

CREATE VIEW exam_legacy_answer_compat_v2 AS
SELECT a.id AS legacy_answer_id,
       'legacy_manual' AS source_kind,
       CASE WHEN m.id IS NULL THEN 'legacy_unmapped' ELSE 'mapped' END AS mapping_state,
       m.attempt_id,
       m.assessment_item_id,
       a.student_id,
       a.question_id,
       a.picked,
       a.score,
       a.status,
       a.created_at
FROM exam_student_answers a
LEFT JOIN exam_legacy_answer_mappings_v2 m
  ON m.legacy_answer_id=a.id AND m.state='active';

-- assessment item 的四个 K1 版本必须属于同一 question version。
CREATE TRIGGER trg_exam_item_k1_refs_insert_v2
BEFORE INSERT ON exam_assessment_items_v2
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM k1_question_versions q
    JOIN k1_answer_key_versions a ON a.id=NEW.answer_key_version_id
    JOIN k1_rubric_versions r ON r.id=NEW.rubric_version_id
    JOIN k1_link_sets l ON l.id=NEW.link_set_id
    WHERE q.id=NEW.question_version_id
      AND a.question_version_id=q.id
      AND r.question_version_id=q.id
      AND l.question_version_id=q.id
  ) THEN RAISE(ABORT, 'M2_K1_VERSION_MISMATCH') END;
END;

-- 只有 draft assessment version 可以调整 item；确认后固定题目与评价版本。
CREATE TRIGGER trg_exam_item_update_guard_v2
BEFORE UPDATE ON exam_assessment_items_v2
WHEN NOT EXISTS (
  SELECT 1 FROM exam_assessment_versions_v2 v
  WHERE v.id=OLD.assessment_version_id AND v.state='draft'
)
BEGIN SELECT RAISE(ABORT, 'M2_ASSESSMENT_VERSION_FROZEN'); END;
CREATE TRIGGER trg_exam_item_delete_guard_v2
BEFORE DELETE ON exam_assessment_items_v2
WHEN NOT EXISTS (
  SELECT 1 FROM exam_assessment_versions_v2 v
  WHERE v.id=OLD.assessment_version_id AND v.state='draft'
)
BEGIN SELECT RAISE(ABORT, 'M2_ASSESSMENT_VERSION_FROZEN'); END;

-- 版本内容不可改，仅允许显式生命周期字段切换。
CREATE TRIGGER trg_exam_assessment_version_content_guard_v2
BEFORE UPDATE ON exam_assessment_versions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.assessment_id IS NOT OLD.assessment_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.template_version IS NOT OLD.template_version
  OR NEW.supersedes_version_id IS NOT OLD.supersedes_version_id
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_ASSESSMENT_VERSION_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_assessment_version_delete_v2
BEFORE DELETE ON exam_assessment_versions_v2
BEGIN SELECT RAISE(ABORT, 'M2_ASSESSMENT_VERSION_IMMUTABLE'); END;

-- attempt 身份不可改；处理状态和 active publication 可推进。
CREATE TRIGGER trg_exam_attempt_identity_guard_v2
BEFORE UPDATE ON exam_attempts_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.assessment_version_id IS NOT OLD.assessment_version_id
  OR NEW.student_id IS NOT OLD.student_id
  OR NEW.attempt_no IS NOT OLD.attempt_no
  OR NEW.source_kind IS NOT OLD.source_kind
  OR NEW.attempt_kind IS NOT OLD.attempt_kind
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_ATTEMPT_IDENTITY_IMMUTABLE'); END;

-- decision 追加写；旧 revision 只允许 active -> superseded/voided。
CREATE TRIGGER trg_exam_decision_content_guard_v2
BEFORE UPDATE ON exam_grade_decisions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.attempt_id IS NOT OLD.attempt_id
  OR NEW.assessment_item_id IS NOT OLD.assessment_item_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.machine_grade_ai_run_id IS NOT OLD.machine_grade_ai_run_id
  OR NEW.teacher_score IS NOT OLD.teacher_score
  OR NEW.point_results_json IS NOT OLD.point_results_json
  OR NEW.teacher_note IS NOT OLD.teacher_note
  OR NEW.confirmation_level IS NOT OLD.confirmation_level
  OR NEW.decided_by IS NOT OLD.decided_by
  OR NEW.decided_at IS NOT OLD.decided_at
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_GRADE_DECISION_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_decision_delete_v2
BEFORE DELETE ON exam_grade_decisions_v2
BEGIN SELECT RAISE(ABORT, 'M2_GRADE_DECISION_IMMUTABLE'); END;

-- publication 及其纳入的 decision set 是历史快照，不能覆盖或删除。
CREATE TRIGGER trg_exam_publication_content_guard_v2
BEFORE UPDATE ON exam_grade_publications_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.assessment_version_id IS NOT OLD.assessment_version_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.published_by IS NOT OLD.published_by
  OR NEW.published_at IS NOT OLD.published_at
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_PUBLICATION_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_publication_delete_v2
BEFORE DELETE ON exam_grade_publications_v2
BEGIN SELECT RAISE(ABORT, 'M2_PUBLICATION_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_publication_item_update_v2
BEFORE UPDATE ON exam_grade_publication_items_v2
BEGIN SELECT RAISE(ABORT, 'M2_PUBLICATION_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_publication_item_delete_v2
BEFORE DELETE ON exam_grade_publication_items_v2
BEGIN SELECT RAISE(ABORT, 'M2_PUBLICATION_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_publication_decision_update_v2
BEFORE UPDATE ON exam_grade_publication_decisions_v2
BEGIN SELECT RAISE(ABORT, 'M2_PUBLICATION_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_publication_decision_delete_v2
BEFORE DELETE ON exam_grade_publication_decisions_v2
BEGIN SELECT RAISE(ABORT, 'M2_PUBLICATION_IMMUTABLE'); END;
