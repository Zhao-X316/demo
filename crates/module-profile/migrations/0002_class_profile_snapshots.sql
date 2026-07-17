-- M6.1-2：班级掌握快照与知识热力图。
-- 班级快照只引用老师已确认的个人快照；所有聚合、输入和单元格均不可变。
-- stale 由读模型比较当前班级名单、最新个人快照和策略水位，不覆盖旧快照。

CREATE TABLE class_profile_policy_versions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  policy_key TEXT NOT NULL,
  revision INTEGER NOT NULL CHECK(revision > 0),
  min_eligible_students INTEGER NOT NULL CHECK(min_eligible_students > 0),
  min_eligible_ratio REAL NOT NULL CHECK(min_eligible_ratio > 0 AND min_eligible_ratio <= 1),
  common_support_ratio_at_or_above REAL NOT NULL
    CHECK(common_support_ratio_at_or_above > 0 AND common_support_ratio_at_or_above <= 1),
  state TEXT NOT NULL CHECK(state IN ('active','retired')),
  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(policy_key, revision)
);

INSERT INTO class_profile_policy_versions
  (public_id,policy_key,revision,min_eligible_students,min_eligible_ratio,
   common_support_ratio_at_or_above,state,created_by,created_at)
VALUES
  ('019-class-profile-policy-default-000001','default',1,3,0.5,0.4,
   'active','system','2026-07-17T00:00:00.000Z');

CREATE TABLE class_profile_snapshots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  class_id INTEGER NOT NULL REFERENCES classes(id) ON DELETE RESTRICT,
  revision INTEGER NOT NULL CHECK(revision > 0),
  range_start TEXT NOT NULL,
  range_end TEXT NOT NULL,
  scope_kind TEXT NOT NULL CHECK(scope_kind='latest_exact_range_student_snapshots'),
  scope_json TEXT NOT NULL CHECK(json_valid(scope_json)),
  source_config_json TEXT NOT NULL CHECK(json_valid(source_config_json)),
  evidence_cutoff_at TEXT NOT NULL,
  policy_id INTEGER NOT NULL REFERENCES class_profile_policy_versions(id) ON DELETE RESTRICT,
  policy_revision INTEGER NOT NULL CHECK(policy_revision > 0),
  source_watermark TEXT NOT NULL CHECK(length(source_watermark)=64),
  total_student_count INTEGER NOT NULL CHECK(total_student_count >= 0),
  snapshot_student_count INTEGER NOT NULL CHECK(snapshot_student_count >= 0),
  eligible_student_count INTEGER NOT NULL CHECK(eligible_student_count >= 0),
  knowledge_node_total INTEGER NOT NULL CHECK(knowledge_node_total >= 0),
  knowledge_node_sample_sufficient INTEGER NOT NULL CHECK(knowledge_node_sample_sufficient >= 0),
  ability_node_total INTEGER NOT NULL CHECK(ability_node_total >= 0),
  ability_node_sample_sufficient INTEGER NOT NULL CHECK(ability_node_sample_sufficient >= 0),
  state TEXT NOT NULL CHECK(state IN ('teacher_confirmed','archived')),
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
  generated_by TEXT NOT NULL,
  generated_at TEXT NOT NULL,
  confirmed_by TEXT NOT NULL,
  confirmed_at TEXT NOT NULL,
  UNIQUE(class_id, revision),
  CHECK(range_start <= range_end),
  CHECK(substr(range_start,5,1)='-' AND substr(range_start,8,1)='-'),
  CHECK(substr(range_end,5,1)='-' AND substr(range_end,8,1)='-')
);
CREATE INDEX idx_class_profile_snapshots_class
  ON class_profile_snapshots(class_id, revision DESC);

CREATE TABLE class_profile_student_inputs (
  snapshot_id INTEGER NOT NULL REFERENCES class_profile_snapshots(id) ON DELETE RESTRICT,
  student_id INTEGER NOT NULL REFERENCES students(id) ON DELETE RESTRICT,
  student_profile_snapshot_id INTEGER REFERENCES profile_snapshots(id) ON DELETE RESTRICT,
  inclusion_status TEXT NOT NULL CHECK(inclusion_status IN (
    'included','missing_snapshot','scope_mismatch','stale_snapshot'
  )),
  detail TEXT NOT NULL,
  PRIMARY KEY(snapshot_id, student_id)
);
CREATE INDEX idx_class_profile_student_inputs_source
  ON class_profile_student_inputs(student_profile_snapshot_id, snapshot_id);

CREATE TABLE class_profile_node_metrics (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  snapshot_id INTEGER NOT NULL REFERENCES class_profile_snapshots(id) ON DELETE RESTRICT,
  target_type TEXT NOT NULL CHECK(target_type IN ('knowledge_node','ability_dimension')),
  target_public_id TEXT NOT NULL,
  target_title TEXT NOT NULL,
  average_mastery_score REAL
    CHECK(average_mastery_score IS NULL OR
          (average_mastery_score >= 0 AND average_mastery_score <= 1)),
  class_status TEXT NOT NULL CHECK(class_status IN (
    'no_evidence','class_evidence_insufficient','observed','common_needs_support'
  )),
  confidence_level TEXT NOT NULL CHECK(confidence_level IN ('none','low','medium','high')),
  total_student_count INTEGER NOT NULL CHECK(total_student_count >= 0),
  snapshot_student_count INTEGER NOT NULL CHECK(snapshot_student_count >= 0),
  assessed_student_count INTEGER NOT NULL CHECK(assessed_student_count >= 0),
  eligible_student_count INTEGER NOT NULL CHECK(eligible_student_count >= 0),
  needs_support_count INTEGER NOT NULL CHECK(needs_support_count >= 0),
  developing_count INTEGER NOT NULL CHECK(developing_count >= 0),
  stable_count INTEGER NOT NULL CHECK(stable_count >= 0),
  insufficient_evidence_count INTEGER NOT NULL CHECK(insufficient_evidence_count >= 0),
  unassessed_count INTEGER NOT NULL CHECK(unassessed_count >= 0),
  missing_snapshot_count INTEGER NOT NULL CHECK(missing_snapshot_count >= 0),
  scope_mismatch_count INTEGER NOT NULL CHECK(scope_mismatch_count >= 0),
  stale_snapshot_count INTEGER NOT NULL CHECK(stale_snapshot_count >= 0),
  eligible_ratio REAL NOT NULL CHECK(eligible_ratio >= 0 AND eligible_ratio <= 1),
  needs_support_ratio REAL
    CHECK(needs_support_ratio IS NULL OR
          (needs_support_ratio >= 0 AND needs_support_ratio <= 1)),
  sample_sufficient INTEGER NOT NULL CHECK(sample_sufficient IN (0,1)),
  last_evidence_at TEXT,
  source_breakdown_json TEXT NOT NULL CHECK(json_valid(source_breakdown_json)),
  explanation TEXT NOT NULL,
  UNIQUE(snapshot_id,target_type,target_public_id)
);
CREATE INDEX idx_class_profile_node_metrics_snapshot
  ON class_profile_node_metrics(snapshot_id,target_type,class_status,target_title);

CREATE TABLE class_profile_student_cells (
  snapshot_id INTEGER NOT NULL REFERENCES class_profile_snapshots(id) ON DELETE RESTRICT,
  node_metric_id INTEGER NOT NULL REFERENCES class_profile_node_metrics(id) ON DELETE RESTRICT,
  student_id INTEGER NOT NULL REFERENCES students(id) ON DELETE RESTRICT,
  student_profile_snapshot_id INTEGER REFERENCES profile_snapshots(id) ON DELETE RESTRICT,
  student_profile_metric_id INTEGER REFERENCES profile_node_metrics(id) ON DELETE RESTRICT,
  status TEXT NOT NULL CHECK(status IN (
    'missing_snapshot','scope_mismatch','stale_snapshot','unassessed',
    'insufficient_evidence','needs_support','developing','stable'
  )),
  mastery_score REAL CHECK(mastery_score IS NULL OR (mastery_score >= 0 AND mastery_score <= 1)),
  confidence_level TEXT NOT NULL CHECK(confidence_level IN ('none','low','medium','high')),
  last_evidence_at TEXT,
  PRIMARY KEY(snapshot_id,node_metric_id,student_id)
);
CREATE INDEX idx_class_profile_student_cells_student
  ON class_profile_student_cells(snapshot_id,student_id,status);

CREATE TRIGGER class_profile_policy_versions_no_update
BEFORE UPDATE ON class_profile_policy_versions
BEGIN SELECT RAISE(ABORT, 'class profile policies are immutable'); END;
CREATE TRIGGER class_profile_policy_versions_no_delete
BEFORE DELETE ON class_profile_policy_versions
BEGIN SELECT RAISE(ABORT, 'class profile policies are retained'); END;
CREATE TRIGGER class_profile_snapshots_no_update
BEFORE UPDATE ON class_profile_snapshots
BEGIN SELECT RAISE(ABORT, 'class profile snapshots are immutable'); END;
CREATE TRIGGER class_profile_snapshots_no_delete
BEFORE DELETE ON class_profile_snapshots
BEGIN SELECT RAISE(ABORT, 'class profile snapshots are retained'); END;
CREATE TRIGGER class_profile_student_inputs_no_update
BEFORE UPDATE ON class_profile_student_inputs
BEGIN SELECT RAISE(ABORT, 'class profile inputs are immutable'); END;
CREATE TRIGGER class_profile_student_inputs_no_delete
BEFORE DELETE ON class_profile_student_inputs
BEGIN SELECT RAISE(ABORT, 'class profile inputs are retained'); END;
CREATE TRIGGER class_profile_node_metrics_no_update
BEFORE UPDATE ON class_profile_node_metrics
BEGIN SELECT RAISE(ABORT, 'class profile metrics are immutable'); END;
CREATE TRIGGER class_profile_node_metrics_no_delete
BEFORE DELETE ON class_profile_node_metrics
BEGIN SELECT RAISE(ABORT, 'class profile metrics are retained'); END;
CREATE TRIGGER class_profile_student_cells_no_update
BEFORE UPDATE ON class_profile_student_cells
BEGIN SELECT RAISE(ABORT, 'class profile cells are immutable'); END;
CREATE TRIGGER class_profile_student_cells_no_delete
BEFORE DELETE ON class_profile_student_cells
BEGIN SELECT RAISE(ABORT, 'class profile cells are retained'); END;
