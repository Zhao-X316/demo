-- M2-B3a0：老师查看已归组原图后，一次确认页面质量并激活正式页面归属。
-- 该快照只记录本次质量确认与派生的 attempt/page-match 身份；不创建评分、发布或学习证据。

CREATE TABLE exam_ordered_grouping_activations_v2 (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  ingest_batch_id INTEGER NOT NULL REFERENCES exam_ingest_batches_v2(id),
  grouping_decision_id INTEGER NOT NULL REFERENCES exam_ordered_grouping_decisions_v2(id),
  revision INTEGER NOT NULL CHECK(revision>0),
  snapshot_hash TEXT NOT NULL CHECK(length(snapshot_hash)=64),
  rejected_page_ids_json TEXT NOT NULL,
  attempt_ids_json TEXT NOT NULL,
  page_match_ids_json TEXT NOT NULL,
  mapped_group_count INTEGER NOT NULL CHECK(mapped_group_count>=0),
  rejected_group_count INTEGER NOT NULL CHECK(rejected_group_count>=0),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','superseded','voided')),
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by))>0),
  created_at TEXT NOT NULL,
  UNIQUE(ingest_batch_id,revision),
  UNIQUE(grouping_decision_id,snapshot_hash),
  CHECK(COALESCE(json_valid(rejected_page_ids_json),0)=1),
  CHECK(COALESCE(json_type(rejected_page_ids_json),0)='object'),
  CHECK(COALESCE(json_type(rejected_page_ids_json,'$.schema_version'),0)='integer'),
  CHECK(COALESCE(json_valid(attempt_ids_json),0)=1),
  CHECK(COALESCE(json_type(attempt_ids_json),0)='object'),
  CHECK(COALESCE(json_type(attempt_ids_json,'$.schema_version'),0)='integer'),
  CHECK(COALESCE(json_valid(page_match_ids_json),0)=1),
  CHECK(COALESCE(json_type(page_match_ids_json),0)='object'),
  CHECK(COALESCE(json_type(page_match_ids_json,'$.schema_version'),0)='integer')
);

CREATE UNIQUE INDEX idx_exam_ordered_grouping_activation_active_v2
  ON exam_ordered_grouping_activations_v2(ingest_batch_id) WHERE state='active';

CREATE TRIGGER trg_exam_ordered_grouping_activation_content_guard_v2
BEFORE UPDATE ON exam_ordered_grouping_activations_v2
WHEN NEW.public_id IS NOT OLD.public_id OR NEW.ingest_batch_id IS NOT OLD.ingest_batch_id
  OR NEW.grouping_decision_id IS NOT OLD.grouping_decision_id
  OR NEW.revision IS NOT OLD.revision OR NEW.snapshot_hash IS NOT OLD.snapshot_hash
  OR NEW.rejected_page_ids_json IS NOT OLD.rejected_page_ids_json
  OR NEW.attempt_ids_json IS NOT OLD.attempt_ids_json
  OR NEW.page_match_ids_json IS NOT OLD.page_match_ids_json
  OR NEW.mapped_group_count IS NOT OLD.mapped_group_count
  OR NEW.rejected_group_count IS NOT OLD.rejected_group_count
  OR NEW.confirmed_by IS NOT OLD.confirmed_by OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M2_ORDERED_GROUPING_ACTIVATION_IMMUTABLE'); END;

CREATE TRIGGER trg_exam_ordered_grouping_activation_state_guard_v2
BEFORE UPDATE OF state ON exam_ordered_grouping_activations_v2
WHEN NOT (OLD.state='active' AND NEW.state IN ('superseded','voided') OR OLD.state=NEW.state)
BEGIN SELECT RAISE(ABORT,'M2_ORDERED_GROUPING_ACTIVATION_STATE_INVALID'); END;

CREATE TRIGGER trg_exam_ordered_grouping_activation_delete_v2
BEFORE DELETE ON exam_ordered_grouping_activations_v2
BEGIN SELECT RAISE(ABORT,'M2_ORDERED_GROUPING_ACTIVATION_IMMUTABLE'); END;
