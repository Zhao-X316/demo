-- M2-B3a0：按学生逻辑页位追加重拍 replacement，原始页面永不覆盖。
-- 只允许尚未建立正式页面归属的待重拍页进入本流程；评分/发布后的纠错另走评分 revision。

CREATE TABLE exam_ordered_page_replacements_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  ingest_batch_id INTEGER NOT NULL REFERENCES exam_ingest_batches_v2(id),
  grouping_decision_id INTEGER NOT NULL REFERENCES exam_ordered_grouping_decisions_v2(id),
  original_page_id INTEGER NOT NULL REFERENCES exam_ingest_pages_v2(id),
  replaces_page_id INTEGER NOT NULL REFERENCES exam_ingest_pages_v2(id),
  replacement_page_id INTEGER NOT NULL UNIQUE REFERENCES exam_ingest_pages_v2(id),
  group_index INTEGER NOT NULL CHECK(group_index>=0),
  student_id INTEGER NOT NULL REFERENCES students(id),
  logical_page_no INTEGER NOT NULL CHECK(logical_page_no>0),
  revision INTEGER NOT NULL CHECK(revision>0),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by))>0),
  created_at TEXT NOT NULL,
  UNIQUE(original_page_id,revision),
  CHECK(original_page_id<>replacement_page_id),
  CHECK(replaces_page_id<>replacement_page_id)
);

CREATE UNIQUE INDEX idx_exam_ordered_page_replacement_active_v2
  ON exam_ordered_page_replacements_v2(original_page_id) WHERE state='active';

CREATE TRIGGER trg_exam_ordered_page_replacement_content_guard_v2
BEFORE UPDATE ON exam_ordered_page_replacements_v2
WHEN NEW.public_id IS NOT OLD.public_id OR NEW.ingest_batch_id IS NOT OLD.ingest_batch_id
  OR NEW.grouping_decision_id IS NOT OLD.grouping_decision_id
  OR NEW.original_page_id IS NOT OLD.original_page_id
  OR NEW.replaces_page_id IS NOT OLD.replaces_page_id
  OR NEW.replacement_page_id IS NOT OLD.replacement_page_id
  OR NEW.group_index IS NOT OLD.group_index OR NEW.student_id IS NOT OLD.student_id
  OR NEW.logical_page_no IS NOT OLD.logical_page_no OR NEW.revision IS NOT OLD.revision
  OR NEW.confirmed_by IS NOT OLD.confirmed_by OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_ORDERED_PAGE_REPLACEMENT_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_ordered_page_replacement_state_guard_v2
BEFORE UPDATE OF state ON exam_ordered_page_replacements_v2
WHEN NOT (OLD.state='active' AND NEW.state IN ('superseded','voided') OR OLD.state=NEW.state)
BEGIN SELECT RAISE(ABORT,'M2_ORDERED_PAGE_REPLACEMENT_STATE_INVALID'); END;

CREATE TRIGGER trg_exam_ordered_page_replacement_delete_v2
BEFORE DELETE ON exam_ordered_page_replacements_v2
BEGIN SELECT RAISE(ABORT,'M2_ORDERED_PAGE_REPLACEMENT_IMMUTABLE'); END;
