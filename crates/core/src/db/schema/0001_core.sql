-- Core 通用表（跨模块）。见 教辅系统_平台总体架构_v1.md §4。
-- 模块专属表在各模块迁移中创建（前缀 rec_/exam_/wb_）。
-- schema_migrations 由迁移运行器创建，不在此文件。

PRAGMA foreign_keys = ON;

-- 模块注册
CREATE TABLE IF NOT EXISTS modules (
    key      TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    enabled  INTEGER NOT NULL DEFAULT 1
);

-- 学科
CREATE TABLE IF NOT EXISTS subjects (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL UNIQUE
);

-- 班级
CREATE TABLE IF NOT EXISTS classes (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL,
    term  TEXT
);

-- 学生（全模块共用）
CREATE TABLE IF NOT EXISTS students (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    student_no  TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    class_id    INTEGER REFERENCES classes(id),
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_students_class ON students(class_id);

-- 知识点（自引用成树；M2/M3/报表共用）
CREATE TABLE IF NOT EXISTS knowledge_points (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    subject_id  INTEGER REFERENCES subjects(id),
    parent_id   INTEGER REFERENCES knowledge_points(id),
    code        TEXT,
    name        TEXT NOT NULL,
    sort        INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_kp_parent ON knowledge_points(parent_id);

-- 通用复习卡片（间隔重复；背诵/错题共用）
CREATE TABLE IF NOT EXISTS memory_cards (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    module          TEXT NOT NULL,
    student_id      INTEGER NOT NULL REFERENCES students(id),
    ref_type        TEXT NOT NULL,
    ref_id          INTEGER NOT NULL,
    state           TEXT NOT NULL DEFAULT 'learning',  -- learning|review|lapsed
    stage           INTEGER NOT NULL DEFAULT 0,
    interval_days   INTEGER NOT NULL DEFAULT 0,
    ease            REAL NOT NULL DEFAULT 2.5,
    last_quality    TEXT,
    last_reviewed_at TEXT,
    due_date        TEXT,
    reps            INTEGER NOT NULL DEFAULT 0,
    lapses          INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(module, student_id, ref_type, ref_id)
);
CREATE INDEX IF NOT EXISTS idx_cards_due ON memory_cards(due_date, state);

-- 通用任务（M1/M2/M3 共用）
CREATE TABLE IF NOT EXISTS tasks (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    module          TEXT NOT NULL,
    student_id      INTEGER NOT NULL REFERENCES students(id),
    subject_id      INTEGER REFERENCES subjects(id),
    ref_type        TEXT NOT NULL,
    ref_id          INTEGER NOT NULL,
    kind            TEXT NOT NULL DEFAULT 'normal',   -- normal|makeup|review
    due_date        TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'open',
    source_task_id  INTEGER REFERENCES tasks(id),
    card_id         INTEGER REFERENCES memory_cards(id),
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(module, student_id, ref_type, ref_id, due_date, kind)
);
CREATE INDEX IF NOT EXISTS idx_tasks_due ON tasks(due_date, status);
CREATE INDEX IF NOT EXISTS idx_tasks_student ON tasks(student_id);

-- 通用提交（音频/图片/文本）
CREATE TABLE IF NOT EXISTS submissions (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    module            TEXT NOT NULL,
    task_id           INTEGER REFERENCES tasks(id),
    student_id        INTEGER REFERENCES students(id),
    ref_id            INTEGER,
    media_type        TEXT NOT NULL,                  -- audio|image|text
    file_path         TEXT NOT NULL,
    archived_path     TEXT,
    file_hash         TEXT NOT NULL UNIQUE,
    duration_ms       INTEGER,
    parsed_meta       TEXT,
    recognized_text   TEXT,
    recognize_meta    TEXT,
    recognize_status  TEXT NOT NULL DEFAULT 'pending',-- pending|ok|failed
    anomaly_type      TEXT,
    status            TEXT NOT NULL DEFAULT 'pending',-- pending|scored|confirmed|anomaly|voided
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_submissions_status ON submissions(status, anomaly_type);

-- 通用判定（两套体系并列）
CREATE TABLE IF NOT EXISTS verdicts (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    submission_id   INTEGER NOT NULL REFERENCES submissions(id),
    module          TEXT NOT NULL,
    primary_score   REAL,                             -- M1=正确率；M2=对错得分
    pass            INTEGER,                          -- 0/1
    secondary_score REAL,                             -- M1=熟练度
    quality         TEXT,                             -- M1=A/B/C
    confidence      REAL,
    scorer          TEXT NOT NULL DEFAULT 'algorithm',
    answer_version  INTEGER NOT NULL DEFAULT 1,
    metrics_json    TEXT,
    machine_note    TEXT,
    human_result    TEXT,                             -- pass|fail|reopen
    human_note      TEXT,
    decided_by      TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_verdicts_submission ON verdicts(submission_id);

-- 文件账本（幂等去重）
CREATE TABLE IF NOT EXISTS file_ledger (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    file_hash     TEXT NOT NULL UNIQUE,
    first_path    TEXT NOT NULL,
    submission_id INTEGER REFERENCES submissions(id),
    seen_count    INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

-- 非敏感设置（敏感凭据存 secrets.json，不入库）
CREATE TABLE IF NOT EXISTS app_settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
