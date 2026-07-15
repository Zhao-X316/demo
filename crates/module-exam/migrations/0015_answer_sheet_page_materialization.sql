-- M2-B3a2：老师把已确认答题卡模板应用到具体学生页面后的不可变结构账本。
-- 配准和题区继续写既有 revision 表；本表固定模板 revision 与派生结构，
-- 不产生答案确认、分数、发布或学习证据。

CREATE TABLE exam_answer_sheet_page_materializations_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  page_id INTEGER NOT NULL REFERENCES exam_ingest_pages_v2(id),
  template_revision_id INTEGER NOT NULL REFERENCES exam_answer_sheet_template_revisions_v2(id),
  alignment_revision_id INTEGER NOT NULL UNIQUE REFERENCES exam_page_alignment_revisions_v2(id),
  region_revision_ids_json TEXT NOT NULL,
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by))>0),
  created_at TEXT NOT NULL,
  UNIQUE(page_id,template_revision_id),
  CHECK(COALESCE(json_valid(region_revision_ids_json),0)=1),
  CHECK(COALESCE(json_type(region_revision_ids_json),0)='object'),
  CHECK(COALESCE(json_type(region_revision_ids_json,'$.schema_version'),0)='integer'),
  CHECK(COALESCE(json_type(region_revision_ids_json,'$.region_revision_ids'),0)='array')
);

CREATE INDEX idx_exam_answer_sheet_page_materialization_page_v2
  ON exam_answer_sheet_page_materializations_v2(page_id,id DESC);

CREATE TRIGGER trg_exam_answer_sheet_page_materialization_update_v2
BEFORE UPDATE ON exam_answer_sheet_page_materializations_v2
BEGIN SELECT RAISE(ABORT,'M2_ANSWER_SHEET_PAGE_MATERIALIZATION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_answer_sheet_page_materialization_delete_v2
BEFORE DELETE ON exam_answer_sheet_page_materializations_v2
BEGIN SELECT RAISE(ABORT,'M2_ANSWER_SHEET_PAGE_MATERIALIZATION_IMMUTABLE'); END;
