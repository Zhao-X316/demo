-- M2-B3a1：老师确认普通试卷整页 AI 结构候选后的不可变来源账本。
-- 真正的配准与题区仍写既有 revision 表；本表只把其固定到唯一 succeeded ai_run，
-- 防止重复确认、来源漂移或把机器建议伪装成无来源的人工结论。

CREATE TABLE exam_ordinary_structure_confirmations_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  ai_run_id INTEGER NOT NULL UNIQUE REFERENCES ai_runs(id),
  page_id INTEGER NOT NULL REFERENCES exam_ingest_pages_v2(id),
  alignment_revision_id INTEGER NOT NULL UNIQUE REFERENCES exam_page_alignment_revisions_v2(id),
  region_revision_ids_json TEXT NOT NULL,
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by))>0),
  created_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(region_revision_ids_json),0)=1),
  CHECK(COALESCE(json_type(region_revision_ids_json),0)='object'),
  CHECK(COALESCE(json_type(region_revision_ids_json,'$.schema_version'),0)='integer'),
  CHECK(COALESCE(json_type(region_revision_ids_json,'$.region_revision_ids'),0)='array')
);

CREATE INDEX idx_exam_ordinary_structure_confirmation_page_v2
  ON exam_ordinary_structure_confirmations_v2(page_id,id DESC);

CREATE TRIGGER trg_exam_ordinary_structure_confirmation_update_v2
BEFORE UPDATE ON exam_ordinary_structure_confirmations_v2
BEGIN SELECT RAISE(ABORT,'M2_ORDINARY_STRUCTURE_CONFIRMATION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_ordinary_structure_confirmation_delete_v2
BEFORE DELETE ON exam_ordinary_structure_confirmations_v2
BEGIN SELECT RAISE(ABORT,'M2_ORDINARY_STRUCTURE_CONFIRMATION_IMMUTABLE'); END;
