use super::*;
use rusqlite::params;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};

const NOW: &str = "2026-07-19T08:00:00.000Z";
const POISON_NAME: &str = "SECRET_STUDENT_ZHANG";
const POISON_PATH: &str = "/Users/private/SECRET_STUDENT_ZHANG.wav";
const POISON_TRANSCRIPT: &str = "SECRET_TRANSCRIPT_1840";
const POISON_ANSWER: &str = "SECRET_STANDARD_ANSWER";
const POISON_TOKEN: &str = "SECRET_PROVIDER_TOKEN";
static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn test_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "jiaofu-diagnostics-{label}-{}-{}",
        std::process::id(),
        TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn seeded() -> (Connection, PathBuf, PathBuf) {
    let dir = test_dir("seeded");
    let audio = dir.join("managed-audio.wav");
    std::fs::write(&audio, b"RIFF synthetic audio bytes").unwrap();
    std::fs::write(
        dir.join("secrets.json"),
        format!(r#"{{"access_token":"{POISON_TOKEN}"}}"#),
    )
    .unwrap();
    let conn = suite_core::db::open_in_memory().unwrap();
    crate::state::run_all_migrations(&conn).unwrap();
    conn.execute(
        "INSERT INTO students(student_no,name,enabled) VALUES ('S-POISON',?1,1)",
        [POISON_NAME],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO rec_contents
                (content_no,title,answer_text,answer_version,enabled,created_at,updated_at)
             VALUES ('C-POISON','Poison title',?1,1,1,?2,?2)",
        params![POISON_ANSWER, NOW],
    )
    .unwrap();
    conn.execute(
            "INSERT INTO submissions
                (module,student_id,ref_id,media_type,file_path,archived_path,file_hash,
                 recognized_text,recognize_meta,recognize_status,status,created_at,updated_at)
             VALUES
                ('recitation',1,1,'audio',?1,?2,'poison-hash',?3,?4,
                 'failed','anomaly',?5,?5)",
            params![
                POISON_PATH,
                audio.to_string_lossy(),
                POISON_TRANSCRIPT,
                format!(
                    r#"{{"error_code":"TIMEOUT","error_message":"{POISON_PATH} {POISON_TOKEN}","retryable":true,"attempts":2}}"#
                ),
                NOW,
            ],
        )
        .unwrap();
    (conn, dir, audio)
}

fn default_request(path: &Path, request_key: &str) -> DiagnosticExportRequest {
    DiagnosticExportRequest {
        output_path: path.to_string_lossy().into_owned(),
        request_key: request_key.to_string(),
        sensitive: DiagnosticSensitiveOptions::default(),
        preview_token: None,
        confirm_sensitive_evidence: false,
    }
}

fn zip_entry(path: &Path, name: &str) -> Vec<u8> {
    let file = File::open(path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut entry = archive.by_name(name).unwrap();
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).unwrap();
    bytes
}

fn zip_names(path: &Path) -> Vec<String> {
    let file = File::open(path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    (0..archive.len())
        .map(|index| archive.by_index(index).unwrap().name().to_string())
        .collect()
}

#[test]
fn default_bundle_excludes_identity_paths_content_and_credentials() {
    let (conn, dir, _) = seeded();
    let output = dir.join("default.zip");
    let result = export_at(
        &conn,
        &dir,
        &default_request(&output, "diagnostic-default-1"),
        NOW,
    )
    .unwrap();
    assert!(!result.sensitive_content_included);
    assert_eq!(
        zip_names(&output),
        vec!["manifest.json", "diagnostics.json", "README.txt"]
    );
    let report = String::from_utf8(zip_entry(&output, "diagnostics.json")).unwrap();
    for poison in [
        POISON_NAME,
        POISON_PATH,
        POISON_TRANSCRIPT,
        POISON_ANSWER,
        POISON_TOKEN,
    ] {
        assert!(!report.contains(poison), "default report leaked {poison}");
    }
    assert!(report.contains("\"error_code\": \"TIMEOUT\""));
    assert!(report.contains("\"absolute_paths_in_report\": false"));
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM audit_events
                 WHERE action='diagnostic_bundle.exported'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn sensitive_export_requires_matching_preview_and_explicit_confirmation() {
    let (conn, dir, _) = seeded();
    let options = DiagnosticSensitiveOptions {
        include_failed_audio: true,
        include_failed_asr: true,
        include_standard_answers: true,
    };
    let preview = preview(
        &conn,
        &DiagnosticPreviewRequest {
            sensitive: options.clone(),
        },
    )
    .unwrap();
    assert_eq!(preview.available_item_count, 3);
    assert_eq!(preview.unavailable_item_count, 0);
    assert!(preview.preview_token.is_some());

    let output = dir.join("sensitive.zip");
    let mut request = DiagnosticExportRequest {
        output_path: output.to_string_lossy().into_owned(),
        request_key: "diagnostic-sensitive-1".to_string(),
        sensitive: options,
        preview_token: preview.preview_token.clone(),
        confirm_sensitive_evidence: false,
    };
    assert!(export_at(&conn, &dir, &request, NOW)
        .unwrap_err()
        .contains("二次明确确认"));
    request.confirm_sensitive_evidence = true;
    request.preview_token = Some("0".repeat(64));
    assert!(export_at(&conn, &dir, &request, NOW)
        .unwrap_err()
        .contains("重新预览"));
    request.preview_token = preview.preview_token;
    let result = export_at(&conn, &dir, &request, NOW).unwrap();
    assert!(result.sensitive_content_included);
    assert_eq!(result.included_sensitive_items, 3);
    let names = zip_names(&output);
    assert!(names.contains(&"sensitive/audio/submission-1.wav".to_string()));
    assert!(names.contains(&"sensitive/asr/submission-1.txt".to_string()));
    assert!(names.contains(&"sensitive/answers/content-1.txt".to_string()));
    assert_eq!(
        zip_entry(&output, "sensitive/asr/submission-1.txt"),
        POISON_TRANSCRIPT.as_bytes()
    );
    assert_eq!(
        zip_entry(&output, "sensitive/answers/content-1.txt"),
        POISON_ANSWER.as_bytes()
    );
}

#[test]
fn changed_audio_invalidates_sensitive_preview_token() {
    let (conn, dir, audio) = seeded();
    let options = DiagnosticSensitiveOptions {
        include_failed_audio: true,
        ..DiagnosticSensitiveOptions::default()
    };
    let preview = preview(
        &conn,
        &DiagnosticPreviewRequest {
            sensitive: options.clone(),
        },
    )
    .unwrap();
    std::fs::write(&audio, b"changed after preview").unwrap();
    let request = DiagnosticExportRequest {
        output_path: dir.join("changed.zip").to_string_lossy().into_owned(),
        request_key: "diagnostic-changed-1".to_string(),
        sensitive: options,
        preview_token: preview.preview_token,
        confirm_sensitive_evidence: true,
    };
    assert!(export_at(&conn, &dir, &request, NOW)
        .unwrap_err()
        .contains("重新预览"));
}

#[test]
fn export_refuses_relative_path_and_existing_target() {
    let (conn, dir, _) = seeded();
    let mut request = default_request(Path::new("relative.zip"), "diagnostic-path-1");
    assert!(export_at(&conn, &dir, &request, NOW)
        .unwrap_err()
        .contains("绝对路径"));
    let output = dir.join("existing.zip");
    std::fs::write(&output, b"keep").unwrap();
    request.output_path = output.to_string_lossy().into_owned();
    assert!(export_at(&conn, &dir, &request, NOW)
        .unwrap_err()
        .contains("不会静默覆盖"));
    assert_eq!(std::fs::read(output).unwrap(), b"keep");
}

#[test]
fn publish_race_never_overwrites_target_created_after_validation() {
    let dir = test_dir("publish-race");
    let temp = dir.join(".bundle.tmp");
    let target = dir.join("bundle.zip");
    std::fs::write(&temp, b"new bundle").unwrap();
    std::fs::write(&target, b"existing content").unwrap();

    let error = publish_without_overwrite(&temp, &target).unwrap_err();

    assert!(error.contains("目标可能已存在"));
    assert_eq!(std::fs::read(&target).unwrap(), b"existing content");
    assert!(!temp.exists());
}

#[test]
fn repeated_request_key_cannot_create_a_second_bundle() {
    let (conn, dir, _) = seeded();
    let first = dir.join("first.zip");
    export_at(
        &conn,
        &dir,
        &default_request(&first, "diagnostic-repeat-1"),
        NOW,
    )
    .unwrap();
    let second = dir.join("second.zip");
    let error = export_at(
        &conn,
        &dir,
        &default_request(&second, "diagnostic-repeat-1"),
        NOW,
    )
    .unwrap_err();
    assert!(error.contains("拒绝重复写出"));
    assert!(!second.exists());
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM audit_events
                 WHERE action='diagnostic_bundle.exported'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn report_contains_schema_integrity_runtime_and_idempotency_summaries() {
    let (conn, dir, _) = seeded();
    let output = dir.join("summary.zip");
    export_at(
        &conn,
        &dir,
        &default_request(&output, "diagnostic-summary-1"),
        NOW,
    )
    .unwrap();
    let report: Value = serde_json::from_slice(&zip_entry(&output, "diagnostics.json")).unwrap();
    assert_eq!(report["database"]["integrity"]["integrity_status"], "ok");
    assert_eq!(
        report["database"]["integrity"]["foreign_key_violation_count"],
        0
    );
    assert_eq!(
        report["database"]["migrations"]["expected_count"],
        report["database"]["migrations"]["applied_count"]
    );
    assert!(report["runtime_summary"]["ai_runs"].is_array());
    assert!(report["runtime_summary"]["decision_effects"].is_array());
    assert!(report["runtime_summary"]["learning_evidence"].is_array());
    assert!(report["idempotency_checks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|check| check["status"] == "ok"));
}

#[cfg(unix)]
#[test]
fn exported_bundle_is_owner_read_write_only() {
    use std::os::unix::fs::PermissionsExt;

    let (conn, dir, _) = seeded();
    let output = dir.join("permissions.zip");
    export_at(
        &conn,
        &dir,
        &default_request(&output, "diagnostic-permissions-1"),
        NOW,
    )
    .unwrap();
    assert_eq!(
        output.metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn backup_restore_audit_exposes_no_file_name_and_is_reported() {
    let (conn, dir, _) = seeded();
    record_backup_restore(
        &conn,
        "jiaofu-20260719T000000+0800--manual.db",
        "jiaofu-20260719T000001+0800--before-restore.db",
    )
    .unwrap();
    let output = dir.join("restore-status.zip");
    export_at(
        &conn,
        &dir,
        &default_request(&output, "diagnostic-restore-1"),
        NOW,
    )
    .unwrap();
    let report: Value = serde_json::from_slice(&zip_entry(&output, "diagnostics.json")).unwrap();
    assert_eq!(
        report["database"]["backups"]["last_restore_status"],
        "completed"
    );
    let audit_meta: String = conn
        .query_row(
            "SELECT meta_json FROM audit_events
                 WHERE action='backup.restore.completed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!audit_meta.contains("jiaofu-"));
}
