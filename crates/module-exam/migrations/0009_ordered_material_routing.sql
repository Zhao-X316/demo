-- M2-B3a0：连续拍摄批次的顺序证据、材料类型、页型和分组路由快照。
-- 原始页面身份仍由 exam_ingest_pages_v2 持有；本迁移只追加可纠正的推断/确认 revision，
-- 不覆盖文件顺序、不直接确认学生归属，也不创建评分或发布副作用。

CREATE TABLE exam_import_order_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  ingest_batch_id INTEGER NOT NULL REFERENCES exam_ingest_batches_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  snapshot_hash TEXT NOT NULL CHECK(length(snapshot_hash)=64),
  sort_policy TEXT NOT NULL CHECK(sort_policy IN (
    'filename_natural_exif_filetime_crosscheck_v1','teacher_corrected_v1'
  )),
  order_confidence REAL NOT NULL CHECK(order_confidence BETWEEN 0 AND 1),
  ordered_sources_json TEXT NOT NULL,
  conflict_codes_json TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_by_type TEXT NOT NULL CHECK(created_by_type IN ('system','teacher')),
  created_by TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(ingest_batch_id,revision),
  CHECK(COALESCE(json_valid(ordered_sources_json),0)=1),
  CHECK(COALESCE(json_type(ordered_sources_json),0)='object'),
  CHECK(COALESCE(json_type(ordered_sources_json,'$.schema_version'),0)='integer'),
  CHECK(COALESCE(json_valid(conflict_codes_json),0)=1),
  CHECK(COALESCE(json_type(conflict_codes_json),0)='object'),
  CHECK(COALESCE(json_type(conflict_codes_json,'$.schema_version'),0)='integer'),
  CHECK((created_by_type='teacher' AND created_by IS NOT NULL AND length(trim(created_by))>0)
        OR (created_by_type='system' AND created_by IS NULL))
);
CREATE UNIQUE INDEX idx_exam_import_order_active_v2
  ON exam_import_order_revisions_v2(ingest_batch_id) WHERE state='active';

CREATE TABLE exam_material_type_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  ingest_batch_id INTEGER NOT NULL REFERENCES exam_ingest_batches_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  material_type TEXT NOT NULL CHECK(material_type IN (
    'ordinary_paper','answer_sheet','dictation','unknown'
  )),
  confidence REAL NOT NULL CHECK(confidence BETWEEN 0 AND 1),
  evidence_json TEXT NOT NULL,
  decision TEXT NOT NULL CHECK(decision IN ('suggested','teacher_confirmed','rejected')),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_by_type TEXT NOT NULL CHECK(created_by_type IN ('system','teacher')),
  created_by TEXT,
  confirmed_by TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(ingest_batch_id,revision),
  CHECK(COALESCE(json_valid(evidence_json),0)=1),
  CHECK(COALESCE(json_type(evidence_json),0)='object'),
  CHECK(COALESCE(json_type(evidence_json,'$.schema_version'),0)='integer'),
  CHECK((decision='teacher_confirmed' AND confirmed_by IS NOT NULL
         AND length(trim(confirmed_by))>0 AND created_by_type='teacher')
        OR (decision<>'teacher_confirmed' AND confirmed_by IS NULL)),
  CHECK((created_by_type='teacher' AND created_by IS NOT NULL AND length(trim(created_by))>0)
        OR (created_by_type='system' AND created_by IS NULL))
);
CREATE UNIQUE INDEX idx_exam_material_type_active_v2
  ON exam_material_type_revisions_v2(ingest_batch_id) WHERE state='active';

CREATE TABLE exam_page_type_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  page_id INTEGER NOT NULL REFERENCES exam_ingest_pages_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  page_type_key TEXT NOT NULL CHECK(length(trim(page_type_key))>0),
  confidence REAL NOT NULL CHECK(confidence BETWEEN 0 AND 1),
  evidence_json TEXT NOT NULL,
  decision TEXT NOT NULL CHECK(decision IN (
    'suggested','teacher_confirmed','unknown','rejected'
  )),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_by_type TEXT NOT NULL CHECK(created_by_type IN ('system','teacher')),
  created_by TEXT,
  confirmed_by TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(page_id,revision),
  CHECK(COALESCE(json_valid(evidence_json),0)=1),
  CHECK(COALESCE(json_type(evidence_json),0)='object'),
  CHECK(COALESCE(json_type(evidence_json,'$.schema_version'),0)='integer'),
  CHECK((decision='teacher_confirmed' AND confirmed_by IS NOT NULL
         AND length(trim(confirmed_by))>0 AND created_by_type='teacher')
        OR (decision<>'teacher_confirmed' AND confirmed_by IS NULL)),
  CHECK((created_by_type='teacher' AND created_by IS NOT NULL AND length(trim(created_by))>0)
        OR (created_by_type='system' AND created_by IS NULL))
);
CREATE UNIQUE INDEX idx_exam_page_type_active_v2
  ON exam_page_type_revisions_v2(page_id) WHERE state='active';

CREATE TABLE exam_ordered_grouping_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  ingest_batch_id INTEGER NOT NULL REFERENCES exam_ingest_batches_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  snapshot_hash TEXT NOT NULL CHECK(length(snapshot_hash)=64),
  import_order_revision_id INTEGER NOT NULL REFERENCES exam_import_order_revisions_v2(id),
  material_type_revision_id INTEGER REFERENCES exam_material_type_revisions_v2(id),
  expected_pages_per_attempt INTEGER CHECK(expected_pages_per_attempt IS NULL OR expected_pages_per_attempt>0),
  route TEXT NOT NULL CHECK(route IN ('preview_ready','review_required','blocked')),
  student_group_count INTEGER NOT NULL CHECK(student_group_count>=0),
  issue_codes_json TEXT NOT NULL,
  grouping_json TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_by_type TEXT NOT NULL CHECK(created_by_type IN ('system','teacher')),
  created_by TEXT,
  created_at TEXT NOT NULL,
  UNIQUE(ingest_batch_id,revision),
  CHECK(COALESCE(json_valid(issue_codes_json),0)=1),
  CHECK(COALESCE(json_type(issue_codes_json),0)='object'),
  CHECK(COALESCE(json_type(issue_codes_json,'$.schema_version'),0)='integer'),
  CHECK(COALESCE(json_valid(grouping_json),0)=1),
  CHECK(COALESCE(json_type(grouping_json),0)='object'),
  CHECK(COALESCE(json_type(grouping_json,'$.schema_version'),0)='integer'),
  CHECK((created_by_type='teacher' AND created_by IS NOT NULL AND length(trim(created_by))>0)
        OR (created_by_type='system' AND created_by IS NULL))
);
CREATE UNIQUE INDEX idx_exam_ordered_grouping_active_v2
  ON exam_ordered_grouping_revisions_v2(ingest_batch_id) WHERE state='active';

-- 四类快照均为追加写；只允许旧 active revision 退出，不允许覆盖证据内容。
CREATE TRIGGER trg_exam_import_order_content_guard_v2
BEFORE UPDATE ON exam_import_order_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id OR NEW.ingest_batch_id IS NOT OLD.ingest_batch_id
  OR NEW.revision IS NOT OLD.revision OR NEW.snapshot_hash IS NOT OLD.snapshot_hash
  OR NEW.sort_policy IS NOT OLD.sort_policy OR NEW.order_confidence IS NOT OLD.order_confidence
  OR NEW.ordered_sources_json IS NOT OLD.ordered_sources_json
  OR NEW.conflict_codes_json IS NOT OLD.conflict_codes_json
  OR NEW.created_by_type IS NOT OLD.created_by_type OR NEW.created_by IS NOT OLD.created_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_IMPORT_ORDER_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_import_order_state_guard_v2
BEFORE UPDATE OF state ON exam_import_order_revisions_v2
WHEN NOT (OLD.state='active' AND NEW.state IN ('superseded','voided') OR OLD.state=NEW.state)
BEGIN SELECT RAISE(ABORT,'M2_IMPORT_ORDER_STATE_INVALID'); END;
CREATE TRIGGER trg_exam_import_order_delete_v2 BEFORE DELETE ON exam_import_order_revisions_v2
BEGIN SELECT RAISE(ABORT,'M2_IMPORT_ORDER_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_material_type_content_guard_v2
BEFORE UPDATE ON exam_material_type_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id OR NEW.ingest_batch_id IS NOT OLD.ingest_batch_id
  OR NEW.revision IS NOT OLD.revision OR NEW.material_type IS NOT OLD.material_type
  OR NEW.confidence IS NOT OLD.confidence OR NEW.evidence_json IS NOT OLD.evidence_json
  OR NEW.decision IS NOT OLD.decision OR NEW.created_by_type IS NOT OLD.created_by_type
  OR NEW.created_by IS NOT OLD.created_by OR NEW.confirmed_by IS NOT OLD.confirmed_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_MATERIAL_TYPE_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_material_type_state_guard_v2
BEFORE UPDATE OF state ON exam_material_type_revisions_v2
WHEN NOT (OLD.state='active' AND NEW.state IN ('superseded','voided') OR OLD.state=NEW.state)
BEGIN SELECT RAISE(ABORT,'M2_MATERIAL_TYPE_STATE_INVALID'); END;
CREATE TRIGGER trg_exam_material_type_delete_v2 BEFORE DELETE ON exam_material_type_revisions_v2
BEGIN SELECT RAISE(ABORT,'M2_MATERIAL_TYPE_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_page_type_content_guard_v2
BEFORE UPDATE ON exam_page_type_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id OR NEW.page_id IS NOT OLD.page_id
  OR NEW.revision IS NOT OLD.revision OR NEW.page_type_key IS NOT OLD.page_type_key
  OR NEW.confidence IS NOT OLD.confidence OR NEW.evidence_json IS NOT OLD.evidence_json
  OR NEW.decision IS NOT OLD.decision OR NEW.created_by_type IS NOT OLD.created_by_type
  OR NEW.created_by IS NOT OLD.created_by OR NEW.confirmed_by IS NOT OLD.confirmed_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_PAGE_TYPE_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_page_type_state_guard_v2
BEFORE UPDATE OF state ON exam_page_type_revisions_v2
WHEN NOT (OLD.state='active' AND NEW.state IN ('superseded','voided') OR OLD.state=NEW.state)
BEGIN SELECT RAISE(ABORT,'M2_PAGE_TYPE_STATE_INVALID'); END;
CREATE TRIGGER trg_exam_page_type_delete_v2 BEFORE DELETE ON exam_page_type_revisions_v2
BEGIN SELECT RAISE(ABORT,'M2_PAGE_TYPE_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_ordered_grouping_content_guard_v2
BEFORE UPDATE ON exam_ordered_grouping_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id OR NEW.ingest_batch_id IS NOT OLD.ingest_batch_id
  OR NEW.revision IS NOT OLD.revision OR NEW.snapshot_hash IS NOT OLD.snapshot_hash
  OR NEW.import_order_revision_id IS NOT OLD.import_order_revision_id
  OR NEW.material_type_revision_id IS NOT OLD.material_type_revision_id
  OR NEW.expected_pages_per_attempt IS NOT OLD.expected_pages_per_attempt
  OR NEW.route IS NOT OLD.route OR NEW.student_group_count IS NOT OLD.student_group_count
  OR NEW.issue_codes_json IS NOT OLD.issue_codes_json OR NEW.grouping_json IS NOT OLD.grouping_json
  OR NEW.created_by_type IS NOT OLD.created_by_type OR NEW.created_by IS NOT OLD.created_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_ORDERED_GROUPING_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_ordered_grouping_state_guard_v2
BEFORE UPDATE OF state ON exam_ordered_grouping_revisions_v2
WHEN NOT (OLD.state='active' AND NEW.state IN ('superseded','voided') OR OLD.state=NEW.state)
BEGIN SELECT RAISE(ABORT,'M2_ORDERED_GROUPING_STATE_INVALID'); END;
CREATE TRIGGER trg_exam_ordered_grouping_delete_v2 BEFORE DELETE ON exam_ordered_grouping_revisions_v2
BEGIN SELECT RAISE(ABORT,'M2_ORDERED_GROUPING_IMMUTABLE'); END;
