-- M2-C0：固定默写模板应用到账页后的不可变物化账本。
-- OCR/评分仍写入 0016 的 transcription/point observation；本表只证明模板、页面和题区。

CREATE TABLE exam_dictation_page_materializations_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  page_id INTEGER NOT NULL REFERENCES exam_ingest_pages_v2(id),
  template_revision_id INTEGER NOT NULL REFERENCES exam_dictation_template_revisions_v2(id),
  alignment_revision_id INTEGER NOT NULL REFERENCES exam_page_alignment_revisions_v2(id),
  region_revision_ids_json TEXT NOT NULL,
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by)) > 0),
  created_at TEXT NOT NULL,
  UNIQUE(page_id,template_revision_id),
  CHECK(COALESCE(json_valid(region_revision_ids_json),0)=1),
  CHECK(COALESCE(json_type(region_revision_ids_json),0)='object'),
  CHECK(COALESCE(json_type(region_revision_ids_json,'$.schema_version'),0)='integer'),
  CHECK(COALESCE(json_type(region_revision_ids_json,'$.region_revision_ids'),0)='array')
);

CREATE TRIGGER trg_exam_dictation_page_materialization_update_v2
BEFORE UPDATE ON exam_dictation_page_materializations_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_PAGE_MATERIALIZATION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_dictation_page_materialization_delete_v2
BEFORE DELETE ON exam_dictation_page_materializations_v2
BEGIN SELECT RAISE(ABORT,'M2_DICTATION_PAGE_MATERIALIZATION_IMMUTABLE'); END;
