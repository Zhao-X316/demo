use super::*;
use suite_core::db::repo::learning_evidence::{create_or_get, NewLearningEvidence};
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
use suite_core::models::{
    AssessmentContext, ConfirmationLevel, EvidenceKind, EvidenceSourceModule,
};

struct Fixture {
    conn: Connection,
    class_id: i64,
    student_id: i64,
    map_public_id: String,
    knowledge: Vec<String>,
    ability: String,
    curriculum: Vec<String>,
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
        "INSERT INTO classes(name,term) VALUES ('八年级一班','2026')",
        [],
    )
    .unwrap();
    let class_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO students(student_no,name,class_id,enabled)
             VALUES ('01','张三',?1,1)",
        [class_id],
    )
    .unwrap();
    let student_id = conn.last_insert_rowid();
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
    let mut curriculum = Vec::new();
    for index in 1..=2 {
        let public_id = format!("curriculum-{index}");
        conn.execute(
                "INSERT INTO k1_curriculum_nodes
                  (public_id,stable_id,knowledge_map_id,node_type,title,order_index,state,created_at)
                 VALUES (?1,?2,?3,'unit',?4,?5,'active','2026-07-01T00:00:00.000Z')",
                params![
                    public_id,
                    format!("curriculum-stable-{index}"),
                    map_id,
                    format!("第{index}单元"),
                    index
                ],
            )
            .unwrap();
        curriculum.push(public_id);
    }
    let curriculum_ids = curriculum
        .iter()
        .map(|public_id| {
            conn.query_row(
                "SELECT id FROM k1_curriculum_nodes WHERE public_id=?1",
                [public_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let mut knowledge = Vec::new();
    for index in 1..=3 {
        let public_id = format!("knowledge-{index}");
        conn.execute(
                "INSERT INTO k1_knowledge_nodes
                  (public_id,stable_id,knowledge_map_id,curriculum_node_id,title,order_index,state,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,'active','2026-07-01T00:00:00.000Z')",
                params![
                    public_id,
                    format!("stable-{index}"),
                    map_id,
                    if index <= 2 {
                        curriculum_ids[0]
                    } else {
                        curriculum_ids[1]
                    },
                    format!("知识点{index}"),
                    index
                ],
            )
            .unwrap();
        knowledge.push(public_id);
    }
    let ability = "ability-1".to_string();
    conn.execute(
        "INSERT INTO k1_ability_dimensions
              (public_id,stable_id,subject_id,revision,code,title,state,created_at)
             VALUES (?1,'ability-stable',?2,1,'fact','事实识记','active',
                     '2026-07-01T00:00:00.000Z')",
        (&ability, subject_id),
    )
    .unwrap();
    Fixture {
        conn,
        class_id,
        student_id,
        map_public_id,
        knowledge,
        ability,
        curriculum,
    }
}

#[allow(clippy::too_many_arguments)]
fn add_evidence(
    fixture: &Fixture,
    key: &str,
    target: &str,
    source: &str,
    occurred_at: &str,
    value: f64,
    context: AssessmentContext,
    confirmation: ConfirmationLevel,
) {
    create_or_get(
        &fixture.conn,
        &NewLearningEvidence {
            idempotency_key: key,
            student_id: fixture.student_id,
            source_module: EvidenceSourceModule::Grading,
            source_type: "objective_question",
            source_ref_type: "assessment_item",
            source_ref_id: source,
            source_revision: 1,
            decision_ref_type: Some("grade_decision"),
            decision_ref_id: Some(key),
            decision_revision: Some(1),
            knowledge_node_id: Some(target),
            ability_dimension_id: None,
            evidence_kind: EvidenceKind::Accuracy,
            value,
            confirmation_level: confirmation,
            evidence_quality: 1.0,
            assessment_context: context,
            occurred_at,
            rule_version: "exam-v1",
            knowledge_map_version: &format!("{}:r1", fixture.map_public_id),
        },
    )
    .unwrap();
}

fn add_correction_evidence(
    fixture: &Fixture,
    key: &str,
    target: &str,
    source: &str,
    occurred_at: &str,
    value: f64,
) {
    create_or_get(
        &fixture.conn,
        &NewLearningEvidence {
            idempotency_key: key,
            student_id: fixture.student_id,
            source_module: EvidenceSourceModule::Correction,
            source_type: "objective_question",
            source_ref_type: "assessment_item",
            source_ref_id: source,
            source_revision: 1,
            decision_ref_type: Some("grade_decision"),
            decision_ref_id: Some(key),
            decision_revision: Some(1),
            knowledge_node_id: Some(target),
            ability_dimension_id: None,
            evidence_kind: EvidenceKind::Accuracy,
            value,
            confirmation_level: ConfirmationLevel::TeacherCorrected,
            evidence_quality: 0.6,
            assessment_context: AssessmentContext::Correction,
            occurred_at,
            rule_version: "correction-v1",
            knowledge_map_version: &format!("{}:r1", fixture.map_public_id),
        },
    )
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn add_recitation_point(
    fixture: &Fixture,
    key: &str,
    target: &str,
    ability: Option<&str>,
    kind: EvidenceKind,
    occurred_at: &str,
    value: f64,
    confirmation: ConfirmationLevel,
) {
    create_or_get(
        &fixture.conn,
        &NewLearningEvidence {
            idempotency_key: key,
            student_id: fixture.student_id,
            source_module: EvidenceSourceModule::Recitation,
            source_type: "recitation_rubric_point",
            source_ref_type: "recitation_point_review",
            source_ref_id: key,
            source_revision: 1,
            decision_ref_type: Some("recitation_review"),
            decision_ref_id: Some(key),
            decision_revision: Some(1),
            knowledge_node_id: Some(target),
            ability_dimension_id: ability,
            evidence_kind: kind,
            value,
            confirmation_level: confirmation,
            evidence_quality: 1.0,
            assessment_context: AssessmentContext::Homework,
            occurred_at,
            rule_version: "recitation-point-evidence-v1",
            knowledge_map_version: &format!("{}:r1", fixture.map_public_id),
        },
    )
    .unwrap();
}

fn add_recitation_history(
    fixture: &Fixture,
    key: &str,
    source_type: &str,
    source_ref_type: &str,
    kind: EvidenceKind,
    occurred_at: &str,
    value: f64,
) {
    create_or_get(
        &fixture.conn,
        &NewLearningEvidence {
            idempotency_key: key,
            student_id: fixture.student_id,
            source_module: EvidenceSourceModule::Recitation,
            source_type,
            source_ref_type,
            source_ref_id: key,
            source_revision: 1,
            decision_ref_type: Some("recitation_review"),
            decision_ref_id: Some(key),
            decision_revision: Some(1),
            knowledge_node_id: None,
            ability_dimension_id: None,
            evidence_kind: kind,
            value,
            confirmation_level: ConfirmationLevel::TeacherOverall,
            evidence_quality: 0.9,
            assessment_context: AssessmentContext::Homework,
            occurred_at,
            rule_version: "recitation-history-v1",
            knowledge_map_version: "unmapped:r1",
        },
    )
    .unwrap();
}

fn scope(fixture: &Fixture) -> StudentProfileScope<'_> {
    StudentProfileScope {
        class_id: fixture.class_id,
        student_id: fixture.student_id,
        range_start: "2026-07-01",
        range_end: "2026-07-31",
    }
}

#[test]
fn preview_is_read_only_and_excludes_machine_only() {
    let fixture = setup();
    add_evidence(
        &fixture,
        "machine",
        &fixture.knowledge[0],
        "q1",
        "2026-07-10T00:00:00.000Z",
        1.0,
        AssessmentContext::ClosedBook,
        ConfirmationLevel::MachineOnly,
    );
    let before: i64 = fixture
        .conn
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    let preview = preview_student_profile(&fixture.conn, &scope(&fixture)).unwrap();
    let after: i64 = fixture
        .conn
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(after, before);
    assert!(!preview.can_generate);
    assert_eq!(preview.counts.machine_only_excluded, 1);
    assert_eq!(preview.counts.mapped_formal_evidence, 0);
}

#[test]
fn curriculum_scope_catalog_filters_and_freezes_personal_snapshot() {
    let mut fixture = setup();
    add_evidence(
        &fixture,
        "unit-1-evidence",
        &fixture.knowledge[0],
        "q-unit-1",
        "2026-07-10T00:00:00.000Z",
        1.0,
        AssessmentContext::ClosedBook,
        ConfirmationLevel::TeacherCorrected,
    );
    add_evidence(
        &fixture,
        "unit-2-evidence",
        &fixture.knowledge[2],
        "q-unit-2",
        "2026-07-11T00:00:00.000Z",
        0.0,
        AssessmentContext::ClosedBook,
        ConfirmationLevel::TeacherCorrected,
    );
    let options = list_profile_scope_options(&fixture.conn).unwrap();
    let selected = options
        .iter()
        .find(|item| item.selector_public_id.as_deref() == Some(&fixture.curriculum[0]))
        .unwrap();
    assert_eq!(selected.knowledge_node_count, 2);
    assert!(options
        .iter()
        .any(|item| item.selector_kind == "knowledge_map"));

    let selection = ProfileScopeSelectionInput {
        selector_kind: "curriculum_node",
        selector_public_id: Some(&fixture.curriculum[0]),
    };
    let preview =
        preview_scoped_student_profile(&fixture.conn, &scope(&fixture), &selection).unwrap();
    assert_eq!(preview.scope_selection.knowledge_node_count, 2);
    assert_eq!(preview.counts.knowledge_node_total, 2);
    assert_eq!(preview.counts.knowledge_node_assessed, 1);
    assert_eq!(preview.counts.mapped_formal_evidence, 1);
    assert_eq!(preview.counts.out_of_scope_excluded, 1);

    let snapshot = generate_scoped_student_profile(
        &mut fixture.conn,
        &GenerateScopedStudentProfileInput {
            scope: StudentProfileScope {
                class_id: fixture.class_id,
                student_id: fixture.student_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            selection,
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    assert_eq!(
        snapshot.scope_selection.selector_public_id.as_deref(),
        Some(fixture.curriculum[0].as_str())
    );
    assert_eq!(snapshot.knowledge_metrics.len(), 2);
    assert!(snapshot
        .knowledge_metrics
        .iter()
        .all(|item| item.target_public_id != fixture.knowledge[2]));
    assert!(!snapshot.is_stale);
}

#[test]
fn unassessed_and_insufficient_are_not_needs_support() {
    let fixture = setup();
    add_evidence(
        &fixture,
        "one",
        &fixture.knowledge[0],
        "q1",
        "2026-07-10T00:00:00.000Z",
        0.0,
        AssessmentContext::ClosedBook,
        ConfirmationLevel::TeacherCorrected,
    );
    let computation = compute(&fixture.conn, &scope(&fixture)).unwrap();
    let first = computation
        .metrics
        .iter()
        .find(|metric| metric.target_public_id == fixture.knowledge[0])
        .unwrap();
    let second = computation
        .metrics
        .iter()
        .find(|metric| metric.target_public_id == fixture.knowledge[1])
        .unwrap();
    assert_eq!(first.status, "insufficient_evidence");
    assert_eq!(second.status, "unassessed");
}

#[test]
fn same_source_same_day_collapses_and_correction_does_not_erase_error() {
    let fixture = setup();
    add_evidence(
        &fixture,
        "first-error",
        &fixture.knowledge[0],
        "q1",
        "2026-07-10T00:00:00.000Z",
        0.0,
        AssessmentContext::ClosedBook,
        ConfirmationLevel::TeacherCorrected,
    );
    add_evidence(
        &fixture,
        "same-day-correction",
        &fixture.knowledge[0],
        "q1",
        "2026-07-10T08:00:00.000Z",
        1.0,
        AssessmentContext::Correction,
        ConfirmationLevel::TeacherCorrected,
    );
    let computation = compute(&fixture.conn, &scope(&fixture)).unwrap();
    let metric = computation
        .metrics
        .iter()
        .find(|metric| metric.target_public_id == fixture.knowledge[0])
        .unwrap();
    assert_eq!(metric.evidence_count, 2);
    assert_eq!(metric.independent_group_count, 1);
    assert_eq!(metric.mastery_score, Some(0.0));
    assert_eq!(metric.status, "insufficient_evidence");
}

#[test]
fn m3_correction_evidence_uses_explicit_adapter_and_keeps_low_weight() {
    let fixture = setup();
    add_correction_evidence(
        &fixture,
        "correction-1",
        &fixture.knowledge[0],
        "item-1",
        "2026-07-12T00:00:00.000Z",
        1.0,
    );
    let preview = preview_student_profile(&fixture.conn, &scope(&fixture)).unwrap();
    assert_eq!(preview.counts.mapped_formal_evidence, 1);
    assert_eq!(preview.counts.unsupported_contract_excluded, 0);
    let computation = compute(&fixture.conn, &scope(&fixture)).unwrap();
    let metric = computation
        .metrics
        .iter()
        .find(|metric| metric.target_public_id == fixture.knowledge[0])
        .unwrap();
    assert_eq!(metric.source_breakdown.get("correction"), Some(&1));
    assert!((metric.evidence[0].2 - 0.36).abs() < 0.000_001);
}

#[test]
fn sufficient_cross_date_sources_form_explainable_status() {
    let fixture = setup();
    for (index, date) in [
        "2026-07-05T00:00:00.000Z",
        "2026-07-12T00:00:00.000Z",
        "2026-07-20T00:00:00.000Z",
    ]
    .iter()
    .enumerate()
    {
        add_evidence(
            &fixture,
            &format!("pass-{index}"),
            &fixture.knowledge[0],
            &format!("q{index}"),
            date,
            1.0,
            AssessmentContext::ClosedBook,
            ConfirmationLevel::TeacherAccepted,
        );
    }
    let computation = compute(&fixture.conn, &scope(&fixture)).unwrap();
    let metric = computation
        .metrics
        .iter()
        .find(|metric| metric.target_public_id == fixture.knowledge[0])
        .unwrap();
    assert_eq!(metric.status, "stable");
    assert_eq!(metric.confidence_level, "medium");
    assert_eq!(metric.mastery_score, Some(1.0));
}

#[test]
fn snapshot_is_immutable_and_becomes_stale_after_new_evidence() {
    let mut fixture = setup();
    add_evidence(
        &fixture,
        "base",
        &fixture.knowledge[0],
        "q1",
        "2026-07-10T00:00:00.000Z",
        1.0,
        AssessmentContext::ClosedBook,
        ConfirmationLevel::TeacherAccepted,
    );
    let class_id = fixture.class_id;
    let student_id = fixture.student_id;
    let generated = generate_student_profile(
        &mut fixture.conn,
        &GenerateStudentProfileInput {
            scope: StudentProfileScope {
                class_id,
                student_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    assert!(!generated.is_stale);
    let audit_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM audit_events
                 WHERE action='profile.snapshot.generated' AND object_id=?1",
            [&generated.public_id],
            |row| row.get(0),
        )
        .unwrap();
    let outbox_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM outbox_events
                 WHERE event_type='student_profile_snapshot_created' AND aggregate_id=?1",
            [&generated.public_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audit_count, 1);
    assert_eq!(outbox_count, 1);
    assert!(fixture
        .conn
        .execute(
            "UPDATE profile_snapshots SET generated_by='tampered' WHERE public_id=?1",
            [&generated.public_id]
        )
        .is_err());
    add_evidence(
        &fixture,
        "new",
        &fixture.knowledge[0],
        "q2",
        "2026-07-15T00:00:00.000Z",
        0.0,
        AssessmentContext::ClosedBook,
        ConfirmationLevel::TeacherCorrected,
    );
    let loaded = get_student_profile(&fixture.conn, &generated.public_id)
        .unwrap()
        .unwrap();
    assert!(loaded.is_stale);
    assert_eq!(loaded.revision, 1);
}

#[test]
fn same_scope_same_contract_refresh_has_personal_trend() {
    let mut fixture = setup();
    add_evidence(
        &fixture,
        "trend-first",
        &fixture.knowledge[0],
        "q1",
        "2026-07-05T00:00:00.000Z",
        0.0,
        AssessmentContext::ClosedBook,
        ConfirmationLevel::TeacherCorrected,
    );
    let class_id = fixture.class_id;
    let student_id = fixture.student_id;
    let first = generate_student_profile(
        &mut fixture.conn,
        &GenerateStudentProfileInput {
            scope: StudentProfileScope {
                class_id,
                student_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    assert_eq!(first.trend.comparison_status, "no_comparable_baseline");
    add_evidence(
        &fixture,
        "trend-pass-2",
        &fixture.knowledge[0],
        "q2",
        "2026-07-12T00:00:00.000Z",
        1.0,
        AssessmentContext::ClosedBook,
        ConfirmationLevel::TeacherAccepted,
    );
    add_evidence(
        &fixture,
        "trend-pass-3",
        &fixture.knowledge[0],
        "q3",
        "2026-07-20T00:00:00.000Z",
        1.0,
        AssessmentContext::ClosedBook,
        ConfirmationLevel::TeacherAccepted,
    );
    let second = generate_student_profile(
        &mut fixture.conn,
        &GenerateStudentProfileInput {
            scope: StudentProfileScope {
                class_id,
                student_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    assert_eq!(second.trend.comparison_status, "comparable");
    assert_eq!(
        second.trend.previous_snapshot_public_id.as_deref(),
        Some(first.public_id.as_str())
    );
    assert_eq!(second.trend.previous_revision, Some(1));
    assert!(second
        .trend
        .changed_nodes
        .iter()
        .any(|node| node.target_public_id == fixture.knowledge[0]));
}

#[test]
fn snapshot_wrongbook_facts_are_frozen_and_do_not_change_metrics() {
    let mut fixture = setup();
    add_evidence(
        &fixture,
        "wrongbook-base",
        &fixture.knowledge[0],
        "q1",
        "2026-07-10T00:00:00.000Z",
        0.0,
        AssessmentContext::ClosedBook,
        ConfirmationLevel::TeacherCorrected,
    );
    let mut computation = compute(&fixture.conn, &scope(&fixture)).unwrap();
    computation.wrongbook_facts.push(ProfileWrongbookFactView {
        question_version_public_id: "question-version-1".into(),
        question_type: "single".into(),
        stem: "洋务运动失败的根本原因是？".into(),
        status: "corrected_once".into(),
        first_error_at: "2026-07-10T00:00:00.000Z".into(),
        last_error_at: "2026-07-10T00:00:00.000Z".into(),
        latest_response_at: "2026-07-12T00:00:00.000Z".into(),
        published_response_count: 2,
        error_response_count: 1,
        repeated_error: false,
        correction_status: Some("published".into()),
        reinforcement_status: None,
        knowledge_nodes: vec![ProfileNamedReference {
            public_id: fixture.knowledge[0].clone(),
            title: "知识点1".into(),
        }],
        ability_dimensions: Vec::new(),
    });
    let tx = fixture.conn.transaction().unwrap();
    let (public_id, _, _) = insert_snapshot(&tx, &computation, "teacher-1").unwrap();
    tx.commit().unwrap();
    let loaded = get_student_profile(&fixture.conn, &public_id)
        .unwrap()
        .unwrap();
    assert_eq!(loaded.wrongbook_summary.fact_count, 1);
    assert_eq!(loaded.wrongbook_summary.corrected_once_count, 1);
    assert_eq!(loaded.knowledge_metrics[0].evidence_count, 1);
    assert!(fixture
        .conn
        .execute(
            "UPDATE profile_wrongbook_fact_links SET status='rechecked_correct'
                 WHERE snapshot_id=(SELECT id FROM profile_snapshots WHERE public_id=?1)",
            [&public_id],
        )
        .is_err());
}

#[test]
fn ability_without_evidence_remains_unassessed() {
    let fixture = setup();
    add_evidence(
        &fixture,
        "knowledge-only",
        &fixture.knowledge[0],
        "q1",
        "2026-07-10T00:00:00.000Z",
        1.0,
        AssessmentContext::ClosedBook,
        ConfirmationLevel::TeacherAccepted,
    );
    let computation = compute(&fixture.conn, &scope(&fixture)).unwrap();
    let ability = computation
        .metrics
        .iter()
        .find(|metric| metric.target_public_id == fixture.ability)
        .unwrap();
    assert_eq!(ability.status, "unassessed");
    assert!(ability.mastery_score.is_none());
}

#[test]
fn recitation_point_adapter_projects_knowledge_but_never_ability() {
    let fixture = setup();
    add_recitation_point(
        &fixture,
        "recitation-point",
        &fixture.knowledge[0],
        Some(&fixture.ability),
        EvidenceKind::Accuracy,
        "2026-07-10T00:00:00.000Z",
        1.0,
        ConfirmationLevel::TeacherAccepted,
    );
    let computation = compute(&fixture.conn, &scope(&fixture)).unwrap();
    let knowledge = computation
        .metrics
        .iter()
        .find(|metric| metric.target_public_id == fixture.knowledge[0])
        .unwrap();
    let ability = computation
        .metrics
        .iter()
        .find(|metric| metric.target_public_id == fixture.ability)
        .unwrap();
    assert_eq!(knowledge.evidence_count, 1);
    assert_eq!(knowledge.source_breakdown.get("recitation"), Some(&1));
    assert_eq!(ability.evidence_count, 0);
    assert_eq!(computation.counts.unsupported_contract_excluded, 1);
}

#[test]
fn recitation_overall_fluency_and_retention_are_read_only_history_only() {
    let fixture = setup();
    add_recitation_history(
        &fixture,
        "overall-1",
        "recitation_overall",
        "recitation_submission",
        EvidenceKind::Accuracy,
        "2026-07-10T00:00:00.000Z",
        1.0,
    );
    add_recitation_history(
        &fixture,
        "fluency-1",
        "recitation_fluency",
        "recitation_submission",
        EvidenceKind::Fluency,
        "2026-07-10T00:00:00.000Z",
        0.72,
    );
    add_recitation_history(
        &fixture,
        "retention-1",
        "recitation_retention",
        "recitation_retention_window",
        EvidenceKind::Retention,
        "2026-07-18T00:00:00.000Z",
        1.0,
    );
    let before: i64 = fixture
        .conn
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    let preview = preview_student_profile(&fixture.conn, &scope(&fixture)).unwrap();
    let after: i64 = fixture
        .conn
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(after, before);
    assert!(!preview.can_generate);
    assert_eq!(preview.counts.mapped_formal_evidence, 0);
    assert_eq!(preview.counts.teacher_overall_excluded, 3);
    assert_eq!(preview.recitation_summary.overall_count, 1);
    assert_eq!(preview.recitation_summary.fluency_count, 1);
    assert_eq!(preview.recitation_summary.retention_count, 1);
    assert_eq!(preview.recitation_summary.latest_fluency_value, Some(0.72));
    assert_eq!(preview.recitation_summary.evidence.len(), 3);
}

#[test]
fn snapshot_freezes_recitation_history_and_new_history_marks_it_stale() {
    let mut fixture = setup();
    add_recitation_point(
        &fixture,
        "point-for-snapshot",
        &fixture.knowledge[0],
        None,
        EvidenceKind::Accuracy,
        "2026-07-10T00:00:00.000Z",
        1.0,
        ConfirmationLevel::TeacherCorrected,
    );
    add_recitation_history(
        &fixture,
        "overall-before",
        "recitation_overall",
        "recitation_submission",
        EvidenceKind::Accuracy,
        "2026-07-10T00:00:00.000Z",
        1.0,
    );
    add_recitation_history(
        &fixture,
        "retention-before",
        "recitation_retention",
        "recitation_retention_window",
        EvidenceKind::Retention,
        "2026-07-18T00:00:00.000Z",
        1.0,
    );
    let class_id = fixture.class_id;
    let student_id = fixture.student_id;
    let generated = generate_student_profile(
        &mut fixture.conn,
        &GenerateStudentProfileInput {
            scope: StudentProfileScope {
                class_id,
                student_id,
                range_start: "2026-07-01",
                range_end: "2026-07-31",
            },
            confirmed_by: "teacher-1",
        },
    )
    .unwrap();
    assert_eq!(generated.recitation_summary.overall_count, 1);
    assert_eq!(generated.recitation_summary.retention_count, 1);
    assert!(fixture
        .conn
        .execute(
            "DELETE FROM profile_recitation_evidence_links
                 WHERE snapshot_id=(SELECT id FROM profile_snapshots WHERE public_id=?1)",
            [&generated.public_id],
        )
        .is_err());

    add_recitation_history(
        &fixture,
        "overall-after",
        "recitation_overall",
        "recitation_submission",
        EvidenceKind::Accuracy,
        "2026-07-20T00:00:00.000Z",
        0.0,
    );
    let loaded = get_student_profile(&fixture.conn, &generated.public_id)
        .unwrap()
        .unwrap();
    assert!(loaded.is_stale);
    assert_eq!(loaded.recitation_summary.overall_count, 1);
    let preview = preview_student_profile(&fixture.conn, &scope(&fixture)).unwrap();
    assert_eq!(preview.recitation_summary.overall_count, 2);
}

#[test]
fn unsupported_and_non_active_recitation_contracts_fail_closed() {
    let fixture = setup();
    add_recitation_point(
        &fixture,
        "active-supported",
        &fixture.knowledge[0],
        None,
        EvidenceKind::Coverage,
        "2026-07-10T00:00:00.000Z",
        1.0,
        ConfirmationLevel::TeacherAccepted,
    );
    add_recitation_point(
        &fixture,
        "reverted-supported",
        &fixture.knowledge[0],
        None,
        EvidenceKind::Accuracy,
        "2026-07-11T00:00:00.000Z",
        1.0,
        ConfirmationLevel::TeacherAccepted,
    );
    fixture
        .conn
        .execute(
            "UPDATE learning_evidence SET state='reverted'
                 WHERE idempotency_key='reverted-supported'",
            [],
        )
        .unwrap();
    create_or_get(
        &fixture.conn,
        &NewLearningEvidence {
            idempotency_key: "unsupported-source-type",
            student_id: fixture.student_id,
            source_module: EvidenceSourceModule::Recitation,
            source_type: "recitation_fluency",
            source_ref_type: "recitation_point_review",
            source_ref_id: "unsupported-source-type",
            source_revision: 1,
            decision_ref_type: Some("recitation_review"),
            decision_ref_id: Some("unsupported-source-type"),
            decision_revision: Some(1),
            knowledge_node_id: Some(&fixture.knowledge[0]),
            ability_dimension_id: None,
            evidence_kind: EvidenceKind::Accuracy,
            value: 1.0,
            confirmation_level: ConfirmationLevel::TeacherAccepted,
            evidence_quality: 1.0,
            assessment_context: AssessmentContext::Homework,
            occurred_at: "2026-07-12T00:00:00.000Z",
            rule_version: "unsupported-v1",
            knowledge_map_version: &format!("{}:r1", fixture.map_public_id),
        },
    )
    .unwrap();
    let preview = preview_student_profile(&fixture.conn, &scope(&fixture)).unwrap();
    assert_eq!(preview.counts.mapped_formal_evidence, 1);
    assert_eq!(preview.counts.unsupported_contract_excluded, 1);
    let computation = compute(&fixture.conn, &scope(&fixture)).unwrap();
    let metric = computation
        .metrics
        .iter()
        .find(|metric| metric.target_public_id == fixture.knowledge[0])
        .unwrap();
    assert_eq!(metric.evidence_count, 1);
}
