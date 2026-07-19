-- M6-4：个人学习掌握报告。
--
-- 报告只允许本机老师从当前、未过期的个人掌握快照明确生成，作为教师内部
-- 学习反馈材料。报告快照冻结生成时的老师补充判断水位，不建立家长授权、
-- 自动发送或公开分享链。

CREATE TABLE student_profile_report_snapshots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE,
  request_payload_sha256 TEXT NOT NULL CHECK(length(request_payload_sha256)=64),
  profile_snapshot_id INTEGER NOT NULL
    REFERENCES profile_snapshots(id) ON DELETE RESTRICT,
  student_id INTEGER NOT NULL REFERENCES students(id) ON DELETE RESTRICT,
  class_id INTEGER NOT NULL REFERENCES classes(id) ON DELETE RESTRICT,
  report_kind TEXT NOT NULL CHECK(report_kind='student_learning_summary'),
  purpose TEXT NOT NULL CHECK(purpose='teacher_internal_feedback'),
  actor_role TEXT NOT NULL CHECK(actor_role='local_teacher'),
  actor_id TEXT NOT NULL CHECK(length(trim(actor_id)) > 0),
  schema_version INTEGER NOT NULL CHECK(schema_version=1),
  rule_version TEXT NOT NULL,
  source_snapshot_payload_sha256 TEXT NOT NULL
    CHECK(length(source_snapshot_payload_sha256)=64),
  teacher_assessment_watermark TEXT NOT NULL
    CHECK(length(teacher_assessment_watermark)=64),
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
  html_sha256 TEXT NOT NULL CHECK(length(html_sha256)=64),
  payload_json TEXT NOT NULL CHECK(json_valid(payload_json)),
  generated_at TEXT NOT NULL
);
CREATE INDEX idx_student_profile_report_snapshots_student
  ON student_profile_report_snapshots(student_id,generated_at DESC,id DESC);

CREATE TRIGGER student_profile_report_snapshot_scope_guard
BEFORE INSERT ON student_profile_report_snapshots
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM profile_snapshots snapshot
    WHERE snapshot.id=NEW.profile_snapshot_id
      AND snapshot.student_id=NEW.student_id
      AND snapshot.class_id=NEW.class_id
      AND snapshot.state='teacher_confirmed'
      AND snapshot.payload_sha256=NEW.source_snapshot_payload_sha256
  ) THEN RAISE(ABORT, 'student profile report source scope mismatch') END;
END;

CREATE TRIGGER student_profile_report_snapshots_no_update
BEFORE UPDATE ON student_profile_report_snapshots
BEGIN SELECT RAISE(ABORT, 'student profile reports are immutable'); END;

CREATE TRIGGER student_profile_report_snapshots_no_delete
BEFORE DELETE ON student_profile_report_snapshots
BEGIN SELECT RAISE(ABORT, 'student profile reports are retained'); END;
