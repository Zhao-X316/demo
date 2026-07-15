-- M2-B3a2：固定答题卡不可变模板。
-- 模板把空白卡、页码、锚点、题号与格位固定到已确认作业版本；识别结果仍走
-- objective observation/teacher decision/publication 既有链路。

CREATE TABLE exam_answer_sheet_template_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  template_version TEXT NOT NULL CHECK(length(trim(template_version)) > 0),
  page_no INTEGER NOT NULL CHECK(page_no > 0),
  blank_artifact_id INTEGER NOT NULL REFERENCES artifacts(id),
  template_hash TEXT NOT NULL CHECK(length(template_hash)=64),
  template_json TEXT NOT NULL,
  source_ai_run_id INTEGER REFERENCES ai_runs(id),
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by)) > 0),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  UNIQUE(assessment_version_id,page_no,revision),
  CHECK(COALESCE(json_valid(template_json),0)=1),
  CHECK(COALESCE(json_type(template_json),0)='object'),
  CHECK(COALESCE(json_type(template_json,'$.schema_version'),0)='integer')
);

CREATE UNIQUE INDEX idx_exam_answer_sheet_template_active_v2
  ON exam_answer_sheet_template_revisions_v2(assessment_version_id,page_no)
  WHERE state='active';

CREATE INDEX idx_exam_answer_sheet_template_hash_v2
  ON exam_answer_sheet_template_revisions_v2(assessment_version_id,page_no,template_hash);

CREATE TRIGGER trg_exam_answer_sheet_template_content_guard_v2
BEFORE UPDATE ON exam_answer_sheet_template_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.assessment_version_id IS NOT OLD.assessment_version_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.template_version IS NOT OLD.template_version
  OR NEW.page_no IS NOT OLD.page_no
  OR NEW.blank_artifact_id IS NOT OLD.blank_artifact_id
  OR NEW.template_hash IS NOT OLD.template_hash
  OR NEW.template_json IS NOT OLD.template_json
  OR NEW.source_ai_run_id IS NOT OLD.source_ai_run_id
  OR NEW.confirmed_by IS NOT OLD.confirmed_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_ANSWER_SHEET_TEMPLATE_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_answer_sheet_template_delete_v2
BEFORE DELETE ON exam_answer_sheet_template_revisions_v2
BEGIN SELECT RAISE(ABORT,'M2_ANSWER_SHEET_TEMPLATE_IMMUTABLE'); END;
