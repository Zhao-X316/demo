-- M6-4：老师确认的班级共性教学输入。
--
-- 预览只汇总当前、未过期班级快照中达到双门槛的共同支持节点。老师可删减、
-- 修改标题与课堂说明后确认；确认记录不可变，不建立作业、不修改成绩、学习
-- 证据、学生标签或背诵排程。

CREATE TABLE class_teaching_input_drafts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE,
  request_payload_sha256 TEXT NOT NULL CHECK(length(request_payload_sha256)=64),
  class_profile_snapshot_id INTEGER NOT NULL
    REFERENCES class_profile_snapshots(id) ON DELETE RESTRICT,
  class_id INTEGER NOT NULL REFERENCES classes(id) ON DELETE RESTRICT,
  title TEXT NOT NULL CHECK(length(trim(title)) BETWEEN 1 AND 100),
  teaching_note TEXT NOT NULL CHECK(length(trim(teaching_note)) BETWEEN 1 AND 4000),
  estimated_minutes INTEGER NOT NULL CHECK(estimated_minutes BETWEEN 1 AND 240),
  selected_node_count INTEGER NOT NULL CHECK(selected_node_count BETWEEN 1 AND 100),
  source_snapshot_payload_sha256 TEXT NOT NULL
    CHECK(length(source_snapshot_payload_sha256)=64),
  schema_version INTEGER NOT NULL CHECK(schema_version=1),
  rule_version TEXT NOT NULL,
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
  state TEXT NOT NULL CHECK(state='teacher_confirmed'),
  confirmed_by TEXT NOT NULL CHECK(length(trim(confirmed_by)) > 0),
  confirmed_at TEXT NOT NULL
);
CREATE INDEX idx_class_teaching_input_drafts_class
  ON class_teaching_input_drafts(class_id,confirmed_at DESC,id DESC);

CREATE TABLE class_teaching_input_items (
  draft_id INTEGER NOT NULL
    REFERENCES class_teaching_input_drafts(id) ON DELETE RESTRICT,
  order_index INTEGER NOT NULL CHECK(order_index >= 0),
  class_profile_node_metric_id INTEGER NOT NULL
    REFERENCES class_profile_node_metrics(id) ON DELETE RESTRICT,
  target_type TEXT NOT NULL CHECK(target_type IN ('knowledge_node','ability_dimension')),
  target_public_id TEXT NOT NULL,
  target_title TEXT NOT NULL,
  confidence_level TEXT NOT NULL CHECK(confidence_level IN ('low','medium','high')),
  total_student_count INTEGER NOT NULL CHECK(total_student_count >= 0),
  eligible_student_count INTEGER NOT NULL CHECK(eligible_student_count > 0),
  needs_support_count INTEGER NOT NULL CHECK(needs_support_count > 0),
  needs_support_ratio REAL NOT NULL
    CHECK(needs_support_ratio >= 0 AND needs_support_ratio <= 1),
  explanation TEXT NOT NULL,
  PRIMARY KEY(draft_id,class_profile_node_metric_id),
  UNIQUE(draft_id,order_index)
);

CREATE TRIGGER class_teaching_input_draft_scope_guard
BEFORE INSERT ON class_teaching_input_drafts
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM class_profile_snapshots snapshot
    WHERE snapshot.id=NEW.class_profile_snapshot_id
      AND snapshot.class_id=NEW.class_id
      AND snapshot.state='teacher_confirmed'
      AND snapshot.payload_sha256=NEW.source_snapshot_payload_sha256
  ) THEN RAISE(ABORT, 'class teaching input source scope mismatch') END;
END;

CREATE TRIGGER class_teaching_input_item_scope_guard
BEFORE INSERT ON class_teaching_input_items
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM class_teaching_input_drafts draft
    JOIN class_profile_node_metrics metric
      ON metric.id=NEW.class_profile_node_metric_id
     AND metric.snapshot_id=draft.class_profile_snapshot_id
     AND metric.target_type=NEW.target_type
     AND metric.target_public_id=NEW.target_public_id
     AND metric.target_title=NEW.target_title
     AND metric.confidence_level=NEW.confidence_level
     AND metric.total_student_count=NEW.total_student_count
     AND metric.eligible_student_count=NEW.eligible_student_count
     AND metric.needs_support_count=NEW.needs_support_count
     AND metric.needs_support_ratio=NEW.needs_support_ratio
     AND metric.explanation=NEW.explanation
     AND metric.class_status='common_needs_support'
     AND metric.sample_sufficient=1
    WHERE draft.id=NEW.draft_id
  ) THEN RAISE(ABORT, 'class teaching input item scope mismatch') END;
END;

CREATE TRIGGER class_teaching_input_drafts_no_update
BEFORE UPDATE ON class_teaching_input_drafts
BEGIN SELECT RAISE(ABORT, 'class teaching inputs are immutable'); END;
CREATE TRIGGER class_teaching_input_drafts_no_delete
BEFORE DELETE ON class_teaching_input_drafts
BEGIN SELECT RAISE(ABORT, 'class teaching inputs are retained'); END;
CREATE TRIGGER class_teaching_input_items_no_update
BEFORE UPDATE ON class_teaching_input_items
BEGIN SELECT RAISE(ABORT, 'class teaching input items are immutable'); END;
CREATE TRIGGER class_teaching_input_items_no_delete
BEFORE DELETE ON class_teaching_input_items
BEGIN SELECT RAISE(ABORT, 'class teaching input items are retained'); END;
