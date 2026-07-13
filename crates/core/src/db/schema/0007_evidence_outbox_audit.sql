-- 正式学习证据、可靠模块事件与不可变审计。
-- 业务事实与 outbox 必须由上层服务放在同一 SQLite transaction 中写入。

CREATE TABLE learning_evidence (
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
    student_id              INTEGER NOT NULL REFERENCES students(id) ON DELETE RESTRICT,
    source_module           TEXT NOT NULL
                              CHECK (source_module IN ('recitation', 'grading', 'correction', 'teacher_observation')),
    source_type             TEXT NOT NULL CHECK (length(trim(source_type)) > 0),
    source_ref_type         TEXT NOT NULL CHECK (length(trim(source_ref_type)) > 0),
    source_ref_id           TEXT NOT NULL CHECK (length(trim(source_ref_id)) > 0),
    source_revision         INTEGER NOT NULL CHECK (source_revision >= 1),
    decision_ref_type       TEXT,
    decision_ref_id         TEXT,
    decision_revision       INTEGER CHECK (decision_revision IS NULL OR decision_revision >= 1),
    knowledge_node_id       TEXT,
    ability_dimension_id    TEXT,
    evidence_kind           TEXT NOT NULL
                              CHECK (evidence_kind IN (
                                  'coverage', 'accuracy', 'contradiction',
                                  'fluency', 'retention', 'reasoning'
                              )),
    value                   REAL NOT NULL CHECK (value >= 0.0 AND value <= 1.0),
    confirmation_level      TEXT NOT NULL
                              CHECK (confirmation_level IN (
                                  'machine_only', 'teacher_overall',
                                  'teacher_accepted', 'teacher_corrected'
                              )),
    evidence_quality        REAL NOT NULL CHECK (evidence_quality >= 0.0 AND evidence_quality <= 1.0),
    assessment_context      TEXT NOT NULL
                              CHECK (assessment_context IN (
                                  'homework', 'in_class', 'closed_book', 'open_book', 'correction'
                              )),
    occurred_at             TEXT NOT NULL,
    rule_version            TEXT NOT NULL CHECK (length(trim(rule_version)) > 0),
    knowledge_map_version   TEXT NOT NULL CHECK (length(trim(knowledge_map_version)) > 0),
    state                   TEXT NOT NULL
                              CHECK (state IN ('active', 'reverted', 'superseded', 'voided')),
    created_at              TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (
        (decision_ref_type IS NULL AND decision_ref_id IS NULL AND decision_revision IS NULL)
        OR (
            length(trim(decision_ref_type)) > 0
            AND length(trim(decision_ref_id)) > 0
            AND decision_revision IS NOT NULL
        )
    ),
    CHECK (knowledge_node_id IS NULL OR length(trim(knowledge_node_id)) > 0),
    CHECK (ability_dimension_id IS NULL OR length(trim(ability_dimension_id)) > 0),
    CHECK (
        confirmation_level <> 'teacher_overall'
        OR (knowledge_node_id IS NULL AND ability_dimension_id IS NULL)
    ),
    CHECK (substr(occurred_at, 11, 1) = 'T' AND substr(occurred_at, -1, 1) = 'Z'),
    CHECK (substr(created_at, 11, 1) = 'T' AND substr(created_at, -1, 1) = 'Z')
);

CREATE INDEX idx_learning_evidence_student
    ON learning_evidence(student_id, state, occurred_at DESC, id DESC);
CREATE INDEX idx_learning_evidence_source
    ON learning_evidence(source_module, source_ref_type, source_ref_id, source_revision);
CREATE INDEX idx_learning_evidence_decision
    ON learning_evidence(decision_ref_type, decision_ref_id, decision_revision, state);
CREATE INDEX idx_learning_evidence_knowledge
    ON learning_evidence(knowledge_node_id, state, confirmation_level);
CREATE INDEX idx_learning_evidence_ability
    ON learning_evidence(ability_dimension_id, state, confirmation_level);

CREATE TABLE outbox_events (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    public_id           TEXT NOT NULL UNIQUE
                          CHECK (
                              length(public_id) = 36
                              AND substr(public_id, 9, 1) = '-'
                              AND substr(public_id, 14, 1) = '-'
                              AND substr(public_id, 19, 1) = '-'
                              AND substr(public_id, 24, 1) = '-'
                          ),
    idempotency_key     TEXT NOT NULL UNIQUE CHECK (length(trim(idempotency_key)) > 0),
    event_type          TEXT NOT NULL CHECK (length(trim(event_type)) > 0),
    event_version       INTEGER NOT NULL CHECK (event_version >= 1),
    aggregate_type      TEXT NOT NULL CHECK (length(trim(aggregate_type)) > 0),
    aggregate_id        TEXT NOT NULL CHECK (length(trim(aggregate_id)) > 0),
    aggregate_revision  INTEGER NOT NULL CHECK (aggregate_revision >= 1),
    payload_json        TEXT NOT NULL,
    occurred_at         TEXT NOT NULL,
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (
        CASE
            WHEN json_valid(payload_json) = 0 THEN 0
            ELSE COALESCE(json_type(payload_json, '$.schema_version') = 'integer', 0)
        END
    ),
    CHECK (substr(occurred_at, 11, 1) = 'T' AND substr(occurred_at, -1, 1) = 'Z'),
    CHECK (substr(created_at, 11, 1) = 'T' AND substr(created_at, -1, 1) = 'Z')
);

CREATE INDEX idx_outbox_events_aggregate
    ON outbox_events(aggregate_type, aggregate_id, aggregate_revision, id);
CREATE INDEX idx_outbox_events_type ON outbox_events(event_type, id);

CREATE TABLE outbox_consumptions (
    event_id            INTEGER NOT NULL REFERENCES outbox_events(id) ON DELETE RESTRICT,
    consumer            TEXT NOT NULL CHECK (length(trim(consumer)) > 0),
    status              TEXT NOT NULL
                          CHECK (status IN ('pending', 'processing', 'succeeded', 'failed')),
    attempts            INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts        INTEGER NOT NULL DEFAULT 1 CHECK (max_attempts >= 1),
    lease_token         TEXT,
    lease_expires_at    TEXT,
    processed_at        TEXT,
    error_meta_json     TEXT,
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY(event_id, consumer),
    CHECK (attempts <= max_attempts),
    CHECK (lease_token IS NULL OR length(trim(lease_token)) > 0),
    CHECK (
        (status = 'processing' AND lease_token IS NOT NULL AND lease_expires_at IS NOT NULL)
        OR (status IN ('pending', 'succeeded', 'failed') AND lease_token IS NULL AND lease_expires_at IS NULL)
    ),
    CHECK (status <> 'succeeded' OR processed_at IS NOT NULL),
    CHECK (status <> 'failed' OR (processed_at IS NOT NULL AND error_meta_json IS NOT NULL)),
    CHECK (
        CASE
            WHEN error_meta_json IS NULL THEN 1
            WHEN json_valid(error_meta_json) = 0 THEN 0
            ELSE COALESCE(json_type(error_meta_json, '$.schema_version') = 'integer', 0)
        END
    ),
    CHECK (lease_expires_at IS NULL OR (substr(lease_expires_at, 11, 1) = 'T' AND substr(lease_expires_at, -1, 1) = 'Z')),
    CHECK (processed_at IS NULL OR (substr(processed_at, 11, 1) = 'T' AND substr(processed_at, -1, 1) = 'Z')),
    CHECK (substr(created_at, 11, 1) = 'T' AND substr(created_at, -1, 1) = 'Z'),
    CHECK (substr(updated_at, 11, 1) = 'T' AND substr(updated_at, -1, 1) = 'Z')
);

CREATE INDEX idx_outbox_consumptions_claim
    ON outbox_consumptions(consumer, status, event_id);
CREATE INDEX idx_outbox_consumptions_lease
    ON outbox_consumptions(status, lease_expires_at);

CREATE TABLE audit_events (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    public_id           TEXT NOT NULL UNIQUE
                          CHECK (
                              length(public_id) = 36
                              AND substr(public_id, 9, 1) = '-'
                              AND substr(public_id, 14, 1) = '-'
                              AND substr(public_id, 19, 1) = '-'
                              AND substr(public_id, 24, 1) = '-'
                          ),
    idempotency_key     TEXT NOT NULL UNIQUE CHECK (length(trim(idempotency_key)) > 0),
    actor_type          TEXT NOT NULL CHECK (actor_type IN ('teacher', 'system', 'migration')),
    actor_id            TEXT,
    action              TEXT NOT NULL CHECK (length(trim(action)) > 0),
    object_type         TEXT NOT NULL CHECK (length(trim(object_type)) > 0),
    object_id           TEXT NOT NULL CHECK (length(trim(object_id)) > 0),
    object_revision     INTEGER CHECK (object_revision IS NULL OR object_revision >= 1),
    note                TEXT,
    meta_json           TEXT,
    occurred_at         TEXT NOT NULL,
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    CHECK (
        (actor_type = 'teacher' AND actor_id IS NOT NULL AND length(trim(actor_id)) > 0)
        OR (actor_type IN ('system', 'migration') AND actor_id IS NULL)
    ),
    CHECK (
        CASE
            WHEN meta_json IS NULL THEN 1
            WHEN json_valid(meta_json) = 0 THEN 0
            ELSE COALESCE(json_type(meta_json, '$.schema_version') = 'integer', 0)
        END
    ),
    CHECK (substr(occurred_at, 11, 1) = 'T' AND substr(occurred_at, -1, 1) = 'Z'),
    CHECK (substr(created_at, 11, 1) = 'T' AND substr(created_at, -1, 1) = 'Z')
);

CREATE INDEX idx_audit_events_object
    ON audit_events(object_type, object_id, object_revision, occurred_at DESC, id DESC);
CREATE INDEX idx_audit_events_actor
    ON audit_events(actor_type, actor_id, occurred_at DESC, id DESC);

-- DB 级不可变边界：业务仓储之外的误写也不能覆盖证据和审计。
CREATE TRIGGER artifacts_content_is_immutable
BEFORE UPDATE ON artifacts
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.kind IS NOT OLD.kind
  OR NEW.sha256 IS NOT OLD.sha256
  OR NEW.mime_type IS NOT OLD.mime_type
  OR NEW.byte_size IS NOT OLD.byte_size
  OR NEW.original_name IS NOT OLD.original_name
  OR NEW.original_path IS NOT OLD.original_path
  OR NEW.archived_path IS NOT OLD.archived_path
  OR NEW.parent_artifact_id IS NOT OLD.parent_artifact_id
  OR NEW.derivative_type IS NOT OLD.derivative_type
  OR NEW.processing_version IS NOT OLD.processing_version
  OR NEW.privacy_class IS NOT OLD.privacy_class
  OR NEW.created_at IS NOT OLD.created_at
BEGIN
    SELECT RAISE(ABORT, 'artifact content is immutable');
END;

CREATE TRIGGER artifacts_no_delete
BEFORE DELETE ON artifacts
BEGIN
    SELECT RAISE(ABORT, 'artifacts are retained for audit; use archive_status');
END;

CREATE TRIGGER learning_evidence_content_is_immutable
BEFORE UPDATE ON learning_evidence
WHEN NEW.public_id IS NOT OLD.public_id
  OR NEW.idempotency_key IS NOT OLD.idempotency_key
  OR NEW.student_id IS NOT OLD.student_id
  OR NEW.source_module IS NOT OLD.source_module
  OR NEW.source_type IS NOT OLD.source_type
  OR NEW.source_ref_type IS NOT OLD.source_ref_type
  OR NEW.source_ref_id IS NOT OLD.source_ref_id
  OR NEW.source_revision IS NOT OLD.source_revision
  OR NEW.decision_ref_type IS NOT OLD.decision_ref_type
  OR NEW.decision_ref_id IS NOT OLD.decision_ref_id
  OR NEW.decision_revision IS NOT OLD.decision_revision
  OR NEW.knowledge_node_id IS NOT OLD.knowledge_node_id
  OR NEW.ability_dimension_id IS NOT OLD.ability_dimension_id
  OR NEW.evidence_kind IS NOT OLD.evidence_kind
  OR NEW.value IS NOT OLD.value
  OR NEW.confirmation_level IS NOT OLD.confirmation_level
  OR NEW.evidence_quality IS NOT OLD.evidence_quality
  OR NEW.assessment_context IS NOT OLD.assessment_context
  OR NEW.occurred_at IS NOT OLD.occurred_at
  OR NEW.rule_version IS NOT OLD.rule_version
  OR NEW.knowledge_map_version IS NOT OLD.knowledge_map_version
  OR NEW.created_at IS NOT OLD.created_at
BEGIN
    SELECT RAISE(ABORT, 'learning evidence content is immutable');
END;

CREATE TRIGGER learning_evidence_no_delete
BEFORE DELETE ON learning_evidence
BEGIN
    SELECT RAISE(ABORT, 'learning evidence is retained for audit; change state instead');
END;

CREATE TRIGGER outbox_events_no_update
BEFORE UPDATE ON outbox_events
BEGIN
    SELECT RAISE(ABORT, 'outbox events are immutable');
END;

CREATE TRIGGER outbox_events_no_delete
BEFORE DELETE ON outbox_events
BEGIN
    SELECT RAISE(ABORT, 'outbox events are retained for audit');
END;

CREATE TRIGGER audit_events_no_update
BEFORE UPDATE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit events are immutable');
END;

CREATE TRIGGER audit_events_no_delete
BEFORE DELETE ON audit_events
BEGIN
    SELECT RAISE(ABORT, 'audit events are retained');
END;
