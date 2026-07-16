-- M2-B3c2：老师采纳上传答案时，记录新 K1/作业版本的不可变来源关系。
-- 当前 ingest batch 继续引用 source_assessment_version_id；新版本仅供以后新批次使用。

CREATE TABLE exam_answer_source_adoptions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  ingest_batch_id INTEGER NOT NULL REFERENCES exam_ingest_batches_v2(id),
  source_ai_run_id INTEGER NOT NULL UNIQUE REFERENCES ai_runs(id),
  source_assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  adopted_assessment_version_id INTEGER NOT NULL UNIQUE REFERENCES exam_assessment_versions_v2(id),
  changed_item_count INTEGER NOT NULL CHECK(changed_item_count > 0),
  details_json TEXT NOT NULL,
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by)) > 0),
  created_at TEXT NOT NULL,
  CHECK(source_assessment_version_id <> adopted_assessment_version_id),
  CHECK(COALESCE(json_valid(details_json),0)=1),
  CHECK(COALESCE(json_type(details_json),0)='object'),
  CHECK(COALESCE(json_type(details_json,'$.schema_version'),0)='integer')
);
CREATE INDEX idx_exam_answer_source_adoption_batch_v2
  ON exam_answer_source_adoptions_v2(ingest_batch_id,source_ai_run_id);

CREATE TRIGGER trg_exam_answer_source_adoption_update_v2
BEFORE UPDATE ON exam_answer_source_adoptions_v2
BEGIN SELECT RAISE(ABORT, 'M2_ANSWER_SOURCE_ADOPTION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_answer_source_adoption_delete_v2
BEFORE DELETE ON exam_answer_source_adoptions_v2
BEGIN SELECT RAISE(ABORT, 'M2_ANSWER_SOURCE_ADOPTION_IMMUTABLE'); END;
