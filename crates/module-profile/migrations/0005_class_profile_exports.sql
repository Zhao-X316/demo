-- M6.1-5（本地安全子集）：班级掌握脱敏聚合导出与权限审计。
--
-- 当前桌面版只有单机 local_teacher 身份，没有学校成员、任课班级或家长关系表。
-- 因此本迁移只保存本机老师明确生成的脱敏班级聚合快照；学校管理员、家长和
-- 跨教师聚合由服务层 fail-closed，不能借 actor_role 字段冒充已经授权。

CREATE TABLE class_profile_export_snapshots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE,
  request_payload_sha256 TEXT NOT NULL CHECK(length(request_payload_sha256)=64),
  class_profile_snapshot_id INTEGER NOT NULL
    REFERENCES class_profile_snapshots(id) ON DELETE RESTRICT,
  class_id INTEGER NOT NULL REFERENCES classes(id) ON DELETE RESTRICT,
  report_kind TEXT NOT NULL CHECK(report_kind='deidentified_class_summary'),
  purpose TEXT NOT NULL CHECK(purpose='internal_teaching'),
  actor_role TEXT NOT NULL CHECK(actor_role IN (
    'local_teacher','school_admin','guardian'
  )),
  actor_id TEXT NOT NULL CHECK(length(trim(actor_id)) > 0),
  min_group_size INTEGER NOT NULL CHECK(min_group_size >= 3),
  schema_version INTEGER NOT NULL CHECK(schema_version=1),
  rule_version TEXT NOT NULL,
  source_snapshot_payload_sha256 TEXT NOT NULL
    CHECK(length(source_snapshot_payload_sha256)=64),
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
  csv_sha256 TEXT NOT NULL CHECK(length(csv_sha256)=64),
  payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
  generated_at TEXT NOT NULL
);
CREATE INDEX idx_class_profile_export_snapshots_class
  ON class_profile_export_snapshots(class_id,generated_at DESC,id DESC);

CREATE TRIGGER class_profile_export_snapshot_scope_guard
BEFORE INSERT ON class_profile_export_snapshots
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM class_profile_snapshots snapshot
    WHERE snapshot.id=NEW.class_profile_snapshot_id
      AND snapshot.class_id=NEW.class_id
      AND snapshot.state='teacher_confirmed'
      AND snapshot.payload_sha256=NEW.source_snapshot_payload_sha256
  ) THEN RAISE(ABORT, 'class profile export source scope mismatch') END;
END;

CREATE TRIGGER class_profile_export_snapshots_no_update
BEFORE UPDATE ON class_profile_export_snapshots
BEGIN SELECT RAISE(ABORT, 'class profile exports are immutable'); END;

CREATE TRIGGER class_profile_export_snapshots_no_delete
BEFORE DELETE ON class_profile_export_snapshots
BEGIN SELECT RAISE(ABORT, 'class profile exports are retained'); END;
