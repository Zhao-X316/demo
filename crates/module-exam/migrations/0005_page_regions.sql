-- M2-B1b：模板配准和答案区域证据。
-- alignment 固定引用 teacher-confirmed match revision；region 固定引用 alignment revision。

CREATE TABLE exam_page_alignment_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  page_id INTEGER NOT NULL REFERENCES exam_ingest_pages_v2(id),
  revision INTEGER NOT NULL CHECK(revision > 0),
  match_revision_id INTEGER NOT NULL REFERENCES exam_page_match_revisions_v2(id),
  template_version TEXT NOT NULL CHECK(length(trim(template_version)) > 0),
  transform_json TEXT NOT NULL,
  confidence REAL NOT NULL CHECK(confidence >= 0.0 AND confidence <= 1.0),
  aligned_artifact_id INTEGER REFERENCES artifacts(id),
  decision TEXT NOT NULL CHECK(decision IN ('suggested','teacher_confirmed','rejected')),
  reason_code TEXT,
  confirmed_by TEXT,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  UNIQUE(page_id, revision),
  CHECK(COALESCE(json_valid(transform_json), 0) = 1),
  CHECK(COALESCE(json_type(transform_json) = 'object', 0)),
  CHECK(COALESCE(json_type(transform_json, '$.schema_version') = 'integer', 0)),
  CHECK((decision IN ('suggested','teacher_confirmed') AND aligned_artifact_id IS NOT NULL)
        OR decision='rejected'),
  CHECK((decision='teacher_confirmed' AND confirmed_by IS NOT NULL
         AND length(trim(confirmed_by)) > 0)
        OR (decision<>'teacher_confirmed' AND confirmed_by IS NULL))
);
CREATE UNIQUE INDEX idx_exam_page_alignment_active_v2
  ON exam_page_alignment_revisions_v2(page_id) WHERE state='active';

CREATE TABLE exam_answer_region_revisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  page_id INTEGER NOT NULL REFERENCES exam_ingest_pages_v2(id),
  assessment_item_id INTEGER NOT NULL REFERENCES exam_assessment_items_v2(id),
  region_index INTEGER NOT NULL CHECK(region_index >= 0),
  revision INTEGER NOT NULL CHECK(revision > 0),
  alignment_revision_id INTEGER NOT NULL REFERENCES exam_page_alignment_revisions_v2(id),
  bbox_json TEXT NOT NULL,
  crop_artifact_id INTEGER REFERENCES artifacts(id),
  mapping_confidence REAL CHECK(mapping_confidence IS NULL OR (
    mapping_confidence >= 0.0 AND mapping_confidence <= 1.0
  )),
  decision TEXT NOT NULL CHECK(decision IN ('suggested','teacher_confirmed','rejected')),
  reason_code TEXT,
  confirmed_by TEXT,
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  created_at TEXT NOT NULL,
  UNIQUE(page_id, assessment_item_id, region_index, revision),
  CHECK(COALESCE(json_valid(bbox_json), 0) = 1),
  CHECK(COALESCE(json_type(bbox_json) = 'object', 0)),
  CHECK(COALESCE(json_type(bbox_json, '$.schema_version') = 'integer', 0)),
  CHECK((decision IN ('suggested','teacher_confirmed')
         AND crop_artifact_id IS NOT NULL AND mapping_confidence IS NOT NULL)
        OR decision='rejected'),
  CHECK((decision='teacher_confirmed' AND confirmed_by IS NOT NULL
         AND length(trim(confirmed_by)) > 0)
        OR (decision<>'teacher_confirmed' AND confirmed_by IS NULL))
);
CREATE UNIQUE INDEX idx_exam_answer_region_active_v2
  ON exam_answer_region_revisions_v2(page_id, assessment_item_id, region_index)
  WHERE state='active';

CREATE TRIGGER trg_exam_page_alignment_content_guard_v2
BEFORE UPDATE ON exam_page_alignment_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.page_id IS NOT OLD.page_id
  OR NEW.revision IS NOT OLD.revision
  OR NEW.match_revision_id IS NOT OLD.match_revision_id
  OR NEW.template_version IS NOT OLD.template_version
  OR NEW.transform_json IS NOT OLD.transform_json
  OR NEW.confidence IS NOT OLD.confidence
  OR NEW.aligned_artifact_id IS NOT OLD.aligned_artifact_id
  OR NEW.decision IS NOT OLD.decision
  OR NEW.reason_code IS NOT OLD.reason_code
  OR NEW.confirmed_by IS NOT OLD.confirmed_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_PAGE_ALIGNMENT_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_page_alignment_delete_v2
BEFORE DELETE ON exam_page_alignment_revisions_v2
BEGIN SELECT RAISE(ABORT, 'M2_PAGE_ALIGNMENT_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_answer_region_content_guard_v2
BEFORE UPDATE ON exam_answer_region_revisions_v2
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.page_id IS NOT OLD.page_id
  OR NEW.assessment_item_id IS NOT OLD.assessment_item_id
  OR NEW.region_index IS NOT OLD.region_index
  OR NEW.revision IS NOT OLD.revision
  OR NEW.alignment_revision_id IS NOT OLD.alignment_revision_id
  OR NEW.bbox_json IS NOT OLD.bbox_json
  OR NEW.crop_artifact_id IS NOT OLD.crop_artifact_id
  OR NEW.mapping_confidence IS NOT OLD.mapping_confidence
  OR NEW.decision IS NOT OLD.decision
  OR NEW.reason_code IS NOT OLD.reason_code
  OR NEW.confirmed_by IS NOT OLD.confirmed_by
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT, 'M2_ANSWER_REGION_IMMUTABLE'); END;
CREATE TRIGGER trg_exam_answer_region_delete_v2
BEFORE DELETE ON exam_answer_region_revisions_v2
BEGIN SELECT RAISE(ABORT, 'M2_ANSWER_REGION_IMMUTABLE'); END;
