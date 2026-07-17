-- M6.1-3：班级教学事件。
-- 教学事件只为快照变化提供背景，不修改学习证据，也不自动声明因果关系。
-- 修正和作废均追加 revision；历史 revision 永久保留。

CREATE TABLE class_teaching_event_revisions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE,
  request_key TEXT NOT NULL UNIQUE,
  event_key TEXT NOT NULL,
  class_id INTEGER NOT NULL REFERENCES classes(id) ON DELETE RESTRICT,
  revision INTEGER NOT NULL CHECK(revision > 0),
  event_type TEXT NOT NULL CHECK(event_type IN (
    'new_lesson','review','quiz','exam','holiday','schedule_pause'
  )),
  title TEXT NOT NULL CHECK(length(trim(title)) > 0),
  range_start TEXT NOT NULL,
  range_end TEXT NOT NULL,
  note TEXT,
  state TEXT NOT NULL CHECK(state IN ('active','voided')),
  supersedes_public_id TEXT REFERENCES class_teaching_event_revisions(public_id) ON DELETE RESTRICT,
  payload_sha256 TEXT NOT NULL CHECK(length(payload_sha256)=64),
  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(event_key, revision),
  CHECK(range_start <= range_end),
  CHECK(substr(range_start,5,1)='-' AND substr(range_start,8,1)='-'),
  CHECK(substr(range_end,5,1)='-' AND substr(range_end,8,1)='-'),
  CHECK((revision=1 AND supersedes_public_id IS NULL) OR
        (revision>1 AND supersedes_public_id IS NOT NULL))
);
CREATE INDEX idx_class_teaching_events_scope
  ON class_teaching_event_revisions(class_id,range_start,range_end,event_key,revision DESC);

CREATE TRIGGER class_teaching_event_scope_guard
BEFORE INSERT ON class_teaching_event_revisions
WHEN NEW.revision > 1
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM class_teaching_event_revisions previous
    WHERE previous.public_id=NEW.supersedes_public_id
      AND previous.event_key=NEW.event_key
      AND previous.class_id=NEW.class_id
      AND previous.revision=NEW.revision-1
  ) THEN RAISE(ABORT, 'teaching event revision scope mismatch') END;
END;

CREATE TRIGGER class_teaching_event_revisions_no_update
BEFORE UPDATE ON class_teaching_event_revisions
BEGIN SELECT RAISE(ABORT, 'teaching event revisions are immutable'); END;

CREATE TRIGGER class_teaching_event_revisions_no_delete
BEFORE DELETE ON class_teaching_event_revisions
BEGIN SELECT RAISE(ABORT, 'teaching event revisions are retained'); END;
