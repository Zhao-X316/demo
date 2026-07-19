-- M2.5-4：固定普通卷印刷题面只选择一份学生页面作为当前作业页的候选来源。
--
-- 学生卷裁图永远不进入 K1；本表只冻结“哪一页、哪次 succeeded 视觉 run、哪套
-- 提取规则”被用于生成文字型私有候选，防止全班同一题被重复沉淀。

CREATE TABLE exam_ordinary_question_source_syncs_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  claim_key TEXT NOT NULL UNIQUE CHECK(length(trim(claim_key)) > 0),
  assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  page_no INTEGER NOT NULL CHECK(page_no > 0),
  owner_id TEXT NOT NULL CHECK(length(trim(owner_id)) > 0),
  extraction_version TEXT NOT NULL CHECK(length(trim(extraction_version)) > 0),
  source_page_id INTEGER NOT NULL REFERENCES exam_ingest_pages_v2(id),
  source_ai_run_id INTEGER NOT NULL UNIQUE REFERENCES ai_runs(id),
  state TEXT NOT NULL CHECK(state IN ('processing','completed','failed')),
  summary_json TEXT,
  error_meta_json TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  CHECK(
    summary_json IS NULL OR (
      COALESCE(json_valid(summary_json), 0) = 1
      AND COALESCE(json_type(summary_json), 0) = 'object'
      AND COALESCE(json_type(summary_json, '$.schema_version'), 0) = 'integer'
    )
  ),
  CHECK(
    error_meta_json IS NULL OR (
      COALESCE(json_valid(error_meta_json), 0) = 1
      AND COALESCE(json_type(error_meta_json), 0) = 'object'
      AND COALESCE(json_type(error_meta_json, '$.schema_version'), 0) = 'integer'
    )
  ),
  CHECK((state='completed' AND summary_json IS NOT NULL AND error_meta_json IS NULL)
        OR (state='failed' AND error_meta_json IS NOT NULL)
        OR state='processing')
);

CREATE INDEX idx_exam_ordinary_question_source_sync_scope_v2
  ON exam_ordinary_question_source_syncs_v2(
    assessment_version_id,page_no,owner_id,extraction_version,state
  );

-- 来源身份不可改；同步器只允许推进状态并追加结果/错误。
CREATE TRIGGER trg_exam_ordinary_question_source_sync_identity_guard_v2
BEFORE UPDATE ON exam_ordinary_question_source_syncs_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.claim_key IS NOT OLD.claim_key
  OR NEW.assessment_version_id IS NOT OLD.assessment_version_id
  OR NEW.page_no IS NOT OLD.page_no
  OR NEW.owner_id IS NOT OLD.owner_id
  OR NEW.extraction_version IS NOT OLD.extraction_version
  OR NEW.source_page_id IS NOT OLD.source_page_id
  OR NEW.source_ai_run_id IS NOT OLD.source_ai_run_id
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_ORDINARY_QUESTION_SOURCE_IDENTITY_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_ordinary_question_source_sync_delete_v2
BEFORE DELETE ON exam_ordinary_question_source_syncs_v2
BEGIN SELECT RAISE(ABORT, 'M2_ORDINARY_QUESTION_SOURCE_RETAINED'); END;
