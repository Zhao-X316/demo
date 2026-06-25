-- M1 背诵批改 · 模块专属表（前缀 rec_）
-- 通用表（students/tasks/memory_cards/submissions/verdicts/file_ledger）由 core 迁移建立。
-- 见 背诵批改系统_技术规格与实施方案_v1.md §4 与 教辅系统_平台总体架构_v1.md §4。

PRAGMA foreign_keys = ON;

-- 背诵内容（答案带版本，改答案 +1，用于重判）
CREATE TABLE IF NOT EXISTS rec_contents (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    content_no      TEXT NOT NULL UNIQUE,        -- 内容编号：文件名匹配字段
    title           TEXT NOT NULL,
    answer_text     TEXT NOT NULL,
    answer_version  INTEGER NOT NULL DEFAULT 1,
    subject_id      INTEGER,                     -- 关联 core.subjects（可空）
    enabled         INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rec_contents_enabled ON rec_contents(enabled);
