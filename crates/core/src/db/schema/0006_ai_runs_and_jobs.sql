-- Core AI 运行账本与可恢复后台任务。
-- 网络/文件处理不能持有 SQLite 锁；worker 只在 claim/finalize 短事务中修改这些表。

CREATE TABLE ai_runs (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    public_id               TEXT NOT NULL UNIQUE
                              CHECK (
                                  length(public_id) = 36
                                  AND substr(public_id, 9, 1) = '-'
                                  AND substr(public_id, 14, 1) = '-'
                                  AND substr(public_id, 19, 1) = '-'
                                  AND substr(public_id, 24, 1) = '-'
                              ),
    idempotency_key         TEXT NOT NULL UNIQUE CHECK (length(trim(idempotency_key)) > 0),
    run_type                TEXT NOT NULL CHECK (length(trim(run_type)) > 0),
    source_module           TEXT NOT NULL CHECK (length(trim(source_module)) > 0),
    business_ref_type       TEXT NOT NULL CHECK (length(trim(business_ref_type)) > 0),
    business_ref_id         TEXT NOT NULL CHECK (length(trim(business_ref_id)) > 0),
    input_artifact_id       INTEGER REFERENCES artifacts(id) ON DELETE RESTRICT,
    provider                TEXT NOT NULL CHECK (length(trim(provider)) > 0),
    model_name              TEXT NOT NULL CHECK (length(trim(model_name)) > 0),
    model_version           TEXT NOT NULL CHECK (length(trim(model_version)) > 0),
    config_version          TEXT NOT NULL CHECK (length(trim(config_version)) > 0),
    prompt_or_rule_version  TEXT NOT NULL CHECK (length(trim(prompt_or_rule_version)) > 0),
    input_hash              TEXT NOT NULL
                              CHECK (
                                  length(input_hash) = 64
                                  AND lower(input_hash) = input_hash
                                  AND input_hash NOT GLOB '*[^0-9a-f]*'
                              ),
    output_hash             TEXT
                              CHECK (
                                  output_hash IS NULL
                                  OR (
                                      length(output_hash) = 64
                                      AND lower(output_hash) = output_hash
                                      AND output_hash NOT GLOB '*[^0-9a-f]*'
                                  )
                              ),
    status                  TEXT NOT NULL
                              CHECK (status IN ('pending', 'processing', 'succeeded', 'failed', 'voided')),
    retry_of_ai_run_id      INTEGER REFERENCES ai_runs(id) ON DELETE RESTRICT,
    remote_run_id           TEXT,
    confidence              REAL CHECK (confidence IS NULL OR (confidence >= 0.0 AND confidence <= 1.0)),
    output_json             TEXT,
    error_meta_json         TEXT,
    started_at              TEXT,
    finished_at             TEXT,
    created_at              TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (retry_of_ai_run_id IS NULL OR retry_of_ai_run_id <> id),
    CHECK (started_at IS NULL OR (substr(started_at, 11, 1) = 'T' AND substr(started_at, -1, 1) = 'Z')),
    CHECK (finished_at IS NULL OR (substr(finished_at, 11, 1) = 'T' AND substr(finished_at, -1, 1) = 'Z')),
    CHECK (substr(created_at, 11, 1) = 'T' AND substr(created_at, -1, 1) = 'Z'),
    CHECK (
        CASE
            WHEN output_json IS NULL THEN 1
            WHEN json_valid(output_json) = 0 THEN 0
            ELSE COALESCE(json_type(output_json, '$.schema_version') = 'integer', 0)
        END
    ),
    CHECK (
        CASE
            WHEN error_meta_json IS NULL THEN 1
            WHEN json_valid(error_meta_json) = 0 THEN 0
            ELSE COALESCE(json_type(error_meta_json, '$.schema_version') = 'integer', 0)
        END
    ),
    CHECK (
        (status = 'pending' AND started_at IS NULL AND finished_at IS NULL)
        OR (status = 'processing' AND started_at IS NOT NULL AND finished_at IS NULL)
        OR (status IN ('succeeded', 'failed', 'voided') AND finished_at IS NOT NULL)
    ),
    CHECK (status <> 'succeeded' OR output_hash IS NOT NULL),
    CHECK (status <> 'failed' OR error_meta_json IS NOT NULL)
);

CREATE INDEX idx_ai_runs_business_ref
    ON ai_runs(source_module, business_ref_type, business_ref_id, id DESC);
CREATE INDEX idx_ai_runs_cache
    ON ai_runs(run_type, input_hash, provider, model_name, model_version,
               config_version, prompt_or_rule_version, status);
CREATE INDEX idx_ai_runs_retry ON ai_runs(retry_of_ai_run_id);
CREATE INDEX idx_ai_runs_artifact ON ai_runs(input_artifact_id);

CREATE TABLE background_jobs (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    public_id           TEXT NOT NULL UNIQUE
                          CHECK (
                              length(public_id) = 36
                              AND substr(public_id, 9, 1) = '-'
                              AND substr(public_id, 14, 1) = '-'
                              AND substr(public_id, 19, 1) = '-'
                              AND substr(public_id, 24, 1) = '-'
                          ),
    job_type            TEXT NOT NULL CHECK (length(trim(job_type)) > 0),
    business_key        TEXT NOT NULL UNIQUE CHECK (length(trim(business_key)) > 0),
    source_module       TEXT NOT NULL CHECK (length(trim(source_module)) > 0),
    business_ref_type   TEXT NOT NULL CHECK (length(trim(business_ref_type)) > 0),
    business_ref_id     TEXT NOT NULL CHECK (length(trim(business_ref_id)) > 0),
    ai_run_id           INTEGER UNIQUE REFERENCES ai_runs(id) ON DELETE RESTRICT,
    stage               TEXT NOT NULL CHECK (length(trim(stage)) > 0),
    status              TEXT NOT NULL
                          CHECK (status IN ('queued', 'claimed', 'processing', 'succeeded', 'failed', 'cancelled')),
    attempts            INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts        INTEGER NOT NULL DEFAULT 1 CHECK (max_attempts >= 1),
    lease_token         TEXT,
    lease_expires_at    TEXT,
    next_retry_at       TEXT,
    progress_json       TEXT,
    error_meta_json     TEXT,
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (attempts <= max_attempts),
    CHECK (lease_token IS NULL OR length(trim(lease_token)) > 0),
    CHECK (lease_expires_at IS NULL OR (substr(lease_expires_at, 11, 1) = 'T' AND substr(lease_expires_at, -1, 1) = 'Z')),
    CHECK (next_retry_at IS NULL OR (substr(next_retry_at, 11, 1) = 'T' AND substr(next_retry_at, -1, 1) = 'Z')),
    CHECK (substr(created_at, 11, 1) = 'T' AND substr(created_at, -1, 1) = 'Z'),
    CHECK (substr(updated_at, 11, 1) = 'T' AND substr(updated_at, -1, 1) = 'Z'),
    CHECK (
        CASE
            WHEN progress_json IS NULL THEN 1
            WHEN json_valid(progress_json) = 0 THEN 0
            ELSE COALESCE(json_type(progress_json, '$.schema_version') = 'integer', 0)
        END
    ),
    CHECK (
        CASE
            WHEN error_meta_json IS NULL THEN 1
            WHEN json_valid(error_meta_json) = 0 THEN 0
            ELSE COALESCE(json_type(error_meta_json, '$.schema_version') = 'integer', 0)
        END
    ),
    CHECK (
        (status IN ('claimed', 'processing') AND lease_token IS NOT NULL AND lease_expires_at IS NOT NULL)
        OR (status IN ('queued', 'succeeded', 'failed', 'cancelled') AND lease_token IS NULL AND lease_expires_at IS NULL)
    ),
    CHECK (status <> 'failed' OR error_meta_json IS NOT NULL)
);

CREATE INDEX idx_background_jobs_claim
    ON background_jobs(job_type, status, next_retry_at, created_at, id);
CREATE INDEX idx_background_jobs_business_ref
    ON background_jobs(source_module, business_ref_type, business_ref_id, id DESC);
CREATE INDEX idx_background_jobs_lease
    ON background_jobs(status, lease_expires_at);
