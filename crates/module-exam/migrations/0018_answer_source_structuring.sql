-- M2-B3c：答案图片/文本结构化后的老师一次确认决议。
-- 机器逐题候选继续复用 exam_answer_authority_candidates_v2(source_kind='ai_draft')；
-- 正式答案仍只存在 K1。本表只记录老师如何处理某次不可变 ai_run，避免刷新后再次
-- 要求确认，也让固定卷预检能够区分“未处理冲突”和“已明确沿用当前答案”。

CREATE TABLE exam_answer_source_resolutions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  ingest_batch_id INTEGER NOT NULL REFERENCES exam_ingest_batches_v2(id),
  source_ai_run_id INTEGER NOT NULL UNIQUE REFERENCES ai_runs(id),
  decision TEXT NOT NULL CHECK(decision IN ('confirmed_matches','kept_bound')),
  summary_hash TEXT NOT NULL CHECK(length(summary_hash)=64),
  summary_json TEXT NOT NULL,
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by))>0),
  created_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(summary_json),0)=1),
  CHECK(COALESCE(json_type(summary_json),0)='object'),
  CHECK(COALESCE(json_type(summary_json,'$.schema_version'),0)='integer')
);
CREATE INDEX idx_exam_answer_source_resolution_batch_v2
  ON exam_answer_source_resolutions_v2(ingest_batch_id,source_ai_run_id);

CREATE TRIGGER trg_exam_answer_source_resolution_update_v2
BEFORE UPDATE ON exam_answer_source_resolutions_v2
BEGIN SELECT RAISE(ABORT, 'M2_ANSWER_SOURCE_RESOLUTION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_answer_source_resolution_delete_v2
BEFORE DELETE ON exam_answer_source_resolutions_v2
BEGIN SELECT RAISE(ABORT, 'M2_ANSWER_SOURCE_RESOLUTION_IMMUTABLE'); END;
