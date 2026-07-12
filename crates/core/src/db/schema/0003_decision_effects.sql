-- 人工终审效果账本。
-- 每次 pass/fail 终审保存应用副作用前的 card/task 快照，支持最新效果的可逆改判。
CREATE TABLE IF NOT EXISTS decision_effects (
    id                           INTEGER PRIMARY KEY AUTOINCREMENT,
    verdict_id                   INTEGER NOT NULL REFERENCES verdicts(id),
    revision                     INTEGER NOT NULL,
    module                       TEXT NOT NULL,
    student_id                   INTEGER NOT NULL REFERENCES students(id),
    ref_type                     TEXT NOT NULL,
    ref_id                       INTEGER NOT NULL,
    result                       TEXT NOT NULL CHECK(result IN ('pass','fail')),
    card_before_json             TEXT NOT NULL,
    task_before_json             TEXT NOT NULL,
    card_after_json              TEXT NOT NULL,
    task_after_json              TEXT NOT NULL,
    created_makeup_task_ids_json TEXT NOT NULL DEFAULT '[]',
    state                        TEXT NOT NULL DEFAULT 'active'
                                 CHECK(state IN ('active','reverted')),
    created_at                   TEXT NOT NULL DEFAULT (datetime('now')),
    reverted_at                  TEXT,
    UNIQUE(verdict_id, revision)
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_decision_effects_active_verdict
    ON decision_effects(verdict_id) WHERE state='active';
CREATE INDEX IF NOT EXISTS idx_decision_effects_scope
    ON decision_effects(module, student_id, ref_type, ref_id, state, id);
