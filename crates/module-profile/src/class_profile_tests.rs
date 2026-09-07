use super::*;
use crate::class_exports::{
    create_export_snapshot, write_export_snapshot_csv, CreateClassProfileExportInput,
    CLASS_PROFILE_EXPORT_MIN_GROUP_SIZE, CLASS_PROFILE_EXPORT_RULE_VERSION, LOCAL_TEACHER_ACTOR_ID,
};
use suite_core::db::repo::learning_evidence::{create_or_get, NewLearningEvidence};
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
use suite_core::models::{
    AssessmentContext, ConfirmationLevel, EvidenceKind, EvidenceSourceModule,
};

struct Fixture {
    conn: Connection,
    class_id: i64,
    students: Vec<i64>,
    map_public_id: String,
    knowledge: String,
    curriculum: String,
}

fn setup() -> Fixture {
    let conn = open_in_memory().unwrap();
    run_migrations(&conn, CORE_MIGRATIONS).unwrap();
    run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
    run_migrations(&conn, module_exam::exam_migrations()).unwrap();
    run_migrations(&conn, module_wrongbook::wrongbook_migrations()).unwrap();
    run_migrations(&conn, crate::profile_migrations()).unwrap();
    conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])
        .unwrap();
    let subject_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO classes(name,term,textbook)
             VALUES ('八年级一班','2026','中国历史八上')",
        [],
    )
    .unwrap();
    let class_id = conn.last_insert_rowid();
    let mut students = Vec::new();
    for index in 1..=4 {
        conn.execute(
            "INSERT INTO students(student_no,name,class_id,enabled)
                 VALUES (?1,?2,?3,1)",
            params![format!("{index:02}"), format!("学生{index}"), class_id],
        )
        .unwrap();
        students.push(conn.last_insert_rowid());
    }
    conn.execute(
        "INSERT INTO k1_textbook_editions
              (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
             VALUES ('edition-1',?1,'pep','2026','八上历史','8','upper','active',
                     '2026-07-01T00:00:00.000Z')",
        [subject_id],
    )
    .unwrap();
    let edition_id = conn.last_insert_rowid();
    let map_public_id = "map-1".to_string();
    conn.execute(
        "INSERT INTO k1_knowledge_maps
              (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
             VALUES (?1,?2,1,'confirmed','2026-07-01T00:00:00.000Z',
                     '2026-07-01T00:00:00.000Z')",
        (&map_public_id, edition_id),
    )
    .unwrap();
    let map_id = conn.last_insert_rowid();
    let curriculum = "curriculum-1".to_string();
    conn.execute(
        "INSERT INTO k1_curriculum_nodes
              (public_id,stable_id,knowledge_map_id,node_type,title,order_index,state,created_at)
             VALUES (?1,'curriculum-stable-1',?2,'unit','第一单元',1,'active',
                     '2026-07-01T00:00:00.000Z')",
        (&curriculum, map_id),
    )
    .unwrap();
    let curriculum_id = conn.last_insert_rowid();
    let knowledge = "knowledge-1".to_string();
    conn.execute(
            "INSERT INTO k1_knowledge_nodes
              (public_id,stable_id,knowledge_map_id,curriculum_node_id,title,order_index,state,created_at)
             VALUES (?1,'stable-1',?2,?3,'洋务运动失败原因',1,'active',
                     '2026-07-01T00:00:00.000Z')",
            (&knowledge, map_id, curriculum_id),
        )
        .unwrap();
    Fixture {
        conn,
        class_id,
        students,
        map_public_id,
        knowledge,
        curriculum,
    }
}

fn add_evidence(
    fixture: &Fixture,
    student_id: i64,
    key: &str,
    source: &str,
    occurred_at: &str,
    value: f64,
) {
    create_or_get(
        &fixture.conn,
        &NewLearningEvidence {
            idempotency_key: key,
            student_id,
            source_module: EvidenceSourceModule::Grading,
            source_type: "question_rubric_point",
            source_ref_type: "rubric_point",
            source_ref_id: source,
            source_revision: 1,
            decision_ref_type: Some("grade_decision"),
            decision_ref_id: Some(key),
            decision_revision: Some(1),
            knowledge_node_id: Some(&fixture.knowledge),
            ability_dimension_id: None,
            evidence_kind: EvidenceKind::Accuracy,
            value,
            confirmation_level: ConfirmationLevel::TeacherCorrected,
            evidence_quality: 1.0,
            assessment_context: AssessmentContext::ClosedBook,
            occurred_at,
            rule_version: "exam-v1",
            knowledge_map_version: &format!("{}:r1", fixture.map_public_id),
        },
    )
    .unwrap();
}

fn generate_personal(
    fixture: &mut Fixture,
    student_id: i64,
    prefix: &str,
    value: f64,
    range_start: &str,
    range_end: &str,
) -> StudentProfileSnapshot {
    for (index, date) in [
        "2026-07-05T00:00:00.000Z",
        "2026-07-12T00:00:00.000Z",
        "2026-07-20T00:00:00.000Z",
    ]
    .iter()
    .enumerate()
    {
        add_evidence(
            fixture,
            student_id,
            &format!("{prefix}-{index}"),
            &format!("{prefix}-q{index}"),
            date,
            value,
        );
    }
    profile::generate_student_profile(
        &mut fixture.conn,
        &profile::GenerateStudentProfileInput {
            scope: profile::StudentProfileScope {
                class_id: fixture.class_id,
                student_id,
                range_start,
                range_end,
            },
            confirmed_by: "teacher-1",
        },
    )
    .unwrap()
}

fn generate_personal_scoped(
    fixture: &mut Fixture,
    student_id: i64,
    prefix: &str,
    value: f64,
    curriculum_public_id: &str,
) -> StudentProfileSnapshot {
    for (index, date) in [
        "2026-07-05T00:00:00.000Z",
        "2026-07-12T00:00:00.000Z",
        "2026-07-20T00:00:00.000Z",
    ]
    .iter()
    .enumerate()
    {
        add_evidence(
            fixture,
            student_id,
            &format!("{prefix}-{index}"),
            &format!("{prefix}-q{index}"),
            date,
            value,
        );
    }
    profile::generate_scoped_student_profile(
        &mut fixture.conn,
        &profile::GenerateScopedStudentProfileInput {
            scope: profile::StudentProfileScope {
                class_id: fixture.class_id,
                student_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            selection: ProfileScopeSelectionInput {
                selector_kind: "curriculum_node",
                selector_public_id: Some(curriculum_public_id),
            },
            confirmed_by: "teacher-1",
        },
    )
    .unwrap()
}

fn scope(fixture: &Fixture) -> ClassProfileScope<'_> {
    ClassProfileScope {
        class_id: fixture.class_id,
        range_start: "2026-07-01",
        range_end: "2026-07-31",
    }
}

#[test]
fn preview_is_read_only_and_separates_missing_scope_and_stale() {
    let mut fixture = setup();
    let first = fixture.students[0];
    let second = fixture.students[1];
    let third = fixture.students[2];
    generate_personal(
        &mut fixture,
        first,
        "included",
        1.0,
        "2026-07-01",
        "2026-07-31",
    );
    generate_personal(
        &mut fixture,
        second,
        "mismatch",
        1.0,
        "2026-07-01",
        "2026-07-20",
    );
    generate_personal(
        &mut fixture,
        third,
        "stale",
        1.0,
        "2026-07-01",
        "2026-07-31",
    );
    add_evidence(
        &fixture,
        third,
        "stale-new",
        "stale-new-q",
        "2026-07-25T00:00:00.000Z",
        0.0,
    );
    let before: i64 = fixture
        .conn
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    let preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
    let after: i64 = fixture
        .conn
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(before, after);
    assert!(preview.can_generate);
    assert_eq!(preview.counts.total_student_count, 4);
    assert_eq!(preview.counts.snapshot_student_count, 1);
    assert_eq!(preview.counts.scope_mismatch_count, 1);
    assert_eq!(preview.counts.stale_snapshot_count, 1);
    assert_eq!(preview.counts.missing_snapshot_count, 1);
}

#[test]
fn class_scope_only_uses_personal_snapshots_with_exact_textbook_scope() {
    let mut fixture = setup();
    let first = fixture.students[0];
    let second = fixture.students[1];
    let curriculum = fixture.curriculum.clone();
    generate_personal_scoped(&mut fixture, first, "scoped", 1.0, &curriculum);
    generate_personal(
        &mut fixture,
        second,
        "automatic",
        1.0,
        "2026-07-01",
        "2026-07-31",
    );
    let selection = ProfileScopeSelectionInput {
        selector_kind: "curriculum_node",
        selector_public_id: Some(&curriculum),
    };
    let preview =
        preview_scoped_class_profile(&fixture.conn, &scope(&fixture), &selection).unwrap();
    assert_eq!(preview.counts.snapshot_student_count, 1);
    assert_eq!(preview.counts.scope_mismatch_count, 1);
    assert_eq!(preview.counts.missing_snapshot_count, 2);
    assert_eq!(
        preview.scope_selection.selector_public_id.as_deref(),
        Some(curriculum.as_str())
    );
    let snapshot = generate_scoped_class_profile(
        &mut fixture.conn,
        &GenerateScopedClassProfileInput {
            scope: ClassProfileScope {
                class_id: fixture.class_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            selection,
            expected_source_watermark: &preview.source_watermark,
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    assert_eq!(snapshot.snapshot_student_count, 1);
    assert_eq!(
        snapshot.scope_selection.selector_public_id.as_deref(),
        Some(curriculum.as_str())
    );
    assert!(!snapshot.is_stale);
}

#[test]
fn generated_snapshot_has_denominators_heatmap_audit_and_immutability() {
    let mut fixture = setup();
    for index in 0..3 {
        let student_id = fixture.students[index];
        generate_personal(
            &mut fixture,
            student_id,
            &format!("weak-{index}"),
            0.0,
            "2026-07-01",
            "2026-07-31",
        );
    }
    let preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
    let generated = generate_class_profile(
        &mut fixture.conn,
        &GenerateClassProfileInput {
            scope: ClassProfileScope {
                class_id: fixture.class_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            expected_source_watermark: &preview.source_watermark,
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    assert_eq!(generated.total_student_count, 4);
    assert_eq!(generated.snapshot_student_count, 3);
    assert_eq!(generated.eligible_student_count, 3);
    assert_eq!(generated.inputs.len(), 4);
    let node = &generated.knowledge_metrics[0];
    assert!(node.sample_sufficient);
    assert_eq!(node.class_status, "common_needs_support");
    assert_eq!(node.needs_support_count, 3);
    assert_eq!(node.eligible_student_count, 3);
    assert_eq!(node.cells.len(), 4);
    assert_eq!(
        node.cells
            .iter()
            .filter(|cell| cell.status == "missing_snapshot")
            .count(),
        1
    );
    assert_eq!(generated.student_status_counts.needs_support_count, 3);
    assert_eq!(generated.student_status_counts.data_unavailable_count, 1);
    assert_eq!(generated.student_statuses.len(), 4);
    assert_eq!(generated.trend.comparison_status, "no_comparable_baseline");
    let audit_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM audit_events
                 WHERE action='profile.class_snapshot.generated' AND object_id=?1",
            [&generated.public_id],
            |row| row.get(0),
        )
        .unwrap();
    let outbox_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM outbox_events
                 WHERE event_type='class_profile_snapshot_created' AND aggregate_id=?1",
            [&generated.public_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audit_count, 1);
    assert_eq!(outbox_count, 1);
    assert!(fixture
        .conn
        .execute(
            "UPDATE class_profile_snapshots SET generated_by='tampered'
                 WHERE public_id=?1",
            [&generated.public_id]
        )
        .is_err());
    assert!(fixture
        .conn
        .execute(
            "DELETE FROM class_profile_student_cells WHERE snapshot_id=
                 (SELECT id FROM class_profile_snapshots WHERE public_id=?1)",
            [&generated.public_id]
        )
        .is_err());
}

#[test]
fn low_sample_never_becomes_common_needs_support() {
    let mut fixture = setup();
    for index in 0..2 {
        let student_id = fixture.students[index];
        generate_personal(
            &mut fixture,
            student_id,
            &format!("low-{index}"),
            0.0,
            "2026-07-01",
            "2026-07-31",
        );
    }
    let preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
    let generated = generate_class_profile(
        &mut fixture.conn,
        &GenerateClassProfileInput {
            scope: ClassProfileScope {
                class_id: fixture.class_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            expected_source_watermark: &preview.source_watermark,
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    let node = &generated.knowledge_metrics[0];
    assert!(!node.sample_sufficient);
    assert_eq!(node.class_status, "class_evidence_insufficient");
    assert_eq!(node.needs_support_count, 2);
}

#[test]
fn stale_preview_watermark_rejects_without_partial_write() {
    let mut fixture = setup();
    let student_id = fixture.students[0];
    generate_personal(
        &mut fixture,
        student_id,
        "base",
        1.0,
        "2026-07-01",
        "2026-07-31",
    );
    let preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
    add_evidence(
        &fixture,
        student_id,
        "after-preview",
        "after-preview-q",
        "2026-07-25T00:00:00.000Z",
        0.0,
    );
    let result = generate_class_profile(
        &mut fixture.conn,
        &GenerateClassProfileInput {
            scope: ClassProfileScope {
                class_id: fixture.class_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            expected_source_watermark: &preview.source_watermark,
            confirmed_by: "teacher-1",
        },
    );
    assert!(result.is_err());
    let count: i64 = fixture
        .conn
        .query_row("SELECT COUNT(*) FROM class_profile_snapshots", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn class_snapshot_becomes_stale_when_personal_input_changes() {
    let mut fixture = setup();
    let student_id = fixture.students[0];
    generate_personal(
        &mut fixture,
        student_id,
        "base",
        1.0,
        "2026-07-01",
        "2026-07-31",
    );
    let preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
    let generated = generate_class_profile(
        &mut fixture.conn,
        &GenerateClassProfileInput {
            scope: ClassProfileScope {
                class_id: fixture.class_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            expected_source_watermark: &preview.source_watermark,
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    assert!(!generated.is_stale);
    add_evidence(
        &fixture,
        student_id,
        "new-evidence",
        "new-q",
        "2026-07-25T00:00:00.000Z",
        0.0,
    );
    let loaded = get_class_profile(&fixture.conn, &generated.public_id)
        .unwrap()
        .unwrap();
    assert!(loaded.is_stale);
    assert_eq!(loaded.revision, 1);
}

#[test]
fn same_scope_same_policy_refresh_has_explainable_non_causal_trend() {
    let mut fixture = setup();
    for index in 0..3 {
        let student_id = fixture.students[index];
        generate_personal(
            &mut fixture,
            student_id,
            &format!("trend-weak-{index}"),
            0.0,
            "2026-07-01",
            "2026-07-31",
        );
    }
    let first_preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
    let first = generate_class_profile(
        &mut fixture.conn,
        &GenerateClassProfileInput {
            scope: ClassProfileScope {
                class_id: fixture.class_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            expected_source_watermark: &first_preview.source_watermark,
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    for index in 0..3 {
        let student_id = fixture.students[index];
        generate_personal(
            &mut fixture,
            student_id,
            &format!("trend-good-{index}"),
            1.0,
            "2026-07-01",
            "2026-07-31",
        );
    }
    let second_preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
    let second = generate_class_profile(
        &mut fixture.conn,
        &GenerateClassProfileInput {
            scope: ClassProfileScope {
                class_id: fixture.class_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            expected_source_watermark: &second_preview.source_watermark,
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    assert_eq!(second.trend.comparison_status, "comparable");
    assert_eq!(
        second.trend.comparison_kind.as_deref(),
        Some("same_scope_refresh")
    );
    assert_eq!(
        second.trend.previous_snapshot_public_id.as_deref(),
        Some(first.public_id.as_str())
    );
    assert_eq!(second.trend.snapshot_student_count_delta, Some(0));
    assert!(second.trend.note.contains("不能据此自动宣称"));
}

#[test]
fn deidentified_export_is_idempotent_immutable_private_and_contains_no_student_rows() {
    let mut fixture = setup();
    for index in 0..3 {
        let student_id = fixture.students[index];
        generate_personal(
            &mut fixture,
            student_id,
            &format!("export-weak-{index}"),
            0.0,
            "2026-07-01",
            "2026-07-31",
        );
    }
    let preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
    let class_snapshot = generate_class_profile(
        &mut fixture.conn,
        &GenerateClassProfileInput {
            scope: ClassProfileScope {
                class_id: fixture.class_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            expected_source_watermark: &preview.source_watermark,
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    let request = CreateClassProfileExportInput {
        request_key: "class-export-request-1",
        snapshot_public_id: &class_snapshot.public_id,
        expected_snapshot_payload_sha256: &class_snapshot.payload_sha256,
        report_kind: "deidentified_class_summary",
        purpose: "internal_teaching",
        actor_role: "local_teacher",
        actor_id: LOCAL_TEACHER_ACTOR_ID,
    };
    let export = create_export_snapshot(&mut fixture.conn, &request).unwrap();
    assert_eq!(export.snapshot_public_id, class_snapshot.public_id);
    assert_eq!(export.class_id, fixture.class_id);
    assert_eq!(export.min_group_size, CLASS_PROFILE_EXPORT_MIN_GROUP_SIZE);
    assert_eq!(export.rule_version, CLASS_PROFILE_EXPORT_RULE_VERSION);
    assert!(export.suggested_file_name.ends_with(".csv"));

    let payload_json: String = fixture
        .conn
        .query_row(
            "SELECT payload_json FROM class_profile_export_snapshots
                 WHERE public_id=?1",
            [&export.public_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(payload_json.contains("洋务运动失败原因"));
    for private_value in ["学生1", "学生2", "学生3", "\"01\"", "\"02\"", "\"03\""] {
        assert!(!payload_json.contains(private_value));
    }
    let student_rows: i64 = fixture
        .conn
        .query_row(
            "SELECT COALESCE(json_array_length(payload_json,'$.nodes'),0)
                 FROM class_profile_export_snapshots WHERE public_id=?1",
            [&export.public_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(student_rows, 1);

    let repeated = create_export_snapshot(&mut fixture.conn, &request).unwrap();
    assert_eq!(repeated.public_id, export.public_id);
    let conflicting_hash = "f".repeat(64);
    let conflicting = create_export_snapshot(
        &mut fixture.conn,
        &CreateClassProfileExportInput {
            request_key: request.request_key,
            snapshot_public_id: request.snapshot_public_id,
            expected_snapshot_payload_sha256: &conflicting_hash,
            report_kind: request.report_kind,
            purpose: request.purpose,
            actor_role: request.actor_role,
            actor_id: request.actor_id,
        },
    );
    assert!(conflicting.is_err());
    let export_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM class_profile_export_snapshots",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let audit_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM audit_events
                 WHERE object_type='class_profile_export_snapshot'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let outbox_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM outbox_events
                 WHERE event_type='class_profile_deidentified_export_created'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!((export_count, audit_count, outbox_count), (1, 1, 1));
    assert!(fixture
        .conn
        .execute(
            "UPDATE class_profile_export_snapshots SET purpose=purpose WHERE public_id=?1",
            [&export.public_id],
        )
        .is_err());
    assert!(fixture
        .conn
        .execute(
            "DELETE FROM class_profile_export_snapshots WHERE public_id=?1",
            [&export.public_id],
        )
        .is_err());

    let output_path = std::env::temp_dir().join(format!(
        "jiaofu-class-profile-export-{}.csv",
        ids::new_public_id()
    ));
    let written = write_export_snapshot_csv(
        &fixture.conn,
        &export.public_id,
        output_path.to_str().unwrap(),
    )
    .unwrap();
    assert_eq!(written.sha256, export.csv_sha256);
    let csv = std::fs::read_to_string(&output_path).unwrap();
    assert!(csv.starts_with('\u{feff}'));
    assert!(csv.contains("洋务运动失败原因"));
    assert!(!csv.contains("学生1"));
    assert!(!csv.contains("学生2"));
    assert!(!csv.contains("学生3"));
    assert!(!csv.contains("排名,"));
    assert!(write_export_snapshot_csv(
        &fixture.conn,
        &export.public_id,
        output_path.to_str().unwrap(),
    )
    .is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&output_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    std::fs::remove_file(output_path).unwrap();
}

#[test]
fn stale_class_snapshot_cannot_be_exported() {
    let mut fixture = setup();
    for index in 0..3 {
        let student_id = fixture.students[index];
        generate_personal(
            &mut fixture,
            student_id,
            &format!("stale-export-{index}"),
            1.0,
            "2026-07-01",
            "2026-07-31",
        );
    }
    let preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
    let class_snapshot = generate_class_profile(
        &mut fixture.conn,
        &GenerateClassProfileInput {
            scope: ClassProfileScope {
                class_id: fixture.class_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            expected_source_watermark: &preview.source_watermark,
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    add_evidence(
        &fixture,
        fixture.students[0],
        "stale-export-new",
        "stale-export-new-q",
        "2026-07-25T00:00:00.000Z",
        0.0,
    );
    let result = create_export_snapshot(
        &mut fixture.conn,
        &CreateClassProfileExportInput {
            request_key: "stale-class-export-request",
            snapshot_public_id: &class_snapshot.public_id,
            expected_snapshot_payload_sha256: &class_snapshot.payload_sha256,
            report_kind: "deidentified_class_summary",
            purpose: "internal_teaching",
            actor_role: "local_teacher",
            actor_id: LOCAL_TEACHER_ACTOR_ID,
        },
    );
    assert!(result.is_err());
    let export_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM class_profile_export_snapshots",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(export_count, 0);
}
