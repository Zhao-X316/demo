-- M2-B3a0：老师对连续拍摄范围、缺交学生和固定页型周期的一次确认。
-- 该表只确认“照片组属于谁、组内第几页”，不绕过页面质量闸门创建 page match，
-- 也不创建评分、发布或学习证据。

CREATE TABLE exam_ordered_grouping_decisions_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  ingest_batch_id INTEGER NOT NULL REFERENCES exam_ingest_batches_v2(id),
  revision INTEGER NOT NULL CHECK(revision>0),
  snapshot_hash TEXT NOT NULL CHECK(length(snapshot_hash)=64),
  source_grouping_revision_id INTEGER NOT NULL REFERENCES exam_ordered_grouping_revisions_v2(id),
  expected_pages_per_attempt INTEGER NOT NULL CHECK(expected_pages_per_attempt>0),
  first_student_id INTEGER NOT NULL REFERENCES students(id),
  roster_scope_json TEXT NOT NULL,
  page_type_cycle_json TEXT NOT NULL,
  assignments_json TEXT NOT NULL,
  decision TEXT NOT NULL CHECK(decision='teacher_confirmed'),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by))>0),
  created_at TEXT NOT NULL,
  UNIQUE(ingest_batch_id,revision),
  CHECK(COALESCE(json_valid(roster_scope_json),0)=1),
  CHECK(COALESCE(json_type(roster_scope_json),0)='object'),
  CHECK(COALESCE(json_type(roster_scope_json,'$.schema_version'),0)='integer'),
  CHECK(COALESCE(json_valid(page_type_cycle_json),0)=1),
  CHECK(COALESCE(json_type(page_type_cycle_json),0)='object'),
  CHECK(COALESCE(json_type(page_type_cycle_json,'$.schema_version'),0)='integer'),
  CHECK(COALESCE(json_valid(assignments_json),0)=1),
  CHECK(COALESCE(json_type(assignments_json),0)='object'),
  CHECK(COALESCE(json_type(assignments_json,'$.schema_version'),0)='integer')
);

CREATE UNIQUE INDEX idx_exam_ordered_grouping_decision_active_v2
  ON exam_ordered_grouping_decisions_v2(ingest_batch_id) WHERE state='active';

CREATE TRIGGER trg_exam_ordered_grouping_decision_content_guard_v2
BEFORE UPDATE ON exam_ordered_grouping_decisions_v2
WHEN NEW.public_id IS NOT OLD.public_id OR NEW.ingest_batch_id IS NOT OLD.ingest_batch_id
  OR NEW.revision IS NOT OLD.revision OR NEW.snapshot_hash IS NOT OLD.snapshot_hash
  OR NEW.source_grouping_revision_id IS NOT OLD.source_grouping_revision_id
  OR NEW.expected_pages_per_attempt IS NOT OLD.expected_pages_per_attempt
  OR NEW.first_student_id IS NOT OLD.first_student_id
  OR NEW.roster_scope_json IS NOT OLD.roster_scope_json
  OR NEW.page_type_cycle_json IS NOT OLD.page_type_cycle_json
  OR NEW.assignments_json IS NOT OLD.assignments_json OR NEW.decision IS NOT OLD.decision
  OR NEW.confirmed_by IS NOT OLD.confirmed_by OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_ORDERED_GROUPING_DECISION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_ordered_grouping_decision_state_guard_v2
BEFORE UPDATE OF state ON exam_ordered_grouping_decisions_v2
WHEN NOT (OLD.state='active' AND NEW.state IN ('superseded','voided') OR OLD.state=NEW.state)
BEGIN SELECT RAISE(ABORT,'M2_ORDERED_GROUPING_DECISION_STATE_INVALID'); END;

CREATE TRIGGER trg_exam_ordered_grouping_decision_delete_v2
BEFORE DELETE ON exam_ordered_grouping_decisions_v2
BEGIN SELECT RAISE(ABORT,'M2_ORDERED_GROUPING_DECISION_IMMUTABLE'); END;
