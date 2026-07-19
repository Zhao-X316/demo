use image::codecs::jpeg::JpegEncoder;
use image::{Rgb, RgbImage};
use rusqlite::Connection;
use suite_core::db::repo::{ai_runs, artifacts};
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
use suite_core::domain::hashing;
use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

use super::ordinary_question_sync::sync_confirmed_printed_questions;
use super::ordinary_structure::{
    confirm_in_transaction, crop_normalized_jpeg, ConfirmOrdinaryStructureInput,
    OrdinaryRegionArtifact,
};
use super::papers::{
    self, NewIngestBatch, NewIngestPage, NewPageMatchRevision, NewPageQualityRevision,
};
use crate::ordinary_paper_recognition::{
    NormalizedRect, OrdinaryPaperAlignment, OrdinaryPaperMarkCell, OrdinaryPaperPrintedOption,
    OrdinaryPaperPrintedPrivacy, OrdinaryPaperPrintedQuestion, OrdinaryPaperQuality,
    OrdinaryPaperQualityResult, OrdinaryPaperRecognitionOutput, OrdinaryPaperRecognitionState,
    OrdinaryPaperRecognizerDescriptor, OrdinaryPaperRegionProposal, ORDINARY_PAPER_SCHEMA_VERSION,
};

struct Fixture {
    conn: Connection,
    page_id: i64,
    source_artifact_id: i64,
    source_hash: String,
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
             VALUES ('assessment-a','普通卷',1,'quiz','include','active','teacher',
                     '2026-07-15T08:00:00.000Z','2026-07-15T08:00:00.000Z');
           INSERT INTO exam_assessment_versions_v2
             (public_id,assessment_id,revision,item_set_hash,template_version,state,
              created_at,confirmed_by,confirmed_at)
             VALUES ('assessment-version-a',1,1,'{hash}','ordinary-v1','confirmed',
                     '2026-07-15T08:00:00.000Z','teacher','2026-07-15T08:00:00.000Z');
           INSERT INTO k1_textbook_editions
             (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
             VALUES ('edition-a',1,'PEP','2024','中国历史八上','8','upper','active',
                     '2026-07-15T08:00:00.000Z');
           INSERT INTO k1_knowledge_maps
             (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
             VALUES ('map-a',1,1,'confirmed','2026-07-15T08:00:00.000Z',
                     '2026-07-15T08:00:00.000Z');
           INSERT INTO k1_questions
             (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
             VALUES ('question-a','personal','teacher','unknown',0,
                     '2026-07-15T08:00:00.000Z');
           INSERT INTO k1_question_versions
             (public_id,question_id,revision,question_type,stem,max_score,content_hash,
              quality_level,state,created_at)
             VALUES ('question-version-a',1,1,'single','洋务运动后期口号是？',1,'{hash}',
                     'L3','published','2026-07-15T08:00:00.000Z');
           INSERT INTO k1_answer_key_versions
             (public_id,question_version_id,revision,answer_json,state,created_at,
              confirmed_by,confirmed_at)
             VALUES ('answer-a',1,1,'{{"schema_version":1,"selected_labels":["B"]}}',
                     'confirmed','2026-07-15T08:00:00.000Z','teacher',
                     '2026-07-15T08:00:00.000Z');
           INSERT INTO k1_rubric_versions
             (public_id,question_version_id,revision,max_score,state,created_at,
              confirmed_by,confirmed_at)
             VALUES ('rubric-a',1,1,1,'confirmed','2026-07-15T08:00:00.000Z',
                     'teacher','2026-07-15T08:00:00.000Z');
           INSERT INTO k1_link_sets
             (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
              confirmed_by,confirmed_at)
             VALUES ('link-a',1,1,1,'confirmed','2026-07-15T08:00:00.000Z',
                     'teacher','2026-07-15T08:00:00.000Z');
           INSERT INTO exam_assessment_items_v2
             (public_id,assessment_version_id,question_version_id,answer_key_version_id,
              rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
              state,created_at)
             VALUES ('item-a',1,1,1,1,1,0,1,
                     '{{"schema_version":1,"question_no":"1","page_no":1}}',
                     'active','2026-07-15T08:00:00.000Z');
           INSERT INTO exam_attempts_v2
             (public_id,assessment_version_id,student_id,attempt_no,source_kind,
              attempt_kind,state,created_at,updated_at)
             VALUES ('attempt-a',1,1,1,'image','first','ingesting',
                     '2026-07-15T08:00:00.000Z','2026-07-15T08:00:00.000Z');"#
    ))
    .unwrap();
    let source_hash = "1".repeat(64);
    let source = artifacts::create_or_get(
        &conn,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Image,
            sha256: &source_hash,
            mime_type: "image/jpeg",
            byte_size: 128,
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
    let batch = papers::create_or_get_ingest_batch(
        &conn,
        &NewIngestBatch {
            assessment_version_id: 1,
            source_kind: "camera",
            idempotency_key: "ordinary-structure-fixture",
            created_by: "teacher",
        },
    )
    .unwrap();
    conn.execute(
        "INSERT INTO exam_material_type_revisions_v2
         (public_id,ingest_batch_id,revision,material_type,confidence,evidence_json,
          decision,state,created_by_type,created_by,confirmed_by,created_at)
         VALUES ('material-a',?1,1,'ordinary_paper',1.0,
                 '{\"schema_version\":1}','teacher_confirmed','active','teacher',
                 'teacher','teacher','2026-07-15T08:00:00.000Z')",
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
    papers::decide_page_match(
        &conn,
        &NewPageMatchRevision {
            page_id: page.id,
            attempt_id: Some(1),
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
    Fixture {
        conn,
        page_id: page.id,
        source_artifact_id: source.id,
        source_hash,
    }
}

fn descriptor() -> OrdinaryPaperRecognizerDescriptor {
    OrdinaryPaperRecognizerDescriptor {
        provider: "fixture".into(),
        model_name: "fixture-vision".into(),
        model_version: "fixture-v1".into(),
        config_version: "ordinary-v2".into(),
        rule_version: "ordinary-json-v2".into(),
    }
}

fn candidate_region() -> OrdinaryPaperRegionProposal {
    OrdinaryPaperRegionProposal {
        assessment_item_id: 1,
        region_index: 0,
        bbox: NormalizedRect {
            x: 0.1,
            y: 0.2,
            width: 0.8,
            height: 0.3,
        },
        mapping_confidence: 0.99,
        mark_cells: vec![
            OrdinaryPaperMarkCell {
                label: "A".into(),
                rect: NormalizedRect {
                    x: 0.05,
                    y: 0.1,
                    width: 0.2,
                    height: 0.4,
                },
            },
            OrdinaryPaperMarkCell {
                label: "B".into(),
                rect: NormalizedRect {
                    x: 0.35,
                    y: 0.1,
                    width: 0.2,
                    height: 0.4,
                },
            },
        ],
    }
}

fn create_run(fixture: &Fixture) -> i64 {
    let input_hash = "b".repeat(64);
    let output = OrdinaryPaperRecognitionOutput {
        schema_version: ORDINARY_PAPER_SCHEMA_VERSION,
        page_id: fixture.page_id,
        input_artifact_id: fixture.source_artifact_id,
        input_artifact_sha256: fixture.source_hash.clone(),
        input_hash: input_hash.clone(),
        expected_page_no: 1,
        descriptor: descriptor(),
        state: OrdinaryPaperRecognitionState::Ready,
        quality: OrdinaryPaperQuality {
            blur_score: 0.01,
            glare_score: 0.01,
            brightness_score: 0.8,
            perspective_score: 0.99,
            rotation_degrees: 0.0,
            crop_complete: true,
            result: OrdinaryPaperQualityResult::Pass,
            issue_codes: vec![],
        },
        alignment: Some(OrdinaryPaperAlignment {
            template_version: "ordinary-v1".into(),
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            confidence: 0.99,
        }),
        regions: vec![candidate_region()],
        printed_questions: vec![OrdinaryPaperPrintedQuestion {
            assessment_item_id: 1,
            stem: "洋务运动后期提出的口号是？".into(),
            material_text: None,
            options: vec![
                OrdinaryPaperPrintedOption {
                    label: "A".into(),
                    content: "自强".into(),
                    order_index: 0,
                },
                OrdinaryPaperPrintedOption {
                    label: "B".into(),
                    content: "求富".into(),
                    order_index: 1,
                },
            ],
            extraction_confidence: 0.99,
            privacy: OrdinaryPaperPrintedPrivacy {
                schema_version: 1,
                sanitized: true,
                student_identity_detected: false,
                student_answer_detected: false,
                teacher_mark_detected: false,
                score_detected: false,
            },
        }],
        confidence: 0.99,
        issue_codes: vec![],
    };
    let output_json = serde_json::to_string(&output).unwrap();
    let run = ai_runs::create_or_get(
        &fixture.conn,
        &ai_runs::NewAiRun {
            idempotency_key: "ordinary-ready-run",
            run_type: "ordinary_paper_structure",
            source_module: "exam",
            business_ref_type: "ingest_page",
            business_ref_id: &fixture.page_id.to_string(),
            input_artifact_id: Some(fixture.source_artifact_id),
            provider: "fixture",
            model_name: "fixture-vision",
            model_version: "fixture-v1",
            config_version: "ordinary-v2",
            prompt_or_rule_version: "ordinary-json-v2",
            input_hash: &input_hash,
            retry_of_ai_run_id: None,
        },
    )
    .unwrap();
    ai_runs::start(&fixture.conn, run.id, "2026-07-15T08:10:00.000Z", None).unwrap();
    ai_runs::finalize_succeeded(
        &fixture.conn,
        run.id,
        &hashing::sha256_hex(output_json.as_bytes()),
        Some(0.99),
        &output_json,
        "2026-07-15T08:11:00.000Z",
    )
    .unwrap();
    run.id
}

fn derivative(
    fixture: &Fixture,
    kind: ArtifactKind,
    hash_digit: char,
    parent_id: i64,
    derivative_type: &str,
) -> i64 {
    artifacts::create_or_get(
        &fixture.conn,
        &artifacts::NewArtifact {
            kind,
            sha256: &hash_digit.to_string().repeat(64),
            mime_type: "image/jpeg",
            byte_size: 64,
            original_name: None,
            original_path: None,
            archived_path: &format!("/archive/{derivative_type}-{hash_digit}.jpg"),
            parent_artifact_id: Some(parent_id),
            derivative_type: Some(derivative_type),
            processing_version: "fixture-v1",
            privacy_class: PrivacyClass::StudentSensitive,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap()
    .id
}

#[test]
fn teacher_confirmation_materializes_once_without_creating_scores() {
    let fixture = setup();
    let run_id = create_run(&fixture);
    let aligned_id = derivative(
        &fixture,
        ArtifactKind::Page,
        '2',
        fixture.source_artifact_id,
        "ordinary_aligned_input",
    );
    let crop_id = derivative(
        &fixture,
        ArtifactKind::Crop,
        '3',
        aligned_id,
        "ordinary_answer_region",
    );
    let region_artifacts = [OrdinaryRegionArtifact {
        assessment_item_id: 1,
        region_index: 0,
        crop_artifact_id: crop_id,
    }];
    let tx = fixture.conn.unchecked_transaction().unwrap();
    let result = confirm_in_transaction(
        &tx,
        &ConfirmOrdinaryStructureInput {
            page_id: fixture.page_id,
            ai_run_id: run_id,
            aligned_artifact_id: aligned_id,
            region_artifacts: &region_artifacts,
            confirmed_by: "teacher",
        },
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(result.alignment.decision, "teacher_confirmed");
    assert_eq!(result.regions.len(), 1);
    assert_eq!(result.regions[0].decision, "teacher_confirmed");
    assert!(result.regions[0].bbox_json.contains("mark_cells"));
    let observations: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_objective_observation_revisions_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(observations, 0);

    let tx = fixture.conn.unchecked_transaction().unwrap();
    let repeated = confirm_in_transaction(
        &tx,
        &ConfirmOrdinaryStructureInput {
            page_id: fixture.page_id,
            ai_run_id: run_id,
            aligned_artifact_id: aligned_id,
            region_artifacts: &region_artifacts,
            confirmed_by: "teacher",
        },
    )
    .unwrap();
    tx.commit().unwrap();
    assert_eq!(repeated.confirmation.id, result.confirmation.id);
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_page_alignment_revisions_v2",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn confirmed_printed_question_enters_private_candidate_once_for_the_whole_assessment_page() {
    let fixture = setup();
    let run_id = create_run(&fixture);
    let aligned_id = derivative(
        &fixture,
        ArtifactKind::Page,
        '6',
        fixture.source_artifact_id,
        "ordinary_aligned_input",
    );
    let crop_id = derivative(
        &fixture,
        ArtifactKind::Crop,
        '7',
        aligned_id,
        "ordinary_answer_region",
    );
    let tx = fixture.conn.unchecked_transaction().unwrap();
    confirm_in_transaction(
        &tx,
        &ConfirmOrdinaryStructureInput {
            page_id: fixture.page_id,
            ai_run_id: run_id,
            aligned_artifact_id: aligned_id,
            region_artifacts: &[OrdinaryRegionArtifact {
                assessment_item_id: 1,
                region_index: 0,
                crop_artifact_id: crop_id,
            }],
            confirmed_by: "teacher",
        },
    )
    .unwrap();
    tx.commit().unwrap();

    let first = sync_confirmed_printed_questions(&fixture.conn, fixture.page_id, run_id, "teacher")
        .unwrap();
    assert_eq!(first.state, "completed");
    assert_eq!(first.eligible_count, 1);
    assert_eq!(first.candidate_created_count, 1);
    assert_eq!(first.matched_count, 0);
    assert_eq!(first.failed_count, 0);
    assert!(!first.reused_existing_source);

    let repeated =
        sync_confirmed_printed_questions(&fixture.conn, fixture.page_id, run_id, "teacher")
            .unwrap();
    assert!(repeated.reused_existing_source);
    assert_eq!(repeated.candidate_created_count, 1);
    let counts: (i64, i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_ordinary_question_source_syncs_v2),
               (SELECT COUNT(*) FROM exam_question_ingest_jobs_v2),
               (SELECT COUNT(*) FROM exam_question_candidates_v2),
               (SELECT COUNT(*) FROM k1_question_versions WHERE quality_level='C0')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 1, 1, 1));
    let candidate: (String, Option<i64>, String) = fixture
        .conn
        .query_row(
            "SELECT privacy_status,reusable_artifact_id,status
             FROM exam_question_candidates_v2",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        candidate,
        ("text_only".into(), None, "candidate_created".into())
    );
}

#[test]
fn region_failure_rolls_back_the_whole_page_confirmation() {
    let fixture = setup();
    let run_id = create_run(&fixture);
    let aligned_id = derivative(
        &fixture,
        ArtifactKind::Page,
        '4',
        fixture.source_artifact_id,
        "ordinary_aligned_input",
    );
    let wrong_crop_parent = derivative(
        &fixture,
        ArtifactKind::Crop,
        '5',
        fixture.source_artifact_id,
        "ordinary_answer_region",
    );
    {
        let tx = fixture.conn.unchecked_transaction().unwrap();
        let result = confirm_in_transaction(
            &tx,
            &ConfirmOrdinaryStructureInput {
                page_id: fixture.page_id,
                ai_run_id: run_id,
                aligned_artifact_id: aligned_id,
                region_artifacts: &[OrdinaryRegionArtifact {
                    assessment_item_id: 1,
                    region_index: 0,
                    crop_artifact_id: wrong_crop_parent,
                }],
                confirmed_by: "teacher",
            },
        );
        assert!(result.is_err());
    }
    let counts: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_page_alignment_revisions_v2),
               (SELECT COUNT(*) FROM exam_answer_region_revisions_v2),
               (SELECT COUNT(*) FROM exam_ordinary_structure_confirmations_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[test]
fn crop_uses_normalized_page_coordinates() {
    let image = RgbImage::from_pixel(100, 80, Rgb([240, 240, 240]));
    let mut source = Vec::new();
    JpegEncoder::new_with_quality(&mut source, 95)
        .encode_image(&image)
        .unwrap();
    let crop = crop_normalized_jpeg(
        &source,
        &NormalizedRect {
            x: 0.1,
            y: 0.25,
            width: 0.5,
            height: 0.5,
        },
    )
    .unwrap();
    let decoded = image::load_from_memory(&crop).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (50, 40));
}
