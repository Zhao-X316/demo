use rusqlite::Connection;
use suite_core::db::repo::artifacts;
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

use super::answer_sheet::{confirm_answer_sheet_template, ConfirmAnswerSheetTemplateInput};
use super::answer_sheet_page::{
    materialize_in_transaction, AnswerSheetRegionArtifact, MaterializeAnswerSheetPageInput,
};
use super::papers::{
    self, NewIngestBatch, NewIngestPage, NewPageMatchRevision, NewPageQualityRevision,
};
use crate::answer_sheet_recognition::{
    AnswerSheetAnchor, AnswerSheetItemTemplate, AnswerSheetTemplateDefinition,
    DetectedAnswerSheetAnchor, LocalOmrPolicy, SheetRect,
};
use crate::objective_recognition::{ObjectiveMarkCell, ObjectiveQuestionType};

struct Fixture {
    conn: Connection,
    page_id: i64,
    template_revision_id: i64,
    aligned_artifact_id: i64,
    crop_artifact_id: i64,
    blank_artifact_id: i64,
}

fn rect(x: f64, y: f64) -> SheetRect {
    SheetRect {
        x,
        y,
        width: 0.05,
        height: 0.05,
    }
}

fn anchors() -> Vec<DetectedAnswerSheetAnchor> {
    vec![
        ("top_left", 20.0, 20.0),
        ("top_right", 980.0, 20.0),
        ("bottom_left", 20.0, 1380.0),
        ("bottom_right", 980.0, 1380.0),
    ]
    .into_iter()
    .map(|(key, source_x, source_y)| DetectedAnswerSheetAnchor {
        key: key.into(),
        source_x,
        source_y,
        confidence: 0.99,
    })
    .collect()
}

fn setup() -> Fixture {
    let mut conn = open_in_memory().unwrap();
    run_migrations(&conn, CORE_MIGRATIONS).unwrap();
    run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
    run_migrations(&conn, crate::exam_migrations()).unwrap();
    let content_hash = "a".repeat(64);
    conn.execute_batch(&format!(
        r#"INSERT INTO subjects(name) VALUES ('历史');
           INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
           INSERT INTO students(student_no,name,class_id) VALUES ('01','小林',1);
           INSERT INTO exam_assessments_v2
             (public_id,title,class_id,assessment_context,evidence_policy,state,
              created_by,created_at,updated_at)
             VALUES ('assessment-a','答题卡',1,'quiz','include','active','teacher',
                     '2026-07-15T09:00:00.000Z','2026-07-15T09:00:00.000Z');
           INSERT INTO exam_assessment_versions_v2
             (public_id,assessment_id,revision,item_set_hash,template_version,state,
              created_at,confirmed_by,confirmed_at)
             VALUES ('assessment-version-a',1,1,'{content_hash}','sheet-v1','confirmed',
                     '2026-07-15T09:00:00.000Z','teacher','2026-07-15T09:00:00.000Z');
           INSERT INTO k1_textbook_editions
             (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
             VALUES ('edition-a',1,'PEP','2024','中国历史八上','8','upper','active',
                     '2026-07-15T09:00:00.000Z');
           INSERT INTO k1_knowledge_maps
             (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
             VALUES ('map-a',1,1,'confirmed','2026-07-15T09:00:00.000Z',
                     '2026-07-15T09:00:00.000Z');
           INSERT INTO k1_questions
             (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
             VALUES ('question-a','personal','teacher','unknown',0,
                     '2026-07-15T09:00:00.000Z');
           INSERT INTO k1_question_versions
             (public_id,question_id,revision,question_type,stem,max_score,content_hash,
              quality_level,state,created_at)
             VALUES ('question-version-a',1,1,'single','第1题',1,'{content_hash}',
                     'L3','published','2026-07-15T09:00:00.000Z');
           INSERT INTO k1_answer_key_versions
             (public_id,question_version_id,revision,answer_json,state,created_at,
              confirmed_by,confirmed_at)
             VALUES ('answer-a',1,1,'{{"schema_version":1,"selected_labels":["B"]}}',
                     'confirmed','2026-07-15T09:00:00.000Z','teacher',
                     '2026-07-15T09:00:00.000Z');
           INSERT INTO k1_rubric_versions
             (public_id,question_version_id,revision,max_score,state,created_at,
              confirmed_by,confirmed_at)
             VALUES ('rubric-a',1,1,1,'confirmed','2026-07-15T09:00:00.000Z',
                     'teacher','2026-07-15T09:00:00.000Z');
           INSERT INTO k1_link_sets
             (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
              confirmed_by,confirmed_at)
             VALUES ('link-a',1,1,1,'confirmed','2026-07-15T09:00:00.000Z',
                     'teacher','2026-07-15T09:00:00.000Z');
           INSERT INTO exam_assessment_items_v2
             (public_id,assessment_version_id,question_version_id,answer_key_version_id,
              rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
              state,created_at)
             VALUES ('item-a',1,1,1,1,1,0,1,
                     '{{"schema_version":1,"question_no":"1","page_no":1}}',
                     'active','2026-07-15T09:00:00.000Z');
           INSERT INTO exam_attempts_v2
             (public_id,assessment_version_id,student_id,attempt_no,source_kind,
              attempt_kind,state,created_at,updated_at)
             VALUES ('attempt-a',1,1,1,'image','first','ingesting',
                     '2026-07-15T09:00:00.000Z','2026-07-15T09:00:00.000Z');"#
    ))
    .unwrap();

    let blank_hash = "b".repeat(64);
    let blank = artifacts::create_or_get(
        &conn,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Image,
            sha256: &blank_hash,
            mime_type: "image/png",
            byte_size: 100,
            original_name: Some("blank-answer-sheet.png"),
            original_path: None,
            archived_path: "/archive/blank-answer-sheet.png",
            parent_artifact_id: None,
            derivative_type: None,
            processing_version: "fixture-v1",
            privacy_class: PrivacyClass::TeachingContent,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap();
    let source_hash = "c".repeat(64);
    let source = artifacts::create_or_get(
        &conn,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Image,
            sha256: &source_hash,
            mime_type: "image/jpeg",
            byte_size: 200,
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
            sha256: &"d".repeat(64),
            mime_type: "image/jpeg",
            byte_size: 180,
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
    let crop = artifacts::create_or_get(
        &conn,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Crop,
            sha256: &"e".repeat(64),
            mime_type: "image/jpeg",
            byte_size: 80,
            original_name: None,
            original_path: None,
            archived_path: "/archive/crop.jpg",
            parent_artifact_id: Some(aligned.id),
            derivative_type: Some("answer_sheet_answer_region"),
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
            idempotency_key: "answer-sheet-page-fixture",
            created_by: "teacher",
        },
    )
    .unwrap();
    conn.execute(
        "INSERT INTO exam_material_type_revisions_v2
         (public_id,ingest_batch_id,revision,material_type,confidence,evidence_json,
          decision,state,created_by_type,created_by,confirmed_by,created_at)
         VALUES ('material-a',?1,1,'answer_sheet',1.0,
                 '{\"schema_version\":1}','teacher_confirmed','active','teacher',
                 'teacher','teacher','2026-07-15T09:00:00.000Z')",
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

    let definition = AnswerSheetTemplateDefinition {
        schema_version: 1,
        assessment_version_id: 1,
        template_version: "sheet-v1".into(),
        page_no: 1,
        canvas_width: 1000,
        canvas_height: 1400,
        blank_artifact_id: blank.id,
        blank_artifact_sha256: blank_hash,
        anchors: vec![
            AnswerSheetAnchor {
                key: "top_left".into(),
                expected: rect(0.02, 0.02),
                search: rect(0.0, 0.0),
            },
            AnswerSheetAnchor {
                key: "top_right".into(),
                expected: rect(0.93, 0.02),
                search: rect(0.90, 0.0),
            },
            AnswerSheetAnchor {
                key: "bottom_left".into(),
                expected: rect(0.02, 0.93),
                search: rect(0.0, 0.90),
            },
            AnswerSheetAnchor {
                key: "bottom_right".into(),
                expected: rect(0.93, 0.93),
                search: rect(0.90, 0.90),
            },
        ],
        items: vec![AnswerSheetItemTemplate {
            assessment_item_id: 1,
            region_index: 0,
            question_type: ObjectiveQuestionType::Single,
            region: SheetRect {
                x: 0.1,
                y: 0.2,
                width: 0.8,
                height: 0.1,
            },
            cells: vec![
                ObjectiveMarkCell {
                    label: "A".into(),
                    x: 0.0,
                    y: 0.0,
                    width: 0.5,
                    height: 1.0,
                },
                ObjectiveMarkCell {
                    label: "B".into(),
                    x: 0.5,
                    y: 0.0,
                    width: 0.5,
                    height: 1.0,
                },
            ],
        }],
        subjective_regions: Vec::new(),
        policy: LocalOmrPolicy {
            blank_max_ratio: 0.02,
            marked_min_ratio: 0.20,
            pixel_delta_threshold: 40,
            cell_inset_ratio: 0.1,
        },
    };
    let template = confirm_answer_sheet_template(
        &mut conn,
        &ConfirmAnswerSheetTemplateInput {
            definition: &definition,
            source_ai_run_id: None,
            confirmed_by: "teacher",
        },
    )
    .unwrap();
    Fixture {
        conn,
        page_id: page.id,
        template_revision_id: template.id,
        aligned_artifact_id: aligned.id,
        crop_artifact_id: crop.id,
        blank_artifact_id: blank.id,
    }
}

fn input(fixture: &Fixture) -> MaterializeAnswerSheetPageInput<'static> {
    let anchors = Box::leak(anchors().into_boxed_slice());
    let regions = Box::leak(
        vec![AnswerSheetRegionArtifact {
            assessment_item_id: 1,
            region_index: 0,
            crop_artifact_id: fixture.crop_artifact_id,
        }]
        .into_boxed_slice(),
    );
    MaterializeAnswerSheetPageInput {
        page_id: fixture.page_id,
        template_revision_id: fixture.template_revision_id,
        aligned_artifact_id: fixture.aligned_artifact_id,
        region_artifacts: regions,
        template_to_source: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        detected_anchors: anchors,
        alignment_confidence: 0.99,
        confirmed_by: "teacher",
    }
}

#[test]
fn materialization_is_atomic_idempotent_and_has_no_grading_effects() {
    let fixture = setup();
    let first = {
        let tx = fixture.conn.unchecked_transaction().unwrap();
        let result = materialize_in_transaction(&tx, &input(&fixture)).unwrap();
        tx.commit().unwrap();
        result
    };
    let second = {
        let tx = fixture.conn.unchecked_transaction().unwrap();
        let result = materialize_in_transaction(&tx, &input(&fixture)).unwrap();
        tx.commit().unwrap();
        result
    };
    assert_eq!(first.materialization.id, second.materialization.id);
    assert_eq!(first.regions.len(), 1);
    assert_eq!(first.routes.len(), 1);
    assert_eq!(first.routes[0].recognition_route, "objective_omr");
    let counts: (i64, i64, i64, i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_answer_sheet_page_materializations_v2),
               (SELECT COUNT(*) FROM exam_page_alignment_revisions_v2),
               (SELECT COUNT(*) FROM exam_answer_region_revisions_v2),
               (SELECT COUNT(*) FROM exam_answer_sheet_region_routes_v2),
               (SELECT COUNT(*) FROM exam_grade_decisions_v2),
               (SELECT COUNT(*) FROM exam_grade_publications_v2)",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(counts, (1, 1, 1, 1, 0, 0));
}

#[test]
fn invalid_crop_rolls_back_alignment_and_ledger() {
    let fixture = setup();
    let bad_regions = [AnswerSheetRegionArtifact {
        assessment_item_id: 1,
        region_index: 0,
        crop_artifact_id: fixture.blank_artifact_id,
    }];
    let detected = anchors();
    let tx = fixture.conn.unchecked_transaction().unwrap();
    let error = materialize_in_transaction(
        &tx,
        &MaterializeAnswerSheetPageInput {
            page_id: fixture.page_id,
            template_revision_id: fixture.template_revision_id,
            aligned_artifact_id: fixture.aligned_artifact_id,
            region_artifacts: &bad_regions,
            template_to_source: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            detected_anchors: &detected,
            alignment_confidence: 0.99,
            confirmed_by: "teacher",
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("答案裁剪"));
    drop(tx);
    let counts: (i64, i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_answer_sheet_page_materializations_v2),
               (SELECT COUNT(*) FROM exam_page_alignment_revisions_v2),
               (SELECT COUNT(*) FROM exam_answer_region_revisions_v2),
               (SELECT COUNT(*) FROM exam_answer_sheet_region_routes_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0, 0));
}

#[test]
fn region_route_migration_backfills_legacy_objective_materialization() {
    let fixture = setup();
    {
        let tx = fixture.conn.unchecked_transaction().unwrap();
        materialize_in_transaction(&tx, &input(&fixture)).unwrap();
        tx.commit().unwrap();
    }

    // 模拟 exam_0020 已有纯客观物化记录、尚未建立显式题区路由的升级现场。
    fixture
        .conn
        .execute_batch("DROP TABLE exam_answer_sheet_region_routes_v2")
        .unwrap();
    fixture
        .conn
        .execute_batch(include_str!(
            "../../migrations/0021_answer_sheet_region_routes.sql"
        ))
        .unwrap();

    let route: (String, String) = fixture
        .conn
        .query_row(
            "SELECT recognition_route,question_type
             FROM exam_answer_sheet_region_routes_v2",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(route, ("objective_omr".into(), "single".into()));
}
