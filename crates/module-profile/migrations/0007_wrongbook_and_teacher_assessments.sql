-- M6-3：随个人快照冻结 M3 当前错题恢复事实，并保存老师补充判断。
-- 错题事实只用于解释恢复过程，不直接替代节点掌握计算；老师判断与系统结论并列，
-- 修正和清除均追加不可变 revision。

CREATE TABLE profile_wrongbook_fact_links (
  snapshot_id INTEGER NOT NULL REFERENCES profile_snapshots(id) ON DELETE RESTRICT,
  question_version_public_id TEXT NOT NULL,
  question_type TEXT NOT NULL,
  stem TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN (
    'needs_correction','corrected_once','rechecked_correct'
  )),
  first_error_at TEXT NOT NULL,
  last_error_at TEXT NOT NULL,
  latest_response_at TEXT NOT NULL,
  published_response_count INTEGER NOT NULL CHECK(published_response_count > 0),
  error_response_count INTEGER NOT NULL CHECK(error_response_count > 0),
  repeated_error INTEGER NOT NULL CHECK(repeated_error IN (0,1)),
  correction_status TEXT,
  reinforcement_status TEXT,
  knowledge_nodes_json TEXT NOT NULL CHECK(json_valid(knowledge_nodes_json)),
  ability_dimensions_json TEXT NOT NULL CHECK(json_valid(ability_dimensions_json)),
  PRIMARY KEY(snapshot_id, question_version_public_id)
);
CREATE INDEX idx_profile_wrongbook_fact_links_snapshot
  ON profile_wrongbook_fact_links(snapshot_id,status,latest_response_at);

CREATE TABLE profile_teacher_assessment_revisions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  snapshot_id INTEGER NOT NULL REFERENCES profile_snapshots(id) ON DELETE RESTRICT,
  node_metric_id INTEGER NOT NULL REFERENCES profile_node_metrics(id) ON DELETE RESTRICT,
  revision INTEGER NOT NULL CHECK(revision > 0),
  assessment TEXT CHECK(assessment IS NULL OR assessment IN (
    'not_taught','needs_support','developing','stable','observe'
  )),
  note TEXT CHECK(note IS NULL OR length(note) <= 500),
  state TEXT NOT NULL CHECK(state IN ('active','voided')),
  supersedes_public_id TEXT
    REFERENCES profile_teacher_assessment_revisions(public_id) ON DELETE RESTRICT,
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
  created_by TEXT NOT NULL CHECK(length(trim(created_by)) > 0),
  created_at TEXT NOT NULL,
  UNIQUE(snapshot_id,node_metric_id,revision),
  CHECK((state='active' AND assessment IS NOT NULL) OR
        (state='voided' AND assessment IS NULL)),
  CHECK((revision=1 AND supersedes_public_id IS NULL) OR
        (revision>1 AND supersedes_public_id IS NOT NULL))
);
CREATE INDEX idx_profile_teacher_assessment_scope
  ON profile_teacher_assessment_revisions(snapshot_id,node_metric_id,revision DESC);

CREATE TRIGGER profile_wrongbook_fact_links_no_update
BEFORE UPDATE ON profile_wrongbook_fact_links
BEGIN SELECT RAISE(ABORT, 'profile wrongbook facts are immutable'); END;

CREATE TRIGGER profile_wrongbook_fact_links_no_delete
BEFORE DELETE ON profile_wrongbook_fact_links
BEGIN SELECT RAISE(ABORT, 'profile wrongbook facts are retained'); END;

CREATE TRIGGER profile_teacher_assessment_scope_guard
BEFORE INSERT ON profile_teacher_assessment_revisions
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM profile_node_metrics metric
    WHERE metric.id=NEW.node_metric_id AND metric.snapshot_id=NEW.snapshot_id
  ) THEN RAISE(ABORT, 'profile teacher assessment metric mismatch') END;

  SELECT CASE WHEN NEW.revision > 1 AND NOT EXISTS (
    SELECT 1 FROM profile_teacher_assessment_revisions previous
    WHERE previous.public_id=NEW.supersedes_public_id
      AND previous.snapshot_id=NEW.snapshot_id
      AND previous.node_metric_id=NEW.node_metric_id
      AND previous.revision=NEW.revision-1
  ) THEN RAISE(ABORT, 'profile teacher assessment revision mismatch') END;
END;

CREATE TRIGGER profile_teacher_assessment_revisions_no_update
BEFORE UPDATE ON profile_teacher_assessment_revisions
BEGIN SELECT RAISE(ABORT, 'profile teacher assessments are immutable'); END;

CREATE TRIGGER profile_teacher_assessment_revisions_no_delete
BEFORE DELETE ON profile_teacher_assessment_revisions
BEGIN SELECT RAISE(ABORT, 'profile teacher assessments are retained'); END;
