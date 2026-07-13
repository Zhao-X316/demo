-- Core 统一资产层：M1 音频、M2 页面/题区裁剪和后续导出共用。
-- 旧 submissions 的路径字段暂时保留；artifact_id 只作为兼容期可空引用，不猜旧数据。

CREATE TABLE artifacts (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    public_id           TEXT NOT NULL UNIQUE
                          CHECK (
                              length(public_id) = 36
                              AND substr(public_id, 9, 1) = '-'
                              AND substr(public_id, 14, 1) = '-'
                              AND substr(public_id, 19, 1) = '-'
                              AND substr(public_id, 24, 1) = '-'
                          ),
    kind                TEXT NOT NULL
                          CHECK (kind IN ('audio', 'document', 'page', 'crop', 'image', 'export')),
    sha256              TEXT NOT NULL
                          CHECK (
                              length(sha256) = 64
                              AND lower(sha256) = sha256
                              AND sha256 NOT GLOB '*[^0-9a-f]*'
                          ),
    mime_type           TEXT NOT NULL CHECK (length(trim(mime_type)) > 0),
    byte_size           INTEGER NOT NULL CHECK (byte_size >= 0),
    original_name       TEXT,
    original_path       TEXT,
    archived_path       TEXT NOT NULL CHECK (length(trim(archived_path)) > 0),
    parent_artifact_id  INTEGER REFERENCES artifacts(id) ON DELETE RESTRICT,
    derivative_type     TEXT,
    processing_version  TEXT NOT NULL CHECK (length(trim(processing_version)) > 0),
    privacy_class       TEXT NOT NULL
                          CHECK (privacy_class IN ('student_sensitive', 'teaching_content', 'public_safe')),
    archive_status      TEXT NOT NULL
                          CHECK (archive_status IN ('pending', 'ready', 'missing', 'failed', 'deleted')),
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (
        (parent_artifact_id IS NULL AND derivative_type IS NULL)
        OR
        (parent_artifact_id IS NOT NULL AND length(trim(derivative_type)) > 0)
    ),
    CHECK (parent_artifact_id IS NULL OR parent_artifact_id <> id)
);

CREATE UNIQUE INDEX uq_artifacts_identity
    ON artifacts (sha256, kind, COALESCE(derivative_type, ''), processing_version);
CREATE INDEX idx_artifacts_parent ON artifacts(parent_artifact_id);
CREATE INDEX idx_artifacts_archive_status ON artifacts(archive_status, kind);
CREATE INDEX idx_artifacts_privacy ON artifacts(privacy_class, kind);

ALTER TABLE submissions
    ADD COLUMN artifact_id INTEGER REFERENCES artifacts(id) ON DELETE RESTRICT;
CREATE INDEX idx_submissions_artifact ON submissions(artifact_id);
