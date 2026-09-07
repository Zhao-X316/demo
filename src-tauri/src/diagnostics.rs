//! 默认脱敏的本地诊断包。
//!
//! 默认包只包含版本、schema、状态计数、内部 ID、脱敏错误码和一致性检查；
//! 姓名、绝对路径、音频、ASR、答案正文和凭据不会被读取进默认报告。
//! 如老师确需内容证据，必须先预览精确清单，再携带预览令牌二次确认。

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::audit;
use suite_core::domain::hashing::{sha256_file, sha256_hex};
use suite_core::models::AuditActorType;
use suite_core::ports::Migration;
use tauri::State;
use zip::write::FileOptions;
use zip::ZipWriter;

use crate::backup;
use crate::state::AppState;

const DIAGNOSTIC_SCHEMA_VERSION: i64 = 1;
const SENSITIVE_PREVIEW_LIMIT: i64 = 20;
const UNKNOWN_ERROR_CODE: &str = "UNKNOWN";
const REDACTED_LABEL: &str = "REDACTED";

type R<T> = Result<T, String>;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticSensitiveOptions {
    pub include_failed_audio: bool,
    pub include_failed_asr: bool,
    pub include_standard_answers: bool,
}

impl DiagnosticSensitiveOptions {
    fn any(&self) -> bool {
        self.include_failed_audio || self.include_failed_asr || self.include_standard_answers
    }

    fn selected_categories(&self) -> Vec<String> {
        let mut categories = Vec::new();
        if self.include_failed_audio {
            categories.push("failed_audio".to_string());
        }
        if self.include_failed_asr {
            categories.push("failed_asr".to_string());
        }
        if self.include_standard_answers {
            categories.push("standard_answers".to_string());
        }
        categories
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticPreviewRequest {
    #[serde(default)]
    pub sensitive: DiagnosticSensitiveOptions,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticExportRequest {
    pub output_path: String,
    pub request_key: String,
    #[serde(default)]
    pub sensitive: DiagnosticSensitiveOptions,
    pub preview_token: Option<String>,
    #[serde(default)]
    pub confirm_sensitive_evidence: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticPreviewItem {
    pub category: String,
    pub internal_ref: String,
    pub archive_name: String,
    pub size_bytes: u64,
    pub available: bool,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticPreview {
    pub schema_version: i64,
    pub sensitive_content_selected: bool,
    pub selected_categories: Vec<String>,
    pub warning: Option<String>,
    pub preview_token: Option<String>,
    pub items: Vec<DiagnosticPreviewItem>,
    pub available_item_count: usize,
    pub unavailable_item_count: usize,
    pub total_available_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticExportResult {
    pub schema_version: i64,
    pub bundle_id: String,
    pub file_name: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub generated_at: String,
    pub sensitive_content_included: bool,
    pub included_sensitive_items: usize,
    pub audit_event_public_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct PrivacyReport {
    default_safe: bool,
    sensitive_content_included: bool,
    selected_sensitive_categories: Vec<String>,
    excluded_by_default: Vec<&'static str>,
    absolute_paths_in_report: bool,
    credentials_in_report: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ApplicationReport {
    app_version: &'static str,
    target_os: &'static str,
    target_arch: &'static str,
    target_family: &'static str,
    build_profile: &'static str,
}

#[derive(Debug, Clone, Serialize)]
struct MigrationReport {
    expected_count: usize,
    applied_count: usize,
    missing_ids: Vec<String>,
    unexpected_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct IntegrityReport {
    integrity_status: String,
    integrity_issue_count: i64,
    foreign_key_violation_count: i64,
}

#[derive(Debug, Clone, Serialize)]
struct BackupReport {
    catalog_status: String,
    total_count: usize,
    by_kind: BTreeMap<String, usize>,
    latest_kind: Option<String>,
    latest_created_at: Option<String>,
    latest_size_bytes: Option<u64>,
    last_restore_status: String,
    last_restore_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DatabaseReport {
    migrations: MigrationReport,
    integrity: IntegrityReport,
    backups: BackupReport,
}

#[derive(Debug, Clone, Serialize)]
struct StatusCount {
    category: String,
    state: String,
    count: i64,
}

#[derive(Debug, Clone, Serialize)]
struct AiVersionCount {
    run_type: String,
    source_module: String,
    provider: String,
    model_name: String,
    model_version: String,
    config_version: String,
    prompt_or_rule_version: String,
    state: String,
    count: i64,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeSummary {
    ai_runs: Vec<StatusCount>,
    background_jobs: Vec<StatusCount>,
    decision_effects: Vec<StatusCount>,
    learning_evidence: Vec<StatusCount>,
    outbox_consumptions: Vec<StatusCount>,
    ai_versions: Vec<AiVersionCount>,
}

#[derive(Debug, Clone, Serialize)]
struct SafeFailureEvent {
    source: String,
    internal_id: String,
    state: String,
    error_code: String,
    retryable: Option<bool>,
    attempts: Option<i64>,
    occurred_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct IdempotencyCheck {
    check: String,
    status: String,
    duplicate_groups: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct DiagnosticReport {
    schema_version: i64,
    bundle_id: String,
    generated_at: String,
    privacy: PrivacyReport,
    application: ApplicationReport,
    database: DatabaseReport,
    runtime_summary: RuntimeSummary,
    recent_failures: Vec<SafeFailureEvent>,
    idempotency_checks: Vec<IdempotencyCheck>,
}

#[derive(Debug, Clone, Serialize)]
struct BundleManifestEntry {
    archive_name: String,
    sha256: String,
    size_bytes: u64,
    sensitive: bool,
}

#[derive(Debug, Clone, Serialize)]
struct BundleManifest {
    schema_version: i64,
    bundle_id: String,
    generated_at: String,
    diagnostic_report_sha256: String,
    sensitive_content_included: bool,
    selected_sensitive_categories: Vec<String>,
    export_audit_policy: &'static str,
    entries: Vec<BundleManifestEntry>,
}

#[derive(Debug)]
enum AttachmentSource {
    File(PathBuf),
    Bytes(Vec<u8>),
}

#[derive(Debug)]
struct SensitiveAttachment {
    preview: DiagnosticPreviewItem,
    sha256: String,
    source: AttachmentSource,
}

#[derive(Debug, Serialize)]
struct PreviewIdentity<'a> {
    schema_version: i64,
    options: &'a DiagnosticSensitiveOptions,
    items: Vec<PreviewIdentityItem<'a>>,
}

#[derive(Debug, Serialize)]
struct PreviewIdentityItem<'a> {
    category: &'a str,
    internal_ref: &'a str,
    archive_name: &'a str,
    size_bytes: u64,
    available: bool,
    sha256: &'a str,
}

struct PreparedBundle {
    target: PathBuf,
    request_key: String,
    generated_at: String,
    bundle_id: String,
    diagnostics_json: Vec<u8>,
    manifest_json: Vec<u8>,
    attachments: Vec<SensitiveAttachment>,
    sensitive_categories: Vec<String>,
}

fn safe_label(value: &str) -> String {
    let trimmed = value.trim();
    let allowed = !trimmed.is_empty()
        && trimmed.len() <= 96
        && trimmed.chars().all(|ch| {
            ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-' | '@' | '+' | ' ')
        });
    if allowed {
        trimmed.to_string()
    } else {
        REDACTED_LABEL.to_string()
    }
}

fn safe_timestamp(value: Option<String>) -> Option<String> {
    value.map(|raw| {
        let trimmed = raw.trim();
        if !trimmed.is_empty()
            && trimmed.len() <= 48
            && trimmed.chars().all(|ch| {
                ch.is_ascii_digit() || matches!(ch, 'T' | 'Z' | '+' | '-' | ':' | '.' | ' ')
            })
        {
            trimmed.to_string()
        } else {
            REDACTED_LABEL.to_string()
        }
    })
}

fn safe_error_code(meta: Option<&str>) -> (String, Option<bool>) {
    let parsed = meta.and_then(|raw| serde_json::from_str::<Value>(raw).ok());
    let code = parsed
        .as_ref()
        .and_then(|value| {
            value
                .get("error_code")
                .or_else(|| value.get("code"))
                .and_then(Value::as_str)
        })
        .map(safe_label)
        .unwrap_or_else(|| UNKNOWN_ERROR_CODE.to_string());
    let retryable = parsed
        .as_ref()
        .and_then(|value| value.get("retryable"))
        .and_then(Value::as_bool);
    (code, retryable)
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1
         )",
        [table],
        |row| row.get::<_, bool>(0),
    )
    .unwrap_or(false)
}

fn all_expected_migrations() -> Vec<&'static Migration> {
    suite_core::db::CORE_MIGRATIONS
        .iter()
        .chain(module_knowledge::knowledge_migrations())
        .chain(module_recitation::recitation_migrations())
        .chain(module_exam::exam_migrations())
        .chain(module_wrongbook::wrongbook_migrations())
        .chain(module_profile::profile_migrations())
        .collect()
}

fn migration_report(conn: &Connection) -> MigrationReport {
    let expected = all_expected_migrations();
    let expected_ids: BTreeSet<String> = expected
        .iter()
        .map(|migration| migration.id.to_string())
        .collect();
    let mut applied_ids = BTreeSet::new();
    if table_exists(conn, "schema_migrations") {
        if let Ok(mut stmt) = conn.prepare("SELECT id FROM schema_migrations ORDER BY id") {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                for row in rows.flatten() {
                    applied_ids.insert(row);
                }
            }
        }
    }
    MigrationReport {
        expected_count: expected_ids.len(),
        applied_count: applied_ids.len(),
        missing_ids: expected_ids.difference(&applied_ids).cloned().collect(),
        unexpected_ids: applied_ids.difference(&expected_ids).cloned().collect(),
    }
}

fn integrity_report(conn: &Connection) -> IntegrityReport {
    let integrity_rows = (|| -> rusqlite::Result<Vec<String>> {
        let mut stmt = conn.prepare("PRAGMA integrity_check")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect()
    })();
    let (integrity_status, integrity_issue_count) = match integrity_rows {
        Ok(rows) if rows.len() == 1 && rows[0] == "ok" => ("ok".to_string(), 0),
        Ok(rows) => ("failed".to_string(), rows.len() as i64),
        Err(_) => ("unavailable".to_string(), -1),
    };
    let foreign_key_violation_count = (|| -> rusqlite::Result<i64> {
        let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
        let mut rows = stmt.query([])?;
        let mut count = 0;
        while rows.next()?.is_some() {
            count += 1;
        }
        Ok(count)
    })()
    .unwrap_or(-1);
    IntegrityReport {
        integrity_status,
        integrity_issue_count,
        foreign_key_violation_count,
    }
}

fn backup_report(conn: &Connection, data_dir: &Path) -> BackupReport {
    let catalog = backup::list_backups(&data_dir.join("backups"));
    let (catalog_status, total_count, by_kind, latest_kind, latest_created_at, latest_size_bytes) =
        match catalog {
            Ok(catalog) => {
                let mut by_kind = BTreeMap::new();
                for item in &catalog.items {
                    *by_kind.entry(safe_label(&item.kind)).or_insert(0) += 1;
                }
                let latest = catalog.items.first();
                (
                    "ok".to_string(),
                    catalog.items.len(),
                    by_kind,
                    latest.map(|item| safe_label(&item.kind)),
                    latest.and_then(|item| safe_timestamp(Some(item.created_at.clone()))),
                    latest.map(|item| item.size_bytes),
                )
            }
            Err(_) => (
                "unavailable".to_string(),
                0,
                BTreeMap::new(),
                None,
                None,
                None,
            ),
        };
    let last_restore_at = if table_exists(conn, "audit_events") {
        conn.query_row(
            "SELECT occurred_at FROM audit_events
             WHERE action='backup.restore.completed'
             ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
        .and_then(|value| safe_timestamp(Some(value)))
    } else {
        None
    };
    BackupReport {
        catalog_status,
        total_count,
        by_kind,
        latest_kind,
        latest_created_at,
        latest_size_bytes,
        last_restore_status: if last_restore_at.is_some() {
            "completed".to_string()
        } else {
            "not_recorded".to_string()
        },
        last_restore_at,
    }
}

fn query_status_counts(
    conn: &Connection,
    table: &str,
    category: &str,
    state_column: &str,
) -> Vec<StatusCount> {
    if !table_exists(conn, table) {
        return Vec::new();
    }
    let sql = format!(
        "SELECT {state_column}, COUNT(*) FROM {table}
         GROUP BY {state_column} ORDER BY {state_column}"
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok(StatusCount {
            category: category.to_string(),
            state: safe_label(&row.get::<_, String>(0)?),
            count: row.get(1)?,
        })
    }) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

fn evidence_counts(conn: &Connection) -> Vec<StatusCount> {
    if !table_exists(conn, "learning_evidence") {
        return Vec::new();
    }
    let Ok(mut stmt) = conn.prepare(
        "SELECT state, evidence_kind, confirmation_level, COUNT(*)
         FROM learning_evidence
         GROUP BY state, evidence_kind, confirmation_level
         ORDER BY state, evidence_kind, confirmation_level",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |row| {
        let state = safe_label(&row.get::<_, String>(0)?);
        let kind = safe_label(&row.get::<_, String>(1)?);
        let confirmation = safe_label(&row.get::<_, String>(2)?);
        Ok(StatusCount {
            category: format!("learning_evidence:{kind}:{confirmation}"),
            state,
            count: row.get(3)?,
        })
    }) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

fn ai_version_counts(conn: &Connection) -> Vec<AiVersionCount> {
    if !table_exists(conn, "ai_runs") {
        return Vec::new();
    }
    let Ok(mut stmt) = conn.prepare(
        "SELECT run_type, source_module, provider, model_name, model_version,
                config_version, prompt_or_rule_version, status, COUNT(*)
         FROM ai_runs
         GROUP BY run_type, source_module, provider, model_name, model_version,
                  config_version, prompt_or_rule_version, status
         ORDER BY source_module, run_type, provider, model_name, status",
    ) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok(AiVersionCount {
            run_type: safe_label(&row.get::<_, String>(0)?),
            source_module: safe_label(&row.get::<_, String>(1)?),
            provider: safe_label(&row.get::<_, String>(2)?),
            model_name: safe_label(&row.get::<_, String>(3)?),
            model_version: safe_label(&row.get::<_, String>(4)?),
            config_version: safe_label(&row.get::<_, String>(5)?),
            prompt_or_rule_version: safe_label(&row.get::<_, String>(6)?),
            state: safe_label(&row.get::<_, String>(7)?),
            count: row.get(8)?,
        })
    }) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

fn recent_failure_events(conn: &Connection) -> Vec<SafeFailureEvent> {
    let mut events = Vec::new();
    if table_exists(conn, "submissions") {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, recognize_status, recognize_meta, created_at
             FROM submissions
             WHERE recognize_status='failed'
             ORDER BY id DESC LIMIT 20",
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            }) {
                for row in rows.flatten() {
                    let (error_code, retryable) = safe_error_code(row.2.as_deref());
                    let attempts = row
                        .2
                        .as_deref()
                        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                        .and_then(|value| value.get("attempts").and_then(Value::as_i64));
                    events.push(SafeFailureEvent {
                        source: "recitation_submission".to_string(),
                        internal_id: row.0.to_string(),
                        state: safe_label(&row.1),
                        error_code,
                        retryable,
                        attempts,
                        occurred_at: safe_timestamp(row.3),
                    });
                }
            }
        }
    }
    for (table, source, state_column, meta_column, attempts_column, time_column) in [
        (
            "ai_runs",
            "ai_run",
            "status",
            "error_meta_json",
            None,
            "finished_at",
        ),
        (
            "background_jobs",
            "background_job",
            "status",
            "error_meta_json",
            Some("attempts"),
            "updated_at",
        ),
    ] {
        if !table_exists(conn, table) {
            continue;
        }
        let attempts_expr = attempts_column.unwrap_or("NULL");
        let sql = format!(
            "SELECT id, {state_column}, {meta_column}, {attempts_expr}, {time_column}
             FROM {table}
             WHERE {state_column}='failed'
             ORDER BY id DESC LIMIT 20"
        );
        let Ok(mut stmt) = conn.prepare(&sql) else {
            continue;
        };
        let Ok(rows) = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        }) else {
            continue;
        };
        for row in rows.flatten() {
            let (error_code, retryable) = safe_error_code(row.2.as_deref());
            events.push(SafeFailureEvent {
                source: source.to_string(),
                internal_id: row.0.to_string(),
                state: safe_label(&row.1),
                error_code,
                retryable,
                attempts: row.3,
                occurred_at: safe_timestamp(row.4),
            });
        }
    }
    if table_exists(conn, "outbox_consumptions") {
        if let Ok(mut stmt) = conn.prepare(
            "SELECT event_id, consumer, status, error_meta_json, attempts, processed_at
             FROM outbox_consumptions
             WHERE status='failed'
             ORDER BY event_id DESC LIMIT 20",
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            }) {
                for row in rows.flatten() {
                    let (error_code, retryable) = safe_error_code(row.3.as_deref());
                    events.push(SafeFailureEvent {
                        source: format!("outbox:{}", safe_label(&row.1)),
                        internal_id: row.0.to_string(),
                        state: safe_label(&row.2),
                        error_code,
                        retryable,
                        attempts: Some(row.4),
                        occurred_at: safe_timestamp(row.5),
                    });
                }
            }
        }
    }
    events.sort_by(|left, right| {
        right
            .occurred_at
            .cmp(&left.occurred_at)
            .then_with(|| right.internal_id.cmp(&left.internal_id))
    });
    events.truncate(50);
    events
}

fn duplicate_check(conn: &Connection, name: &str, sql: &str) -> IdempotencyCheck {
    match conn.query_row(sql, [], |row| row.get::<_, i64>(0)) {
        Ok(duplicate_groups) => IdempotencyCheck {
            check: name.to_string(),
            status: if duplicate_groups == 0 {
                "ok".to_string()
            } else {
                "issue".to_string()
            },
            duplicate_groups: Some(duplicate_groups),
        },
        Err(_) => IdempotencyCheck {
            check: name.to_string(),
            status: "unavailable".to_string(),
            duplicate_groups: None,
        },
    }
}

fn idempotency_checks(conn: &Connection) -> Vec<IdempotencyCheck> {
    vec![
        duplicate_check(
            conn,
            "ai_run_idempotency_key",
            "SELECT COUNT(*) FROM (
                 SELECT idempotency_key FROM ai_runs
                 GROUP BY idempotency_key HAVING COUNT(*) > 1
             )",
        ),
        duplicate_check(
            conn,
            "learning_evidence_idempotency_key",
            "SELECT COUNT(*) FROM (
                 SELECT idempotency_key FROM learning_evidence
                 GROUP BY idempotency_key HAVING COUNT(*) > 1
             )",
        ),
        duplicate_check(
            conn,
            "outbox_idempotency_key",
            "SELECT COUNT(*) FROM (
                 SELECT idempotency_key FROM outbox_events
                 GROUP BY idempotency_key HAVING COUNT(*) > 1
             )",
        ),
        duplicate_check(
            conn,
            "audit_idempotency_key",
            "SELECT COUNT(*) FROM (
                 SELECT idempotency_key FROM audit_events
                 GROUP BY idempotency_key HAVING COUNT(*) > 1
             )",
        ),
        duplicate_check(
            conn,
            "active_effect_per_verdict",
            "SELECT COUNT(*) FROM (
                 SELECT verdict_id FROM decision_effects
                 WHERE state='active'
                 GROUP BY verdict_id HAVING COUNT(*) > 1
             )",
        ),
        duplicate_check(
            conn,
            "active_score_per_submission",
            "SELECT COUNT(*) FROM (
                 SELECT submission_id FROM rec_score_runs
                 WHERE state='active'
                 GROUP BY submission_id HAVING COUNT(*) > 1
             )",
        ),
    ]
}

fn safe_extension(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("wav") => "wav",
        Some("mp3") => "mp3",
        Some("m4a") => "m4a",
        Some("aac") => "aac",
        Some("ogg") => "ogg",
        Some("flac") => "flac",
        Some("mp4") => "mp4",
        _ => "bin",
    }
}

fn push_unavailable(
    attachments: &mut Vec<SensitiveAttachment>,
    category: &str,
    internal_ref: String,
    archive_name: String,
    reason_code: &str,
) {
    attachments.push(SensitiveAttachment {
        preview: DiagnosticPreviewItem {
            category: category.to_string(),
            internal_ref,
            archive_name,
            size_bytes: 0,
            available: false,
            reason_code: Some(reason_code.to_string()),
        },
        sha256: String::new(),
        source: AttachmentSource::Bytes(Vec::new()),
    });
}

fn sensitive_attachments(
    conn: &Connection,
    options: &DiagnosticSensitiveOptions,
) -> R<Vec<SensitiveAttachment>> {
    if !options.any() {
        return Ok(Vec::new());
    }
    if !table_exists(conn, "submissions") || !table_exists(conn, "rec_contents") {
        return Err("诊断内容证据所需表不存在，无法生成精确预览".to_string());
    }
    let mut stmt = conn
        .prepare(
            "SELECT submission.id, submission.file_path, submission.archived_path,
                    submission.recognized_text, submission.ref_id, content.answer_text
             FROM submissions submission
             LEFT JOIN rec_contents content ON content.id=submission.ref_id
             WHERE submission.module='recitation'
               AND (
                   submission.recognize_status='failed'
                   OR submission.status='anomaly'
                   OR EXISTS (
                       SELECT 1 FROM ai_runs run
                       WHERE run.source_module='recitation'
                         AND run.business_ref_type='submission'
                         AND run.business_ref_id=CAST(submission.id AS TEXT)
                         AND run.status='failed'
                   )
               )
             ORDER BY submission.id DESC
             LIMIT ?1",
        )
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([SENSITIVE_PREVIEW_LIMIT], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(|err| err.to_string())?;
    let mut attachments = Vec::new();
    let mut included_answers = HashSet::new();
    for row in rows {
        let (submission_id, file_path, archived_path, recognized_text, ref_id, answer_text) =
            row.map_err(|err| err.to_string())?;
        if options.include_failed_audio {
            let preferred = archived_path
                .as_deref()
                .map(Path::new)
                .filter(|path| path.is_file())
                .or_else(|| {
                    let path = Path::new(&file_path);
                    path.is_file().then_some(path)
                });
            match preferred {
                Some(path) => {
                    let size_bytes = path.metadata().map_err(|err| err.to_string())?.len();
                    let digest = sha256_file(path).map_err(|err| err.to_string())?;
                    let archive_name = format!(
                        "sensitive/audio/submission-{submission_id}.{}",
                        safe_extension(path)
                    );
                    attachments.push(SensitiveAttachment {
                        preview: DiagnosticPreviewItem {
                            category: "failed_audio".to_string(),
                            internal_ref: format!("submission:{submission_id}"),
                            archive_name,
                            size_bytes,
                            available: true,
                            reason_code: None,
                        },
                        sha256: digest,
                        source: AttachmentSource::File(path.to_path_buf()),
                    });
                }
                None => push_unavailable(
                    &mut attachments,
                    "failed_audio",
                    format!("submission:{submission_id}"),
                    format!("sensitive/audio/submission-{submission_id}.bin"),
                    "AUDIO_NOT_AVAILABLE",
                ),
            }
        }
        if options.include_failed_asr {
            match recognized_text.filter(|text| !text.is_empty()) {
                Some(text) => {
                    let bytes = text.into_bytes();
                    attachments.push(SensitiveAttachment {
                        preview: DiagnosticPreviewItem {
                            category: "failed_asr".to_string(),
                            internal_ref: format!("submission:{submission_id}"),
                            archive_name: format!("sensitive/asr/submission-{submission_id}.txt"),
                            size_bytes: bytes.len() as u64,
                            available: true,
                            reason_code: None,
                        },
                        sha256: sha256_hex(&bytes),
                        source: AttachmentSource::Bytes(bytes),
                    });
                }
                None => push_unavailable(
                    &mut attachments,
                    "failed_asr",
                    format!("submission:{submission_id}"),
                    format!("sensitive/asr/submission-{submission_id}.txt"),
                    "ASR_NOT_AVAILABLE",
                ),
            }
        }
        if options.include_standard_answers {
            match (ref_id, answer_text) {
                (Some(content_id), Some(answer)) if included_answers.insert(content_id) => {
                    let bytes = answer.into_bytes();
                    attachments.push(SensitiveAttachment {
                        preview: DiagnosticPreviewItem {
                            category: "standard_answers".to_string(),
                            internal_ref: format!("content:{content_id}"),
                            archive_name: format!("sensitive/answers/content-{content_id}.txt"),
                            size_bytes: bytes.len() as u64,
                            available: true,
                            reason_code: None,
                        },
                        sha256: sha256_hex(&bytes),
                        source: AttachmentSource::Bytes(bytes),
                    });
                }
                (Some(content_id), None) if included_answers.insert(content_id) => {
                    push_unavailable(
                        &mut attachments,
                        "standard_answers",
                        format!("content:{content_id}"),
                        format!("sensitive/answers/content-{content_id}.txt"),
                        "ANSWER_NOT_AVAILABLE",
                    );
                }
                (None, _) => push_unavailable(
                    &mut attachments,
                    "standard_answers",
                    format!("submission:{submission_id}"),
                    format!("sensitive/answers/submission-{submission_id}.txt"),
                    "CONTENT_NOT_LINKED",
                ),
                _ => {}
            }
        }
    }
    Ok(attachments)
}

fn preview_token(
    options: &DiagnosticSensitiveOptions,
    attachments: &[SensitiveAttachment],
) -> R<String> {
    let identity = PreviewIdentity {
        schema_version: DIAGNOSTIC_SCHEMA_VERSION,
        options,
        items: attachments
            .iter()
            .map(|attachment| PreviewIdentityItem {
                category: &attachment.preview.category,
                internal_ref: &attachment.preview.internal_ref,
                archive_name: &attachment.preview.archive_name,
                size_bytes: attachment.preview.size_bytes,
                available: attachment.preview.available,
                sha256: &attachment.sha256,
            })
            .collect(),
    };
    serde_json::to_vec(&identity)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|err| err.to_string())
}

fn build_preview(
    conn: &Connection,
    options: &DiagnosticSensitiveOptions,
) -> R<(DiagnosticPreview, Vec<SensitiveAttachment>)> {
    let attachments = sensitive_attachments(conn, options)?;
    let available_item_count = attachments
        .iter()
        .filter(|item| item.preview.available)
        .count();
    let unavailable_item_count = attachments.len() - available_item_count;
    let total_available_bytes = attachments
        .iter()
        .filter(|item| item.preview.available)
        .map(|item| item.preview.size_bytes)
        .sum();
    let token = if options.any() {
        Some(preview_token(options, &attachments)?)
    } else {
        None
    };
    let preview = DiagnosticPreview {
        schema_version: DIAGNOSTIC_SCHEMA_VERSION,
        sensitive_content_selected: options.any(),
        selected_categories: options.selected_categories(),
        warning: options.any().then(|| {
            "所选内容可能包含学生声音、背诵正文或标准答案；请逐项核对后再确认导出。".to_string()
        }),
        preview_token: token,
        items: attachments
            .iter()
            .map(|item| item.preview.clone())
            .collect(),
        available_item_count,
        unavailable_item_count,
        total_available_bytes,
    };
    Ok((preview, attachments))
}

pub fn preview(conn: &Connection, request: &DiagnosticPreviewRequest) -> R<DiagnosticPreview> {
    build_preview(conn, &request.sensitive).map(|(preview, _)| preview)
}

fn build_runtime_summary(conn: &Connection) -> RuntimeSummary {
    RuntimeSummary {
        ai_runs: query_status_counts(conn, "ai_runs", "ai_run", "status"),
        background_jobs: query_status_counts(conn, "background_jobs", "background_job", "status"),
        decision_effects: query_status_counts(conn, "decision_effects", "decision_effect", "state"),
        learning_evidence: evidence_counts(conn),
        outbox_consumptions: query_status_counts(
            conn,
            "outbox_consumptions",
            "outbox_consumption",
            "status",
        ),
        ai_versions: ai_version_counts(conn),
    }
}

fn validate_request(request: &DiagnosticExportRequest) -> R<PathBuf> {
    let target = PathBuf::from(request.output_path.trim());
    if !target.is_absolute() {
        return Err("诊断包必须保存到老师明确选择的绝对路径".to_string());
    }
    if target.extension().and_then(|value| value.to_str()) != Some("zip") {
        return Err("诊断包文件名必须以 .zip 结尾".to_string());
    }
    if target.exists() {
        return Err("目标诊断包已存在，系统不会静默覆盖".to_string());
    }
    let request_key = request.request_key.trim();
    if request_key.len() < 8
        || request_key.len() > 128
        || !request_key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ':' | '-' | '_' | '.'))
    {
        return Err("诊断包 request_key 格式无效".to_string());
    }
    Ok(target)
}

fn build_report(
    conn: &Connection,
    data_dir: &Path,
    generated_at: &str,
    bundle_id: &str,
    sensitive_options: &DiagnosticSensitiveOptions,
    sensitive_item_count: usize,
) -> DiagnosticReport {
    DiagnosticReport {
        schema_version: DIAGNOSTIC_SCHEMA_VERSION,
        bundle_id: bundle_id.to_string(),
        generated_at: generated_at.to_string(),
        privacy: PrivacyReport {
            default_safe: !sensitive_options.any(),
            sensitive_content_included: sensitive_item_count > 0,
            selected_sensitive_categories: sensitive_options.selected_categories(),
            excluded_by_default: vec![
                "api_keys_and_credentials",
                "student_names_and_numbers",
                "absolute_file_paths",
                "raw_audio",
                "full_asr",
                "student_answer_text",
                "standard_answer_text",
            ],
            absolute_paths_in_report: false,
            credentials_in_report: false,
        },
        application: ApplicationReport {
            app_version: env!("CARGO_PKG_VERSION"),
            target_os: std::env::consts::OS,
            target_arch: std::env::consts::ARCH,
            target_family: std::env::consts::FAMILY,
            build_profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        },
        database: DatabaseReport {
            migrations: migration_report(conn),
            integrity: integrity_report(conn),
            backups: backup_report(conn, data_dir),
        },
        runtime_summary: build_runtime_summary(conn),
        recent_failures: recent_failure_events(conn),
        idempotency_checks: idempotency_checks(conn),
    }
}

fn prepare_bundle(
    conn: &Connection,
    data_dir: &Path,
    request: &DiagnosticExportRequest,
    generated_at: &str,
) -> R<PreparedBundle> {
    let target = validate_request(request)?;
    let audit_key = format!("diagnostic.export:{}", request.request_key.trim());
    if table_exists(conn, "audit_events")
        && audit::get_by_key(conn, &audit_key)
            .map_err(|err| err.to_string())?
            .is_some()
    {
        return Err("该诊断导出请求已经完成，系统拒绝重复写出".to_string());
    }
    let (preview, attachments) = build_preview(conn, &request.sensitive)?;
    if request.sensitive.any() {
        if !request.confirm_sensitive_evidence {
            return Err("包含内容证据时必须二次明确确认".to_string());
        }
        if request.preview_token.as_deref() != preview.preview_token.as_deref() {
            return Err("内容证据清单已变化，请重新预览后再确认".to_string());
        }
    } else if request.confirm_sensitive_evidence || request.preview_token.is_some() {
        return Err("默认脱敏导出不得携带敏感确认参数".to_string());
    }

    let available_attachments: Vec<SensitiveAttachment> = attachments
        .into_iter()
        .filter(|attachment| attachment.preview.available)
        .collect();
    let seed = serde_json::json!({
        "schema_version": DIAGNOSTIC_SCHEMA_VERSION,
        "generated_at": generated_at,
        "migration": migration_report(conn),
        "sensitive_categories": request.sensitive.selected_categories(),
        "sensitive_identity": available_attachments.iter().map(|attachment| serde_json::json!({
            "archive_name": attachment.preview.archive_name,
            "sha256": attachment.sha256,
            "size_bytes": attachment.preview.size_bytes,
        })).collect::<Vec<_>>(),
    });
    let seed_bytes = serde_json::to_vec(&seed).map_err(|err| err.to_string())?;
    let bundle_id = sha256_hex(&seed_bytes);
    let report = build_report(
        conn,
        data_dir,
        generated_at,
        &bundle_id,
        &request.sensitive,
        available_attachments.len(),
    );
    let diagnostics_json = serde_json::to_vec_pretty(&report).map_err(|err| err.to_string())?;
    let diagnostic_report_sha256 = sha256_hex(&diagnostics_json);
    let mut entries = vec![
        BundleManifestEntry {
            archive_name: "diagnostics.json".to_string(),
            sha256: diagnostic_report_sha256.clone(),
            size_bytes: diagnostics_json.len() as u64,
            sensitive: false,
        },
        BundleManifestEntry {
            archive_name: "README.txt".to_string(),
            sha256: sha256_hex(readme_bytes()),
            size_bytes: readme_bytes().len() as u64,
            sensitive: false,
        },
    ];
    entries.extend(
        available_attachments
            .iter()
            .map(|attachment| BundleManifestEntry {
                archive_name: attachment.preview.archive_name.clone(),
                sha256: attachment.sha256.clone(),
                size_bytes: attachment.preview.size_bytes,
                sensitive: true,
            }),
    );
    let manifest = BundleManifest {
        schema_version: DIAGNOSTIC_SCHEMA_VERSION,
        bundle_id: bundle_id.clone(),
        generated_at: generated_at.to_string(),
        diagnostic_report_sha256,
        sensitive_content_included: !available_attachments.is_empty(),
        selected_sensitive_categories: request.sensitive.selected_categories(),
        export_audit_policy: "atomic_publish_then_audit_or_bundle_removed",
        entries,
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest).map_err(|err| err.to_string())?;
    Ok(PreparedBundle {
        target,
        request_key: request.request_key.trim().to_string(),
        generated_at: generated_at.to_string(),
        bundle_id,
        diagnostics_json,
        manifest_json,
        attachments: available_attachments,
        sensitive_categories: request.sensitive.selected_categories(),
    })
}

fn readme_bytes() -> &'static [u8] {
    b"Jiaofu diagnostic bundle schema v1\n\
Default diagnostics exclude credentials, student identity, absolute paths, audio, ASR and answer content.\n\
Files under sensitive/ were included only after an explicit preview and confirmation.\n\
The immutable local audit stores only the bundle hash, category names and item counts, never the output path or content.\n"
}

fn temp_path(target: &Path, bundle_id: &str) -> R<PathBuf> {
    let parent = target
        .parent()
        .filter(|path| path.is_dir())
        .ok_or_else(|| "诊断包目标目录不存在".to_string())?;
    let file_name = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "诊断包文件名无效".to_string())?;
    Ok(parent.join(format!(".{file_name}.{}.tmp", &bundle_id[..16])))
}

fn zip_options() -> FileOptions {
    FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o600)
}

fn write_zip(plan: &PreparedBundle) -> R<PathBuf> {
    let temp = temp_path(&plan.target, &plan.bundle_id)?;
    if temp.exists() {
        return Err("诊断包临时文件已存在，请稍后重试".to_string());
    }
    let result = (|| -> R<()> {
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|err| format!("创建诊断包失败: {err}"))?;
        let mut zip = ZipWriter::new(file);
        zip.start_file("manifest.json", zip_options())
            .map_err(|err| err.to_string())?;
        zip.write_all(&plan.manifest_json)
            .map_err(|err| err.to_string())?;
        zip.start_file("diagnostics.json", zip_options())
            .map_err(|err| err.to_string())?;
        zip.write_all(&plan.diagnostics_json)
            .map_err(|err| err.to_string())?;
        zip.start_file("README.txt", zip_options())
            .map_err(|err| err.to_string())?;
        zip.write_all(readme_bytes())
            .map_err(|err| err.to_string())?;
        for attachment in &plan.attachments {
            zip.start_file(&attachment.preview.archive_name, zip_options())
                .map_err(|err| err.to_string())?;
            match &attachment.source {
                AttachmentSource::Bytes(bytes) => {
                    if sha256_hex(bytes) != attachment.sha256 {
                        return Err("内容证据在导出前发生变化，请重新预览".to_string());
                    }
                    zip.write_all(bytes).map_err(|err| err.to_string())?;
                }
                AttachmentSource::File(path) => {
                    if sha256_file(path).map_err(|err| err.to_string())? != attachment.sha256 {
                        return Err("音频证据在导出前发生变化，请重新预览".to_string());
                    }
                    let mut source = File::open(path).map_err(|err| err.to_string())?;
                    std::io::copy(&mut source, &mut zip).map_err(|err| err.to_string())?;
                    if sha256_file(path).map_err(|err| err.to_string())? != attachment.sha256 {
                        return Err("音频证据在导出过程中发生变化，请重新预览".to_string());
                    }
                }
            }
        }
        let file = zip.finish().map_err(|err| err.to_string())?;
        file.sync_all().map_err(|err| err.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))
                .map_err(|err| err.to_string())?;
        }
        Ok(())
    })();
    if let Err(err) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(err);
    }
    Ok(temp)
}

fn publish_without_overwrite(temp: &Path, target: &Path) -> R<()> {
    if let Err(err) = std::fs::hard_link(temp, target) {
        let _ = std::fs::remove_file(temp);
        return Err(format!(
            "诊断包原子发布失败，目标可能已存在，请使用新的文件名重试: {err}"
        ));
    }
    if let Err(err) = std::fs::remove_file(temp) {
        let cleanup_result = std::fs::remove_file(target);
        return match cleanup_result {
            Ok(()) => Err(format!("诊断包临时文件清理失败，已取消导出: {err}")),
            Err(cleanup_err) => Err(format!(
                "诊断包临时文件清理失败且目标副本清理失败，请人工删除刚才选择的目标文件: {err}; {cleanup_err}"
            )),
        };
    }
    Ok(())
}

fn record_export_audit(
    conn: &Connection,
    plan: &PreparedBundle,
    bundle_sha256: &str,
    bundle_size: u64,
) -> R<String> {
    let meta = serde_json::json!({
        "schema_version": DIAGNOSTIC_SCHEMA_VERSION,
        "bundle_sha256": bundle_sha256,
        "bundle_size_bytes": bundle_size,
        "sensitive_categories": plan.sensitive_categories,
        "sensitive_item_count": plan.attachments.len(),
        "output_path_stored": false,
    });
    let meta_json = serde_json::to_string(&meta).map_err(|err| err.to_string())?;
    let event = audit::append(
        conn,
        &audit::NewAuditEvent {
            idempotency_key: &format!("diagnostic.export:{}", plan.request_key),
            actor_type: AuditActorType::Teacher,
            actor_id: Some("local-teacher"),
            action: "diagnostic_bundle.exported",
            object_type: "diagnostic_bundle",
            object_id: &plan.bundle_id,
            object_revision: Some(1),
            note: None,
            meta_json: Some(&meta_json),
            occurred_at: &plan.generated_at,
        },
    )
    .map_err(|err| err.to_string())?;
    Ok(event.public_id)
}

pub fn export_at(
    conn: &Connection,
    data_dir: &Path,
    request: &DiagnosticExportRequest,
    generated_at: &str,
) -> R<DiagnosticExportResult> {
    let plan = prepare_bundle(conn, data_dir, request, generated_at)?;
    let temp = write_zip(&plan)?;
    let bundle_sha256 = match sha256_file(&temp) {
        Ok(hash) => hash,
        Err(err) => {
            let _ = std::fs::remove_file(&temp);
            return Err(err.to_string());
        }
    };
    let bundle_size = match temp.metadata() {
        Ok(meta) => meta.len(),
        Err(err) => {
            let _ = std::fs::remove_file(&temp);
            return Err(err.to_string());
        }
    };
    publish_without_overwrite(&temp, &plan.target)?;
    let audit_event_public_id = match record_export_audit(conn, &plan, &bundle_sha256, bundle_size)
    {
        Ok(public_id) => public_id,
        Err(err) => {
            if let Err(cleanup_err) = std::fs::remove_file(&plan.target) {
                return Err(format!(
                    "诊断包审计失败且清理失败，请人工删除刚才选择的目标文件: {err}; {cleanup_err}"
                ));
            }
            return Err(format!("诊断包未写入审计，已取消并删除导出文件: {err}"));
        }
    };
    let file_name = plan
        .target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("diagnostic.zip")
        .to_string();
    Ok(DiagnosticExportResult {
        schema_version: DIAGNOSTIC_SCHEMA_VERSION,
        bundle_id: plan.bundle_id,
        file_name,
        sha256: bundle_sha256,
        size_bytes: bundle_size,
        generated_at: plan.generated_at,
        sensitive_content_included: !plan.attachments.is_empty(),
        included_sensitive_items: plan.attachments.len(),
        audit_event_public_id,
    })
}

pub fn record_backup_restore(
    conn: &Connection,
    restored_file_name: &str,
    protective_file_name: &str,
) -> R<()> {
    let occurred_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let meta = serde_json::json!({
        "schema_version": DIAGNOSTIC_SCHEMA_VERSION,
        "restored_backup_kind": safe_label(
            restored_file_name
                .split("--")
                .nth(1)
                .and_then(|value| value.split('.').next())
                .unwrap_or("unknown")
        ),
        "protective_snapshot_created": true,
        "file_names_stored": false,
    });
    let meta_json = serde_json::to_string(&meta).map_err(|err| err.to_string())?;
    audit::append(
        conn,
        &audit::NewAuditEvent {
            idempotency_key: &format!(
                "backup.restore:{}",
                sha256_hex(format!("{restored_file_name}:{protective_file_name}").as_bytes())
            ),
            actor_type: AuditActorType::Teacher,
            actor_id: Some("local-teacher"),
            action: "backup.restore.completed",
            object_type: "database_backup",
            object_id: &sha256_hex(restored_file_name.as_bytes()),
            object_revision: Some(1),
            note: None,
            meta_json: Some(&meta_json),
            occurred_at: &occurred_at,
        },
    )
    .map(|_| ())
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn diagnostic_preview(
    state: State<'_, AppState>,
    request: DiagnosticPreviewRequest,
) -> R<DiagnosticPreview> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    preview(&conn, &request)
}

#[tauri::command]
pub fn diagnostic_export(
    state: State<'_, AppState>,
    request: DiagnosticExportRequest,
) -> R<DiagnosticExportResult> {
    let generated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    export_at(&conn, &state.data_dir, &request, &generated_at)
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
