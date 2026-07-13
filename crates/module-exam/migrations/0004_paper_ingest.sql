-- M2-B1a：纸面证据导入、质量闸门、学生/页码匹配和异常池。
-- 页面原始文件由 core.artifacts 保存；机器匹配只追加建议，不能静默写入 attempt 身份。

CREATE TABLE exam_ingest_batches_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  assessment_version_id INTEGER NOT NULL REFERENCES exam_assessment_versions_v2(id),
  source_kind TEXT NOT NULL CHECK(source_kind IN (
    'fixed_fixture','image_folder','scanner','camera'
  )),
  idempotency_key TEXT NOT NULL CHECK(length(trim(idempotency_key)) > 0),
  state TEXT NOT NULL DEFAULT 'processing' CHECK(state IN (
    'processing','ready','needs_review','failed','voided'
  )),
  created_by TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(assessment_version_id, idempotency_key)
);

CREATE TABLE exam_ingest_pages_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  batch_id INTEGER NOT NULL REFERENCES exam_ingest_batches_v2(id),
  source_artifact_id INTEGER NOT NULL REFERENCES artifacts(id),
  import_index INTEGER NOT NULL CHECK(import_index >= 0),
  expected_page_no INTEGER CHECK(expected_page_no IS NULL OR expected_page_no > 0),
  state TEXT NOT NULL DEFAULT 'imported' CHECK(state IN (
    'imported','quality_checked','matched','aligned','segmented','needs_review','voided'
  )),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE(batch_id, source_artifact_id),
  UNIQUE(batch_id, import_index)
);

CREATE TABLE exam_page_quality_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  page_id INTEGER NOT NULL REFERENCES exam_ingest_pages_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  blur_score REAL NOT NULL CHECK(blur_score >= 0.0 AND blur_score <= 1.0),
  glare_score REAL NOT NULL CHECK(glare_score >= 0.0 AND glare_score <= 1.0),
  brightness_score REAL NOT NULL CHECK(brightness_score >= 0.0 AND brightness_score <= 1.0),
  perspective_score REAL NOT NULL CHECK(perspective_score >= 0.0 AND perspective_score <= 1.0),
  rotation_degrees REAL NOT NULL CHECK(rotation_degrees >= -180.0 AND rotation_degrees <= 180.0),
  crop_complete INTEGER NOT NULL CHECK(crop_complete IN (0,1)),
  result TEXT NOT NULL CHECK(result IN ('pass','needs_review','reject')),
  issue_codes_json TEXT NOT NULL,
  checked_by_type TEXT NOT NULL CHECK(checked_by_type IN ('fixture','rule','teacher')),
  checked_by TEXT,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  UNIQUE(page_id, revision),
  CHECK(COALESCE(json_valid(issue_codes_json), 0) = 1),
  CHECK(COALESCE(json_type(issue_codes_json) = 'object', 0)),
  CHECK(COALESCE(json_type(issue_codes_json, '$.schema_version') = 'integer', 0)),
  CHECK((checked_by_type='teacher' AND checked_by IS NOT NULL AND length(trim(checked_by)) > 0)
        OR (checked_by_type<>'teacher' AND checked_by IS NULL))
);
CREATE UNIQUE INDEX idx_exam_page_quality_active_v2
  ON exam_page_quality_revisions_v2(page_id) WHERE state='active';

CREATE TABLE exam_page_match_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  page_id INTEGER NOT NULL REFERENCES exam_ingest_pages_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  attempt_id INTEGER REFERENCES exam_attempts_v2(id),
  page_no INTEGER CHECK(page_no IS NULL OR page_no > 0),
  student_confidence REAL CHECK(student_confidence IS NULL OR (
    student_confidence >= 0.0 AND student_confidence <= 1.0
  )),
  page_no_confidence REAL CHECK(page_no_confidence IS NULL OR (
    page_no_confidence >= 0.0 AND page_no_confidence <= 1.0
  )),
  template_confidence REAL CHECK(template_confidence IS NULL OR (
    template_confidence >= 0.0 AND template_confidence <= 1.0
  )),
  decision TEXT NOT NULL CHECK(decision IN (
    'suggested','teacher_confirmed','rejected','unmatched'
  )),
  reason_code TEXT,
  confirmed_by TEXT,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  UNIQUE(page_id, revision),
  CHECK(
    (decision IN ('suggested','teacher_confirmed')
      AND attempt_id IS NOT NULL AND page_no IS NOT NULL
      AND student_confidence IS NOT NULL AND page_no_confidence IS NOT NULL)
    OR decision IN ('rejected','unmatched')
  ),
  CHECK((decision='teacher_confirmed' AND confirmed_by IS NOT NULL
         AND length(trim(confirmed_by)) > 0)
        OR (decision<>'teacher_confirmed' AND confirmed_by IS NULL))
);
CREATE UNIQUE INDEX idx_exam_page_match_active_v2
  ON exam_page_match_revisions_v2(page_id) WHERE state='active';

CREATE TABLE exam_pipeline_issues_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  target_type TEXT NOT NULL CHECK(target_type IN (
    'batch','page','quality','match','alignment','region'
  )),
  target_public_id TEXT NOT NULL CHECK(length(trim(target_public_id)) > 0),
  issue_code TEXT NOT NULL CHECK(length(trim(issue_code)) > 0),
  severity TEXT NOT NULL CHECK(severity IN ('warning','blocking')),
  details_json TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'open' CHECK(state IN ('open','resolved','voided')),
  resolved_by TEXT,
  resolved_at TEXT,
  resolution_note TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  CHECK(COALESCE(json_valid(details_json), 0) = 1),
  CHECK(COALESCE(json_type(details_json) = 'object', 0)),
  CHECK(COALESCE(json_type(details_json, '$.schema_version') = 'integer', 0)),
  CHECK((state='resolved' AND resolved_by IS NOT NULL AND resolved_at IS NOT NULL)
        OR state<>'resolved')
);
CREATE UNIQUE INDEX idx_exam_pipeline_issue_open_v2
  ON exam_pipeline_issues_v2(target_type, target_public_id, issue_code)
  WHERE state='open';

-- 导入批次和页面身份不可变；处理状态只能由服务推进。
CREATE TRIGGER trg_exam_ingest_batch_identity_guard_v2
BEFORE UPDATE ON exam_ingest_batches_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.assessment_version_id IS NOT OLD.assessment_version_id
  OR NEW.source_kind IS NOT OLD.source_kind
  OR NEW.idempotency_key IS NOT OLD.idempotency_key
  OR NEW.created_by IS NOT OLD.created_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_INGEST_BATCH_IDENTITY_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_ingest_page_identity_guard_v2
BEFORE UPDATE ON exam_ingest_pages_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.batch_id IS NOT OLD.batch_id
  OR NEW.source_artifact_id IS NOT OLD.source_artifact_id
  OR NEW.import_index IS NOT OLD.import_index
  OR NEW.expected_page_no IS NOT OLD.expected_page_no
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_INGEST_PAGE_IDENTITY_IMMUTABLE'); END;

-- 质量/匹配结论追加写；旧 revision 只允许 active -> superseded/voided。
CREATE TRIGGER trg_exam_page_quality_content_guard_v2
BEFORE UPDATE ON exam_page_quality_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.page_id IS NOT OLD.page_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.blur_score IS NOT OLD.blur_score
  OR NEW.glare_score IS NOT OLD.glare_score
  OR NEW.brightness_score IS NOT OLD.brightness_score
  OR NEW.perspective_score IS NOT OLD.perspective_score
  OR NEW.rotation_degrees IS NOT OLD.rotation_degrees
  OR NEW.crop_complete IS NOT OLD.crop_complete
  OR NEW.result IS NOT OLD.result
  OR NEW.issue_codes_json IS NOT OLD.issue_codes_json
  OR NEW.checked_by_type IS NOT OLD.checked_by_type
  OR NEW.checked_by IS NOT OLD.checked_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_PAGE_QUALITY_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_page_quality_delete_v2
BEFORE DELETE ON exam_page_quality_revisions_v2
BEGIN SELECT RAISE(ABORT, 'M2_PAGE_QUALITY_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_page_match_content_guard_v2
BEFORE UPDATE ON exam_page_match_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.page_id IS NOT OLD.page_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.attempt_id IS NOT OLD.attempt_id
  OR NEW.page_no IS NOT OLD.page_no
  OR NEW.student_confidence IS NOT OLD.student_confidence
  OR NEW.page_no_confidence IS NOT OLD.page_no_confidence
  OR NEW.template_confidence IS NOT OLD.template_confidence
  OR NEW.decision IS NOT OLD.decision
  OR NEW.reason_code IS NOT OLD.reason_code
  OR NEW.confirmed_by IS NOT OLD.confirmed_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_PAGE_MATCH_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_page_match_delete_v2
BEFORE DELETE ON exam_page_match_revisions_v2
BEGIN SELECT RAISE(ABORT, 'M2_PAGE_MATCH_IMMUTABLE'); END;

-- 异常事实不可覆盖；只允许显式解决/作废并补充解决信息。
CREATE TRIGGER trg_exam_pipeline_issue_content_guard_v2
BEFORE UPDATE ON exam_pipeline_issues_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.target_type IS NOT OLD.target_type
  OR NEW.target_public_id IS NOT OLD.target_public_id
  OR NEW.issue_code IS NOT OLD.issue_code
  OR NEW.severity IS NOT OLD.severity
  OR NEW.details_json IS NOT OLD.details_json
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_PIPELINE_ISSUE_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_pipeline_issue_delete_v2
BEFORE DELETE ON exam_pipeline_issues_v2
BEGIN SELECT RAISE(ABORT, 'M2_PIPELINE_ISSUE_IMMUTABLE'); END;
