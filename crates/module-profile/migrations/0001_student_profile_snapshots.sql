-- M6-1：学生个人学习掌握快照。
-- 快照和节点结果均为不可变审计产物；上游变化由读模型比较 source watermark，
-- 不静默改写历史快照。

CREATE TABLE profile_policy_versions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  policy_key TEXT NOT NULL,
  revision INTEGER NOT NULL CHECK(revision > 0),
  min_independent_groups INTEGER NOT NULL CHECK(min_independent_groups > 0),
  min_distinct_dates INTEGER NOT NULL CHECK(min_distinct_dates > 0),
  min_distinct_sources INTEGER NOT NULL CHECK(min_distinct_sources > 0),
  needs_support_below REAL NOT NULL CHECK(needs_support_below >= 0 AND needs_support_below <= 1),
  stable_at_or_above REAL NOT NULL CHECK(stable_at_or_above >= 0 AND stable_at_or_above <= 1),
  freshness_days INTEGER NOT NULL CHECK(freshness_days > 0),
  state TEXT NOT NULL CHECK(state IN ('active','retired')),
  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(policy_key, revision),
  CHECK(needs_support_below < stable_at_or_above)
);

INSERT INTO profile_policy_versions
  (public_id,policy_key,revision,min_independent_groups,min_distinct_dates,
   min_distinct_sources,needs_support_below,stable_at_or_above,freshness_days,
   state,created_by,created_at)
VALUES
  ('019-profile-policy-default-000000000001','default',1,3,2,2,0.5,0.8,90,
   'active','system','2026-07-17T00:00:00.000Z');

CREATE TABLE profile_snapshots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  student_id INTEGER NOT NULL REFERENCES students(id) ON DELETE RESTRICT,
  class_id INTEGER NOT NULL REFERENCES classes(id) ON DELETE RESTRICT,
  revision INTEGER NOT NULL CHECK(revision > 0),
  range_start TEXT NOT NULL,
  range_end TEXT NOT NULL,
  scope_kind TEXT NOT NULL CHECK(scope_kind IN ('confirmed_evidence_maps')),
  scope_json TEXT NOT NULL CHECK(json_valid(scope_json)),
  source_config_json TEXT NOT NULL CHECK(json_valid(source_config_json)),
  evidence_cutoff_at TEXT NOT NULL,
  policy_id INTEGER NOT NULL REFERENCES profile_policy_versions(id) ON DELETE RESTRICT,
  policy_revision INTEGER NOT NULL CHECK(policy_revision > 0),
  source_watermark TEXT NOT NULL CHECK(length(source_watermark) = 64),
  evidence_count INTEGER NOT NULL CHECK(evidence_count >= 0),
  knowledge_node_total INTEGER NOT NULL CHECK(knowledge_node_total >= 0),
  knowledge_node_assessed INTEGER NOT NULL CHECK(knowledge_node_assessed >= 0),
  knowledge_node_eligible INTEGER NOT NULL CHECK(knowledge_node_eligible >= 0),
  ability_node_total INTEGER NOT NULL CHECK(ability_node_total >= 0),
  ability_node_assessed INTEGER NOT NULL CHECK(ability_node_assessed >= 0),
  ability_node_eligible INTEGER NOT NULL CHECK(ability_node_eligible >= 0),
  state TEXT NOT NULL CHECK(state IN ('teacher_confirmed','archived')),
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256) = 64),
  generated_by TEXT NOT NULL,
  generated_at TEXT NOT NULL,
  confirmed_by TEXT NOT NULL,
  confirmed_at TEXT NOT NULL,
  UNIQUE(student_id, revision),
  CHECK(range_start <= range_end),
  CHECK(substr(range_start,5,1)='-' AND substr(range_start,8,1)='-'),
  CHECK(substr(range_end,5,1)='-' AND substr(range_end,8,1)='-')
);
CREATE INDEX idx_profile_snapshots_student
  ON profile_snapshots(student_id, revision DESC);

CREATE TABLE profile_node_metrics (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  snapshot_id INTEGER NOT NULL REFERENCES profile_snapshots(id) ON DELETE RESTRICT,
  target_type TEXT NOT NULL CHECK(target_type IN ('knowledge_node','ability_dimension')),
  target_public_id TEXT NOT NULL,
  target_title TEXT NOT NULL,
  mastery_score REAL CHECK(mastery_score IS NULL OR (mastery_score >= 0 AND mastery_score <= 1)),
  status TEXT NOT NULL CHECK(status IN (
    'unassessed','insufficient_evidence','needs_support','developing','stable'
  )),
  confidence_level TEXT NOT NULL CHECK(confidence_level IN ('none','low','medium','high')),
  freshness TEXT NOT NULL CHECK(freshness IN ('none','fresh','aging','stale')),
  evidence_count INTEGER NOT NULL CHECK(evidence_count >= 0),
  independent_group_count INTEGER NOT NULL CHECK(independent_group_count >= 0),
  distinct_date_count INTEGER NOT NULL CHECK(distinct_date_count >= 0),
  distinct_source_count INTEGER NOT NULL CHECK(distinct_source_count >= 0),
  last_evidence_at TEXT,
  source_breakdown_json TEXT NOT NULL CHECK(json_valid(source_breakdown_json)),
  explanation TEXT NOT NULL,
  UNIQUE(snapshot_id, target_type, target_public_id)
);
CREATE INDEX idx_profile_node_metrics_snapshot
  ON profile_node_metrics(snapshot_id, target_type, status, target_title);

CREATE TABLE profile_evidence_links (
  snapshot_id INTEGER NOT NULL REFERENCES profile_snapshots(id) ON DELETE RESTRICT,
  node_metric_id INTEGER NOT NULL REFERENCES profile_node_metrics(id) ON DELETE RESTRICT,
  learning_evidence_id INTEGER NOT NULL REFERENCES learning_evidence(id) ON DELETE RESTRICT,
  independence_group_key TEXT NOT NULL,
  effective_weight REAL NOT NULL CHECK(effective_weight >= 0 AND effective_weight <= 1),
  PRIMARY KEY(snapshot_id, node_metric_id, learning_evidence_id)
);
CREATE INDEX idx_profile_evidence_links_evidence
  ON profile_evidence_links(learning_evidence_id, snapshot_id);

CREATE TRIGGER profile_policy_versions_no_update
BEFORE UPDATE ON profile_policy_versions
BEGIN SELECT RAISE(ABORT, 'profile policies are immutable'); END;
CREATE TRIGGER profile_policy_versions_no_delete
BEFORE DELETE ON profile_policy_versions
BEGIN SELECT RAISE(ABORT, 'profile policies are retained'); END;
CREATE TRIGGER profile_snapshots_no_update
BEFORE UPDATE ON profile_snapshots
BEGIN SELECT RAISE(ABORT, 'profile snapshots are immutable'); END;
CREATE TRIGGER profile_snapshots_no_delete
BEFORE DELETE ON profile_snapshots
BEGIN SELECT RAISE(ABORT, 'profile snapshots are retained'); END;
CREATE TRIGGER profile_node_metrics_no_update
BEFORE UPDATE ON profile_node_metrics
BEGIN SELECT RAISE(ABORT, 'profile metrics are immutable'); END;
CREATE TRIGGER profile_node_metrics_no_delete
BEFORE DELETE ON profile_node_metrics
BEGIN SELECT RAISE(ABORT, 'profile metrics are retained'); END;
CREATE TRIGGER profile_evidence_links_no_update
BEFORE UPDATE ON profile_evidence_links
BEGIN SELECT RAISE(ABORT, 'profile evidence links are immutable'); END;
CREATE TRIGGER profile_evidence_links_no_delete
BEFORE DELETE ON profile_evidence_links
BEGIN SELECT RAISE(ABORT, 'profile evidence links are retained'); END;
