-- M3-4：错题统计与导出快照。
-- 导出冻结当时的范围、口径、版本、水位和完整 payload；不保存用户选择的外部文件路径。

CREATE TABLE wb_report_snapshots (
  id                            INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id                     TEXT NOT NULL UNIQUE,
  report_kind                   TEXT NOT NULL CHECK(report_kind IN ('class_summary','student_parent')),
  class_id                      INTEGER NOT NULL REFERENCES classes(id),
  student_id                    INTEGER REFERENCES students(id),
  range_start                   TEXT NOT NULL CHECK(
    range_start GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
    AND date(range_start)=range_start
  ),
  range_end                     TEXT NOT NULL CHECK(
    range_end GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'
    AND date(range_end)=range_end
  ),
  schema_version                INTEGER NOT NULL CHECK(schema_version > 0),
  rule_version                  TEXT NOT NULL CHECK(length(trim(rule_version)) > 0),
  source_exam_watermark         TEXT,
  evidence_count                INTEGER NOT NULL CHECK(evidence_count >= 0),
  fact_count                    INTEGER NOT NULL CHECK(fact_count >= 0),
  confirmed_cause_review_count  INTEGER NOT NULL CHECK(confirmed_cause_review_count >= 0),
  payload_sha256                TEXT NOT NULL CHECK(
    length(payload_sha256)=64
    AND lower(payload_sha256)=payload_sha256
    AND payload_sha256 NOT GLOB '*[^0-9a-f]*'
  ),
  csv_sha256                    TEXT NOT NULL CHECK(
    length(csv_sha256)=64
    AND lower(csv_sha256)=csv_sha256
    AND csv_sha256 NOT GLOB '*[^0-9a-f]*'
  ),
  payload_json                  TEXT NOT NULL CHECK(json_valid(payload_json)),
  generated_by                  TEXT NOT NULL CHECK(length(trim(generated_by)) > 0),
  generated_at                  TEXT NOT NULL,
  CHECK(range_start <= range_end),
  CHECK(
    (report_kind='class_summary' AND student_id IS NULL)
    OR
    (report_kind='student_parent' AND student_id IS NOT NULL)
  )
);

CREATE INDEX idx_wb_report_snapshots_scope
  ON wb_report_snapshots(class_id,student_id,generated_at);

CREATE TRIGGER trg_wb_report_snapshot_insert
BEFORE INSERT ON wb_report_snapshots
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1 FROM classes class
    WHERE class.id=NEW.class_id
  ) THEN RAISE(ABORT,'WB_REPORT_CLASS_MISMATCH') END;

  SELECT CASE WHEN NEW.student_id IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM students student
    WHERE student.id=NEW.student_id
      AND student.class_id=NEW.class_id
      AND student.enabled=1
  ) THEN RAISE(ABORT,'WB_REPORT_STUDENT_MISMATCH') END;
END;

CREATE TRIGGER trg_wb_report_snapshot_update
BEFORE UPDATE ON wb_report_snapshots
BEGIN SELECT RAISE(ABORT,'WB_REPORT_SNAPSHOT_IMMUTABLE'); END;

CREATE TRIGGER trg_wb_report_snapshot_delete
BEFORE DELETE ON wb_report_snapshots
BEGIN SELECT RAISE(ABORT,'WB_REPORT_SNAPSHOT_IMMUTABLE'); END;
