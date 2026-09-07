use super::*;
use crate::service::question_candidate_review::{
    discard_candidate, list_candidate_review_inbox, promote_candidate_to_l1,
    CandidateReviewOptionInput, DiscardCandidateRequest, PromoteCandidateRequest,
};
use suite_core::db::repo::artifacts::{create_or_get, NewArtifact};
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
use suite_core::models::ArtifactKind;

struct Fixture {
    conn: Connection,
    region_id: i64,
    current_question_version_id: i64,
    reusable_artifact_id: i64,
}

fn exact_draft() -> QuestionDraft {
    QuestionDraft {
        question_type: "true_false".into(),
        stem: "鸦片战争爆发于1840年。".into(),
        material_text: None,
        max_score: 1.0,
        options: Vec::new(),
    }
}

fn new_draft() -> QuestionDraft {
    QuestionDraft {
        question_type: "single".into(),
        stem: "洋务运动后期提出的口号是？".into(),
        material_text: None,
        max_score: 2.0,
        options: vec![
            QuestionOptionDraft {
                label: "A".into(),
                content: "自强".into(),
                order_index: 0,
            },
            QuestionOptionDraft {
                label: "B".into(),
                content: "求富".into(),
                order_index: 1,
            },
        ],
    }
}

fn safe_scan() -> PrivacyScan {
    PrivacyScan {
        schema_version: 1,
        sanitized: true,
        student_identity_detected: false,
        student_answer_detected: false,
        teacher_mark_detected: false,
        score_detected: false,
    }
}

fn setup() -> Fixture {
    let conn = open_in_memory().unwrap();
    run_migrations(&conn, CORE_MIGRATIONS).unwrap();
    run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
    run_migrations(&conn, crate::exam_migrations()).unwrap();
    conn.execute_batch(
            "INSERT INTO subjects(name) VALUES ('历史');
             INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
             INSERT INTO students(student_no,name,class_id) VALUES ('S001','小林',1);
             INSERT INTO k1_textbook_editions
               (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
               VALUES ('edition-a',1,'PEP','2024','中国历史八上','8','upper','active',
                       '2026-07-13T08:00:00.000Z');
             INSERT INTO k1_knowledge_maps
               (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
               VALUES ('map-a',1,1,'confirmed','2026-07-13T08:00:00.000Z',
                       '2026-07-13T08:00:00.000Z');",
        )
        .unwrap();

    let question = content::create_question(
        &conn,
        &NewQuestion {
            owner_scope: "personal",
            owner_id: "teacher",
            question_family_id: None,
            rights_status: "unknown",
            sharing_allowed: false,
        },
    )
    .unwrap();
    let exact = exact_draft();
    let no_options: Vec<NewQuestionOption<'_>> = Vec::new();
    let version = content::create_question_version(
        &conn,
        &NewQuestionVersion {
            question_id: question.id,
            revision: 1,
            question_type: &exact.question_type,
            stem: &exact.stem,
            material_text: None,
            max_score: exact.max_score,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "C0",
            state: "candidate",
            options: &no_options,
        },
    )
    .unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO k1_answer_key_versions
               (public_id,question_version_id,revision,answer_json,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('answer-a',{},1,'{{\"schema_version\":1,\"correct\":true}}','confirmed',
                       '2026-07-13T08:00:00.000Z','teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO k1_rubric_versions
               (public_id,question_version_id,revision,max_score,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('rubric-a',{},1,1,'confirmed','2026-07-13T08:00:00.000Z',
                       'teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO k1_link_sets
               (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('link-a',{},1,1,'confirmed','2026-07-13T08:00:00.000Z',
                       'teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_assessments_v2
               (public_id,title,class_id,assessment_context,evidence_policy,state,
                created_by,created_at,updated_at)
               VALUES ('assessment-a','随堂测',1,'quiz','include','active','teacher',
                       '2026-07-13T08:00:00.000Z','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_assessment_versions_v2
               (public_id,assessment_id,revision,item_set_hash,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('assessment-version-a',1,1,'{}','confirmed',
                       '2026-07-13T08:00:00.000Z','teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_assessment_items_v2
               (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                state,created_at)
               VALUES ('item-a',1,{},1,1,1,0,1,
                       '{{\"schema_version\":1,\"question_no\":\"1\"}}','active',
                       '2026-07-13T08:00:00.000Z');
             INSERT INTO exam_attempts_v2
               (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                attempt_kind,state,created_at,updated_at)
               VALUES ('attempt-a',1,1,1,'image','first','grading',
                       '2026-07-13T08:00:00.000Z','2026-07-13T08:00:00.000Z');",
        version.id,
        version.id,
        version.id,
        "a".repeat(64),
        version.id,
    ))
    .unwrap();

    let page = create_or_get(
        &conn,
        &NewArtifact {
            kind: ArtifactKind::Page,
            sha256: &"1".repeat(64),
            mime_type: "image/png",
            byte_size: 100,
            original_name: Some("student-page.png"),
            original_path: Some("/fixture/student-page.png"),
            archived_path: "/archive/student-page.png",
            parent_artifact_id: None,
            derivative_type: None,
            processing_version: "fixture-v1",
            privacy_class: PrivacyClass::StudentSensitive,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap();
    let aligned = create_or_get(
        &conn,
        &NewArtifact {
            kind: ArtifactKind::Page,
            sha256: &"2".repeat(64),
            mime_type: "image/png",
            byte_size: 90,
            original_name: None,
            original_path: None,
            archived_path: "/archive/aligned.png",
            parent_artifact_id: Some(page.id),
            derivative_type: Some("aligned"),
            processing_version: "fixture-v1",
            privacy_class: PrivacyClass::StudentSensitive,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap();
    let crop = create_or_get(
        &conn,
        &NewArtifact {
            kind: ArtifactKind::Crop,
            sha256: &"3".repeat(64),
            mime_type: "image/png",
            byte_size: 40,
            original_name: None,
            original_path: None,
            archived_path: "/archive/crop.png",
            parent_artifact_id: Some(aligned.id),
            derivative_type: Some("answer_region"),
            processing_version: "fixture-v1",
            privacy_class: PrivacyClass::StudentSensitive,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap();
    let reusable = create_or_get(
        &conn,
        &NewArtifact {
            kind: ArtifactKind::Crop,
            sha256: &"4".repeat(64),
            mime_type: "image/png",
            byte_size: 30,
            original_name: Some("clean-question.png"),
            original_path: Some("/fixture/clean-question.png"),
            archived_path: "/archive/clean-question.png",
            parent_artifact_id: None,
            derivative_type: None,
            processing_version: "fixture-v1",
            privacy_class: PrivacyClass::TeachingContent,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap();
    conn.execute_batch(&format!(
        "INSERT INTO exam_ingest_batches_v2
               (public_id,assessment_version_id,source_kind,idempotency_key,state,
                created_by,created_at,updated_at)
               VALUES ('batch-a',1,'fixed_fixture','batch-a','ready','teacher',
                       '2026-07-13T08:00:00.000Z','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_ingest_pages_v2
               (public_id,batch_id,source_artifact_id,import_index,expected_page_no,state,
                created_at,updated_at)
               VALUES ('page-a',1,{},0,1,'segmented','2026-07-13T08:00:00.000Z',
                       '2026-07-13T08:00:00.000Z');
             INSERT INTO exam_page_match_revisions_v2
               (public_id,page_id,revision,attempt_id,page_no,student_confidence,
                page_no_confidence,template_confidence,decision,confirmed_by,state,created_at)
               VALUES ('match-a',1,1,1,1,1,1,1,'teacher_confirmed','teacher','active',
                       '2026-07-13T08:00:00.000Z');
             INSERT INTO exam_page_alignment_revisions_v2
               (public_id,page_id,revision,match_revision_id,template_version,transform_json,
                confidence,aligned_artifact_id,decision,confirmed_by,state,created_at)
               VALUES ('alignment-a',1,1,1,'fixture-v1',
                       '{{\"schema_version\":1,\"matrix\":[1,0,0,0,1,0,0,0,1]}}',
                       1,{},'teacher_confirmed','teacher','active','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_answer_region_revisions_v2
               (public_id,page_id,assessment_item_id,region_index,revision,
                alignment_revision_id,bbox_json,crop_artifact_id,mapping_confidence,
                decision,confirmed_by,state,created_at)
               VALUES ('region-a',1,1,0,1,1,
                       '{{\"schema_version\":1,\"x\":0,\"y\":0,\"width\":1,\"height\":1}}',
                       {},1,'teacher_confirmed','teacher','active','2026-07-13T08:00:00.000Z');",
        page.id, aligned.id, crop.id
    ))
    .unwrap();
    Fixture {
        conn,
        region_id: 1,
        current_question_version_id: version.id,
        reusable_artifact_id: reusable.id,
    }
}

fn enqueue<'a>(
    fixture: &Fixture,
    draft: &'a QuestionDraft,
    scan: &'a PrivacyScan,
    source_type: &'a str,
    reusable_artifact_id: Option<i64>,
) -> CoreResult<QuestionIngestJob> {
    enqueue_question_ingest(
        &fixture.conn,
        &NewQuestionIngest {
            answer_region_revision_id: fixture.region_id,
            owner_id: "teacher",
            source_type,
            extraction_version: "extract-v1",
            draft,
            privacy_scan: scan,
            reusable_artifact_id,
        },
    )
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}

#[test]
fn exact_current_version_is_reused_without_duplicate_question() {
    let fixture = setup();
    let draft = exact_draft();
    let job = enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
    assert_eq!(job.state, "pending");
    assert_eq!(job.privacy_status, "text_only");
    let before = count(&fixture.conn, "k1_questions");
    let result = process_next_question_ingest(&fixture.conn)
        .unwrap()
        .unwrap();
    assert_eq!(result.candidate_status, "matched");
    assert_eq!(
        result.question_version_id,
        Some(fixture.current_question_version_id)
    );
    assert!(result.item_link_id.is_some());
    assert_eq!(count(&fixture.conn, "k1_questions"), before);
    let source_artifact: Option<i64> = fixture
        .conn
        .query_row(
            "SELECT reusable_artifact_id FROM exam_question_candidates_v2 WHERE id=?1",
            [result.candidate_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source_artifact, None);
}

#[test]
fn new_student_paper_question_creates_private_text_only_c0_once() {
    let fixture = setup();
    let draft = new_draft();
    let first = enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
    let same = enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
    assert_eq!(first.id, same.id);
    let result = process_next_question_ingest(&fixture.conn)
        .unwrap()
        .unwrap();
    assert_eq!(result.candidate_status, "candidate_created");
    let version_id = result.question_version_id.unwrap();
    let row: (String, String, bool, String, String, Option<i64>) = fixture
        .conn
        .query_row(
            "SELECT q.owner_scope,q.owner_id,q.sharing_allowed,v.quality_level,v.state,
                        v.source_artifact_id
                 FROM k1_question_versions v JOIN k1_questions q ON q.id=v.question_id
                 WHERE v.id=?1",
            [version_id],
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
    assert_eq!(
        row,
        (
            "personal".into(),
            "teacher".into(),
            false,
            "C0".into(),
            "candidate".into(),
            None
        )
    );
    assert!(process_next_question_ingest(&fixture.conn)
        .unwrap()
        .is_none());
    assert_eq!(count(&fixture.conn, "exam_question_candidates_v2"), 1);
}

#[test]
fn privacy_rejection_persists_no_content_or_outbox_and_keeps_grading_state() {
    let fixture = setup();
    let mut scan = safe_scan();
    scan.student_answer_detected = true;
    scan.sanitized = false;
    let job = enqueue(
        &fixture,
        &new_draft(),
        &scan,
        "student_paper",
        Some(fixture.reusable_artifact_id),
    )
    .unwrap();
    assert_eq!(job.state, "failed");
    assert_eq!(job.privacy_status, "rejected");
    let stored: (Option<String>, Option<String>, Option<i64>) = fixture
        .conn
        .query_row(
            "SELECT structured_payload_json,content_hash,active_event_id
                 FROM exam_question_ingest_jobs_v2 WHERE id=?1",
            [job.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(stored, (None, None, None));
    assert_eq!(count(&fixture.conn, "outbox_events"), 0);
    assert_eq!(count(&fixture.conn, "exam_question_candidates_v2"), 0);
    let region_state: (String, String) = fixture
        .conn
        .query_row(
            "SELECT decision,state FROM exam_answer_region_revisions_v2 WHERE id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(region_state, ("teacher_confirmed".into(), "active".into()));
}

#[test]
fn outbox_and_job_enqueue_are_atomic() {
    let fixture = setup();
    fixture
        .conn
        .execute_batch(
            "CREATE TRIGGER fail_question_job BEFORE INSERT ON exam_question_ingest_jobs_v2
                 BEGIN SELECT RAISE(ABORT, 'injected job failure'); END;",
        )
        .unwrap();
    assert!(enqueue(
        &fixture,
        &new_draft(),
        &safe_scan(),
        "source_document",
        Some(fixture.reusable_artifact_id)
    )
    .is_err());
    assert_eq!(count(&fixture.conn, "outbox_events"), 0);
    assert_eq!(count(&fixture.conn, "exam_question_ingest_jobs_v2"), 0);
}

#[test]
fn processing_failure_rolls_back_k1_and_retry_finishes_once() {
    let fixture = setup();
    let draft = new_draft();
    let job = enqueue(
        &fixture,
        &draft,
        &safe_scan(),
        "source_document",
        Some(fixture.reusable_artifact_id),
    )
    .unwrap();
    let questions_before = count(&fixture.conn, "k1_questions");
    assert!(process_next_internal(&fixture.conn, true).is_err());
    assert_eq!(count(&fixture.conn, "k1_questions"), questions_before);
    assert_eq!(count(&fixture.conn, "exam_question_candidates_v2"), 0);
    let failed = get_job(&fixture.conn, job.id).unwrap().unwrap();
    assert_eq!(failed.state, "failed");
    assert_eq!(failed.attempts, 1);
    retry_question_ingest(&fixture.conn, job.id).unwrap();
    let result = process_next_question_ingest(&fixture.conn)
        .unwrap()
        .unwrap();
    assert_eq!(result.candidate_status, "candidate_created");
    assert_eq!(count(&fixture.conn, "k1_questions"), questions_before + 1);
    assert_eq!(count(&fixture.conn, "exam_question_candidates_v2"), 1);
    let completed = get_job(&fixture.conn, job.id).unwrap().unwrap();
    assert_eq!(completed.state, "completed");
    assert_eq!(completed.attempts, 2);
    let source_artifact_id: Option<i64> = fixture
        .conn
        .query_row(
            "SELECT source_artifact_id FROM k1_question_versions WHERE id=?1",
            [result.question_version_id.unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source_artifact_id, Some(fixture.reusable_artifact_id));
}

#[test]
fn ambiguous_exact_matches_require_teacher_selection() {
    let fixture = setup();
    let second = content::create_question(
        &fixture.conn,
        &NewQuestion {
            owner_scope: "personal",
            owner_id: "teacher",
            question_family_id: None,
            rights_status: "unknown",
            sharing_allowed: false,
        },
    )
    .unwrap();
    let draft = exact_draft();
    content::create_question_version(
        &fixture.conn,
        &NewQuestionVersion {
            question_id: second.id,
            revision: 1,
            question_type: &draft.question_type,
            stem: &draft.stem,
            material_text: None,
            max_score: draft.max_score,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "C0",
            state: "candidate",
            options: &[],
        },
    )
    .unwrap();
    enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
    let result = process_next_question_ingest(&fixture.conn)
        .unwrap()
        .unwrap();
    assert_eq!(result.candidate_status, "needs_review");
    assert_eq!(result.question_version_id, None);
    assert_eq!(result.item_link_id, None);
    assert_eq!(count(&fixture.conn, "exam_duplicate_candidates_v2"), 2);
    assert_eq!(
        count(&fixture.conn, "exam_assessment_item_question_links_v2"),
        0
    );
}

#[test]
fn exact_auto_never_overwrites_an_active_teacher_link() {
    let fixture = setup();
    let other_question = content::create_question(
        &fixture.conn,
        &NewQuestion {
            owner_scope: "personal",
            owner_id: "teacher",
            question_family_id: None,
            rights_status: "unknown",
            sharing_allowed: false,
        },
    )
    .unwrap();
    let other_draft = new_draft();
    let options = borrowed_options(&other_draft);
    let other_version = content::create_question_version(
        &fixture.conn,
        &NewQuestionVersion {
            question_id: other_question.id,
            revision: 1,
            question_type: &other_draft.question_type,
            stem: &other_draft.stem,
            material_text: None,
            max_score: other_draft.max_score,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "C0",
            state: "candidate",
            options: &options,
        },
    )
    .unwrap();
    assert!(fixture
        .conn
        .execute(
            "INSERT INTO exam_assessment_item_question_links_v2
                 (public_id,assessment_item_id,revision,question_version_id,link_source,
                  state,created_at)
                 VALUES ('illegal-auto-link',1,1,?1,'exact_auto','active',
                         '2026-07-13T08:00:00.000Z')",
            [other_version.id],
        )
        .is_err());
    fixture
        .conn
        .execute(
            "INSERT INTO exam_assessment_item_question_links_v2
                 (public_id,assessment_item_id,revision,question_version_id,link_source,
                  state,linked_by,created_at)
                 VALUES ('teacher-link',1,1,?1,'teacher_selected','active','teacher',
                         '2026-07-13T08:00:00.000Z')",
            [other_version.id],
        )
        .unwrap();
    let draft = exact_draft();
    enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
    let result = process_next_question_ingest(&fixture.conn)
        .unwrap()
        .unwrap();
    assert_eq!(result.candidate_status, "needs_review");
    assert_eq!(result.item_link_id, None);
    let active: (i64, String) = fixture
        .conn
        .query_row(
            "SELECT question_version_id,state
                 FROM exam_assessment_item_question_links_v2 WHERE public_id='teacher-link'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(active, (other_version.id, "active".into()));
}

#[test]
fn claimed_but_interrupted_event_is_failed_and_can_be_retried() {
    let fixture = setup();
    let draft = new_draft();
    let job = enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
    outbox::ensure_pending_for_consumer(&fixture.conn, CONSUMER, EVENT_TYPE, 1).unwrap();
    let (_, consumption) = outbox::claim_next(
        &fixture.conn,
        CONSUMER,
        "2026-07-13T08:00:00.000Z",
        "interrupted-lease",
        "2026-07-13T08:05:00.000Z",
    )
    .unwrap()
    .unwrap();
    assert_eq!(consumption.event_id, job.active_event_id.unwrap());
    assert_eq!(
        recover_interrupted_question_ingest(&fixture.conn).unwrap(),
        1
    );
    assert_eq!(
        get_job(&fixture.conn, job.id).unwrap().unwrap().state,
        "failed"
    );
    retry_question_ingest(&fixture.conn, job.id).unwrap();
    assert_eq!(
        process_next_question_ingest(&fixture.conn)
            .unwrap()
            .unwrap()
            .candidate_status,
        "candidate_created"
    );
}

#[test]
fn teacher_promotes_private_candidate_to_new_l1_without_rebinding_assessment() {
    let mut fixture = setup();
    let draft = new_draft();
    enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
    process_next_question_ingest(&fixture.conn)
        .unwrap()
        .unwrap();

    let inbox = list_candidate_review_inbox(&fixture.conn, "teacher", false, 100).unwrap();
    assert_eq!(inbox.pending_count, 1);
    let candidate = inbox.items.into_iter().next().unwrap();
    let versions_before = count(&fixture.conn, "k1_question_versions");
    let request = PromoteCandidateRequest {
        request_key: "candidate-promote-a".into(),
        candidate_public_id: candidate.candidate_public_id,
        expected_content_hash: candidate.content_hash,
        question_type: candidate.question_type,
        stem: candidate.stem,
        material_text: candidate.material_text,
        max_score: candidate.max_score,
        options: candidate
            .options
            .into_iter()
            .map(|option| CandidateReviewOptionInput {
                label: option.label,
                content: option.content,
                order_index: option.order_index,
            })
            .collect(),
        answer_text: "B".into(),
        note: Some("老师核对题干和答案".into()),
        reviewed_by: "teacher".into(),
    };

    let decision = promote_candidate_to_l1(&mut fixture.conn, "teacher", &request).unwrap();
    assert_eq!(decision.action, "promote_l1");
    assert_eq!(decision.result_quality_level.as_deref(), Some("L1"));
    assert!(!decision.current_assessment_rebound);
    assert_eq!(
        count(&fixture.conn, "k1_question_versions"),
        versions_before + 1
    );
    let result_public_id = decision
        .result_question_version_public_id
        .as_deref()
        .unwrap();
    let result: (i64, String, String, Option<i64>) = fixture
        .conn
        .query_row(
            "SELECT revision,quality_level,state,supersedes_version_id
                 FROM k1_question_versions WHERE public_id=?1",
            [result_public_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(result.0, 2);
    assert_eq!(result.1, "L1");
    assert_eq!(result.2, "published");
    assert!(result.3.is_some());
    let answer_json: String = fixture
        .conn
        .query_row(
            "SELECT answer_json FROM k1_answer_key_versions
                 WHERE public_id=?1 AND state='confirmed'",
            [decision
                .result_answer_key_version_public_id
                .as_deref()
                .unwrap()],
            |row| row.get(0),
        )
        .unwrap();
    let answer: serde_json::Value = serde_json::from_str(&answer_json).unwrap();
    assert_eq!(answer["correct_labels"], serde_json::json!(["B"]));
    let fixed_version_id: i64 = fixture
        .conn
        .query_row(
            "SELECT question_version_id FROM exam_assessment_items_v2 WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(fixed_version_id, fixture.current_question_version_id);
    let link: (String, String) = fixture
        .conn
        .query_row(
            "SELECT link_source,v.public_id
                 FROM exam_assessment_item_question_links_v2 link
                 JOIN k1_question_versions v ON v.id=link.question_version_id
                 WHERE link.assessment_item_id=1 AND link.state='active'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(link, ("candidate_promoted".into(), result_public_id.into()));

    let replay = promote_candidate_to_l1(&mut fixture.conn, "teacher", &request).unwrap();
    assert_eq!(replay.public_id, decision.public_id);
    assert_eq!(
        count(&fixture.conn, "k1_question_versions"),
        versions_before + 1
    );
    assert_eq!(
        count(&fixture.conn, "exam_question_candidate_reviews_v2"),
        1
    );
    assert!(fixture
        .conn
        .execute(
            "UPDATE exam_question_candidate_reviews_v2 SET note='changed' WHERE public_id=?1",
            [&decision.public_id],
        )
        .is_err());
    assert!(fixture
        .conn
        .execute(
            "DELETE FROM exam_question_candidate_reviews_v2 WHERE public_id=?1",
            [&decision.public_id],
        )
        .is_err());
}

#[test]
fn teacher_discards_candidate_without_creating_version_or_link() {
    let mut fixture = setup();
    enqueue(&fixture, &new_draft(), &safe_scan(), "student_paper", None).unwrap();
    process_next_question_ingest(&fixture.conn)
        .unwrap()
        .unwrap();
    let candidate = list_candidate_review_inbox(&fixture.conn, "teacher", false, 100)
        .unwrap()
        .items
        .into_iter()
        .next()
        .unwrap();
    let versions_before = count(&fixture.conn, "k1_question_versions");
    let request = DiscardCandidateRequest {
        request_key: "candidate-discard-a".into(),
        candidate_public_id: candidate.candidate_public_id,
        expected_content_hash: candidate.content_hash,
        note: Some("本次题面识别不完整".into()),
        reviewed_by: "teacher".into(),
    };

    let decision = discard_candidate(&mut fixture.conn, "teacher", &request).unwrap();
    assert_eq!(decision.action, "discard");
    assert_eq!(decision.result_question_version_public_id, None);
    assert_eq!(
        count(&fixture.conn, "k1_question_versions"),
        versions_before
    );
    assert_eq!(
        count(&fixture.conn, "exam_assessment_item_question_links_v2"),
        0
    );
    assert_eq!(
        list_candidate_review_inbox(&fixture.conn, "teacher", false, 100)
            .unwrap()
            .pending_count,
        0
    );
    let reviewed = list_candidate_review_inbox(&fixture.conn, "teacher", true, 100)
        .unwrap()
        .items;
    assert_eq!(reviewed.len(), 1);
    assert_eq!(reviewed[0].review_action.as_deref(), Some("discard"));

    let replay = discard_candidate(&mut fixture.conn, "teacher", &request).unwrap();
    assert_eq!(replay.public_id, decision.public_id);
    assert_eq!(
        count(&fixture.conn, "exam_question_candidate_reviews_v2"),
        1
    );
}

#[test]
fn promotion_rejects_candidate_whose_source_is_no_longer_current() {
    let mut fixture = setup();
    let draft = new_draft();
    enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
    process_next_question_ingest(&fixture.conn)
        .unwrap()
        .unwrap();
    let candidate = list_candidate_review_inbox(&fixture.conn, "teacher", false, 100)
        .unwrap()
        .items
        .into_iter()
        .next()
        .unwrap();
    let (question_id, source_version_id): (i64, i64) = fixture
        .conn
        .query_row(
            "SELECT question_id,id FROM k1_question_versions WHERE public_id=?1",
            [&candidate.source_question_version_public_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let options = borrowed_options(&draft);
    content::create_question_version(
        &fixture.conn,
        &NewQuestionVersion {
            question_id,
            revision: 2,
            question_type: &draft.question_type,
            stem: "洋务运动后期的主要口号是？",
            material_text: None,
            max_score: draft.max_score,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: Some(source_version_id),
            quality_level: "L0",
            state: "draft",
            options: &options,
        },
    )
    .unwrap();
    let versions_before = count(&fixture.conn, "k1_question_versions");
    let request = PromoteCandidateRequest {
        request_key: "candidate-promote-stale-source".into(),
        candidate_public_id: candidate.candidate_public_id,
        expected_content_hash: candidate.content_hash,
        question_type: candidate.question_type,
        stem: candidate.stem,
        material_text: candidate.material_text,
        max_score: candidate.max_score,
        options: candidate
            .options
            .into_iter()
            .map(|option| CandidateReviewOptionInput {
                label: option.label,
                content: option.content,
                order_index: option.order_index,
            })
            .collect(),
        answer_text: "B".into(),
        note: None,
        reviewed_by: "teacher".into(),
    };

    let error = promote_candidate_to_l1(&mut fixture.conn, "teacher", &request).unwrap_err();
    assert!(error.to_string().contains("新的题目版本取代"));
    assert_eq!(
        count(&fixture.conn, "k1_question_versions"),
        versions_before
    );
    assert_eq!(
        count(&fixture.conn, "exam_question_candidate_reviews_v2"),
        0
    );
}

#[test]
fn active_assessment_link_blocks_promotion_and_rolls_back_new_library_rows() {
    let mut fixture = setup();
    enqueue(&fixture, &new_draft(), &safe_scan(), "student_paper", None).unwrap();
    process_next_question_ingest(&fixture.conn)
        .unwrap()
        .unwrap();
    let candidate = list_candidate_review_inbox(&fixture.conn, "teacher", false, 100)
        .unwrap()
        .items
        .into_iter()
        .next()
        .unwrap();
    fixture
        .conn
        .execute(
            "INSERT INTO exam_assessment_item_question_links_v2
                 (public_id,assessment_item_id,revision,question_version_id,link_source,
                  state,linked_by,created_at)
                 VALUES ('existing-teacher-link',1,1,?1,'teacher_selected','active',
                         'teacher','2026-07-19T12:00:00.000Z')",
            [fixture.current_question_version_id],
        )
        .unwrap();
    let versions_before = count(&fixture.conn, "k1_question_versions");
    let answers_before = count(&fixture.conn, "k1_answer_key_versions");
    let request = PromoteCandidateRequest {
        request_key: "candidate-promote-active-link".into(),
        candidate_public_id: candidate.candidate_public_id,
        expected_content_hash: candidate.content_hash,
        question_type: candidate.question_type,
        stem: candidate.stem,
        material_text: candidate.material_text,
        max_score: candidate.max_score,
        options: candidate
            .options
            .into_iter()
            .map(|option| CandidateReviewOptionInput {
                label: option.label,
                content: option.content,
                order_index: option.order_index,
            })
            .collect(),
        answer_text: "B".into(),
        note: None,
        reviewed_by: "teacher".into(),
    };

    let error = promote_candidate_to_l1(&mut fixture.conn, "teacher", &request).unwrap_err();
    assert!(error.to_string().contains("不会静默覆盖"));
    assert_eq!(
        count(&fixture.conn, "k1_question_versions"),
        versions_before
    );
    assert_eq!(
        count(&fixture.conn, "k1_answer_key_versions"),
        answers_before
    );
    assert_eq!(
        count(&fixture.conn, "exam_question_candidate_reviews_v2"),
        0
    );
    let active_link: (String, i64) = fixture
        .conn
        .query_row(
            "SELECT public_id,question_version_id
                 FROM exam_assessment_item_question_links_v2
                 WHERE assessment_item_id=1 AND state='active'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        active_link,
        (
            "existing-teacher-link".into(),
            fixture.current_question_version_id
        )
    );
}

#[test]
fn promotion_rejects_exact_collision_with_other_identity_atomically() {
    let mut fixture = setup();
    let draft = new_draft();
    enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
    process_next_question_ingest(&fixture.conn)
        .unwrap()
        .unwrap();
    let candidate = list_candidate_review_inbox(&fixture.conn, "teacher", false, 100)
        .unwrap()
        .items
        .into_iter()
        .next()
        .unwrap();
    let duplicate_identity = content::create_question(
        &fixture.conn,
        &NewQuestion {
            owner_scope: "personal",
            owner_id: "teacher",
            question_family_id: None,
            rights_status: "unknown",
            sharing_allowed: false,
        },
    )
    .unwrap();
    let options = borrowed_options(&draft);
    content::create_question_version(
        &fixture.conn,
        &NewQuestionVersion {
            question_id: duplicate_identity.id,
            revision: 1,
            question_type: &draft.question_type,
            stem: &draft.stem,
            material_text: draft.material_text.as_deref(),
            max_score: draft.max_score,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "C0",
            state: "candidate",
            options: &options,
        },
    )
    .unwrap();
    let versions_before = count(&fixture.conn, "k1_question_versions");
    let request = PromoteCandidateRequest {
        request_key: "candidate-promote-collision".into(),
        candidate_public_id: candidate.candidate_public_id,
        expected_content_hash: candidate.content_hash,
        question_type: candidate.question_type,
        stem: candidate.stem,
        material_text: candidate.material_text,
        max_score: candidate.max_score,
        options: candidate
            .options
            .into_iter()
            .map(|option| CandidateReviewOptionInput {
                label: option.label,
                content: option.content,
                order_index: option.order_index,
            })
            .collect(),
        answer_text: "B".into(),
        note: None,
        reviewed_by: "teacher".into(),
    };

    let error = promote_candidate_to_l1(&mut fixture.conn, "teacher", &request).unwrap_err();
    assert!(error.to_string().contains("完全一致"));
    assert_eq!(
        count(&fixture.conn, "k1_question_versions"),
        versions_before
    );
    assert_eq!(
        count(&fixture.conn, "exam_question_candidate_reviews_v2"),
        0
    );
    assert_eq!(
        count(&fixture.conn, "exam_assessment_item_question_links_v2"),
        0
    );
}
