use super::*;
use std::path::PathBuf;
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

fn test_root(label: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("jiaofu-exam-intake-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn seed() -> Connection {
    let conn = open_in_memory().unwrap();
    run_migrations(&conn, CORE_MIGRATIONS).unwrap();
    run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
    run_migrations(&conn, module_exam::exam_migrations()).unwrap();
    let hash = "a".repeat(64);
    conn.execute_batch(&format!(
            r#"INSERT INTO subjects(name) VALUES ('历史');
               INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
               INSERT INTO students(student_no,name,class_id,enabled)
                 VALUES ('1','学生一',1,1),('2','学生二',1,1),('10','学生十',1,1);
               INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('edition',1,'PEP','2024','中国历史八上','8','upper','active','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('map',1,1,'confirmed','2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('question','personal','teacher','unknown',0,'2026-07-14T08:00:00.000Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,quality_level,state,created_at)
                 VALUES ('question-v1',1,1,'true_false','鸦片战争爆发于1840年。',1,'{hash}','L3','published','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('answer-v1',1,1,'{{"schema_version":1,"correct":true}}','confirmed','2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('rubric-v1',1,1,1,'confirmed','2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('link-v1',1,1,1,'confirmed','2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,created_by,created_at,updated_at)
                 VALUES ('assessment','固定试卷',1,'quiz','include','active','teacher','2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('assessment-v1',1,1,'{hash}','fixed-template-v1','confirmed','2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,state,created_at)
                 VALUES ('item',1,1,1,1,1,0,1,'{{"schema_version":1}}','active','2026-07-14T08:00:00.000Z');"#
        ))
        .unwrap();
    conn
}

fn write_jpeg(path: &Path, value: &[u8]) {
    std::fs::write(path, value).unwrap();
}

#[test]
fn workspace_resume_is_read_only_and_preserves_batch_scope_after_reopen() {
    let root = test_root("workspace-resume");
    let conn = seed();
    crate::state::run_all_migrations(&conn).unwrap();
    let mut batch_ids = Vec::new();
    for index in 1..=2 {
        let path = root.join(format!("paper{index}.jpg"));
        write_jpeg(&path, format!("synthetic-paper-{index}").as_bytes());
        let result = prepare_fixed_intake(
            &conn,
            &root,
            &FixedIntakeRequest {
                assessment_version_id: 1,
                student_paths: vec![path.to_string_lossy().into_owned()],
                answer_path: None,
                answer_text: None,
                expected_pages_per_attempt: 1,
                material_type: Some("ordinary_paper".into()),
                idempotency_key: format!("resume-{index}"),
            },
        )
        .unwrap();
        confirm_intake_grouping(&conn, result.batch_id, &index.to_string(), &[]).unwrap();
        confirm_intake_grouping_quality(&conn, result.batch_id, &[]).unwrap();
        batch_ids.push(result.batch_id);
    }
    let (page_id, artifact_id): (i64, i64) = conn
        .query_row(
            "SELECT id,source_artifact_id FROM exam_ingest_pages_v2 WHERE batch_id=?1",
            [batch_ids[0]],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let run = suite_core::db::repo::ai_runs::create_or_get(
        &conn,
        &suite_core::db::repo::ai_runs::NewAiRun {
            idempotency_key: "resume-processing",
            run_type: "ordinary_paper_structure",
            source_module: "exam",
            business_ref_type: "ingest_page",
            business_ref_id: &page_id.to_string(),
            input_artifact_id: Some(artifact_id),
            provider: "synthetic",
            model_name: "synthetic",
            model_version: "1",
            config_version: "1",
            prompt_or_rule_version: "1",
            input_hash: &"a".repeat(64),
            retry_of_ai_run_id: None,
        },
    )
    .unwrap();
    suite_core::db::repo::ai_runs::start(&conn, run.id, "2026-09-07T00:00:00Z", None).unwrap();
    let path = root.join("reopened.db");
    conn.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])
        .unwrap();
    drop(conn);
    let reopened =
        Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let first = resume::read(&reopened, batch_ids[0]).unwrap();
    let second = resume::read(&reopened, batch_ids[1]).unwrap();
    assert_eq!(first.processing_history.len(), 1);
    assert_eq!(first.processing_history[0].status, "processing");
    assert!(second.processing_history.is_empty());
    assert_eq!(first.class_id, 1);
    assert_eq!(first.assessment_version_id, 1);
    assert!(first.result.quality_review_completed);
    assert_eq!(first.result.grouping_first_student_no.as_deref(), Some("1"));
    assert_eq!(
        second.result.grouping_first_student_no.as_deref(),
        Some("2")
    );
    assert!(!first
        .result
        .reason_codes
        .iter()
        .any(|code| code == "PAGE_IDENTITY_UNCONFIRMED"));
    let first_ids = crate::workspace::batch_attempt_ids(&reopened, batch_ids[0]).unwrap();
    let second_ids = crate::workspace::batch_attempt_ids(&reopened, batch_ids[1]).unwrap();
    assert_eq!(first_ids.len(), 1);
    assert_eq!(second_ids.len(), 1);
    assert_ne!(first_ids, second_ids);
    let tasks = crate::workspace::read_tasks(&reopened).unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(
        tasks
            .iter()
            .find(|task| task.source_id == batch_ids[0])
            .unwrap()
            .status,
        "processing"
    );
    assert_eq!(
        tasks
            .iter()
            .find(|task| task.source_id == batch_ids[1])
            .unwrap()
            .status,
        "needs_material"
    );
    assert!(tasks.iter().all(|task| task.status != "published"));
    assert!(resume::read(&reopened, 99999).is_err());
    assert!(crate::workspace::read_exam_review(&reopened, "recitation", 1).is_err());
    assert!(crate::workspace::read_exam_review(&reopened, "exam_batch", batch_ids[0]).is_ok());
}

fn confirm_workspace_attempt(conn: &Connection, attempt_id: i64) {
    module_exam::service::assessment::decide_grade(
        conn,
        &module_exam::service::assessment::NewGradeDecision {
            attempt_id,
            assessment_item_id: 1,
            machine_grade_ai_run_id: None,
            teacher_score: 1.0,
            point_results_json: r#"{"schema_version":1,"result":"confirmed"}"#,
            teacher_note: Some("合成验收"),
            confirmation_level: "teacher_corrected",
            decided_by: "teacher",
        },
    )
    .unwrap();
}

fn legacy_workspace_batch(conn: &Connection) -> (i64, Vec<i64>) {
    let batch = papers::create_or_get_ingest_batch(
        conn,
        &NewIngestBatch {
            assessment_version_id: 1,
            source_kind: "fixed_fixture",
            idempotency_key: "workspace-legacy",
            created_by: "teacher",
        },
    )
    .unwrap();
    let mut attempts = Vec::new();
    for index in 1..=2 {
        let attempt =
            module_exam::service::assessment::create_attempt(conn, 1, index, "image", "first")
                .unwrap();
        let artifact = artifacts::create_or_get(
            conn,
            &NewArtifact {
                kind: ArtifactKind::Image,
                sha256: &format!("{index:064x}"),
                mime_type: "image/jpeg",
                byte_size: 1,
                original_name: None,
                original_path: None,
                archived_path: "/synthetic.jpg",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: "workspace-test",
                privacy_class: PrivacyClass::StudentSensitive,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        let page = papers::register_ingest_page(
            conn,
            &NewIngestPage {
                batch_id: batch.id,
                source_artifact_id: artifact.id,
                import_index: index - 1,
                expected_page_no: Some(1),
            },
        )
        .unwrap();
        papers::record_page_quality(
            conn,
            &papers::NewPageQualityRevision {
                page_id: page.id,
                blur_score: 0.0,
                glare_score: 0.0,
                brightness_score: 0.5,
                perspective_score: 0.0,
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
            conn,
            &papers::NewPageMatchRevision {
                page_id: page.id,
                attempt_id: Some(attempt.id),
                page_no: Some(1),
                student_confidence: Some(1.0),
                page_no_confidence: Some(1.0),
                template_confidence: Some(1.0),
                decision: "teacher_confirmed",
                reason_code: None,
                confirmed_by: Some("teacher"),
            },
        )
        .unwrap();
        attempts.push(attempt.id);
    }
    (batch.id, attempts)
}

#[test]
fn workspace_legacy_batch_tracks_confirmation_and_publication_after_reopen() {
    let conn = seed();
    crate::state::run_all_migrations(&conn).unwrap();
    let (batch_id, attempts) = legacy_workspace_batch(&conn);
    for id in &attempts {
        confirm_workspace_attempt(&conn, *id);
    }
    let tasks = crate::workspace::read_tasks(&conn).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, "ready_to_publish");
    module_exam::service::assessment::publish_attempt(&conn, attempts[0], "teacher").unwrap();
    assert_eq!(
        crate::workspace::read_tasks(&conn).unwrap()[0].status,
        "ready_to_publish"
    );
    module_exam::service::assessment::publish_attempt(&conn, attempts[1], "teacher").unwrap();
    let root = test_root("workspace-legacy-reopen");
    let path = root.join("data.db");
    conn.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])
        .unwrap();
    drop(conn);
    let reopened =
        Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let tasks = crate::workspace::read_tasks(&reopened).unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].source_id, batch_id);
    assert_eq!(tasks[0].attempt_ids, attempts);
    assert_eq!(tasks[0].status, "published");
    assert_eq!(
        reopened
            .query_row("SELECT total_changes()", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn workspace_legacy_batch_with_unmatched_page_is_not_complete() {
    let conn = seed();
    crate::state::run_all_migrations(&conn).unwrap();
    let (_, attempts) = legacy_workspace_batch(&conn);
    for id in attempts {
        confirm_workspace_attempt(&conn, id);
        module_exam::service::assessment::publish_attempt(&conn, id, "teacher").unwrap();
    }
    conn.execute(
        "UPDATE exam_page_match_revisions_v2 SET state='superseded' WHERE page_id=2",
        [],
    )
    .unwrap();
    let task = crate::workspace::read_tasks(&conn)
        .unwrap()
        .into_iter()
        .find(|t| t.kind == "exam_batch")
        .unwrap();
    assert_ne!(task.status, "published");
    assert_ne!(task.status, "ready_to_publish");
}

#[test]
fn workspace_ordered_batch_never_falls_back_when_grouping_is_incomplete() {
    let root = test_root("workspace-ordered-completion");
    let conn = seed();
    crate::state::run_all_migrations(&conn).unwrap();
    let path = root.join("paper.jpg");
    write_jpeg(&path, b"synthetic ordered paper");
    let result = prepare_fixed_intake(
        &conn,
        &root,
        &FixedIntakeRequest {
            assessment_version_id: 1,
            student_paths: vec![path.to_string_lossy().into_owned()],
            answer_path: None,
            answer_text: None,
            expected_pages_per_attempt: 1,
            material_type: Some("ordinary_paper".into()),
            idempotency_key: "ordered-completion".into(),
        },
    )
    .unwrap();
    confirm_intake_grouping(&conn, result.batch_id, "1", &[]).unwrap();
    confirm_intake_grouping_quality(&conn, result.batch_id, &[]).unwrap();
    let id = crate::workspace::batch_attempt_ids(&conn, result.batch_id).unwrap()[0];
    confirm_workspace_attempt(&conn, id);
    module_exam::service::assessment::publish_attempt(&conn, id, "teacher").unwrap();
    assert_eq!(
        crate::workspace::read_tasks(&conn).unwrap()[0].status,
        "published"
    );
    // All attempts are published, but rejected/unmapped grouping must still block completion.
    for (revision, mapped, rejected) in [(2, 1, 1), (3, 0, 0)] {
        conn.execute("UPDATE exam_ordered_grouping_activations_v2 SET state='superseded' WHERE state='active'", []).unwrap();
        conn.execute("INSERT INTO exam_ordered_grouping_activations_v2
            (public_id,ingest_batch_id,grouping_decision_id,revision,snapshot_hash,rejected_page_ids_json,
             attempt_ids_json,page_match_ids_json,mapped_group_count,rejected_group_count,state,confirmed_by,created_at)
            SELECT public_id||?1,ingest_batch_id,grouping_decision_id,?1,?2,rejected_page_ids_json,
                attempt_ids_json,page_match_ids_json,?3,?4,'active',confirmed_by,created_at
            FROM exam_ordered_grouping_activations_v2 WHERE revision=1",
            rusqlite::params![revision, format!("{revision:064x}"), mapped, rejected]).unwrap();
        assert_ne!(
            crate::workspace::read_tasks(&conn).unwrap()[0].status,
            "published"
        );
    }
    conn.execute(
        "UPDATE exam_ordered_grouping_revisions_v2 SET state='superseded'",
        [],
    )
    .unwrap();
    assert_ne!(
        crate::workspace::read_tasks(&conn).unwrap()[0].status,
        "published"
    );
}

#[test]
fn office_files_are_accepted_only_as_answer_sources() {
    assert_eq!(
        source_format(Path::new("答案.docx"), true).unwrap(),
        SourceFormat::Docx
    );
    assert_eq!(
        source_format(Path::new("答案.xlsx"), true).unwrap(),
        SourceFormat::Xlsx
    );
    assert!(source_format(Path::new("学生作业.docx"), false).is_err());
    assert!(source_format(Path::new("学生作业.xlsx"), false).is_err());
}

#[test]
fn exif_datetime_original_is_read_without_trusting_filename() {
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"II");
    tiff.extend_from_slice(&42_u16.to_le_bytes());
    tiff.extend_from_slice(&8_u32.to_le_bytes());
    tiff.extend_from_slice(&1_u16.to_le_bytes());
    tiff.extend_from_slice(&0x8769_u16.to_le_bytes());
    tiff.extend_from_slice(&4_u16.to_le_bytes());
    tiff.extend_from_slice(&1_u32.to_le_bytes());
    tiff.extend_from_slice(&26_u32.to_le_bytes());
    tiff.extend_from_slice(&0_u32.to_le_bytes());
    tiff.extend_from_slice(&1_u16.to_le_bytes());
    tiff.extend_from_slice(&0x9003_u16.to_le_bytes());
    tiff.extend_from_slice(&2_u16.to_le_bytes());
    tiff.extend_from_slice(&20_u32.to_le_bytes());
    tiff.extend_from_slice(&44_u32.to_le_bytes());
    tiff.extend_from_slice(&0_u32.to_le_bytes());
    tiff.extend_from_slice(b"2026:07:15 02:25:00\0");
    assert_eq!(
        exif_capture_time_from_tiff(&tiff).as_deref(),
        Some("2026:07:15 02:25:00")
    );
}

#[test]
fn options_only_include_confirmed_objective_assessments() {
    let conn = seed();
    let options = list_options(&conn).unwrap();
    assert_eq!(options.len(), 1);
    assert_eq!(options[0].class_name, "八年级一班");
    assert_eq!(options[0].item_count, 1);
    assert!(options[0].is_default);
    assert_eq!(options[0].default_selection_public_id, None);
}

#[test]
fn explicit_future_default_is_listed_before_later_non_default_branch() {
    let conn = seed();
    let hash = "b".repeat(64);
    conn.execute_batch(&format!(
            r#"INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,confirmed_by,confirmed_at)
               VALUES ('answer-v2',1,2,'{{"schema_version":1,"correct":false}}','confirmed',
                       '2026-07-19T08:00:00.000Z','teacher','2026-07-19T08:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,confirmed_by,confirmed_at)
               VALUES ('rubric-v2',1,2,1,'confirmed','2026-07-19T08:00:00.000Z',
                       'teacher','2026-07-19T08:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,confirmed_by,confirmed_at)
               VALUES ('link-v2',1,1,2,'confirmed','2026-07-19T08:00:00.000Z',
                       'teacher','2026-07-19T08:00:00.000Z');
               INSERT INTO exam_question_version_impact_plans_v2
                 (public_id,request_key,request_hash,question_version_id,
                  target_answer_key_version_id,target_rubric_version_id,target_link_set_id,
                  expected_preview_hash,action,impact_json,planned_by,planned_at)
               VALUES ('plan','plan-key','{hash}',1,2,2,2,'{hash}','future_only',
                       '{{"schemaVersion":1}}','teacher','2026-07-19T08:00:00.000Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,
                  supersedes_version_id,created_at,confirmed_by,confirmed_at)
               VALUES ('assessment-v2',1,2,'{hash}','fixed-template-v1','confirmed',1,
                       '2026-07-19T08:00:00.000Z','teacher','2026-07-19T08:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                  state,created_at)
               VALUES ('item-v2',2,1,2,2,2,0,1,'{{"schema_version":1}}','active',
                       '2026-07-19T08:00:00.000Z');
               INSERT INTO exam_assessment_default_version_selections_v2
                 (public_id,request_key,request_hash,assessment_id,revision,
                  previous_assessment_version_id,selected_assessment_version_id,
                  source_impact_plan_id,selected_by,selected_at)
               VALUES ('selection-v1','selection-key','{hash}',1,1,1,2,1,'teacher',
                       '2026-07-19T08:00:00.000Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,
                  supersedes_version_id,created_at,confirmed_by,confirmed_at)
               VALUES ('assessment-v3',1,3,'{hash}','fixed-template-v1','confirmed',2,
                       '2026-07-19T09:00:00.000Z','teacher','2026-07-19T09:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                  state,created_at)
               VALUES ('item-v3',3,1,2,2,2,0,1,'{{"schema_version":1}}','active',
                       '2026-07-19T09:00:00.000Z');"#
        ))
        .unwrap();
    let options = list_options(&conn).unwrap();
    assert_eq!(options.len(), 3);
    assert_eq!(options[0].assessment_version_id, 2);
    assert!(options[0].is_default);
    assert_eq!(
        options[0].default_selection_public_id.as_deref(),
        Some("selection-v1")
    );
    assert!(!options[1].is_default);
    assert_eq!(options[1].assessment_version_id, 3);
}

#[cfg(target_os = "macos")]
#[test]
fn pdf_page_count_is_suggested_before_any_database_write() {
    let root = test_root("pdf-cycle-suggestion");
    let student = root.join("student.pdf");
    std::fs::write(&student, crate::pdf_pages::two_page_pdf_fixture()).unwrap();
    let suggestion = infer_page_cycle_paths(&[student.to_string_lossy().into_owned()]).unwrap();
    assert_eq!(suggestion.expected_pages_per_attempt, 2);
    assert_eq!(suggestion.source, "pdf_document_page_count");
    assert!(suggestion.needs_teacher_input);
}

#[test]
fn jpeg_and_pasted_answer_are_archived_without_creating_scores() {
    let root = test_root("jpeg");
    let student = root.join("student.jpg");
    write_jpeg(&student, b"jpeg-student-paper");
    let conn = seed();
    let request = FixedIntakeRequest {
        assessment_version_id: 1,
        student_paths: vec![student.to_string_lossy().into_owned()],
        answer_path: None,
        answer_text: Some("1. 正确".into()),
        expected_pages_per_attempt: 1,
        material_type: None,
        idempotency_key: "jpeg-intake".into(),
    };
    let result = prepare_fixed_intake(&conn, &root, &request).unwrap();
    assert_eq!(result.student_document_count, 1);
    assert_eq!(result.student_page_count, 1);
    assert_eq!(result.answer_document_count, 1);
    assert_eq!(result.route, "blocked");
    assert!(result
        .reason_codes
        .iter()
        .any(|code| code == "PAGE_QUALITY_REVIEW_REQUIRED"));
    assert!(result
        .reason_codes
        .iter()
        .any(|code| code == "ANSWER_SOURCE_STRUCTURE_PENDING"));
    let privacy: Vec<String> = conn
        .prepare("SELECT privacy_class FROM artifacts ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(privacy, vec!["student_sensitive", "teaching_content"]);
    let grade_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM exam_grade_decisions_v2", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(grade_count, 0);
}

#[test]
fn retry_with_same_key_is_idempotent() {
    let root = test_root("retry");
    let student = root.join("student.jpeg");
    write_jpeg(&student, b"same-student-paper");
    let conn = seed();
    let request = FixedIntakeRequest {
        assessment_version_id: 1,
        student_paths: vec![student.to_string_lossy().into_owned()],
        answer_path: None,
        answer_text: None,
        expected_pages_per_attempt: 1,
        material_type: None,
        idempotency_key: "same-intake".into(),
    };
    let first = prepare_fixed_intake(&conn, &root, &request).unwrap();
    let second = prepare_fixed_intake(&conn, &root, &request).unwrap();
    assert_eq!(first.batch_id, second.batch_id);
    for table in [
        "exam_ingest_batches_v2",
        "exam_fixed_input_documents_v2",
        "exam_ingest_pages_v2",
        "exam_fixed_preflight_revisions_v2",
        "exam_import_order_revisions_v2",
        "exam_material_type_revisions_v2",
        "exam_page_type_revisions_v2",
        "exam_ordered_grouping_revisions_v2",
    ] {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1, "{table}");
    }
    let decision_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM exam_ordered_grouping_decisions_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(decision_count, 0);
}

#[test]
fn photo_files_are_persisted_in_natural_filename_order_with_order_snapshot() {
    let root = test_root("natural-order");
    let ten = root.join("IMG_10.jpg");
    let two = root.join("IMG_2.jpg");
    let one = root.join("IMG_1.jpg");
    write_jpeg(&ten, b"paper-ten");
    write_jpeg(&two, b"paper-two");
    write_jpeg(&one, b"paper-one");
    let conn = seed();
    let request = FixedIntakeRequest {
        assessment_version_id: 1,
        student_paths: vec![
            ten.to_string_lossy().into_owned(),
            two.to_string_lossy().into_owned(),
            one.to_string_lossy().into_owned(),
        ],
        answer_path: None,
        answer_text: None,
        expected_pages_per_attempt: 1,
        material_type: Some("ordinary_paper".into()),
        idempotency_key: "natural-order-intake".into(),
    };
    let result = prepare_fixed_intake(&conn, &root, &request).unwrap();
    let names = result
        .documents
        .iter()
        .filter(|document| document.role == "student_work")
        .map(|document| document.original_name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["IMG_1.jpg", "IMG_2.jpg", "IMG_10.jpg"]);
    assert_eq!(
        result.order_policy,
        "filename_natural_exif_filetime_crosscheck_v1"
    );
    assert_eq!(result.material_type_decision, "teacher_confirmed");
    let snapshot: String = conn
        .query_row(
            "SELECT ordered_sources_json FROM exam_import_order_revisions_v2
                 WHERE state='active'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let value: serde_json::Value = serde_json::from_str(&snapshot).unwrap();
    assert_eq!(value["entries"][0]["original_request_index"], 2);
    assert_eq!(value["entries"][2]["original_request_index"], 0);
}

#[test]
fn fixed_page_count_records_a_repeating_page_cycle_instead_of_unknown_types() {
    let root = test_root("fixed-page-cycle");
    let second = root.join("IMG_2.jpg");
    let first = root.join("IMG_1.jpg");
    write_jpeg(&second, b"paper-page-two");
    write_jpeg(&first, b"paper-page-one");
    let conn = seed();
    let request = FixedIntakeRequest {
        assessment_version_id: 1,
        student_paths: vec![
            second.to_string_lossy().into_owned(),
            first.to_string_lossy().into_owned(),
        ],
        answer_path: None,
        answer_text: None,
        expected_pages_per_attempt: 2,
        material_type: Some("ordinary_paper".into()),
        idempotency_key: "fixed-page-cycle-intake".into(),
    };
    let result = prepare_fixed_intake(&conn, &root, &request).unwrap();
    assert_eq!(result.grouping_route, "review_required");
    let page_types = conn
        .prepare("SELECT page_type_key,decision FROM exam_page_type_revisions_v2 ORDER BY page_id")
        .unwrap()
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        page_types,
        vec![
            ("page_1".into(), "suggested".into()),
            ("page_2".into(), "suggested".into())
        ]
    );
    assert!(!result
        .grouping_issue_codes
        .iter()
        .any(|code| code == "PAGE_TYPE_CYCLE_UNVERIFIED"));
}

#[test]
fn low_confidence_material_type_is_confirmed_once_with_new_revision() {
    let root = test_root("material-confirm");
    let student = root.join("IMG_1.jpg");
    write_jpeg(&student, b"paper-material-confirm");
    let conn = seed();
    let request = FixedIntakeRequest {
        assessment_version_id: 1,
        student_paths: vec![student.to_string_lossy().into_owned()],
        answer_path: None,
        answer_text: None,
        expected_pages_per_attempt: 1,
        material_type: Some("auto".into()),
        idempotency_key: "material-confirm-intake".into(),
    };
    let result = prepare_fixed_intake(&conn, &root, &request).unwrap();
    assert!(result.material_type_needs_confirmation);
    let confirmed = confirm_intake_material_type(&conn, result.batch_id, "dictation").unwrap();
    assert_eq!(confirmed.material_type, "dictation");
    assert_eq!(confirmed.material_type_decision, "teacher_confirmed");
    let rows: Vec<(i64, String)> = conn
        .prepare("SELECT revision,state FROM exam_material_type_revisions_v2 ORDER BY revision")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, vec![(1, "superseded".into()), (2, "active".into())]);
}

#[test]
fn teacher_confirms_start_student_and_absence_without_creating_page_match() {
    let root = test_root("grouping-confirm");
    let second = root.join("IMG_2.jpg");
    let first = root.join("IMG_1.jpg");
    write_jpeg(&second, b"paper-second-group");
    write_jpeg(&first, b"paper-first-group");
    let conn = seed();
    let request = FixedIntakeRequest {
        assessment_version_id: 1,
        student_paths: vec![
            second.to_string_lossy().into_owned(),
            first.to_string_lossy().into_owned(),
        ],
        answer_path: None,
        answer_text: None,
        expected_pages_per_attempt: 1,
        material_type: Some("ordinary_paper".into()),
        idempotency_key: "grouping-confirm-intake".into(),
    };
    let prepared = prepare_fixed_intake(&conn, &root, &request).unwrap();
    assert!(!prepared.grouping_confirmed);
    assert_eq!(prepared.grouping_roster.len(), 3);

    let confirmed =
        confirm_intake_grouping(&conn, prepared.batch_id, "1", &["2".to_string()]).unwrap();
    assert!(confirmed.grouping_confirmed);
    assert_eq!(confirmed.grouping_first_student_no, "1");
    assert_eq!(confirmed.grouping_last_student_no, "10");
    assert_eq!(confirmed.student_group_count, 2);

    let assignment_json: String = conn
        .query_row(
            "SELECT assignments_json FROM exam_ordered_grouping_decisions_v2
                 WHERE ingest_batch_id=?1 AND state='active'",
            [prepared.batch_id],
            |row| row.get(0),
        )
        .unwrap();
    let assignments: serde_json::Value = serde_json::from_str(&assignment_json).unwrap();
    let student_nos = assignments["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|group| group["student_no"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(student_nos, vec!["1", "10"]);
    let page_match_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM exam_page_match_revisions_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(page_match_count, 0);

    let retried = prepare_fixed_intake(&conn, &root, &request).unwrap();
    assert!(retried.grouping_confirmed);
    assert_eq!(retried.grouping_first_student_no.as_deref(), Some("1"));
    assert_eq!(retried.grouping_last_student_no.as_deref(), Some("10"));
    assert_eq!(retried.material_type_decision, "teacher_confirmed");
    let material_revisions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM exam_material_type_revisions_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(material_revisions, 1);
}

#[test]
fn quality_confirmation_maps_clear_groups_and_only_blocks_the_retake_group() {
    let root = test_root("quality-map");
    let fourth = root.join("IMG_4.jpg");
    let second = root.join("IMG_2.jpg");
    let third = root.join("IMG_3.jpg");
    let first = root.join("IMG_1.jpg");
    write_jpeg(&fourth, b"paper-fourth-quality");
    write_jpeg(&second, b"paper-second-quality");
    write_jpeg(&third, b"paper-third-quality");
    write_jpeg(&first, b"paper-first-quality");
    let conn = seed();
    let request = FixedIntakeRequest {
        assessment_version_id: 1,
        student_paths: vec![
            fourth.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
            third.to_string_lossy().into_owned(),
            first.to_string_lossy().into_owned(),
        ],
        answer_path: None,
        answer_text: None,
        expected_pages_per_attempt: 2,
        material_type: Some("ordinary_paper".into()),
        idempotency_key: "quality-map-intake".into(),
    };
    let prepared = prepare_fixed_intake(&conn, &root, &request).unwrap();
    confirm_intake_grouping(&conn, prepared.batch_id, "1", &["2".to_string()]).unwrap();
    let evidence = intake_grouping_evidence(&conn, prepared.batch_id).unwrap();
    assert_eq!(evidence.len(), 2);
    assert_eq!(
        evidence
            .iter()
            .map(|group| group.student_no.as_str())
            .collect::<Vec<_>>(),
        vec!["1", "10"]
    );

    let rejected_page_id = evidence[0].pages[0].page_id;
    let second_rejected_page_id = evidence[0].pages[1].page_id;
    assert_eq!(evidence[0].pages.len(), 2);
    assert_eq!(evidence[1].pages.len(), 2);
    let result = confirm_intake_grouping_quality(
        &conn,
        prepared.batch_id,
        &[rejected_page_id, second_rejected_page_id],
    )
    .unwrap();
    assert_eq!(
        (result.mapped_group_count, result.rejected_group_count),
        (1, 1)
    );
    let counts: (i64, i64, i64, i64) = conn
        .query_row(
            "SELECT
                   (SELECT COUNT(*) FROM exam_page_quality_revisions_v2),
                   (SELECT COUNT(*) FROM exam_attempts_v2),
                   (SELECT COUNT(*) FROM exam_page_match_revisions_v2),
                   (SELECT COUNT(*) FROM exam_grade_decisions_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(counts, (4, 1, 2, 0));
    let mapped_student_no: String = conn
        .query_row(
            "SELECT s.student_no FROM exam_page_match_revisions_v2 m
                 JOIN exam_attempts_v2 a ON a.id=m.attempt_id
                 JOIN students s ON s.id=a.student_id
                 WHERE m.state='active'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(mapped_student_no, "10");
    let rejected_group_match_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM exam_page_match_revisions_v2 m
                 JOIN exam_ingest_pages_v2 p ON p.id=m.page_id
                 WHERE p.id IN (?1,?2)",
            [evidence[0].pages[0].page_id, evidence[0].pages[1].page_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        rejected_group_match_count, 0,
        "同一学生任一页需重拍时，该学生整组都不能建立正式归属"
    );
    let rejected_state: String = conn
        .query_row(
            "SELECT state FROM exam_ingest_pages_v2 WHERE id=?1",
            [rejected_page_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rejected_state, "needs_review");

    let retried = confirm_intake_grouping_quality(
        &conn,
        prepared.batch_id,
        &[rejected_page_id, second_rejected_page_id],
    )
    .unwrap();
    assert_eq!(retried.mapped_group_count, 1);
    let attempt_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM exam_attempts_v2", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(attempt_count, 1, "相同质量确认重试不得新增 attempt");

    let retake = root.join("IMG_retake_1.jpg");
    write_jpeg(&retake, b"paper-first-quality-retake");
    let first_replaced = replace_intake_rejected_page(
        &conn,
        &root,
        prepared.batch_id,
        rejected_page_id,
        &retake.to_string_lossy(),
    )
    .unwrap();
    assert!(!first_replaced.activated_student);
    assert_eq!(
        (
            first_replaced.mapped_group_count,
            first_replaced.rejected_group_count
        ),
        (1, 1)
    );
    let refreshed = intake_grouping_evidence(&conn, prepared.batch_id).unwrap();
    assert_eq!(
        refreshed[0].pages[0].replaced_page_id,
        Some(rejected_page_id)
    );
    assert_eq!(
        refreshed[0].pages[0].page_id,
        first_replaced.replacement_page_id
    );
    assert_eq!(
        refreshed[0].pages[0].quality_result.as_deref(),
        Some("pass")
    );
    assert_eq!(refreshed[0].pages[0].match_decision, None);
    assert_eq!(
        refreshed[0].pages[1].quality_result.as_deref(),
        Some("reject")
    );

    let second_retake = root.join("IMG_retake_2.jpg");
    write_jpeg(&second_retake, b"paper-second-quality-retake");
    let replaced = replace_intake_rejected_page(
        &conn,
        &root,
        prepared.batch_id,
        second_rejected_page_id,
        &second_retake.to_string_lossy(),
    )
    .unwrap();
    assert!(replaced.activated_student);
    assert_eq!(
        (replaced.mapped_group_count, replaced.rejected_group_count),
        (2, 0)
    );
    let completed = intake_grouping_evidence(&conn, prepared.batch_id).unwrap();
    assert_eq!(
        completed[0].pages[0].match_decision.as_deref(),
        Some("teacher_confirmed")
    );
    assert_eq!(
        completed[0].pages[1].match_decision.as_deref(),
        Some("teacher_confirmed")
    );
    let states: (String, String, String, String) = conn
        .query_row(
            "SELECT
                   (SELECT state FROM exam_ingest_pages_v2 WHERE id=?1),
                   (SELECT state FROM exam_ingest_pages_v2 WHERE id=?2),
                   (SELECT state FROM exam_ingest_pages_v2 WHERE id=?3),
                   (SELECT state FROM exam_ingest_pages_v2 WHERE id=?4)",
            [
                rejected_page_id,
                second_rejected_page_id,
                first_replaced.replacement_page_id,
                replaced.replacement_page_id,
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        states,
        (
            "voided".into(),
            "voided".into(),
            "matched".into(),
            "matched".into()
        )
    );
    let final_counts: (i64, i64, i64, i64) = conn
        .query_row(
            "SELECT
                   (SELECT COUNT(*) FROM exam_attempts_v2),
                   (SELECT COUNT(*) FROM exam_page_match_revisions_v2 WHERE state='active'),
                   (SELECT COUNT(*) FROM exam_ordered_grouping_activations_v2),
                   (SELECT COUNT(*) FROM exam_grade_decisions_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(final_counts, (2, 4, 3, 0));
}

#[test]
fn invalid_retake_page_rolls_back_without_quality_or_identity_facts() {
    let root = test_root("quality-invalid");
    let student = root.join("IMG_1.jpg");
    write_jpeg(&student, b"paper-quality-invalid");
    let conn = seed();
    let request = FixedIntakeRequest {
        assessment_version_id: 1,
        student_paths: vec![student.to_string_lossy().into_owned()],
        answer_path: None,
        answer_text: None,
        expected_pages_per_attempt: 1,
        material_type: Some("ordinary_paper".into()),
        idempotency_key: "quality-invalid-intake".into(),
    };
    let prepared = prepare_fixed_intake(&conn, &root, &request).unwrap();
    confirm_intake_grouping(&conn, prepared.batch_id, "1", &[]).unwrap();
    assert!(confirm_intake_grouping_quality(&conn, prepared.batch_id, &[999]).is_err());
    let counts: (i64, i64, i64) = conn
        .query_row(
            "SELECT
                   (SELECT COUNT(*) FROM exam_page_quality_revisions_v2),
                   (SELECT COUNT(*) FROM exam_attempts_v2),
                   (SELECT COUNT(*) FROM exam_page_match_revisions_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[cfg(target_os = "macos")]
#[test]
fn pdf_registers_real_page_derivatives_and_retry_reuses_them() {
    let root = test_root("pdf");
    let student = root.join("student.pdf");
    std::fs::write(&student, crate::pdf_pages::two_page_pdf_fixture()).unwrap();
    let conn = seed();
    let request = FixedIntakeRequest {
        assessment_version_id: 1,
        student_paths: vec![student.to_string_lossy().into_owned()],
        answer_path: None,
        answer_text: None,
        expected_pages_per_attempt: 2,
        material_type: None,
        idempotency_key: "pdf-intake".into(),
    };
    let first = prepare_fixed_intake(&conn, &root, &request).unwrap();
    let second = prepare_fixed_intake(&conn, &root, &request).unwrap();
    assert_eq!(first.batch_id, second.batch_id);
    assert_eq!(first.student_page_count, 2);
    let kinds: Vec<String> = conn
        .prepare("SELECT kind FROM artifacts ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(kinds, vec!["document", "page", "page"]);
    let page_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM exam_ingest_pages_v2", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(page_count, 2);
}

#[test]
fn duplicate_student_content_is_rejected_before_batch_creation() {
    let root = test_root("duplicate");
    let first = root.join("first.jpg");
    let second = root.join("second.jpg");
    write_jpeg(&first, b"same-paper");
    write_jpeg(&second, b"same-paper");
    let conn = seed();
    let request = FixedIntakeRequest {
        assessment_version_id: 1,
        student_paths: vec![
            first.to_string_lossy().into_owned(),
            second.to_string_lossy().into_owned(),
        ],
        answer_path: None,
        answer_text: None,
        expected_pages_per_attempt: 1,
        material_type: None,
        idempotency_key: "duplicate-intake".into(),
    };
    let error = prepare_fixed_intake(&conn, &root, &request)
        .unwrap_err()
        .to_string();
    assert!(error.contains("重复试卷"));
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM exam_ingest_batches_v2", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn invalid_student_format_fails_before_batch_creation() {
    let root = test_root("invalid");
    let student = root.join("student.png");
    std::fs::write(&student, b"png").unwrap();
    let conn = seed();
    let request = FixedIntakeRequest {
        assessment_version_id: 1,
        student_paths: vec![student.to_string_lossy().into_owned()],
        answer_path: None,
        answer_text: None,
        expected_pages_per_attempt: 1,
        material_type: None,
        idempotency_key: "invalid-intake".into(),
    };
    assert!(prepare_fixed_intake(&conn, &root, &request).is_err());
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM exam_ingest_batches_v2", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}
