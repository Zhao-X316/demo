use super::*;
use crate::objective_recognition::{
    ObjectiveQuestionType, ObjectiveRecognitionErrorCode, ObjectiveRecognitionFailure,
    ObjectiveRecognitionOutput, ObjectiveRecognitionState, ObjectiveRecognizedAnswer,
    ObjectiveRecognizerDescriptor,
};
use suite_core::db::repo::ai_runs::NewAiRun;
use suite_core::db::repo::artifacts::{create_or_get, NewArtifact};
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

struct Fixture {
    conn: Connection,
    region_one: i64,
    region_two: i64,
    attempt_one: i64,
    attempt_two: i64,
}

fn artifact(
    conn: &Connection,
    kind: ArtifactKind,
    digit: char,
    parent_artifact_id: Option<i64>,
    derivative_type: Option<&str>,
) -> i64 {
    create_or_get(
        conn,
        &NewArtifact {
            kind,
            sha256: &digit.to_string().repeat(64),
            mime_type: "image/png",
            byte_size: 128,
            original_name: None,
            original_path: None,
            archived_path: &format!("/fixture/objective-{digit}.png"),
            parent_artifact_id,
            derivative_type,
            processing_version: "objective-fixture-v1",
            privacy_class: PrivacyClass::StudentSensitive,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap()
    .id
}

fn add_page_chain(conn: &Connection, attempt_id: i64, index: i64, digit: char) -> i64 {
    let page_artifact = artifact(conn, ArtifactKind::Page, digit, None, None);
    let aligned_artifact = artifact(
        conn,
        ArtifactKind::Page,
        char::from_u32(digit as u32 + 1).unwrap(),
        Some(page_artifact),
        Some("page_alignment"),
    );
    let crop_artifact = artifact(
        conn,
        ArtifactKind::Crop,
        char::from_u32(digit as u32 + 2).unwrap(),
        Some(aligned_artifact),
        Some("answer_region"),
    );
    let now = "2026-07-14T08:00:00.000Z";
    conn.execute(
        "INSERT INTO exam_ingest_batches_v2
             (public_id,assessment_version_id,source_kind,idempotency_key,state,
              created_by,created_at,updated_at)
             VALUES (?1,1,'fixed_fixture',?2,'ready','teacher',?3,?3)",
        (
            format!("objective-batch-{index}"),
            format!("batch-{index}"),
            now,
        ),
    )
    .unwrap();
    let batch_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO exam_ingest_pages_v2
             (public_id,batch_id,source_artifact_id,import_index,expected_page_no,state,
              created_at,updated_at)
             VALUES (?1,?2,?3,0,1,'segmented',?4,?4)",
        (
            format!("objective-page-{index}"),
            batch_id,
            page_artifact,
            now,
        ),
    )
    .unwrap();
    let page_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO exam_page_match_revisions_v2
             (public_id,page_id,revision,attempt_id,page_no,student_confidence,
              page_no_confidence,template_confidence,decision,confirmed_by,state,created_at)
             VALUES (?1,?2,1,?3,1,0.99,0.99,0.99,'teacher_confirmed','teacher','active',?4)",
        (format!("objective-match-{index}"), page_id, attempt_id, now),
    )
    .unwrap();
    let match_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO exam_page_alignment_revisions_v2
             (public_id,page_id,revision,match_revision_id,template_version,transform_json,
              confidence,aligned_artifact_id,decision,confirmed_by,state,created_at)
             VALUES (?1,?2,1,?3,'fixture-v1',
                     '{\"schema_version\":1,\"matrix\":[1,0,0,1,0,0]}',0.99,?4,
                     'teacher_confirmed','teacher','active',?5)",
        (
            format!("objective-alignment-{index}"),
            page_id,
            match_id,
            aligned_artifact,
            now,
        ),
    )
    .unwrap();
    let alignment_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO exam_answer_region_revisions_v2
             (public_id,page_id,assessment_item_id,region_index,revision,
              alignment_revision_id,bbox_json,crop_artifact_id,mapping_confidence,
              decision,confirmed_by,state,created_at)
             VALUES (?1,?2,1,0,1,?3,
                     '{\"schema_version\":1,\"x\":0.1,\"y\":0.1,\"width\":0.2,\"height\":0.1}',
                     ?4,0.99,'teacher_confirmed','teacher','active',?5)",
        (
            format!("objective-region-{index}"),
            page_id,
            alignment_id,
            crop_artifact,
            now,
        ),
    )
    .unwrap();
    conn.last_insert_rowid()
}

fn setup() -> Fixture {
    let conn = open_in_memory().unwrap();
    run_migrations(&conn, CORE_MIGRATIONS).unwrap();
    run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
    run_migrations(&conn, crate::exam_migrations()).unwrap();
    let hash = "a".repeat(64);
    conn.execute_batch(&format!(
            r#"INSERT INTO subjects(name) VALUES ('历史');
               INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
               INSERT INTO students(student_no,name,class_id) VALUES ('S001','小林',1);
               INSERT INTO students(student_no,name,class_id) VALUES ('S002','小周',1);
               INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('objective-edition',1,'PEP','2024','八上','8','upper','active',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('objective-map',1,1,'confirmed','2026-07-14T08:00:00.000Z',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('objective-question','personal','teacher','unknown',0,
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES ('objective-question-v1',1,1,'true_false','鸦片战争爆发于1840年。',1,
                         '{hash}','L2','published','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('objective-answer-v1',1,1,'{{"schema_version":1,"correct":true}}',
                         'confirmed','2026-07-14T08:00:00.000Z','teacher',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('objective-rubric-v1',1,1,1,'confirmed',
                         '2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('objective-link-set',1,1,1,'confirmed',
                         '2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,
                  created_by,created_at,updated_at)
                 VALUES ('objective-assessment','判断题测验',1,'quiz','include','active','teacher',
                         '2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('objective-assessment-v1',1,1,'{hash}','confirmed',
                         '2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                  state,created_at)
                 VALUES ('objective-item',1,1,1,1,1,0,1,
                         '{{"schema_version":1,"question_no":"1"}}','active',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES ('objective-attempt-1',1,1,1,'image','first','ingesting',
                         '2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES ('objective-attempt-2',1,2,1,'image','first','ingesting',
                         '2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');"#
        ))
        .unwrap();
    let region_one = add_page_chain(&conn, 1, 1, '1');
    let region_two = add_page_chain(&conn, 2, 2, '4');
    Fixture {
        conn,
        region_one,
        region_two,
        attempt_one: 1,
        attempt_two: 2,
    }
}

fn observed<'a>(region_id: i64, key: &'a str, confidence: f64) -> NewObjectiveObservation<'a> {
    NewObjectiveObservation {
        answer_region_revision_id: region_id,
        source_kind: "fixed_fixture",
        result_state: if confidence >= DEFAULT_STRICT_BATCH_CONFIDENCE {
            "recognized"
        } else {
            "low_confidence"
        },
        observed_answer_json: Some(r#"{"schema_version":1,"selected":true}"#),
        confidence: Some(confidence),
        ai_run_id: None,
        failure_meta_json: None,
        idempotency_key: key,
    }
}

fn region_artifact(conn: &Connection, region_id: i64) -> (i64, String) {
    conn.query_row(
        "SELECT a.id,a.sha256
             FROM exam_answer_region_revisions_v2 r
             JOIN artifacts a ON a.id=r.crop_artifact_id
             WHERE r.id=?1",
        [region_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .unwrap()
}

fn succeeded_omr_run(
    conn: &Connection,
    region_id: i64,
    artifact_id: i64,
    artifact_sha256: &str,
    key: &str,
) -> i64 {
    let input_hash = "b".repeat(64);
    let descriptor = ObjectiveRecognizerDescriptor {
        provider: "local-template".into(),
        model_name: "fixed-template-omr".into(),
        model_version: "1".into(),
        config_version: "config-v1".into(),
        rule_version: "rule-v1".into(),
    };
    let output = ObjectiveRecognitionOutput {
        schema_version: 1,
        answer_region_revision_id: region_id,
        input_artifact_id: artifact_id,
        input_artifact_sha256: artifact_sha256.into(),
        input_hash: input_hash.clone(),
        question_type: ObjectiveQuestionType::TrueFalse,
        template_version: "fixture-v1".into(),
        descriptor,
        result_state: ObjectiveRecognitionState::Recognized,
        answer: Some(ObjectiveRecognizedAnswer::TrueFalse { selected: true }),
        confidence: Some(0.99),
        issue_codes: Vec::new(),
        measurements: Vec::new(),
    };
    let output_json = output.to_json().unwrap();
    let run = ai_runs::create_or_get(
        conn,
        &NewAiRun {
            idempotency_key: key,
            run_type: "omr",
            source_module: "exam",
            business_ref_type: "answer_region_revision",
            business_ref_id: &region_id.to_string(),
            input_artifact_id: Some(artifact_id),
            provider: "local-template",
            model_name: "fixed-template-omr",
            model_version: "1",
            config_version: "config-v1",
            prompt_or_rule_version: "rule-v1",
            input_hash: &input_hash,
            retry_of_ai_run_id: None,
        },
    )
    .unwrap();
    ai_runs::start(conn, run.id, "2026-07-14T08:00:00.000Z", None).unwrap();
    ai_runs::finalize_succeeded(
        conn,
        run.id,
        &hashing::sha256_hex(output_json.as_bytes()),
        Some(0.99),
        &output_json,
        "2026-07-14T08:00:01.000Z",
    )
    .unwrap();
    run.id
}

#[test]
fn observation_is_idempotent_and_never_confirms_or_publishes_by_itself() {
    let fixture = setup();
    let first = record_objective_observation(
        &fixture.conn,
        &observed(fixture.region_one, "observe-one", 0.99),
    )
    .unwrap();
    let repeated = record_objective_observation(
        &fixture.conn,
        &observed(fixture.region_one, "observe-one", 0.99),
    )
    .unwrap();
    assert_eq!(first.observation.id, repeated.observation.id);
    assert_eq!(first.suggestion.outcome, "correct");
    assert!(first.suggestion.batch_eligible);
    let before: (i64, i64) = fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_publications_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(before, (0, 0));

    let decision =
        accept_objective_suggestion(&fixture.conn, first.suggestion.id, "teacher").unwrap();
    let repeated_decision =
        accept_objective_suggestion(&fixture.conn, first.suggestion.id, "teacher").unwrap();
    assert_eq!(decision.id, repeated_decision.id);
    assert_eq!(decision.teacher_score, 1.0);
    let publication_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_grade_publications_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(publication_count, 0, "老师单题确认后仍需显式发布");
}

#[test]
fn omr_run_maps_through_existing_observation_and_rejects_cross_region_binding() {
    let fixture = setup();
    let (artifact_id, artifact_sha256) = region_artifact(&fixture.conn, fixture.region_one);
    let run_id = succeeded_omr_run(
        &fixture.conn,
        fixture.region_one,
        artifact_id,
        &artifact_sha256,
        "omr-run-success",
    );
    let result =
        record_omr_ai_run_observation(&fixture.conn, run_id, "omr-observation-success").unwrap();
    assert_eq!(result.observation.source_kind, "omr");
    assert_eq!(result.observation.ai_run_id, Some(run_id));
    assert_eq!(result.suggestion.outcome, "correct");

    let wrong_run_id = succeeded_omr_run(
        &fixture.conn,
        fixture.region_two,
        artifact_id,
        &artifact_sha256,
        "omr-run-cross-region",
    );
    assert!(record_omr_ai_run_observation(
        &fixture.conn,
        wrong_run_id,
        "omr-observation-cross-region",
    )
    .is_err());
}

#[test]
fn failed_omr_run_becomes_unscored_exception_with_same_sanitized_error() {
    let fixture = setup();
    let (artifact_id, _) = region_artifact(&fixture.conn, fixture.region_two);
    let input_hash = "c".repeat(64);
    let region_ref = fixture.region_two.to_string();
    let run = ai_runs::create_or_get(
        &fixture.conn,
        &NewAiRun {
            idempotency_key: "omr-run-failed",
            run_type: "omr",
            source_module: "exam",
            business_ref_type: "answer_region_revision",
            business_ref_id: &region_ref,
            input_artifact_id: Some(artifact_id),
            provider: "local-template",
            model_name: "fixed-template-omr",
            model_version: "1",
            config_version: "config-v1",
            prompt_or_rule_version: "rule-v1",
            input_hash: &input_hash,
            retry_of_ai_run_id: None,
        },
    )
    .unwrap();
    ai_runs::start(&fixture.conn, run.id, "2026-07-14T08:00:00.000Z", None).unwrap();
    let failure = ObjectiveRecognitionFailure {
        schema_version: 1,
        code: ObjectiveRecognitionErrorCode::TemplateMismatch,
        safe_message: "模板锚点不匹配".into(),
        retryable: false,
    };
    let failure_json = failure.to_json().unwrap();
    ai_runs::finalize_failed(
        &fixture.conn,
        run.id,
        &failure_json,
        "2026-07-14T08:00:01.000Z",
    )
    .unwrap();

    let result =
        record_omr_ai_run_observation(&fixture.conn, run.id, "omr-observation-failed").unwrap();
    assert_eq!(result.observation.result_state, "failed");
    assert_eq!(
        result.observation.failure_meta_json.as_deref(),
        Some(failure_json.as_str())
    );
    assert_eq!(result.suggestion.outcome, "unscored");
    assert_eq!(
        result.suggestion.exclusion_reason.as_deref(),
        Some("RECOGNITION_FAILED")
    );
}

#[test]
fn teacher_correction_completes_an_unscored_exception_idempotently() {
    let fixture = setup();
    let blank = record_objective_observation(
        &fixture.conn,
        &NewObjectiveObservation {
            answer_region_revision_id: fixture.region_one,
            source_kind: "fixed_fixture",
            result_state: "blank",
            observed_answer_json: None,
            confidence: None,
            ai_run_id: None,
            failure_meta_json: None,
            idempotency_key: "teacher-correct-blank",
        },
    )
    .unwrap();
    assert_eq!(blank.suggestion.outcome, "unscored");

    let first = correct_objective_suggestion(
        &fixture.conn,
        blank.suggestion.id,
        0.5,
        Some("已查看原始题区，按部分作答记分"),
        "teacher",
    )
    .unwrap();
    let repeated = correct_objective_suggestion(
        &fixture.conn,
        blank.suggestion.id,
        0.5,
        Some("已查看原始题区，按部分作答记分"),
        "teacher",
    )
    .unwrap();
    assert_eq!(first.id, repeated.id);
    assert_eq!(first.teacher_score, 0.5);
    assert_eq!(first.confirmation_level, "teacher_corrected");

    let workbench = list_objective_workbench(&fixture.conn, Some(1), 100).unwrap();
    assert!(workbench.rows[0].current_suggestion_confirmed);
    assert_eq!(workbench.rows[0].teacher_score, Some(0.5));
    assert_eq!(
        workbench.rows[0].confirmation_level.as_deref(),
        Some("teacher_corrected")
    );
    assert!(workbench.attempts[0].can_publish);
}

#[test]
fn strict_batch_confirms_only_high_confidence_and_records_exclusions() {
    let fixture = setup();
    let high = record_objective_observation(
        &fixture.conn,
        &observed(fixture.region_one, "observe-high", 0.99),
    )
    .unwrap();
    let low = record_objective_observation(
        &fixture.conn,
        &observed(fixture.region_two, "observe-low", 0.72),
    )
    .unwrap();
    assert_eq!(low.suggestion.outcome, "correct");
    assert!(!low.suggestion.batch_eligible);
    let review = StrictBatchReview {
        suggestion_ids: &[high.suggestion.id, low.suggestion.id],
        confidence_threshold: 0.95,
        reviewed_by: "teacher",
        idempotency_key: "strict-review-1",
    };
    let batch = strict_batch_accept(&fixture.conn, &review).unwrap();
    let repeated = strict_batch_accept(&fixture.conn, &review).unwrap();
    assert_eq!(batch.id, repeated.id);
    assert_eq!(batch.confirmed_count, 1);
    assert_eq!(batch.excluded_count, 1);
    assert_eq!(batch.items[0].outcome, "confirmed");
    assert_eq!(
        batch.items[1].reason_code.as_deref(),
        Some("LOW_CONFIDENCE")
    );
    let states: (String, String) = fixture
        .conn
        .query_row(
            "SELECT (SELECT state FROM exam_attempts_v2 WHERE id=?1),
                        (SELECT state FROM exam_attempts_v2 WHERE id=?2)",
            (fixture.attempt_one, fixture.attempt_two),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(states.0, "ready_to_publish");
    assert_eq!(states.1, "ingesting");
    let publication_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_grade_publications_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(publication_count, 0);
}

#[test]
fn blank_and_altered_are_unscored_and_atomic_batch_failure_leaves_no_half_state() {
    let fixture = setup();
    let blank = record_objective_observation(
        &fixture.conn,
        &NewObjectiveObservation {
            answer_region_revision_id: fixture.region_one,
            source_kind: "fixed_fixture",
            result_state: "blank",
            observed_answer_json: None,
            confidence: Some(0.99),
            ai_run_id: None,
            failure_meta_json: None,
            idempotency_key: "blank-one",
        },
    )
    .unwrap();
    assert_eq!(blank.suggestion.outcome, "unscored");
    assert!(accept_objective_suggestion(&fixture.conn, blank.suggestion.id, "teacher").is_err());
    let altered = record_objective_observation(
        &fixture.conn,
        &NewObjectiveObservation {
            answer_region_revision_id: fixture.region_two,
            source_kind: "fixed_fixture",
            result_state: "altered",
            observed_answer_json: Some(r#"{"schema_version":1,"selected":true}"#),
            confidence: Some(0.98),
            ai_run_id: None,
            failure_meta_json: None,
            idempotency_key: "altered-two",
        },
    )
    .unwrap();
    assert_eq!(
        altered.suggestion.exclusion_reason.as_deref(),
        Some("ALTERED")
    );

    let fixture = setup();
    let one = record_objective_observation(
        &fixture.conn,
        &observed(fixture.region_one, "atomic-one", 0.99),
    )
    .unwrap();
    let two = record_objective_observation(
        &fixture.conn,
        &observed(fixture.region_two, "atomic-two", 0.99),
    )
    .unwrap();
    fixture
        .conn
        .execute_batch(&format!(
            "CREATE TRIGGER fail_second_batch_item
                 BEFORE INSERT ON exam_objective_review_batch_items_v2
                 WHEN NEW.suggestion_id={}
                 BEGIN SELECT RAISE(ABORT, 'injected batch failure'); END;",
            two.suggestion.id
        ))
        .unwrap();
    assert!(strict_batch_accept(
        &fixture.conn,
        &StrictBatchReview {
            suggestion_ids: &[one.suggestion.id, two.suggestion.id],
            confidence_threshold: 0.95,
            reviewed_by: "teacher",
            idempotency_key: "atomic-review",
        },
    )
    .is_err());
    let counts: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM exam_objective_review_batches_v2),
                        (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_decision_objective_sources_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[test]
fn workbench_read_model_tracks_review_totals_and_explicit_publication() {
    let fixture = setup();
    let high = record_objective_observation(
        &fixture.conn,
        &observed(fixture.region_one, "workbench-high", 0.99),
    )
    .unwrap();
    record_objective_observation(
        &fixture.conn,
        &observed(fixture.region_two, "workbench-low", 0.72),
    )
    .unwrap();

    let initial = list_objective_workbench(&fixture.conn, None, 100).unwrap();
    assert_eq!(initial.rows.len(), 2);
    assert_eq!(initial.attempts.len(), 2);
    assert_eq!(initial.rows[0].question_no, "1");
    assert_eq!(initial.rows[0].observation_state, "recognized");
    assert!(!initial.rows[0].current_suggestion_confirmed);
    assert_eq!(
        initial.rows[1].exclusion_reason.as_deref(),
        Some("LOW_CONFIDENCE")
    );

    accept_objective_suggestion(&fixture.conn, high.suggestion.id, "teacher").unwrap();
    let reviewed = list_objective_workbench(&fixture.conn, Some(1), 100).unwrap();
    let reviewed_row = reviewed
        .rows
        .iter()
        .find(|row| row.suggestion_id == high.suggestion.id)
        .unwrap();
    assert!(reviewed_row.current_suggestion_confirmed);
    assert_eq!(reviewed_row.teacher_score, Some(1.0));
    let reviewed_attempt = reviewed
        .attempts
        .iter()
        .find(|attempt| attempt.attempt_id == fixture.attempt_one)
        .unwrap();
    assert_eq!(reviewed_attempt.confirmed_count, 1);
    assert_eq!(reviewed_attempt.teacher_total_score, 1.0);
    assert!(reviewed_attempt.can_publish);
    assert_eq!(reviewed_attempt.published_total_score, None);

    super::super::assessment::publish_attempt(&fixture.conn, fixture.attempt_one, "teacher")
        .unwrap();
    let published = list_objective_workbench(&fixture.conn, Some(1), 100).unwrap();
    let published_attempt = published
        .attempts
        .iter()
        .find(|attempt| attempt.attempt_id == fixture.attempt_one)
        .unwrap();
    assert_eq!(published_attempt.attempt_state, "published");
    assert!(!published_attempt.can_publish);
    assert_eq!(published_attempt.published_total_score, Some(1.0));
}

#[test]
fn scoped_workspace_read_excludes_other_attempts_before_limit() {
    let fixture = setup();
    record_objective_observation(
        &fixture.conn,
        &observed(fixture.region_one, "scope-first", 0.99),
    )
    .unwrap();
    record_objective_observation(
        &fixture.conn,
        &observed(fixture.region_two, "scope-second", 0.98),
    )
    .unwrap();
    let all = list_objective_workbench(&fixture.conn, None, 100).unwrap();
    assert_eq!(all.rows.len(), 2);
    let target = all.rows[1].attempt_id;
    fixture.conn.execute_batch("PRAGMA query_only=ON").unwrap();
    let scoped =
        list_objective_workbench_scoped(&fixture.conn, Some(1), 1, Some(&[target])).unwrap();
    assert_eq!(scoped.rows.len(), 1);
    assert_eq!(scoped.rows[0].attempt_id, target);
    assert_eq!(scoped.attempts.len(), 1);
    assert_eq!(scoped.attempts[0].attempt_id, target);
    let empty = list_objective_workbench_scoped(&fixture.conn, None, 1, Some(&[])).unwrap();
    assert!(empty.rows.is_empty() && empty.attempts.is_empty());
}
