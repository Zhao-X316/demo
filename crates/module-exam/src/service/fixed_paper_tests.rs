use super::*;
use crate::service::objective::{record_objective_observation, NewObjectiveObservation};
use crate::service::papers::{
    create_or_get_ingest_batch, decide_page_match, record_answer_region, record_page_alignment,
    record_page_quality, register_ingest_page, NewAnswerRegionRevision, NewIngestBatch,
    NewIngestPage, NewPageAlignmentRevision, NewPageMatchRevision, NewPageQualityRevision,
};
use suite_core::db::repo::artifacts::{create_or_get, NewArtifact};
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

struct Fixture {
    conn: Connection,
    batch_id: i64,
    item_id: i64,
    bound_answer_id: i64,
    attempt_one: i64,
    attempt_two: i64,
    first_document_id: i64,
}

#[allow(clippy::too_many_arguments)]
fn create_artifact(
    conn: &Connection,
    kind: ArtifactKind,
    hash_char: char,
    mime_type: &str,
    parent_artifact_id: Option<i64>,
    derivative_type: Option<&str>,
    privacy_class: PrivacyClass,
    name: &str,
) -> i64 {
    create_or_get(
        conn,
        &NewArtifact {
            kind,
            sha256: &hash_char.to_string().repeat(64),
            mime_type,
            byte_size: 128,
            original_name: Some(name),
            original_path: Some(&format!("/fixture/{name}")),
            archived_path: &format!("/archive/{name}"),
            parent_artifact_id,
            derivative_type,
            processing_version: "fixed-paper-fixture-v1",
            privacy_class,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap()
    .id
}

fn seed_base() -> Fixture {
    let conn = open_in_memory().unwrap();
    run_migrations(&conn, CORE_MIGRATIONS).unwrap();
    run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
    run_migrations(&conn, crate::exam_migrations()).unwrap();
    let hash = "a".repeat(64);
    conn.execute_batch(&format!(
            r#"INSERT INTO subjects(name) VALUES ('历史');
               INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
               INSERT INTO students(student_no,name,class_id) VALUES ('S001','学生一',1);
               INSERT INTO students(student_no,name,class_id) VALUES ('S002','学生二',1);
               INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('fixed-edition',1,'PEP','2024','中国历史八上','8','upper','active',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('fixed-map',1,1,'confirmed','2026-07-14T08:00:00.000Z',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('fixed-question','personal','teacher','unknown',0,
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES ('fixed-question-v1',1,1,'true_false','鸦片战争爆发于1840年。',1,
                         '{hash}','L3','published','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('fixed-answer-v1',1,1,'{{"schema_version":1,"correct":true}}',
                         'confirmed','2026-07-14T08:00:00.000Z','teacher',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('fixed-rubric-v1',1,1,1,'confirmed','2026-07-14T08:00:00.000Z',
                         'teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('fixed-link-v1',1,1,1,'confirmed','2026-07-14T08:00:00.000Z',
                         'teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,
                  created_by,created_at,updated_at)
                 VALUES ('fixed-assessment','固定试卷',1,'quiz','include','active','teacher',
                         '2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,
                  created_at,confirmed_by,confirmed_at)
                 VALUES ('fixed-assessment-v1',1,1,'{hash}','fixed-template-v1','confirmed',
                         '2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                  state,created_at)
                 VALUES ('fixed-item',1,1,1,1,1,0,1,
                         '{{"schema_version":1,"question_no":"1"}}','active',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES ('fixed-attempt-1',1,1,1,'image','first','ingesting',
                         '2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES ('fixed-attempt-2',1,2,1,'image','first','ingesting',
                         '2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');"#
        ))
        .unwrap();
    let batch = create_or_get_ingest_batch(
        &conn,
        &NewIngestBatch {
            assessment_version_id: 1,
            source_kind: "image_folder",
            idempotency_key: "fixed-real-batch",
            created_by: "teacher",
        },
    )
    .unwrap();
    let first_document_id = add_ready_page(&conn, batch.id, 0, 1, '1', '3', '5', true);
    add_ready_page(&conn, batch.id, 1, 2, '2', '4', '6', true);
    Fixture {
        conn,
        batch_id: batch.id,
        item_id: 1,
        bound_answer_id: 1,
        attempt_one: 1,
        attempt_two: 2,
        first_document_id,
    }
}

#[allow(clippy::too_many_arguments)]
fn add_ready_page(
    conn: &Connection,
    batch_id: i64,
    import_index: i64,
    attempt_id: i64,
    source_hash: char,
    aligned_hash: char,
    crop_hash: char,
    record_observation: bool,
) -> i64 {
    let source_name = format!("student-{attempt_id}-page-{import_index}.jpg");
    let source_id = create_artifact(
        conn,
        ArtifactKind::Image,
        source_hash,
        "image/jpeg",
        None,
        None,
        PrivacyClass::StudentSensitive,
        &source_name,
    );
    let document = register_fixed_input_document(
        conn,
        &NewFixedInputDocument {
            ingest_batch_id: batch_id,
            source_artifact_id: source_id,
            document_role: "student_work",
            source_format: "jpeg",
            import_index,
            page_count: 1,
            idempotency_key: &format!("input-{import_index}"),
            created_by: "teacher",
        },
    )
    .unwrap();
    let page = register_ingest_page(
        conn,
        &NewIngestPage {
            batch_id,
            source_artifact_id: source_id,
            import_index,
            expected_page_no: Some(1),
        },
    )
    .unwrap();
    record_page_quality(
        conn,
        &NewPageQualityRevision {
            page_id: page.id,
            blur_score: 0.01,
            glare_score: 0.01,
            brightness_score: 0.8,
            perspective_score: 0.99,
            rotation_degrees: 0.0,
            crop_complete: true,
            result: "pass",
            issue_codes_json: r#"{"schema_version":1,"codes":[]}"#,
            checked_by_type: "rule",
            checked_by: None,
        },
    )
    .unwrap();
    decide_page_match(
        conn,
        &NewPageMatchRevision {
            page_id: page.id,
            attempt_id: Some(attempt_id),
            page_no: Some(1),
            student_confidence: Some(0.99),
            page_no_confidence: Some(0.99),
            template_confidence: Some(0.99),
            decision: "teacher_confirmed",
            reason_code: None,
            confirmed_by: Some("teacher"),
        },
    )
    .unwrap();
    let aligned_id = create_artifact(
        conn,
        ArtifactKind::Page,
        aligned_hash,
        "image/jpeg",
        Some(source_id),
        Some("aligned_page"),
        PrivacyClass::StudentSensitive,
        &format!("aligned-{attempt_id}.jpg"),
    );
    record_page_alignment(
        conn,
        &NewPageAlignmentRevision {
            page_id: page.id,
            template_version: "fixed-template-v1",
            transform_json: r#"{"schema_version":1,"matrix":[1,0,0,0,1,0,0,0,1]}"#,
            confidence: 0.99,
            aligned_artifact_id: Some(aligned_id),
            decision: "teacher_confirmed",
            reason_code: None,
            confirmed_by: Some("teacher"),
        },
    )
    .unwrap();
    let crop_id = create_artifact(
        conn,
        ArtifactKind::Crop,
        crop_hash,
        "image/jpeg",
        Some(aligned_id),
        Some("answer_region"),
        PrivacyClass::StudentSensitive,
        &format!("crop-{attempt_id}.jpg"),
    );
    let region = record_answer_region(
        conn,
        &NewAnswerRegionRevision {
            page_id: page.id,
            assessment_item_id: 1,
            region_index: 0,
            bbox_json: r#"{"schema_version":1,"x":0.1,"y":0.1,"width":0.3,"height":0.2}"#,
            crop_artifact_id: Some(crop_id),
            mapping_confidence: Some(0.99),
            decision: "teacher_confirmed",
            reason_code: None,
            confirmed_by: Some("teacher"),
        },
    )
    .unwrap();
    if record_observation {
        record_objective_observation(
            conn,
            &NewObjectiveObservation {
                answer_region_revision_id: region.id,
                source_kind: "fixed_fixture",
                result_state: "recognized",
                observed_answer_json: Some(r#"{"schema_version":1,"selected":true}"#),
                confidence: Some(0.99),
                ai_run_id: None,
                failure_meta_json: None,
                idempotency_key: &format!("observation-{attempt_id}-{import_index}"),
            },
        )
        .unwrap();
    }
    document.id
}

fn preflight(fixture: &Fixture) -> FixedPaperPreflightRevision {
    preflight_fixed_paper_batch(
        &fixture.conn,
        &FixedPaperPreflightInput {
            ingest_batch_id: fixture.batch_id,
            expected_pages_per_attempt: 1,
            created_by_type: "system",
            created_by: None,
        },
    )
    .unwrap()
}

#[test]
fn jpeg_and_pdf_input_registration_is_idempotent_and_role_checked() {
    let fixture = seed_base();
    let existing = get_input_document(&fixture.conn, fixture.first_document_id)
        .unwrap()
        .unwrap();
    let repeated = register_fixed_input_document(
        &fixture.conn,
        &NewFixedInputDocument {
            ingest_batch_id: fixture.batch_id,
            source_artifact_id: existing.source_artifact_id,
            document_role: "student_work",
            source_format: "jpeg",
            import_index: 0,
            page_count: 1,
            idempotency_key: "input-0",
            created_by: "teacher",
        },
    )
    .unwrap();
    assert_eq!(existing.id, repeated.id);

    let pdf_id = create_artifact(
        &fixture.conn,
        ArtifactKind::Document,
        '7',
        "application/pdf",
        None,
        None,
        PrivacyClass::StudentSensitive,
        "student-batch.pdf",
    );
    let pdf = register_fixed_input_document(
        &fixture.conn,
        &NewFixedInputDocument {
            ingest_batch_id: fixture.batch_id,
            source_artifact_id: pdf_id,
            document_role: "student_work",
            source_format: "pdf",
            import_index: 2,
            page_count: 4,
            idempotency_key: "input-pdf",
            created_by: "teacher",
        },
    )
    .unwrap();
    assert_eq!(pdf.page_count, 4);
    assert!(register_fixed_input_document(
        &fixture.conn,
        &NewFixedInputDocument {
            ingest_batch_id: fixture.batch_id,
            source_artifact_id: pdf_id,
            document_role: "answer_source",
            source_format: "pdf",
            import_index: 0,
            page_count: 4,
            idempotency_key: "wrong-privacy-role",
            created_by: "teacher",
        }
    )
    .is_err());
}

#[test]
fn office_documents_are_answer_sources_not_student_work() {
    let fixture = seed_base();
    for (index, (format, mime_type, hash_char)) in [
        (
            "docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            '8',
        ),
        (
            "xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            '9',
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let artifact_id = create_artifact(
            &fixture.conn,
            ArtifactKind::Document,
            hash_char,
            mime_type,
            None,
            None,
            PrivacyClass::TeachingContent,
            &format!("answer.{format}"),
        );
        let answer = register_fixed_input_document(
            &fixture.conn,
            &NewFixedInputDocument {
                ingest_batch_id: fixture.batch_id,
                source_artifact_id: artifact_id,
                document_role: "answer_source",
                source_format: "text",
                import_index: index as i64,
                page_count: 1,
                idempotency_key: &format!("answer-{format}"),
                created_by: "teacher",
            },
        )
        .unwrap();
        assert_eq!(answer.source_format, "text");
        assert!(register_fixed_input_document(
            &fixture.conn,
            &NewFixedInputDocument {
                ingest_batch_id: fixture.batch_id,
                source_artifact_id: artifact_id,
                document_role: "student_work",
                source_format: "text",
                import_index: index as i64,
                page_count: 1,
                idempotency_key: &format!("student-{format}"),
                created_by: "teacher",
            }
        )
        .is_err());
    }
}

#[test]
fn confirmed_contiguous_groups_and_current_answers_route_to_batch_confirm() {
    let fixture = seed_base();
    let first = preflight(&fixture);
    let repeated = preflight(&fixture);
    assert_eq!(first.id, repeated.id);
    assert_eq!(first.route, "ready_for_batch_confirm");
    assert_eq!((first.target_count, first.ready_count), (2, 2));
    assert_eq!((first.review_count, first.blocked_count), (0, 0));
    let groups: Value = serde_json::from_str(&first.grouping_json).unwrap();
    assert_eq!(groups["group_count"], 2);
    let publications: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_grade_publications_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(publications, 0, "预检不得确认或发布成绩");
}

#[test]
fn quality_uncertainty_routes_to_review_without_overriding_identity() {
    let fixture = seed_base();
    let second_page_id: i64 = fixture
        .conn
        .query_row(
            "SELECT id FROM exam_ingest_pages_v2 WHERE batch_id=?1 AND import_index=1",
            [fixture.batch_id],
            |row| row.get(0),
        )
        .unwrap();
    record_page_quality(
        &fixture.conn,
        &NewPageQualityRevision {
            page_id: second_page_id,
            blur_score: 0.8,
            glare_score: 0.1,
            brightness_score: 0.5,
            perspective_score: 0.8,
            rotation_degrees: 0.0,
            crop_complete: true,
            result: "needs_review",
            issue_codes_json: r#"{"schema_version":1,"codes":["BLUR"]}"#,
            checked_by_type: "rule",
            checked_by: None,
        },
    )
    .unwrap();
    let result = preflight(&fixture);
    assert_eq!(result.route, "review_required");
    assert!(result
        .reason_codes_json
        .contains("PAGE_QUALITY_REVIEW_REQUIRED"));
    assert_eq!(result.blocked_count, 0);
}

#[test]
fn noncontiguous_student_pages_are_blocked() {
    let fixture = seed_base();
    add_ready_page(
        &fixture.conn,
        fixture.batch_id,
        2,
        fixture.attempt_one,
        '8',
        '9',
        'b',
        false,
    );
    let result = preflight_fixed_paper_batch(
        &fixture.conn,
        &FixedPaperPreflightInput {
            ingest_batch_id: fixture.batch_id,
            expected_pages_per_attempt: 2,
            created_by_type: "teacher",
            created_by: Some("teacher"),
        },
    )
    .unwrap();
    assert_eq!(result.route, "blocked");
    assert!(result
        .reason_codes_json
        .contains("STUDENT_PAGES_NONCONTIGUOUS"));
}

fn add_answer_version(conn: &Connection, revision: i64, selected: bool, public_id: &str) -> i64 {
    conn.execute(
        "INSERT INTO k1_answer_key_versions
             (public_id,question_version_id,revision,answer_json,state,created_at,
              confirmed_by,confirmed_at)
             VALUES (?1,1,?2,?3,'confirmed','2026-07-14T08:01:00.000Z',
                     'teacher','2026-07-14T08:01:00.000Z')",
        (
            public_id,
            revision,
            serde_json::json!({"schema_version":1,"correct":selected}).to_string(),
        ),
    )
    .unwrap();
    conn.last_insert_rowid()
}

#[test]
fn higher_answer_authority_wins_and_same_level_conflict_blocks_when_needed() {
    let fixture = seed_base();
    let false_answer = add_answer_version(&fixture.conn, 2, false, "fixed-answer-v2");
    let true_answer = add_answer_version(&fixture.conn, 3, true, "fixed-answer-v3");
    let answer_artifact_id = create_artifact(
        &fixture.conn,
        ArtifactKind::Image,
        'c',
        "image/jpeg",
        None,
        None,
        PrivacyClass::TeachingContent,
        "official-answer.jpg",
    );
    register_fixed_input_document(
        &fixture.conn,
        &NewFixedInputDocument {
            ingest_batch_id: fixture.batch_id,
            source_artifact_id: answer_artifact_id,
            document_role: "answer_source",
            source_format: "jpeg",
            import_index: 0,
            page_count: 1,
            idempotency_key: "answer-source",
            created_by: "teacher",
        },
    )
    .unwrap();
    for (key, answer_id, answer_json) in [
        (
            "official-false",
            false_answer,
            r#"{"schema_version":1,"correct":false}"#,
        ),
        (
            "official-true",
            true_answer,
            r#"{"schema_version":1,"correct":true}"#,
        ),
    ] {
        record_answer_authority_candidate(
            &fixture.conn,
            &NewAnswerAuthorityCandidate {
                ingest_batch_id: fixture.batch_id,
                assessment_item_id: fixture.item_id,
                source_kind: "uploaded_official",
                answer_key_version_id: Some(answer_id),
                source_artifact_id: Some(answer_artifact_id),
                candidate_answer_json: answer_json,
                source_anchor_json: r#"{"schema_version":1,"page":1,"question_no":"1"}"#,
                teacher_confirmed: true,
                idempotency_key: key,
                created_by_type: "teacher",
                created_by: Some("teacher"),
            },
        )
        .unwrap();
    }
    let bound_wins = preflight(&fixture);
    assert_eq!(bound_wins.route, "ready_for_batch_confirm");
    assert!(bound_wins
        .answer_authority_json
        .contains("assessment_bound"));

    fixture
        .conn
        .execute(
            "UPDATE k1_answer_key_versions SET state='retired' WHERE id=?1",
            [fixture.bound_answer_id],
        )
        .unwrap();
    let conflict = preflight(&fixture);
    assert_eq!(conflict.route, "blocked");
    assert!(conflict
        .reason_codes_json
        .contains("ANSWER_SAME_LEVEL_CONFLICT"));
}

#[test]
fn ai_draft_never_becomes_formal_answer_and_snapshot_update_is_atomic() {
    let fixture = seed_base();
    let initial = preflight(&fixture);
    assert_eq!(initial.revision, 1);
    record_answer_authority_candidate(
        &fixture.conn,
        &NewAnswerAuthorityCandidate {
            ingest_batch_id: fixture.batch_id,
            assessment_item_id: fixture.item_id,
            source_kind: "ai_draft",
            answer_key_version_id: None,
            source_artifact_id: None,
            candidate_answer_json: r#"{"schema_version":1,"selected":false}"#,
            source_anchor_json: r#"{"schema_version":1,"generator":"fixture"}"#,
            teacher_confirmed: false,
            idempotency_key: "ai-draft",
            created_by_type: "system",
            created_by: None,
        },
    )
    .unwrap();
    fixture
        .conn
        .execute(
            "UPDATE k1_answer_key_versions SET state='retired' WHERE id=?1",
            [fixture.bound_answer_id],
        )
        .unwrap();
    let failed = preflight_fixed_paper_batch_impl(
        &fixture.conn,
        &FixedPaperPreflightInput {
            ingest_batch_id: fixture.batch_id,
            expected_pages_per_attempt: 1,
            created_by_type: "system",
            created_by: None,
        },
        true,
    );
    assert!(failed.is_err());
    let active: (i64, String) = fixture
        .conn
        .query_row(
            "SELECT revision,route FROM exam_fixed_preflight_revisions_v2
                 WHERE ingest_batch_id=?1 AND state='active'",
            [fixture.batch_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(active, (1, "ready_for_batch_confirm".into()));

    let blocked = preflight(&fixture);
    assert_eq!(blocked.route, "blocked");
    assert!(blocked
        .reason_codes_json
        .contains("ANSWER_AUTHORITY_MISSING"));
}

#[test]
fn preflight_never_touches_teacher_confirmation_or_publication() {
    let fixture = seed_base();
    let result = preflight(&fixture);
    assert_eq!(result.ready_count, 2);
    let counts: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_publications_v2),
                        (SELECT COUNT(*) FROM learning_evidence)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0));
    assert_ne!(fixture.attempt_one, fixture.attempt_two);
}
