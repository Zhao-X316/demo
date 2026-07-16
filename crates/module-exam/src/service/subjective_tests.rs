use rusqlite::Connection;
use suite_core::db::repo::{ai_runs, artifacts};
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
use suite_core::domain::{hashing, time};
use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

use super::papers::{
    self, NewAnswerRegionRevision, NewIngestBatch, NewIngestPage, NewPageAlignmentRevision,
    NewPageMatchRevision, NewPageQualityRevision,
};
use super::subjective;
use crate::dictation::DictationRecognitionState;
use crate::dictation_recognition::{
    DictationOcrOutput, DictationOcrRequest, DictationRecognizerDescriptor,
};

struct Fixture {
    conn: Connection,
    region_id: i64,
    crop_id: i64,
    crop_hash: String,
    crop_bytes: Vec<u8>,
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
           INSERT INTO students(student_no,name,class_id) VALUES ('01','小林',1);
           INSERT INTO exam_assessments_v2
             (public_id,title,class_id,assessment_context,evidence_policy,state,
              created_by,created_at,updated_at)
             VALUES ('assessment-s','主观题答题卡',1,'quiz','include','active','teacher',
                     '2026-07-16T09:00:00.000Z','2026-07-16T09:00:00.000Z');
           INSERT INTO exam_assessment_versions_v2
             (public_id,assessment_id,revision,item_set_hash,template_version,state,
              created_at,confirmed_by,confirmed_at)
             VALUES ('assessment-version-s',1,1,'{hash}','sheet-v1','confirmed',
                     '2026-07-16T09:00:00.000Z','teacher','2026-07-16T09:00:00.000Z');
           INSERT INTO k1_textbook_editions
             (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
             VALUES ('edition-s',1,'PEP','2024','中国历史八上','8','upper','active',
                     '2026-07-16T09:00:00.000Z');
           INSERT INTO k1_knowledge_maps
             (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
             VALUES ('map-s',1,1,'confirmed','2026-07-16T09:00:00.000Z',
                     '2026-07-16T09:00:00.000Z');
           INSERT INTO k1_questions
             (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
             VALUES ('question-s','personal','teacher','unknown',0,
                     '2026-07-16T09:00:00.000Z');
           INSERT INTO k1_question_versions
             (public_id,question_id,revision,question_type,stem,max_score,content_hash,
              quality_level,state,created_at)
             VALUES ('question-version-s',1,1,'fill_blank','《南京条约》签订于____年',1,
                     '{hash}','L3','published','2026-07-16T09:00:00.000Z');
           INSERT INTO k1_answer_key_versions
             (public_id,question_version_id,revision,answer_json,state,created_at,
              confirmed_by,confirmed_at)
             VALUES ('answer-s',1,1,'{{"schema_version":1,"values":["1842年"]}}',
                     'confirmed','2026-07-16T09:00:00.000Z','teacher',
                     '2026-07-16T09:00:00.000Z');
           INSERT INTO k1_rubric_versions
             (public_id,question_version_id,revision,max_score,state,created_at,
              confirmed_by,confirmed_at)
             VALUES ('rubric-s',1,1,1,'confirmed','2026-07-16T09:00:00.000Z',
                     'teacher','2026-07-16T09:00:00.000Z');
           INSERT INTO k1_rubric_points
             (public_id,rubric_version_id,stable_id,order_index,canonical_text,
              max_score,created_at)
             VALUES ('rubric-point-s',1,'year',0,'1842年',1,
                     '2026-07-16T09:00:00.000Z');
           INSERT INTO k1_link_sets
             (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
              confirmed_by,confirmed_at)
             VALUES ('link-s',1,1,1,'confirmed','2026-07-16T09:00:00.000Z',
                     'teacher','2026-07-16T09:00:00.000Z');
           INSERT INTO exam_assessment_items_v2
             (public_id,assessment_version_id,question_version_id,answer_key_version_id,
              rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
              state,created_at)
             VALUES ('item-s',1,1,1,1,1,0,1,
                     '{{"schema_version":1,"question_no":"1","page_no":1}}',
                     'active','2026-07-16T09:00:00.000Z');
           INSERT INTO exam_attempts_v2
             (public_id,assessment_version_id,student_id,attempt_no,source_kind,
              attempt_kind,state,created_at,updated_at)
             VALUES ('attempt-s',1,1,1,'image','first','ingesting',
                     '2026-07-16T09:00:00.000Z','2026-07-16T09:00:00.000Z');"#
    ))
    .unwrap();

    let source = artifacts::create_or_get(
        &conn,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Image,
            sha256: &"b".repeat(64),
            mime_type: "image/jpeg",
            byte_size: 100,
            original_name: Some("01-1.jpg"),
            original_path: None,
            archived_path: "/archive/01-1.jpg",
            parent_artifact_id: None,
            derivative_type: None,
            processing_version: "fixture-v1",
            privacy_class: PrivacyClass::StudentSensitive,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap();
    let aligned = artifacts::create_or_get(
        &conn,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Page,
            sha256: &"c".repeat(64),
            mime_type: "image/jpeg",
            byte_size: 90,
            original_name: None,
            original_path: None,
            archived_path: "/archive/aligned.jpg",
            parent_artifact_id: Some(source.id),
            derivative_type: Some("answer_sheet_aligned_input"),
            processing_version: "fixture-v1",
            privacy_class: PrivacyClass::StudentSensitive,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap();
    let crop_bytes = b"student-wrote-1840".to_vec();
    let crop_hash = hashing::sha256_hex(&crop_bytes);
    let crop = artifacts::create_or_get(
        &conn,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Crop,
            sha256: &crop_hash,
            mime_type: "image/jpeg",
            byte_size: crop_bytes.len() as i64,
            original_name: None,
            original_path: None,
            archived_path: "/archive/subjective-crop.jpg",
            parent_artifact_id: Some(aligned.id),
            derivative_type: Some("answer_sheet_subjective_region"),
            processing_version: "fixture-v1",
            privacy_class: PrivacyClass::StudentSensitive,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap();
    let blank = artifacts::create_or_get(
        &conn,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Image,
            sha256: &"d".repeat(64),
            mime_type: "image/png",
            byte_size: 80,
            original_name: Some("blank.png"),
            original_path: None,
            archived_path: "/archive/blank.png",
            parent_artifact_id: None,
            derivative_type: None,
            processing_version: "fixture-v1",
            privacy_class: PrivacyClass::TeachingContent,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap();
    let batch = papers::create_or_get_ingest_batch(
        &conn,
        &NewIngestBatch {
            assessment_version_id: 1,
            source_kind: "camera",
            idempotency_key: "subjective-fixture",
            created_by: "teacher",
        },
    )
    .unwrap();
    conn.execute(
        "INSERT INTO exam_material_type_revisions_v2
         (public_id,ingest_batch_id,revision,material_type,confidence,evidence_json,
          decision,state,created_by_type,created_by,confirmed_by,created_at)
         VALUES ('material-s',?1,1,'answer_sheet',1.0,'{\"schema_version\":1}',
                 'teacher_confirmed','active','teacher','teacher','teacher',
                 '2026-07-16T09:00:00.000Z')",
        [batch.id],
    )
    .unwrap();
    let page = papers::register_ingest_page(
        &conn,
        &NewIngestPage {
            batch_id: batch.id,
            source_artifact_id: source.id,
            import_index: 0,
            expected_page_no: Some(1),
        },
    )
    .unwrap();
    papers::record_page_quality(
        &conn,
        &NewPageQualityRevision {
            page_id: page.id,
            blur_score: 0.05,
            glare_score: 0.05,
            brightness_score: 0.95,
            perspective_score: 0.95,
            rotation_degrees: 0.0,
            crop_complete: true,
            result: "pass",
            issue_codes_json: r#"{"schema_version":1,"codes":[]}"#,
            checked_by_type: "teacher",
            checked_by: Some("teacher"),
        },
    )
    .unwrap();
    papers::decide_page_match(
        &conn,
        &NewPageMatchRevision {
            page_id: page.id,
            attempt_id: Some(1),
            page_no: Some(1),
            student_confidence: Some(1.0),
            page_no_confidence: Some(1.0),
            template_confidence: Some(1.0),
            decision: "teacher_confirmed",
            reason_code: Some("FIXTURE"),
            confirmed_by: Some("teacher"),
        },
    )
    .unwrap();
    let alignment = papers::record_page_alignment(
        &conn,
        &NewPageAlignmentRevision {
            page_id: page.id,
            template_version: "sheet-v1",
            transform_json: r#"{"schema_version":1,"matrix":[1,0,0,0,1,0,0,0,1]}"#,
            confidence: 1.0,
            aligned_artifact_id: Some(aligned.id),
            decision: "teacher_confirmed",
            reason_code: Some("FIXTURE"),
            confirmed_by: Some("teacher"),
        },
    )
    .unwrap();
    let region = papers::record_answer_region(
        &conn,
        &NewAnswerRegionRevision {
            page_id: page.id,
            assessment_item_id: 1,
            region_index: 0,
            bbox_json: r#"{"schema_version":1,"x":0.1,"y":0.2,"width":0.8,"height":0.1}"#,
            crop_artifact_id: Some(crop.id),
            mapping_confidence: Some(0.99),
            decision: "teacher_confirmed",
            reason_code: Some("FIXTURE"),
            confirmed_by: Some("teacher"),
        },
    )
    .unwrap();
    conn.execute(
        "INSERT INTO exam_answer_sheet_template_revisions_v2
         (public_id,assessment_version_id,revision,template_version,page_no,
          blank_artifact_id,template_hash,template_json,confirmed_by,state,created_at)
         VALUES ('template-s',1,1,'sheet-v1',1,?1,?2,
                 '{\"schema_version\":2}','teacher','active',
                 '2026-07-16T09:00:00.000Z')",
        (blank.id, "e".repeat(64)),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO exam_answer_sheet_page_materializations_v2
         (public_id,page_id,template_revision_id,alignment_revision_id,
          region_revision_ids_json,confirmed_by,created_at)
         VALUES ('materialization-s',?1,1,?2,?3,'teacher',
                 '2026-07-16T09:00:00.000Z')",
        (
            page.id,
            alignment.id,
            format!(
                "{{\"schema_version\":1,\"region_revision_ids\":[{}]}}",
                region.id
            ),
        ),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO exam_answer_sheet_region_routes_v2
         (materialization_id,answer_region_revision_id,assessment_item_id,region_index,
          recognition_route,question_type,created_at)
         VALUES (1,?1,1,0,'handwriting_ocr','fill_blank',
                 '2026-07-16T09:00:00.000Z')",
        [region.id],
    )
    .unwrap();

    Fixture {
        conn,
        region_id: region.id,
        crop_id: crop.id,
        crop_hash,
        crop_bytes,
    }
}

fn descriptor() -> DictationRecognizerDescriptor {
    DictationRecognizerDescriptor {
        provider: "fixture".into(),
        model_name: "fixture".into(),
        model_version: "v1".into(),
        config_version: "subjective-ocr-v1".into(),
        rule_version: "raw-only-v1".into(),
    }
}

fn successful_run(fixture: &mut Fixture, key: &str, text: &str) -> i64 {
    let request = DictationOcrRequest {
        answer_region_revision_id: fixture.region_id,
        crop_artifact_id: fixture.crop_id,
        crop_artifact_sha256: &fixture.crop_hash,
        mime_type: "image/jpeg",
        image_bytes: &fixture.crop_bytes,
    };
    let input_hash = request.input_hash().unwrap();
    let run = ai_runs::create_or_get(
        &fixture.conn,
        &ai_runs::NewAiRun {
            idempotency_key: key,
            run_type: "handwriting_ocr",
            source_module: "exam",
            business_ref_type: "answer_region_revision",
            business_ref_id: &fixture.region_id.to_string(),
            input_artifact_id: Some(fixture.crop_id),
            provider: "fixture",
            model_name: "fixture",
            model_version: "v1",
            config_version: "subjective-ocr-v1",
            prompt_or_rule_version: "raw-only-v1",
            input_hash: &input_hash,
            retry_of_ai_run_id: None,
        },
    )
    .unwrap();
    ai_runs::start(&fixture.conn, run.id, &time::utc_now_rfc3339(), None).unwrap();
    let output = DictationOcrOutput {
        schema_version: 1,
        answer_region_revision_id: fixture.region_id,
        crop_artifact_id: fixture.crop_id,
        crop_artifact_sha256: fixture.crop_hash.clone(),
        input_hash,
        descriptor: descriptor(),
        state: DictationRecognitionState::Recognized,
        raw_text: Some(text.into()),
        normalized_text: Some(text.into()),
        confidence: Some(0.98),
        issue_codes: vec![],
    };
    let output_json = output.to_json_against(&request).unwrap();
    ai_runs::finalize_succeeded(
        &fixture.conn,
        run.id,
        &hashing::sha256_hex(output_json.as_bytes()),
        output.confidence,
        &output_json,
        &time::utc_now_rfc3339(),
    )
    .unwrap();
    run.id
}

fn add_short_answer_region(fixture: &mut Fixture) -> i64 {
    let hash = "f".repeat(64);
    fixture
        .conn
        .execute_batch(&format!(
            r#"INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('question-short','personal','teacher','unknown',0,
                         '2026-07-16T09:00:00.000Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES ('question-version-short',2,1,'short_answer',
                         '概括洋务运动失败的原因',4,'{hash}','L3','published',
                         '2026-07-16T09:00:00.000Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-short',2,1,
                         '{{"schema_version":1,"reference_answer":"没有改变封建制度"}}',
                         'confirmed','2026-07-16T09:00:00.000Z','teacher',
                         '2026-07-16T09:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('rubric-short',2,1,4,'confirmed','2026-07-16T09:00:00.000Z',
                         'teacher','2026-07-16T09:00:00.000Z');
               INSERT INTO k1_rubric_points
                 (public_id,rubric_version_id,stable_id,order_index,canonical_text,
                  max_score,created_at)
                 VALUES ('rubric-point-short',2,'institution',0,'没有改变封建制度',4,
                         '2026-07-16T09:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('link-short',2,1,1,'confirmed','2026-07-16T09:00:00.000Z',
                         'teacher','2026-07-16T09:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                  state,created_at)
                 VALUES ('item-short',1,2,2,2,2,1,4,
                         '{{"schema_version":1,"question_no":"2","page_no":1}}',
                         'active','2026-07-16T09:00:00.000Z');"#
        ))
        .unwrap();
    let region = papers::record_answer_region(
        &fixture.conn,
        &NewAnswerRegionRevision {
            page_id: 1,
            assessment_item_id: 2,
            region_index: 0,
            bbox_json: r#"{"schema_version":1,"x":0.1,"y":0.5,"width":0.8,"height":0.3}"#,
            crop_artifact_id: Some(fixture.crop_id),
            mapping_confidence: Some(0.99),
            decision: "teacher_confirmed",
            reason_code: Some("FIXTURE"),
            confirmed_by: Some("teacher"),
        },
    )
    .unwrap();
    fixture
        .conn
        .execute(
            "INSERT INTO exam_answer_sheet_region_routes_v2
             (materialization_id,answer_region_revision_id,assessment_item_id,region_index,
              recognition_route,question_type,created_at)
             VALUES (1,?1,2,0,'handwriting_ocr','short_answer',
                     '2026-07-16T09:00:00.000Z')",
            [region.id],
        )
        .unwrap();
    region.id
}

#[test]
fn handwriting_ocr_persists_raw_text_without_answer_and_is_idempotent() {
    let mut fixture = setup();
    let scope = subjective::load_region_scope(&fixture.conn, fixture.region_id).unwrap();
    assert_eq!(scope.question_type, "fill_blank");
    let run_id = successful_run(&mut fixture, "subjective-ocr-1", "1840年");
    let first = subjective::record_ocr_ai_run_transcription(&mut fixture.conn, run_id).unwrap();
    assert_eq!(first.raw_ocr_text.as_deref(), Some("1840年"));
    assert_eq!(first.revision, 1);
    let repeated = subjective::record_ocr_ai_run_transcription(&mut fixture.conn, run_id).unwrap();
    assert_eq!(repeated.id, first.id);
    let count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_subjective_transcription_revisions_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    let suggestion = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .pop()
        .unwrap();
    assert_eq!(suggestion.suggestion_outcome, "incorrect");
    assert_eq!(suggestion.suggested_score, Some(0.0));
    assert!(!suggestion.batch_eligible);
    assert_eq!(
        suggestion.exclusion_reason.as_deref(),
        Some("ANSWER_MISMATCH_REQUIRES_REVIEW")
    );
    let decisions: i64 = fixture
        .conn
        .query_row("SELECT COUNT(*) FROM exam_grade_decisions_v2", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(decisions, 0);
}

#[test]
fn exact_fill_suggestion_never_becomes_a_grade_until_teacher_accepts() {
    let mut fixture = setup();
    let run_id = successful_run(&mut fixture, "subjective-ocr-exact", "1842 年");
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, run_id).unwrap();
    let workbench = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10).unwrap();
    let row = &workbench.rows[0];
    assert_eq!(row.suggestion_outcome, "correct");
    assert_eq!(row.suggested_score, Some(1.0));
    assert!(row.batch_eligible);
    assert_eq!(workbench.attempts[0].confirmed_count, 0);

    let decision =
        subjective::accept_subjective_suggestion(&fixture.conn, row.suggestion_id, "teacher")
            .unwrap();
    assert_eq!(decision.teacher_score, 1.0);
    assert_eq!(decision.confirmation_level, "teacher_accepted");
    let publication_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_grade_publications_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let evidence_count: i64 = fixture
        .conn
        .query_row("SELECT COUNT(*) FROM learning_evidence", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!((publication_count, evidence_count), (0, 0));
}

#[test]
fn teacher_correction_appends_revision_and_preserves_machine_text() {
    let mut fixture = setup();
    let run_id = successful_run(&mut fixture, "subjective-ocr-2", "一八四零年");
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, run_id).unwrap();
    let corrected = subjective::teacher_correct_transcription(
        &mut fixture.conn,
        fixture.region_id,
        "1842 年",
        "teacher",
    )
    .unwrap();
    assert_eq!(corrected.revision, 2);
    assert_eq!(corrected.raw_ocr_text.as_deref(), Some("一八四零年"));
    assert_eq!(corrected.teacher_corrected_text.as_deref(), Some("1842 年"));
    let old_state: String = fixture
        .conn
        .query_row(
            "SELECT state FROM exam_subjective_transcription_revisions_v2 WHERE revision=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(old_state, "superseded");
    let suggestion_states: Vec<String> = fixture
        .conn
        .prepare("SELECT state FROM exam_subjective_grade_suggestions_v2 ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(suggestion_states, vec!["superseded", "active"]);
    let current = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .pop()
        .unwrap();
    assert_eq!(current.suggestion_outcome, "correct");
    assert_eq!(current.suggested_score, Some(1.0));
}

#[test]
fn short_answer_stays_unscored_until_teacher_or_rubric_ai_reviews_it() {
    let mut fixture = setup();
    let region_id = add_short_answer_region(&mut fixture);
    fixture.region_id = region_id;
    let run_id = successful_run(
        &mut fixture,
        "subjective-ocr-short",
        "只学习技术，没有改变封建制度",
    );
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, run_id).unwrap();
    let workbench = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10).unwrap();
    let row = workbench
        .rows
        .iter()
        .find(|row| row.question_type == "short_answer")
        .unwrap();
    assert_eq!(row.suggestion_outcome, "unscored");
    assert_eq!(row.suggested_score, None);
    assert_eq!(
        row.exclusion_reason.as_deref(),
        Some("SHORT_ANSWER_AI_REQUIRED")
    );
    assert!(
        subjective::accept_subjective_suggestion(&fixture.conn, row.suggestion_id, "teacher")
            .is_err()
    );
    let decision = subjective::correct_subjective_suggestion(
        &fixture.conn,
        row.suggestion_id,
        3.0,
        Some("原图覆盖制度局限，但缺少其他原因"),
        "teacher",
    )
    .unwrap();
    assert_eq!(decision.teacher_score, 3.0);
    assert_eq!(decision.confirmation_level, "teacher_corrected");
}

#[test]
fn database_rejects_wrong_question_type_or_non_routed_scope() {
    let fixture = setup();
    let error = fixture
        .conn
        .execute(
            "INSERT INTO exam_subjective_transcription_revisions_v2
             (public_id,attempt_id,assessment_item_id,answer_region_revision_id,question_type,
              revision,result_state,state,created_at)
             VALUES ('wrong-type',1,1,?1,'short_answer',1,'not_written','active',
                     '2026-07-16T09:00:00.000Z')",
            [fixture.region_id],
        )
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("M2_SUBJECTIVE_TRANSCRIPTION_SCOPE_MISMATCH"));
    assert!(subjective::load_region_scope(&fixture.conn, 999_999).is_err());
}
