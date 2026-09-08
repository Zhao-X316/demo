use rusqlite::Connection;
use suite_core::db::repo::{ai_runs, artifacts};
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
use suite_core::domain::hashing;
use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

use super::assessment::publish_attempt;
use super::dictation_pipeline::{
    accept_dictation_suggestion, confirm_template_and_policies, correct_dictation_grade,
    list_dictation_workbench, materialize_in_transaction, record_ocr_ai_run_transcription,
    strict_batch_accept, teacher_correct_transcription, DictationRegionArtifact,
    MaterializeDictationPageInput, StrictDictationBatchReview,
};
use super::papers::{
    self, NewIngestBatch, NewIngestPage, NewPageMatchRevision, NewPageQualityRevision,
};
use crate::dictation::DictationRecognitionState;
use crate::dictation_recognition::{
    DictationOcrOutput, DictationRecognizerDescriptor, DictationRegionProposal,
    DictationTemplateOutput, DictationTemplateState, DICTATION_OCR_SCHEMA_VERSION,
    DICTATION_TEMPLATE_SCHEMA_VERSION,
};
use crate::ordinary_paper_recognition::NormalizedRect;

struct Fixture {
    conn: Connection,
    page_id: i64,
    blank_artifact_id: i64,
    blank_hash: String,
    aligned_artifact_id: i64,
    crop_artifact_id: i64,
    crop_hash: String,
}

fn descriptor(rule: &str) -> DictationRecognizerDescriptor {
    DictationRecognizerDescriptor {
        provider: "fixture".into(),
        model_name: "fixture-vision".into(),
        model_version: "v1".into(),
        config_version: "v1".into(),
        rule_version: rule.into(),
    }
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
             VALUES ('assessment-d','默写练习',1,'homework','include','active','teacher',
                     '2026-07-15T10:00:00.000Z','2026-07-15T10:00:00.000Z');
           INSERT INTO exam_assessment_versions_v2
             (public_id,assessment_id,revision,item_set_hash,template_version,state,
              created_at,confirmed_by,confirmed_at)
             VALUES ('assessment-version-d',1,1,'{hash}','dictation-v1','confirmed',
                     '2026-07-15T10:00:00.000Z','teacher','2026-07-15T10:00:00.000Z');
           INSERT INTO k1_textbook_editions
             (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
             VALUES ('edition-d',1,'PEP','2024','中国历史八上','8','upper','active',
                     '2026-07-15T10:00:00.000Z');
           INSERT INTO k1_knowledge_maps
             (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
             VALUES ('map-d',1,1,'confirmed','2026-07-15T10:00:00.000Z',
                     '2026-07-15T10:00:00.000Z');
           INSERT INTO k1_questions
             (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
             VALUES ('question-d','personal','teacher','unknown',0,
                     '2026-07-15T10:00:00.000Z');
           INSERT INTO k1_question_versions
             (public_id,question_id,revision,question_type,stem,max_score,content_hash,
              quality_level,state,created_at)
             VALUES ('question-version-d',1,1,'fill_blank','《南京条约》签订于____年',2,
                     '{hash}','L3','published','2026-07-15T10:00:00.000Z');
           INSERT INTO k1_answer_key_versions
             (public_id,question_version_id,revision,answer_json,state,created_at,
              confirmed_by,confirmed_at)
             VALUES ('answer-d',1,1,'{{"schema_version":1,"text":"1842年"}}',
                     'confirmed','2026-07-15T10:00:00.000Z','teacher',
                     '2026-07-15T10:00:00.000Z');
           INSERT INTO k1_rubric_versions
             (public_id,question_version_id,revision,max_score,state,created_at,
              confirmed_by,confirmed_at)
             VALUES ('rubric-d',1,1,2,'confirmed','2026-07-15T10:00:00.000Z',
                     'teacher','2026-07-15T10:00:00.000Z');
           INSERT INTO k1_rubric_points
             (public_id,stable_id,rubric_version_id,order_index,canonical_text,
              allowed_paraphrases_json,max_score,created_at)
             VALUES ('point-d','treaty-year',1,0,'1842年','["一八四二年"]',2,
                     '2026-07-15T10:00:00.000Z');
           INSERT INTO k1_link_sets
             (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
              confirmed_by,confirmed_at)
             VALUES ('link-d',1,1,1,'confirmed','2026-07-15T10:00:00.000Z',
                     'teacher','2026-07-15T10:00:00.000Z');
           INSERT INTO k1_knowledge_nodes
             (public_id,stable_id,knowledge_map_id,code,title,order_index,state,created_at)
             VALUES ('knowledge-d','knowledge-d-stable',1,'K-D','南京条约签订时间',0,'active',
                     '2026-07-15T10:00:00.000Z');
           INSERT INTO k1_ability_dimensions
             (public_id,stable_id,subject_id,revision,code,title,state,created_at)
             VALUES ('ability-d','ability-d-stable',1,1,'fact_recall','事实识记与提取','active',
                     '2026-07-15T10:00:00.000Z');
           INSERT INTO k1_knowledge_links
             (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,
              relation_type,confirmation_level,verified_by,verified_at,created_at)
             VALUES ('knowledge-link-d',1,'rubric_point','point-d',1,'rubric_basis',
                     'teacher_confirmed','teacher','2026-07-15T10:00:00.000Z',
                     '2026-07-15T10:00:00.000Z');
           INSERT INTO k1_ability_links
             (public_id,link_set_id,source_type,source_public_id,ability_dimension_id,
              evidence_strength,response_mode,confirmation_level,verified_by,verified_at,created_at)
             VALUES ('ability-link-d',1,'rubric_point','point-d',1,0.8,'recall',
                     'teacher_confirmed','teacher','2026-07-15T10:00:00.000Z',
                     '2026-07-15T10:00:00.000Z');
           INSERT INTO exam_assessment_items_v2
             (public_id,assessment_version_id,question_version_id,answer_key_version_id,
              rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
              state,created_at)
             VALUES ('item-d',1,1,1,1,1,0,2,
                     '{{"schema_version":1,"question_no":"1","page_no":1}}',
                     'active','2026-07-15T10:00:00.000Z');
           INSERT INTO exam_attempts_v2
             (public_id,assessment_version_id,student_id,attempt_no,source_kind,
              attempt_kind,state,created_at,updated_at)
             VALUES ('attempt-d',1,1,1,'image','first','grading',
                     '2026-07-15T10:00:00.000Z','2026-07-15T10:00:00.000Z');"#
    ))
    .unwrap();
    let blank_hash = "b".repeat(64);
    let blank = artifacts::create_or_get(
        &conn,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Image,
            sha256: &blank_hash,
            mime_type: "image/jpeg",
            byte_size: 100,
            original_name: Some("blank.jpg"),
            original_path: None,
            archived_path: "/archive/dictation-blank.jpg",
            parent_artifact_id: None,
            derivative_type: None,
            processing_version: "fixture-v1",
            privacy_class: PrivacyClass::TeachingContent,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap();
    let source = artifacts::create_or_get(
        &conn,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Image,
            sha256: &"c".repeat(64),
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
    let crop_hash = "d".repeat(64);
    let aligned = artifacts::create_or_get(
        &conn,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Page,
            sha256: &"9".repeat(64),
            mime_type: "image/jpeg",
            byte_size: 180,
            original_name: None,
            original_path: None,
            archived_path: "/archive/dictation-aligned.jpg",
            parent_artifact_id: Some(source.id),
            derivative_type: Some("dictation_aligned_input"),
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
            sha256: &crop_hash,
            mime_type: "image/jpeg",
            byte_size: 80,
            original_name: None,
            original_path: None,
            archived_path: "/archive/dictation-crop.jpg",
            parent_artifact_id: Some(aligned.id),
            derivative_type: Some("dictation_answer_region"),
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
            idempotency_key: "dictation-fixture",
            created_by: "teacher",
        },
    )
    .unwrap();
    conn.execute(
        "INSERT INTO exam_material_type_revisions_v2
         (public_id,ingest_batch_id,revision,material_type,confidence,evidence_json,
          decision,state,created_by_type,created_by,confirmed_by,created_at)
         VALUES ('material-d',?1,1,'dictation',1.0,'{\"schema_version\":1}',
                 'teacher_confirmed','active','teacher','teacher','teacher',
                 '2026-07-15T10:00:00.000Z')",
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
        blank_artifact_id: blank.id,
        blank_hash,
        aligned_artifact_id: aligned.id,
        crop_artifact_id: crop.id,
        crop_hash,
    }
}

fn create_template(fixture: &mut Fixture) -> i64 {
    let input_hash = "e".repeat(64);
    let output = DictationTemplateOutput {
        schema_version: DICTATION_TEMPLATE_SCHEMA_VERSION,
        assessment_version_id: 1,
        page_no: 1,
        blank_artifact_id: fixture.blank_artifact_id,
        blank_artifact_sha256: fixture.blank_hash.clone(),
        input_hash: input_hash.clone(),
        template_version: "dictation-v1".into(),
        descriptor: descriptor("template-v1"),
        state: DictationTemplateState::Ready,
        canvas_width: 1000,
        canvas_height: 1400,
        regions: vec![DictationRegionProposal {
            assessment_item_id: 1,
            region_index: 0,
            bbox: NormalizedRect {
                x: 0.1,
                y: 0.2,
                width: 0.8,
                height: 0.1,
            },
            mapping_confidence: 0.99,
        }],
        confidence: 0.99,
        issue_codes: vec![],
    };
    let output_json = serde_json::to_string(&output).unwrap();
    let run = ai_runs::create_or_get(
        &fixture.conn,
        &ai_runs::NewAiRun {
            idempotency_key: "dictation-template-fixture",
            run_type: "dictation_template",
            source_module: "exam",
            business_ref_type: "assessment_page",
            business_ref_id: "1:1",
            input_artifact_id: Some(fixture.blank_artifact_id),
            provider: "fixture",
            model_name: "fixture-vision",
            model_version: "v1",
            config_version: "v1",
            prompt_or_rule_version: "template-v1",
            input_hash: &input_hash,
            retry_of_ai_run_id: None,
        },
    )
    .unwrap();
    ai_runs::start(&fixture.conn, run.id, "2026-07-15T10:00:01.000Z", None).unwrap();
    ai_runs::finalize_succeeded(
        &fixture.conn,
        run.id,
        &hashing::sha256_hex(output_json.as_bytes()),
        Some(0.99),
        &output_json,
        "2026-07-15T10:00:02.000Z",
    )
    .unwrap();
    let confirmed =
        confirm_template_and_policies(&mut fixture.conn, &output, run.id, "teacher").unwrap();
    confirmed.template.id
}

fn create_ocr_run(fixture: &Fixture, region_id: i64) -> i64 {
    let input_hash = "f".repeat(64);
    let output = DictationOcrOutput {
        schema_version: DICTATION_OCR_SCHEMA_VERSION,
        answer_region_revision_id: region_id,
        crop_artifact_id: fixture.crop_artifact_id,
        crop_artifact_sha256: fixture.crop_hash.clone(),
        input_hash: input_hash.clone(),
        descriptor: descriptor("ocr-v1"),
        state: DictationRecognitionState::Recognized,
        raw_text: Some("1840年".into()),
        normalized_text: Some("1840年".into()),
        confidence: Some(0.98),
        issue_codes: vec![],
    };
    let output_json = serde_json::to_string(&output).unwrap();
    let region = region_id.to_string();
    let run = ai_runs::create_or_get(
        &fixture.conn,
        &ai_runs::NewAiRun {
            idempotency_key: "dictation-ocr-fixture",
            run_type: "dictation_ocr",
            source_module: "exam",
            business_ref_type: "answer_region_revision",
            business_ref_id: &region,
            input_artifact_id: Some(fixture.crop_artifact_id),
            provider: "fixture",
            model_name: "fixture-vision",
            model_version: "v1",
            config_version: "v1",
            prompt_or_rule_version: "ocr-v1",
            input_hash: &input_hash,
            retry_of_ai_run_id: None,
        },
    )
    .unwrap();
    ai_runs::start(&fixture.conn, run.id, "2026-07-15T10:00:03.000Z", None).unwrap();
    ai_runs::finalize_succeeded(
        &fixture.conn,
        run.id,
        &hashing::sha256_hex(output_json.as_bytes()),
        Some(0.98),
        &output_json,
        "2026-07-15T10:00:04.000Z",
    )
    .unwrap();
    run.id
}

fn prepare_machine_result(fixture: &mut Fixture) -> (i64, i64) {
    let template_revision_id = create_template(fixture);
    let materialized = materialize_in_transaction(
        &fixture.conn,
        &MaterializeDictationPageInput {
            page_id: fixture.page_id,
            template_revision_id,
            aligned_artifact_id: fixture.aligned_artifact_id,
            regions: &[DictationRegionArtifact {
                assessment_item_id: 1,
                region_index: 0,
                crop_artifact_id: fixture.crop_artifact_id,
            }],
            confirmed_by: "teacher",
        },
    )
    .unwrap();
    let region_id = materialized.regions[0].id;
    let ai_run_id = create_ocr_run(fixture, region_id);
    let machine = record_ocr_ai_run_transcription(&mut fixture.conn, ai_run_id).unwrap();
    (region_id, machine.transcription.id)
}

#[test]
fn wrong_ocr_is_never_repaired_and_teacher_correction_is_append_only() {
    let mut fixture = setup();
    let template_revision_id = create_template(&mut fixture);
    let materialized = materialize_in_transaction(
        &fixture.conn,
        &MaterializeDictationPageInput {
            page_id: fixture.page_id,
            template_revision_id,
            aligned_artifact_id: fixture.aligned_artifact_id,
            regions: &[DictationRegionArtifact {
                assessment_item_id: 1,
                region_index: 0,
                crop_artifact_id: fixture.crop_artifact_id,
            }],
            confirmed_by: "teacher",
        },
    )
    .unwrap();
    let region_id = materialized.regions[0].id;
    let ai_run_id = create_ocr_run(&fixture, region_id);
    let machine = record_ocr_ai_run_transcription(&mut fixture.conn, ai_run_id).unwrap();
    assert_eq!(
        machine.transcription.raw_ocr_text.as_deref(),
        Some("1840年")
    );
    assert_eq!(machine.observation.result, "needs_review");
    assert_eq!(machine.observation.suggested_score, None);

    let corrected =
        teacher_correct_transcription(&mut fixture.conn, region_id, "1842年", "teacher").unwrap();
    assert_eq!(corrected.transcription.revision, 2);
    assert_eq!(
        corrected.transcription.raw_ocr_text.as_deref(),
        Some("1840年")
    );
    assert_eq!(
        corrected.transcription.teacher_corrected_text.as_deref(),
        Some("1842年")
    );
    assert_eq!(corrected.observation.result, "exact");
    assert_eq!(corrected.observation.suggested_score, Some(2.0));
    let old_state: String = fixture
        .conn
        .query_row(
            "SELECT state FROM exam_dictation_transcription_revisions_v2 WHERE id=?1",
            [machine.transcription.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(old_state, "superseded");
    let workbench = list_dictation_workbench(&fixture.conn, Some(1), 10).unwrap();
    assert_eq!(workbench.rows.len(), 1);
    assert!(!workbench.rows[0].requires_teacher_review);
    assert_eq!(workbench.rows[0].raw_ocr_text.as_deref(), Some("1840年"));
    let grade_count: i64 = fixture
        .conn
        .query_row("SELECT COUNT(*) FROM exam_grade_decisions_v2", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(grade_count, 0);
}

#[test]
fn corrected_dictation_requires_review_then_explicit_publication_activates_point_evidence() {
    let mut fixture = setup();
    let (region_id, machine_transcription_id) = prepare_machine_result(&mut fixture);
    let corrected =
        teacher_correct_transcription(&mut fixture.conn, region_id, "1842年", "teacher").unwrap();

    assert!(
        accept_dictation_suggestion(&fixture.conn, machine_transcription_id, "teacher").is_err()
    );
    let decision =
        accept_dictation_suggestion(&fixture.conn, corrected.transcription.id, "teacher").unwrap();
    assert_eq!(decision.confirmation_level, "teacher_corrected");
    assert_eq!(decision.teacher_score, 2.0);

    let before_publish: (i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_grade_publications_v2),
               (SELECT COUNT(*) FROM learning_evidence)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(before_publish, (0, 0));
    let workbench = list_dictation_workbench(&fixture.conn, Some(1), 10).unwrap();
    assert!(workbench.rows[0].current_transcription_confirmed);
    assert!(workbench.attempts[0].can_publish);

    let publication = publish_attempt(&fixture.conn, 1, "teacher").unwrap();
    assert_eq!(publication.total_score, 2.0);
    let evidence: Vec<(String, String, String, String)> = {
        let mut stmt = fixture
            .conn
            .prepare(
                "SELECT source_type,source_ref_type,source_ref_id,confirmation_level
                 FROM learning_evidence WHERE state='active' ORDER BY id",
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
    assert!(evidence.iter().all(|value| {
        value.0 == "dictation_rubric_point"
            && value.1 == "rubric_point"
            && value.2 == "point-d"
            && value.3 == "teacher_corrected"
    }));
    assert!(fixture
        .conn
        .execute(
            "UPDATE exam_grade_decision_dictation_sources_v2
             SET teacher_evidence_text='tampered' WHERE grade_decision_id=?1",
            [decision.id],
        )
        .is_err());
}

#[test]
fn manual_dictation_grade_keeps_machine_text_and_requires_a_reason() {
    let mut fixture = setup();
    let (_region_id, transcription_id) = prepare_machine_result(&mut fixture);
    assert!(correct_dictation_grade(
        &fixture.conn,
        transcription_id,
        1.0,
        None,
        Some("1840年"),
        "teacher",
    )
    .is_err());
    let decision = correct_dictation_grade(
        &fixture.conn,
        transcription_id,
        1.0,
        Some("原图可辨，年份写错，按规则给部分分"),
        Some("1840年"),
        "teacher",
    )
    .unwrap();
    assert_eq!(decision.confirmation_level, "teacher_corrected");
    let source: (String, String, Option<String>) = fixture
        .conn
        .query_row(
            "SELECT t.raw_ocr_text,t.state,s.teacher_evidence_text
             FROM exam_grade_decision_dictation_sources_v2 s
             JOIN exam_dictation_transcription_revisions_v2 t
               ON t.id=s.transcription_revision_id
             WHERE s.grade_decision_id=?1",
            [decision.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        source,
        ("1840年".into(), "active".into(), Some("1840年".into()))
    );
}

#[test]
fn strict_dictation_batch_is_idempotent_and_records_already_confirmed_exclusion() {
    let mut fixture = setup();
    let (region_id, _machine_id) = prepare_machine_result(&mut fixture);
    let corrected =
        teacher_correct_transcription(&mut fixture.conn, region_id, "1842年", "teacher").unwrap();
    let input = StrictDictationBatchReview {
        transcription_revision_ids: &[corrected.transcription.id],
        reviewed_by: "teacher",
        idempotency_key: "dictation-batch-1",
    };
    let batch = strict_batch_accept(&fixture.conn, &input).unwrap();
    let repeated = strict_batch_accept(&fixture.conn, &input).unwrap();
    assert_eq!(batch.id, repeated.id);
    assert_eq!((batch.confirmed_count, batch.excluded_count), (1, 0));

    let excluded = strict_batch_accept(
        &fixture.conn,
        &StrictDictationBatchReview {
            transcription_revision_ids: &[corrected.transcription.id],
            reviewed_by: "teacher",
            idempotency_key: "dictation-batch-2",
        },
    )
    .unwrap();
    assert_eq!((excluded.confirmed_count, excluded.excluded_count), (0, 1));
    assert_eq!(
        excluded.items[0].reason_code.as_deref(),
        Some("ALREADY_CONFIRMED")
    );
}

#[test]
fn scoped_workspace_dictation_read_is_readonly_and_fail_closed() {
    let mut fixture = setup();
    prepare_machine_result(&mut fixture);
    let all = list_dictation_workbench(&fixture.conn, Some(1), 10).unwrap();
    let target = all.rows[0].attempt_id;
    fixture.conn.execute_batch("PRAGMA query_only=ON").unwrap();
    let scoped = super::dictation_pipeline::list_dictation_workbench_scoped(
        &fixture.conn,
        Some(1),
        1,
        Some(&[target]),
    )
    .unwrap();
    assert_eq!(scoped, all);
    for ids in [&[][..], &[target + 999][..]] {
        let empty = super::dictation_pipeline::list_dictation_workbench_scoped(
            &fixture.conn,
            None,
            1,
            Some(ids),
        )
        .unwrap();
        assert!(empty.rows.is_empty() && empty.attempts.is_empty());
    }
}
