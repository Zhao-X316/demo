use rusqlite::Connection;
use suite_core::db::repo::{ai_runs, artifacts};
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
use suite_core::domain::{hashing, time};
use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

use super::papers::{
    self, NewAnswerRegionRevision, NewIngestBatch, NewIngestPage, NewPageAlignmentRevision,
    NewPageMatchRevision, NewPageQualityRevision,
};
use super::{assessment, subjective};
use crate::dictation::DictationRecognitionState;
use crate::dictation_recognition::{
    DictationOcrOutput, DictationOcrRequest, DictationRecognizerDescriptor,
};
use crate::short_answer_grading::{
    ShortAnswerGradeOutput, ShortAnswerGradeState, ShortAnswerGraderDescriptor,
    ShortAnswerPointResult, ShortAnswerPointStatus, SHORT_ANSWER_GRADE_SCHEMA_VERSION,
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
           INSERT INTO k1_answer_slots
             (public_id,stable_id,answer_key_version_id,order_index,
              canonical_answers_json,max_score,created_at)
             VALUES ('answer-slot-s','year',1,0,
                     '{{"schema_version":1,"answers":["1842年"]}}',1,
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

fn add_multi_slot_fill_region(fixture: &mut Fixture) -> i64 {
    let hash = "9".repeat(64);
    fixture
        .conn
        .execute_batch(&format!(
            r#"INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('question-multi-fill','personal','teacher','unknown',0,
                         '2026-07-16T09:00:00.000Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES ('question-version-multi-fill',2,1,'fill_blank',
                         '《南京条约》签订于____年，开放____等通商口岸',2,'{hash}',
                         'L3','published','2026-07-16T09:00:00.000Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-multi-fill',2,1,
                         '{{"schema_version":1,"slots":[
                           {{"stable_id":"year","canonical_answers":["1842年"]}},
                           {{"stable_id":"port","canonical_answers":["广州"]}}
                         ]}}',
                         'confirmed','2026-07-16T09:00:00.000Z','teacher',
                         '2026-07-16T09:00:00.000Z');
               INSERT INTO k1_answer_slots
                 (public_id,stable_id,answer_key_version_id,order_index,
                  canonical_answers_json,max_score,created_at)
                 VALUES
                 ('answer-slot-multi-year','year',2,0,
                  '{{"schema_version":1,"answers":["1842年"]}}',1,
                  '2026-07-16T09:00:00.000Z'),
                 ('answer-slot-multi-port','port',2,1,
                  '{{"schema_version":1,"answers":["广州"]}}',1,
                  '2026-07-16T09:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('rubric-multi-fill',2,1,2,'confirmed',
                         '2026-07-16T09:00:00.000Z','teacher',
                         '2026-07-16T09:00:00.000Z');
               INSERT INTO k1_rubric_points
                 (public_id,rubric_version_id,stable_id,order_index,canonical_text,
                  max_score,created_at)
                 VALUES
                 ('rubric-point-multi-year',2,'year',0,'1842年',1,
                  '2026-07-16T09:00:00.000Z'),
                 ('rubric-point-multi-port',2,'port',1,'广州',1,
                  '2026-07-16T09:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('link-multi-fill',2,1,1,'confirmed',
                         '2026-07-16T09:00:00.000Z','teacher',
                         '2026-07-16T09:00:00.000Z');
               INSERT INTO k1_knowledge_nodes
                 (public_id,stable_id,knowledge_map_id,code,title,order_index,state,created_at)
                 VALUES
                 ('knowledge-multi-year','nanjing-treaty-year',1,'K-M1',
                  '南京条约签订时间',1,'active','2026-07-16T09:00:00.000Z'),
                 ('knowledge-multi-port','nanjing-treaty-port',1,'K-M2',
                  '南京条约通商口岸',2,'active','2026-07-16T09:00:00.000Z');
               INSERT INTO k1_knowledge_links
                 (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,
                  relation_type,confirmation_level,verified_by,verified_at,created_at)
                 VALUES
                 ('knowledge-link-multi-year',2,'answer_slot','answer-slot-multi-year',1,
                  'answer_basis','teacher_confirmed','teacher',
                  '2026-07-16T09:00:00.000Z','2026-07-16T09:00:00.000Z'),
                 ('knowledge-link-multi-port',2,'answer_slot','answer-slot-multi-port',2,
                  'answer_basis','teacher_confirmed','teacher',
                  '2026-07-16T09:00:00.000Z','2026-07-16T09:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                  state,created_at)
                 VALUES ('item-multi-fill',1,2,2,2,2,1,2,
                         '{{"schema_version":1,"question_no":"2","page_no":1}}',
                         'active','2026-07-16T09:00:00.000Z');"#
        ))
        .unwrap();
    let region = papers::record_answer_region(
        &fixture.conn,
        &NewAnswerRegionRevision {
            page_id: 1,
            assessment_item_id: 2,
            region_index: 1,
            bbox_json: r#"{"schema_version":1,"x":0.1,"y":0.5,"width":0.8,"height":0.15}"#,
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
             VALUES (1,?1,2,1,'handwriting_ocr','fill_blank',
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
fn published_single_slot_fill_activates_teacher_confirmed_slot_evidence() {
    let mut fixture = setup();
    fixture
        .conn
        .execute_batch(
            "INSERT INTO k1_knowledge_nodes
           (public_id,stable_id,knowledge_map_id,code,title,order_index,state,created_at)
         VALUES ('knowledge-fill','nanjing-treaty-year',1,'K1','南京条约签订时间',1,'active',
                 '2026-07-16T09:00:00.000Z');
         INSERT INTO k1_ability_dimensions
           (public_id,stable_id,subject_id,revision,code,title,state,created_at)
         VALUES ('ability-fill','fact-recall',1,1,'fact_recall','事实识记与提取','active',
                 '2026-07-16T09:00:00.000Z');
         INSERT INTO k1_knowledge_links
           (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,relation_type,
            confirmation_level,verified_by,verified_at,created_at)
         VALUES ('knowledge-link-fill',1,'answer_slot','answer-slot-s',1,'answer_basis',
                 'teacher_confirmed','teacher','2026-07-16T09:00:00.000Z',
                 '2026-07-16T09:00:00.000Z');
         INSERT INTO k1_ability_links
           (public_id,link_set_id,source_type,source_public_id,ability_dimension_id,
            evidence_strength,response_mode,confirmation_level,verified_by,verified_at,created_at)
         VALUES ('ability-link-fill',1,'answer_slot','answer-slot-s',1,0.6,'recall',
                 'teacher_confirmed','teacher','2026-07-16T09:00:00.000Z',
                 '2026-07-16T09:00:00.000Z');",
        )
        .unwrap();
    let run_id = successful_run(&mut fixture, "subjective-ocr-publish-fill", "1842 年");
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, run_id).unwrap();
    let row = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .pop()
        .unwrap();
    subjective::accept_subjective_suggestion(&fixture.conn, row.suggestion_id, "teacher").unwrap();
    assessment::publish_attempt(&fixture.conn, 1, "teacher").unwrap();
    let evidence: Vec<(String, String, String, f64)> = {
        let mut stmt = fixture
            .conn
            .prepare(
                "SELECT source_type,source_ref_type,source_ref_id,value
             FROM learning_evidence ORDER BY id",
            )
            .unwrap();
        stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    };
    assert_eq!(evidence.len(), 2);
    assert!(evidence.iter().all(|row| row.0 == "fill_blank_slot"));
    assert!(evidence
        .iter()
        .all(|row| row.1 == "answer_slot" && row.2 == "answer-slot-s"));
    assert!(evidence.iter().all(|row| (row.3 - 1.0).abs() < 0.000_001));
}

#[test]
fn multi_slot_fill_component_review_is_idempotent_immutable_and_publishes_slot_evidence() {
    let mut fixture = setup();
    let multi_region_id = add_multi_slot_fill_region(&mut fixture);

    let first_run = successful_run(&mut fixture, "subjective-ocr-base-before-multi", "1842年");
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, first_run).unwrap();
    let first_row = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .into_iter()
        .find(|row| row.assessment_item_id == 1)
        .unwrap();
    subjective::accept_subjective_suggestion(&fixture.conn, first_row.suggestion_id, "teacher")
        .unwrap();

    fixture.region_id = multi_region_id;
    let multi_run = successful_run(&mut fixture, "subjective-ocr-multi-fill", "1842年，广州");
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, multi_run).unwrap();
    let multi_row = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .into_iter()
        .find(|row| row.assessment_item_id == 2)
        .unwrap();
    assert_eq!(multi_row.suggestion_outcome, "unscored");
    assert_eq!(
        multi_row.exclusion_reason.as_deref(),
        Some("ANSWER_FORMAT_REVIEW_REQUIRED")
    );
    assert!(multi_row
        .answer_slots_json
        .contains("answer-slot-multi-year"));
    assert!(multi_row
        .answer_slots_json
        .contains("answer-slot-multi-port"));

    let components = vec![
        subjective::SubjectiveComponentGradeInput {
            source_type: "answer_slot".into(),
            source_public_id: "answer-slot-multi-year".into(),
            teacher_score: 1.0,
            evidence_text: Some("1842年".into()),
            teacher_note: None,
        },
        subjective::SubjectiveComponentGradeInput {
            source_type: "answer_slot".into(),
            source_public_id: "answer-slot-multi-port".into(),
            teacher_score: 0.0,
            evidence_text: None,
            teacher_note: Some("未按本题要求给分".into()),
        },
    ];
    let decision = subjective::correct_subjective_components(
        &fixture.conn,
        multi_row.suggestion_id,
        &components,
        "第一空正确，第二空不计分",
        "teacher",
    )
    .unwrap();
    assert_eq!(decision.teacher_score, 1.0);
    assert_eq!(decision.revision, 1);

    let repeated = subjective::correct_subjective_components(
        &fixture.conn,
        multi_row.suggestion_id,
        &components,
        "第一空正确，第二空不计分",
        "teacher",
    )
    .unwrap();
    assert_eq!(repeated.id, decision.id);
    let stored: Vec<(String, f64, String, Option<String>)> = {
        let mut stmt = fixture
            .conn
            .prepare(
                "SELECT source_public_id,teacher_score,result_status,evidence_text
                 FROM exam_grade_decision_subjective_components_v2
                 WHERE grade_decision_id=?1 ORDER BY order_index",
            )
            .unwrap();
        stmt.query_map([decision.id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    };
    assert_eq!(
        stored,
        vec![
            (
                "answer-slot-multi-year".into(),
                1.0,
                "correct".into(),
                Some("1842年".into())
            ),
            (
                "answer-slot-multi-port".into(),
                0.0,
                "incorrect".into(),
                None
            ),
        ]
    );
    let update_error = fixture
        .conn
        .execute(
            "UPDATE exam_grade_decision_subjective_components_v2
             SET teacher_score=0 WHERE grade_decision_id=?1",
            [decision.id],
        )
        .unwrap_err();
    assert!(update_error
        .to_string()
        .contains("M2_SUBJECTIVE_COMPONENT_IMMUTABLE"));
    let delete_error = fixture
        .conn
        .execute(
            "DELETE FROM exam_grade_decision_subjective_components_v2
             WHERE grade_decision_id=?1",
            [decision.id],
        )
        .unwrap_err();
    assert!(delete_error
        .to_string()
        .contains("M2_SUBJECTIVE_COMPONENT_IMMUTABLE"));

    assessment::publish_attempt(&fixture.conn, 1, "teacher").unwrap();
    let evidence: Vec<(String, String, f64, String)> = {
        let mut stmt = fixture
            .conn
            .prepare(
                "SELECT source_type,source_ref_id,value,rule_version
                 FROM learning_evidence
                 WHERE source_ref_id IN ('answer-slot-multi-year','answer-slot-multi-port')
                 ORDER BY source_ref_id",
            )
            .unwrap();
        stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap()
    };
    assert_eq!(
        evidence,
        vec![
            (
                "fill_blank_slot".into(),
                "answer-slot-multi-port".into(),
                0.0,
                "fill-blank-teacher-components-v1".into()
            ),
            (
                "fill_blank_slot".into(),
                "answer-slot-multi-year".into(),
                1.0,
                "fill-blank-teacher-components-v1".into()
            ),
        ]
    );
}

#[test]
fn component_review_rejects_incomplete_foreign_and_non_evidence_inputs_atomically() {
    let mut fixture = setup();
    let run_id = successful_run(&mut fixture, "subjective-ocr-invalid-component", "1842年");
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, run_id).unwrap();
    let row = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .pop()
        .unwrap();

    let incomplete = subjective::correct_subjective_components(
        &fixture.conn,
        row.suggestion_id,
        &[],
        "查看原图",
        "teacher",
    )
    .unwrap_err();
    assert!(incomplete.to_string().contains("必须完整提交"));
    let foreign = subjective::correct_subjective_components(
        &fixture.conn,
        row.suggestion_id,
        &[subjective::SubjectiveComponentGradeInput {
            source_type: "answer_slot".into(),
            source_public_id: "answer-slot-from-another-version".into(),
            teacher_score: 1.0,
            evidence_text: Some("1842年".into()),
            teacher_note: None,
        }],
        "查看原图",
        "teacher",
    )
    .unwrap_err();
    assert!(foreign.to_string().contains("必须完整提交"));
    let invented_evidence = subjective::correct_subjective_components(
        &fixture.conn,
        row.suggestion_id,
        &[subjective::SubjectiveComponentGradeInput {
            source_type: "answer_slot".into(),
            source_public_id: "answer-slot-s".into(),
            teacher_score: 1.0,
            evidence_text: Some("1840年".into()),
            teacher_note: None,
        }],
        "查看原图",
        "teacher",
    )
    .unwrap_err();
    assert!(invented_evidence
        .to_string()
        .contains("不是当前学生转写原文片段"));
    let counts: (i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_grade_decisions_v2),
               (SELECT COUNT(*) FROM exam_grade_decision_subjective_components_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0));
}

#[test]
fn teacher_can_promote_full_score_fill_variant_without_changing_current_grade() {
    let mut fixture = setup();
    let run_id = successful_run(
        &mut fixture,
        "subjective-ocr-accepted-variant",
        "一八四二年",
    );
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, run_id).unwrap();
    let row = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .pop()
        .unwrap();
    assert_eq!(row.suggestion_outcome, "incorrect");
    let decision = subjective::correct_subjective_suggestion(
        &fixture.conn,
        row.suggestion_id,
        1.0,
        Some("中文年份写法等价，查看原图后判满分"),
        "teacher",
    )
    .unwrap();

    let promoted =
        subjective::promote_fill_accepted_answer(&mut fixture.conn, decision.id, "teacher")
            .unwrap();
    assert_eq!(promoted.outcome, "created_new_version");
    assert_eq!(promoted.accepted_text, "一八四二年");
    assert_eq!(promoted.adopted_assessment_revision, 2);
    assert!(promoted.current_grade_unchanged);
    assert!(promoted.current_publication_unchanged);

    let old_answer: String = fixture
        .conn
        .query_row(
            "SELECT answer_json FROM k1_answer_key_versions WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!old_answer.contains("一八四二年"));
    let (new_answer, supersedes): (String, i64) = fixture
        .conn
        .query_row(
            "SELECT answer_json,supersedes_answer_key_id
             FROM k1_answer_key_versions WHERE id=?1",
            [promoted.adopted_answer_key_version_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(new_answer.contains("一八四二年"));
    assert_eq!(supersedes, 1);
    let new_slot: String = fixture
        .conn
        .query_row(
            "SELECT canonical_answers_json FROM k1_answer_slots
             WHERE answer_key_version_id=?1 AND stable_id='year'",
            [promoted.adopted_answer_key_version_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(new_slot.contains("一八四二年"));

    let unchanged: (i64, f64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT attempt.assessment_version_id,decision.teacher_score,
                    (SELECT COUNT(*) FROM exam_grade_publications_v2),
                    (SELECT COUNT(*) FROM learning_evidence)
             FROM exam_grade_decisions_v2 decision
             JOIN exam_attempts_v2 attempt ON attempt.id=decision.attempt_id
             WHERE decision.id=?1 AND decision.state='active'",
            [decision.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(unchanged, (1, 1.0, 0, 0));
    let workbench = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10).unwrap();
    assert_eq!(
        workbench.rows[0].accepted_answer_promotion_id,
        promoted.promotion_id
    );

    let repeated =
        subjective::promote_fill_accepted_answer(&mut fixture.conn, decision.id, "teacher")
            .unwrap();
    assert_eq!(repeated.outcome, "already_promoted");
    assert_eq!(repeated.promotion_id, promoted.promotion_id);
    let version_counts: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM k1_answer_key_versions),
                    (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                    (SELECT COUNT(*) FROM exam_accepted_answer_promotions_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(version_counts, (2, 2, 1));
}

#[test]
fn later_accepted_variants_accumulate_on_the_latest_future_answer_version() {
    let mut fixture = setup();
    let first_run = successful_run(&mut fixture, "subjective-ocr-first-variant", "一八四二年");
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, first_run).unwrap();
    let first_row = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .pop()
        .unwrap();
    let first_decision = subjective::correct_subjective_suggestion(
        &fixture.conn,
        first_row.suggestion_id,
        1.0,
        Some("确认中文年份写法"),
        "teacher",
    )
    .unwrap();
    subjective::promote_fill_accepted_answer(&mut fixture.conn, first_decision.id, "teacher")
        .unwrap();

    subjective::teacher_correct_transcription(
        &mut fixture.conn,
        fixture.region_id,
        "公元一八四二年",
        "teacher",
    )
    .unwrap();
    let second_row = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .pop()
        .unwrap();
    let second_decision = subjective::correct_subjective_suggestion(
        &fixture.conn,
        second_row.suggestion_id,
        1.0,
        Some("确认带公元前缀的等价写法"),
        "teacher",
    )
    .unwrap();
    let second =
        subjective::promote_fill_accepted_answer(&mut fixture.conn, second_decision.id, "teacher")
            .unwrap();
    assert_eq!(second.adopted_assessment_revision, 3);
    let latest_answer: String = fixture
        .conn
        .query_row(
            "SELECT answer_json FROM k1_answer_key_versions WHERE id=?1",
            [second.adopted_answer_key_version_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(latest_answer.contains("一八四二年"));
    assert!(latest_answer.contains("公元一八四二年"));
    let counts: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM k1_answer_key_versions),
                    (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                    (SELECT COUNT(*) FROM exam_accepted_answer_promotions_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (3, 3, 2));
}

#[test]
fn accepted_answer_promotion_rejects_non_manual_or_non_full_score_decisions() {
    let mut exact_fixture = setup();
    let exact_run = successful_run(&mut exact_fixture, "subjective-ocr-existing", "1842年");
    subjective::record_ocr_ai_run_transcription(&mut exact_fixture.conn, exact_run).unwrap();
    let exact_row = subjective::list_subjective_workbench(&exact_fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .pop()
        .unwrap();
    let accepted = subjective::accept_subjective_suggestion(
        &exact_fixture.conn,
        exact_row.suggestion_id,
        "teacher",
    )
    .unwrap();
    assert!(subjective::promote_fill_accepted_answer(
        &mut exact_fixture.conn,
        accepted.id,
        "teacher",
    )
    .is_err());

    let mut zero_fixture = setup();
    let zero_run = successful_run(&mut zero_fixture, "subjective-ocr-zero", "1840年");
    subjective::record_ocr_ai_run_transcription(&mut zero_fixture.conn, zero_run).unwrap();
    let zero_row = subjective::list_subjective_workbench(&zero_fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .pop()
        .unwrap();
    let zero = subjective::correct_subjective_suggestion(
        &zero_fixture.conn,
        zero_row.suggestion_id,
        0.0,
        Some("年份错误"),
        "teacher",
    )
    .unwrap();
    assert!(
        subjective::promote_fill_accepted_answer(&mut zero_fixture.conn, zero.id, "teacher",)
            .is_err()
    );
    let side_effects: (i64, i64, i64) = zero_fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM k1_answer_key_versions),
                    (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                    (SELECT COUNT(*) FROM exam_accepted_answer_promotions_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(side_effects, (1, 1, 0));
}

#[test]
fn accepted_answer_promotion_ledger_is_immutable() {
    let mut fixture = setup();
    let run_id = successful_run(&mut fixture, "subjective-ocr-ledger", "一八四二年");
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, run_id).unwrap();
    let row = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .pop()
        .unwrap();
    let decision = subjective::correct_subjective_suggestion(
        &fixture.conn,
        row.suggestion_id,
        1.0,
        Some("确认等价写法"),
        "teacher",
    )
    .unwrap();
    let promotion =
        subjective::promote_fill_accepted_answer(&mut fixture.conn, decision.id, "teacher")
            .unwrap();
    let promotion_id = promotion.promotion_id.unwrap();
    let update_error = fixture
        .conn
        .execute(
            "UPDATE exam_accepted_answer_promotions_v2 SET accepted_text='改写' WHERE id=?1",
            [promotion_id],
        )
        .unwrap_err();
    assert!(update_error
        .to_string()
        .contains("M2_ACCEPTED_ANSWER_PROMOTION_IMMUTABLE"));
    let delete_error = fixture
        .conn
        .execute(
            "DELETE FROM exam_accepted_answer_promotions_v2 WHERE id=?1",
            [promotion_id],
        )
        .unwrap_err();
    assert!(delete_error
        .to_string()
        .contains("M2_ACCEPTED_ANSWER_PROMOTION_IMMUTABLE"));
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
fn teacher_corrected_short_answer_points_publish_rubric_evidence() {
    let mut fixture = setup();
    let base_run = successful_run(
        &mut fixture,
        "subjective-ocr-base-before-short-components",
        "1842年",
    );
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, base_run).unwrap();
    let base_row = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .pop()
        .unwrap();
    subjective::accept_subjective_suggestion(&fixture.conn, base_row.suggestion_id, "teacher")
        .unwrap();

    fixture.region_id = add_short_answer_region(&mut fixture);
    fixture
        .conn
        .execute_batch(
            "INSERT INTO k1_knowledge_nodes
               (public_id,stable_id,knowledge_map_id,code,title,order_index,state,created_at)
             VALUES ('knowledge-short-institution','westernization-institution',1,'K-S1',
                     '洋务运动制度局限',1,'active','2026-07-16T09:00:00.000Z');
             INSERT INTO k1_knowledge_links
               (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,
                relation_type,confirmation_level,verified_by,verified_at,created_at)
             VALUES ('knowledge-link-short-institution',2,'rubric_point',
                     'rubric-point-short',1,'rubric_basis','teacher_confirmed','teacher',
                     '2026-07-16T09:00:00.000Z','2026-07-16T09:00:00.000Z');",
        )
        .unwrap();
    let short_run = successful_run(
        &mut fixture,
        "subjective-ocr-short-components",
        "只学习技术，没有改变封建制度",
    );
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, short_run).unwrap();
    let row = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .into_iter()
        .find(|row| row.question_type == "short_answer")
        .unwrap();
    let decision = subjective::correct_subjective_components(
        &fixture.conn,
        row.suggestion_id,
        &[subjective::SubjectiveComponentGradeInput {
            source_type: "rubric_point".into(),
            source_public_id: "rubric-point-short".into(),
            teacher_score: 4.0,
            evidence_text: Some("没有改变封建制度".into()),
            teacher_note: Some("明确写出根本局限".into()),
        }],
        "按评分点核对原图后给满分",
        "teacher",
    )
    .unwrap();
    assert_eq!(decision.teacher_score, 4.0);
    assert!(decision
        .point_results_json
        .contains("answer_sheet_subjective_teacher_component_correction"));

    let current = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .into_iter()
        .find(|row| row.question_type == "short_answer")
        .unwrap();
    assert!(current.current_suggestion_confirmed);
    assert!(current
        .teacher_components_json
        .contains("rubric-point-short"));
    assert!(current.teacher_components_json.contains("没有改变封建制度"));

    assessment::publish_attempt(&fixture.conn, 1, "teacher").unwrap();
    let evidence: (String, String, f64, String, String) = fixture
        .conn
        .query_row(
            "SELECT source_type,source_ref_id,value,rule_version,confirmation_level
             FROM learning_evidence
             WHERE source_ref_id='rubric-point-short'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        evidence,
        (
            "question_rubric_point".into(),
            "rubric-point-short".into(),
            1.0,
            "short-answer-teacher-components-v1".into(),
            "teacher_corrected".into(),
        )
    );
}

#[test]
fn teacher_can_promote_short_answer_evidence_to_future_rubric_without_regrading_history() {
    let mut fixture = setup();
    fixture.region_id = add_short_answer_region(&mut fixture);
    fixture
        .conn
        .execute_batch(
            "INSERT INTO k1_knowledge_nodes
               (public_id,stable_id,knowledge_map_id,code,title,order_index,state,created_at)
             VALUES ('knowledge-rubric-promotion','westernization-institution',1,'K-RP',
                     '洋务运动制度局限',1,'active','2026-07-16T09:00:00.000Z');
             INSERT INTO k1_ability_dimensions
               (public_id,stable_id,subject_id,revision,code,title,state,created_at)
             VALUES ('ability-rubric-promotion','historical-explanation',1,1,'A-RP',
                     '历史解释','active','2026-07-16T09:00:00.000Z');
             INSERT INTO k1_knowledge_links
               (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,
                relation_type,confirmation_level,verified_by,verified_at,created_at)
             VALUES ('knowledge-link-rubric-promotion',2,'rubric_point',
                     'rubric-point-short',1,'rubric_basis','teacher_confirmed','teacher',
                     '2026-07-16T09:00:00.000Z','2026-07-16T09:00:00.000Z');
             INSERT INTO k1_ability_links
               (public_id,link_set_id,source_type,source_public_id,ability_dimension_id,
                evidence_strength,response_mode,confirmation_level,verified_by,verified_at,
                created_at)
             VALUES ('ability-link-rubric-promotion',2,'rubric_point',
                     'rubric-point-short',1,0.8,'structured_response',
                     'teacher_confirmed','teacher','2026-07-16T09:00:00.000Z',
                     '2026-07-16T09:00:00.000Z');",
        )
        .unwrap();
    let run_id = successful_run(
        &mut fixture,
        "subjective-ocr-rubric-promotion",
        "只学习技术，没有改变封建制度",
    );
    subjective::record_ocr_ai_run_transcription(&mut fixture.conn, run_id).unwrap();
    let row = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10)
        .unwrap()
        .rows
        .into_iter()
        .find(|row| row.question_type == "short_answer")
        .unwrap();
    let decision = subjective::correct_subjective_components(
        &fixture.conn,
        row.suggestion_id,
        &[subjective::SubjectiveComponentGradeInput {
            source_type: "rubric_point".into(),
            source_public_id: "rubric-point-short".into(),
            teacher_score: 4.0,
            evidence_text: Some("只学习技术".into()),
            teacher_note: Some("属于制度局限的合理表述".into()),
        }],
        "查看原图后确认该表述可给分",
        "teacher",
    )
    .unwrap();

    let promoted = subjective::promote_short_answer_rubric_evidence(
        &mut fixture.conn,
        decision.id,
        "rubric-point-short",
        "teacher",
    )
    .unwrap();
    assert_eq!(promoted.outcome, "created_new_version");
    assert_eq!(promoted.evidence_text, "只学习技术");
    assert_eq!(promoted.adopted_assessment_revision, 2);
    assert_eq!(promoted.carried_knowledge_link_count, 1);
    assert_eq!(promoted.carried_ability_link_count, 1);
    assert!(promoted.current_grade_unchanged);
    assert!(promoted.current_publication_unchanged);

    let old_allowed: Option<String> = fixture
        .conn
        .query_row(
            "SELECT allowed_paraphrases_json FROM k1_rubric_points WHERE id=2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(old_allowed.is_none());
    let (new_public_id, new_allowed): (String, String) = fixture
        .conn
        .query_row(
            "SELECT public_id,allowed_paraphrases_json
             FROM k1_rubric_points
             WHERE rubric_version_id=?1 AND stable_id='institution'",
            [promoted.adopted_rubric_version_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_ne!(new_public_id, "rubric-point-short");
    assert!(new_allowed.contains("只学习技术"));
    let carried_sources: (String, String) = fixture
        .conn
        .query_row(
            "SELECT
               (SELECT source_public_id FROM k1_knowledge_links
                WHERE link_set_id=?1),
               (SELECT source_public_id FROM k1_ability_links
                WHERE link_set_id=?1)",
            [promoted.adopted_link_set_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(carried_sources, (new_public_id.clone(), new_public_id));
    let unchanged: (i64, f64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT attempt.assessment_version_id,decision.teacher_score,
                    (SELECT COUNT(*) FROM exam_grade_publications_v2),
                    (SELECT COUNT(*) FROM learning_evidence)
             FROM exam_grade_decisions_v2 decision
             JOIN exam_attempts_v2 attempt ON attempt.id=decision.attempt_id
             WHERE decision.id=?1 AND decision.state='active'",
            [decision.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(unchanged, (1, 4.0, 0, 0));
    let workbench = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10).unwrap();
    let promoted_row = workbench
        .rows
        .into_iter()
        .find(|row| row.question_type == "short_answer")
        .unwrap();
    assert!(promoted_row
        .rubric_evidence_promotions_json
        .contains("只学习技术"));

    let repeated = subjective::promote_short_answer_rubric_evidence(
        &mut fixture.conn,
        decision.id,
        "rubric-point-short",
        "teacher",
    )
    .unwrap();
    assert_eq!(repeated.outcome, "already_promoted");
    assert_eq!(repeated.promotion_id, promoted.promotion_id);
    let promotion_id = promoted.promotion_id.unwrap();
    let update_error = fixture
        .conn
        .execute(
            "UPDATE exam_rubric_evidence_promotions_v2
             SET evidence_text='改写' WHERE id=?1",
            [promotion_id],
        )
        .unwrap_err();
    assert!(update_error
        .to_string()
        .contains("M2_RUBRIC_EVIDENCE_PROMOTION_IMMUTABLE"));
    let delete_error = fixture
        .conn
        .execute(
            "DELETE FROM exam_rubric_evidence_promotions_v2 WHERE id=?1",
            [promotion_id],
        )
        .unwrap_err();
    assert!(delete_error
        .to_string()
        .contains("M2_RUBRIC_EVIDENCE_PROMOTION_IMMUTABLE"));
}

#[test]
fn answer_grade_run_becomes_audited_point_suggestion_only_after_teacher_accepts() {
    let mut fixture = setup();
    fixture.region_id = add_short_answer_region(&mut fixture);
    let ocr_run_id = successful_run(
        &mut fixture,
        "subjective-ocr-short-grade",
        "只学习技术，没有改变封建制度",
    );
    let transcription =
        subjective::record_ocr_ai_run_transcription(&mut fixture.conn, ocr_run_id).unwrap();
    let request =
        subjective::load_short_answer_grade_request(&fixture.conn, transcription.id).unwrap();
    assert_eq!(request.rubric_points.len(), 1);
    let descriptor = ShortAnswerGraderDescriptor {
        provider: "fixture".into(),
        model_name: "fixture-grader".into(),
        model_version: "v1".into(),
        config_version: "short-answer-v1".into(),
        rule_version: "rubric-evidence-v1".into(),
    };
    let input_hash = request.input_hash().unwrap();
    let run = ai_runs::create_or_get(
        &fixture.conn,
        &ai_runs::NewAiRun {
            idempotency_key: "subjective-short-answer-grade-1",
            run_type: "answer_grade",
            source_module: "exam",
            business_ref_type: "subjective_transcription_revision",
            business_ref_id: &transcription.id.to_string(),
            input_artifact_id: Some(request.crop_artifact_id),
            provider: &descriptor.provider,
            model_name: &descriptor.model_name,
            model_version: &descriptor.model_version,
            config_version: &descriptor.config_version,
            prompt_or_rule_version: &descriptor.rule_version,
            input_hash: &input_hash,
            retry_of_ai_run_id: None,
        },
    )
    .unwrap();
    ai_runs::start(&fixture.conn, run.id, &time::utc_now_rfc3339(), None).unwrap();
    let output = ShortAnswerGradeOutput {
        schema_version: SHORT_ANSWER_GRADE_SCHEMA_VERSION,
        transcription_revision_id: transcription.id,
        assessment_item_id: request.assessment_item_id,
        answer_key_version_id: request.answer_key_version_id,
        rubric_version_id: request.rubric_version_id,
        input_hash,
        descriptor,
        state: ShortAnswerGradeState::Ready,
        suggested_score: 4.0,
        point_results: vec![ShortAnswerPointResult {
            rubric_point_id: request.rubric_points[0].rubric_point_id,
            stable_id: request.rubric_points[0].stable_id.clone(),
            status: ShortAnswerPointStatus::Covered,
            suggested_score: 4.0,
            evidence_snippets: vec!["没有改变封建制度".into()],
            reason: "学生明确写出制度局限".into(),
            confidence: 0.97,
        }],
        confidence: 0.97,
        issue_codes: vec![],
    };
    let output_json = output.to_json_against(&request).unwrap();
    ai_runs::finalize_succeeded(
        &fixture.conn,
        run.id,
        &hashing::sha256_hex(output_json.as_bytes()),
        Some(output.confidence),
        &output_json,
        &time::utc_now_rfc3339(),
    )
    .unwrap();

    let analysis = subjective::record_short_answer_grade_ai_run(&mut fixture.conn, run.id).unwrap();
    assert_eq!(analysis.outcome, "correct");
    assert_eq!(analysis.suggested_score, 4.0);
    let workbench = subjective::list_subjective_workbench(&fixture.conn, Some(1), 10).unwrap();
    let row = workbench
        .rows
        .iter()
        .find(|row| row.question_type == "short_answer")
        .unwrap();
    assert_eq!(row.short_answer_analysis_id, Some(analysis.id));
    assert_eq!(row.machine_grade_ai_run_id, Some(run.id));
    assert_eq!(row.suggested_score, Some(4.0));
    assert!(!row.batch_eligible);
    assert_eq!(workbench.attempts[0].confirmed_count, 0);

    let decision =
        subjective::accept_subjective_suggestion(&fixture.conn, row.suggestion_id, "teacher")
            .unwrap();
    assert_eq!(decision.machine_grade_ai_run_id, Some(run.id));
    let source: (i64, i64) = fixture
        .conn
        .query_row(
            "SELECT analysis_id,machine_grade_ai_run_id
             FROM exam_grade_decision_short_answer_sources_v2
             WHERE grade_decision_id=?1",
            [decision.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(source, (analysis.id, run.id));
    let publication_and_evidence: (i64, i64) = (
        fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_grade_publications_v2",
                [],
                |row| row.get(0),
            )
            .unwrap(),
        fixture
            .conn
            .query_row("SELECT COUNT(*) FROM learning_evidence", [], |row| {
                row.get(0)
            })
            .unwrap(),
    );
    assert_eq!(publication_and_evidence, (0, 0));
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
