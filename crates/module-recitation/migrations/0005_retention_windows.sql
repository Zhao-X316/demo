-- M1.2-2：跨日期背诵保持窗口。
--
-- decision_context 固定老师终审时的 Asia/Shanghai 业务日期和原任务计划；
-- retention_window 只比较不同业务日期的老师确认结果。同一天重复终审只允许
-- 一个 active 窗口，新事实通过 revision 取代旧窗口，不把重复练习伪装成独立证据。

CREATE TABLE rec_decision_contexts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE
    CHECK(length(public_id)=36
      AND substr(public_id,9,1)='-'
      AND substr(public_id,14,1)='-'
      AND substr(public_id,19,1)='-'
      AND substr(public_id,24,1)='-'),
  decision_effect_id INTEGER NOT NULL UNIQUE
    REFERENCES decision_effects(id) ON DELETE RESTRICT,
  decision_date TEXT NOT NULL CHECK(date(decision_date)=decision_date),
  task_id INTEGER REFERENCES tasks(id) ON DELETE RESTRICT,
  task_kind TEXT CHECK(task_kind IS NULL OR task_kind IN ('normal','makeup','review')),
  task_due_date TEXT CHECK(task_due_date IS NULL OR date(task_due_date)=task_due_date),
  state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','reverted')),
  created_at TEXT NOT NULL,
  reverted_at TEXT,
  CHECK(
    (task_id IS NULL AND task_kind IS NULL AND task_due_date IS NULL)
    OR (task_id IS NOT NULL AND task_kind IS NOT NULL AND task_due_date IS NOT NULL)
  ),
  CHECK(
    (state='active' AND reverted_at IS NULL)
    OR (state='reverted' AND reverted_at IS NOT NULL)
  )
);
CREATE INDEX idx_rec_decision_context_scope
  ON rec_decision_contexts(decision_date,decision_effect_id,state);

CREATE TABLE rec_retention_windows (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  public_id TEXT NOT NULL UNIQUE
    CHECK(length(public_id)=36
      AND substr(public_id,9,1)='-'
      AND substr(public_id,14,1)='-'
      AND substr(public_id,19,1)='-'
      AND substr(public_id,24,1)='-'),
  student_id INTEGER NOT NULL REFERENCES students(id) ON DELETE RESTRICT,
  content_id INTEGER NOT NULL REFERENCES rec_contents(id) ON DELETE RESTRICT,
  window_date TEXT NOT NULL CHECK(date(window_date)=window_date),
  revision INTEGER NOT NULL CHECK(revision>=1),
  current_effect_id INTEGER NOT NULL UNIQUE
    REFERENCES decision_effects(id) ON DELETE RESTRICT,
  previous_effect_id INTEGER NOT NULL
    REFERENCES decision_effects(id) ON DELETE RESTRICT,
  current_context_id INTEGER NOT NULL UNIQUE
    REFERENCES rec_decision_contexts(id) ON DELETE RESTRICT,
  previous_context_id INTEGER NOT NULL
    REFERENCES rec_decision_contexts(id) ON DELETE RESTRICT,
  task_id INTEGER REFERENCES tasks(id) ON DELETE RESTRICT,
  task_kind TEXT CHECK(task_kind IS NULL OR task_kind IN ('normal','makeup','review')),
  task_due_date TEXT CHECK(task_due_date IS NULL OR date(task_due_date)=task_due_date),
  planned_interval_days INTEGER CHECK(planned_interval_days IS NULL OR planned_interval_days>=0),
  actual_interval_days INTEGER NOT NULL CHECK(actual_interval_days>=1),
  result TEXT NOT NULL CHECK(result IN ('pass','fail')),
  recovered_after_lapse INTEGER NOT NULL DEFAULT 0 CHECK(recovered_after_lapse IN (0,1)),
  state TEXT NOT NULL DEFAULT 'active'
    CHECK(state IN ('active','superseded','reverted')),
  created_at TEXT NOT NULL,
  superseded_at TEXT,
  reverted_at TEXT,
  UNIQUE(student_id,content_id,window_date,revision),
  CHECK(
    (task_id IS NULL AND task_kind IS NULL AND task_due_date IS NULL
      AND planned_interval_days IS NULL)
    OR (task_id IS NOT NULL AND task_kind IS NOT NULL AND task_due_date IS NOT NULL
      AND planned_interval_days IS NOT NULL)
  ),
  CHECK(
    (state='active' AND superseded_at IS NULL AND reverted_at IS NULL)
    OR (state='superseded' AND superseded_at IS NOT NULL AND reverted_at IS NULL)
    OR (state='reverted' AND reverted_at IS NOT NULL)
  )
);
CREATE UNIQUE INDEX idx_rec_retention_window_active
  ON rec_retention_windows(student_id,content_id,window_date) WHERE state='active';
CREATE INDEX idx_rec_retention_window_scope
  ON rec_retention_windows(student_id,content_id,window_date,state,revision DESC);

CREATE TRIGGER trg_rec_decision_context_scope_insert
BEFORE INSERT ON rec_decision_contexts
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM decision_effects effect
    WHERE effect.id=NEW.decision_effect_id
      AND effect.module='recitation'
      AND effect.ref_type='content'
      AND effect.state='active'
  ) THEN RAISE(ABORT,'M1_DECISION_CONTEXT_SCOPE_MISMATCH') END;
  SELECT CASE WHEN NEW.task_id IS NOT NULL AND NOT EXISTS (
    SELECT 1
    FROM tasks task
    JOIN decision_effects effect ON effect.id=NEW.decision_effect_id
    WHERE task.id=NEW.task_id
      AND task.module='recitation'
      AND task.student_id=effect.student_id
      AND task.ref_type=effect.ref_type
      AND task.ref_id=effect.ref_id
      AND task.kind=NEW.task_kind
      AND task.due_date=NEW.task_due_date
  ) THEN RAISE(ABORT,'M1_DECISION_CONTEXT_TASK_MISMATCH') END;
END;

CREATE TRIGGER trg_rec_retention_window_scope_insert
BEFORE INSERT ON rec_retention_windows
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM rec_decision_contexts current_context
    JOIN decision_effects current_effect
      ON current_effect.id=current_context.decision_effect_id
    JOIN rec_decision_contexts previous_context
      ON previous_context.id=NEW.previous_context_id
    JOIN decision_effects previous_effect
      ON previous_effect.id=previous_context.decision_effect_id
    WHERE current_context.id=NEW.current_context_id
      AND current_context.decision_effect_id=NEW.current_effect_id
      AND current_context.decision_date=NEW.window_date
      AND current_context.state='active'
      AND current_effect.state='active'
      AND current_effect.module='recitation'
      AND current_effect.student_id=NEW.student_id
      AND current_effect.ref_type='content'
      AND current_effect.ref_id=NEW.content_id
      AND current_effect.result=NEW.result
      AND previous_context.decision_effect_id=NEW.previous_effect_id
      AND previous_context.state='active'
      AND previous_context.decision_date<NEW.window_date
      AND previous_effect.state='active'
      AND previous_effect.module='recitation'
      AND previous_effect.student_id=NEW.student_id
      AND previous_effect.ref_type='content'
      AND previous_effect.ref_id=NEW.content_id
      AND CAST(julianday(NEW.window_date)-julianday(previous_context.decision_date) AS INTEGER)
          =NEW.actual_interval_days
  ) THEN RAISE(ABORT,'M1_RETENTION_WINDOW_SCOPE_MISMATCH') END;
END;

CREATE TRIGGER trg_rec_decision_context_fact_immutable
BEFORE UPDATE ON rec_decision_contexts
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.decision_effect_id IS NOT OLD.decision_effect_id
  OR NEW.decision_date IS NOT OLD.decision_date
  OR NEW.task_id IS NOT OLD.task_id
  OR NEW.task_kind IS NOT OLD.task_kind
  OR NEW.task_due_date IS NOT OLD.task_due_date
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M1_DECISION_CONTEXT_IMMUTABLE'); END;

CREATE TRIGGER trg_rec_decision_context_state_transition
BEFORE UPDATE OF state ON rec_decision_contexts
WHEN NOT (OLD.state='active' AND NEW.state='reverted')
BEGIN SELECT RAISE(ABORT,'M1_DECISION_CONTEXT_STATE_TRANSITION_INVALID'); END;

CREATE TRIGGER trg_rec_decision_context_before_revert
BEFORE UPDATE OF state ON rec_decision_contexts
WHEN OLD.state='active'
  AND NEW.state='reverted'
  AND EXISTS (
    SELECT 1 FROM rec_retention_windows window
    WHERE window.current_context_id=OLD.id AND window.state='active'
  )
BEGIN SELECT RAISE(ABORT,'M1_RETENTION_WINDOW_MUST_REVERT_FIRST'); END;

CREATE TRIGGER trg_rec_decision_context_delete
BEFORE DELETE ON rec_decision_contexts
BEGIN SELECT RAISE(ABORT,'M1_DECISION_CONTEXT_IMMUTABLE'); END;

CREATE TRIGGER trg_rec_retention_window_fact_immutable
BEFORE UPDATE ON rec_retention_windows
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.student_id IS NOT OLD.student_id
  OR NEW.content_id IS NOT OLD.content_id
  OR NEW.window_date IS NOT OLD.window_date
  OR NEW.revision IS NOT OLD.revision
  OR NEW.current_effect_id IS NOT OLD.current_effect_id
  OR NEW.previous_effect_id IS NOT OLD.previous_effect_id
  OR NEW.current_context_id IS NOT OLD.current_context_id
  OR NEW.previous_context_id IS NOT OLD.previous_context_id
  OR NEW.task_id IS NOT OLD.task_id
  OR NEW.task_kind IS NOT OLD.task_kind
  OR NEW.task_due_date IS NOT OLD.task_due_date
  OR NEW.planned_interval_days IS NOT OLD.planned_interval_days
  OR NEW.actual_interval_days IS NOT OLD.actual_interval_days
  OR NEW.result IS NOT OLD.result
  OR NEW.recovered_after_lapse IS NOT OLD.recovered_after_lapse
  OR NEW.created_at IS NOT OLD.created_at
BEGIN SELECT RAISE(ABORT,'M1_RETENTION_WINDOW_IMMUTABLE'); END;

CREATE TRIGGER trg_rec_retention_window_state_transition
BEFORE UPDATE OF state ON rec_retention_windows
WHEN NOT (
  OLD.state='active' AND NEW.state IN ('superseded','reverted')
)
BEGIN SELECT RAISE(ABORT,'M1_RETENTION_WINDOW_STATE_TRANSITION_INVALID'); END;

CREATE TRIGGER trg_rec_retention_window_before_inactivate
BEFORE UPDATE OF state ON rec_retention_windows
WHEN OLD.state='active'
  AND NEW.state IN ('superseded','reverted')
  AND EXISTS (
    SELECT 1 FROM learning_evidence evidence
    WHERE evidence.source_module='recitation'
      AND evidence.source_ref_type='recitation_retention_window'
      AND evidence.source_ref_id=OLD.public_id
      AND evidence.source_revision=OLD.revision
      AND evidence.state='active'
  )
BEGIN SELECT RAISE(ABORT,'M1_RETENTION_EVIDENCE_MUST_INACTIVATE_FIRST'); END;

CREATE TRIGGER trg_rec_retention_window_delete
BEFORE DELETE ON rec_retention_windows
BEGIN SELECT RAISE(ABORT,'M1_RETENTION_WINDOW_IMMUTABLE'); END;

CREATE TRIGGER trg_rec_retention_before_effect_revert
BEFORE UPDATE OF state ON decision_effects
WHEN OLD.module='recitation'
  AND OLD.state='active'
  AND NEW.state='reverted'
  AND (
    EXISTS (
      SELECT 1 FROM rec_decision_contexts context
      WHERE context.decision_effect_id=OLD.id AND context.state='active'
    )
    OR EXISTS (
      SELECT 1 FROM rec_retention_windows window
      WHERE window.current_effect_id=OLD.id AND window.state='active'
    )
  )
  AND NOT EXISTS (
    SELECT 1
    FROM learning_evidence evidence
    WHERE evidence.source_module='recitation'
      AND evidence.decision_ref_type='recitation_decision_effect'
      AND evidence.decision_ref_id=CAST(OLD.id AS TEXT)
      AND evidence.decision_revision=OLD.revision
      AND evidence.state='active'
  )
BEGIN SELECT RAISE(ABORT,'M1_RETENTION_CONTEXT_MUST_REVERT_FIRST'); END;
