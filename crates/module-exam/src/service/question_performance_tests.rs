use super::*;
use module_knowledge::db::content::{
    create_answer_key_version, create_link_set, create_question, create_rubric_version,
    NewAnswerKeyVersion, NewQuestion, NewRubricPoint, NewRubricVersion, QuestionVersion,
};
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

const NOW: &str = "2026-07-19T10:00:00Z";

struct Fixture {
    conn: Connection,
    question_version_public_id: String,
    question_version_id: i64,
    old_answer_id: i64,
    old_rubric_id: i64,
    old_link_id: i64,
}

fn fixture_with_question_type(question_type: &str) -> Fixture {
    let conn = open_in_memory().unwrap();
    run_migrations(&conn, CORE_MIGRATIONS).unwrap();
    run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
    run_migrations(&conn, crate::exam_migrations()).unwrap();
    conn.execute_batch(
        "CREATE TABLE profile_evidence_links(
           snapshot_id INTEGER NOT NULL,
           node_metric_id INTEGER NOT NULL,
           learning_evidence_id INTEGER NOT NULL
         );
         INSERT INTO subjects(name) VALUES ('历史');
         INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
         INSERT INTO students(name,student_no,class_id,enabled)
         VALUES ('学生甲','01',1,1),('学生乙','02',1,1);
         INSERT INTO k1_textbook_editions
         (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
         VALUES ('edition-history-8a',1,'pep','2026','中国历史八年级上册','八年级',
                 'upper','active','2026-07-01T00:00:00Z');
         INSERT INTO k1_knowledge_maps
         (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
         VALUES ('knowledge-map-history-8a-v1',1,1,'confirmed',
                 '2026-07-01T00:00:00Z','2026-07-01T00:00:00Z');",
    )
    .unwrap();
    let question = create_question(
        &conn,
        &NewQuestion {
            owner_scope: "personal",
            owner_id: "local_teacher",
            question_family_id: None,
            rights_status: "cleared",
            sharing_allowed: false,
        },
    )
    .unwrap();
    let version_public_id = "question-version-opium-war-year".to_string();
    conn.execute(
        "INSERT INTO k1_question_versions
         (public_id,question_id,revision,question_type,stem,max_score,content_hash,
          quality_level,state,created_at)
         VALUES (?1,?2,1,?3,'鸦片战争爆发于哪一年？',2,
                 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                 'L3','published',?4)",
        params![&version_public_id, question.id, question_type, NOW],
    )
    .unwrap();
    let version = QuestionVersion {
        id: conn.last_insert_rowid(),
        public_id: version_public_id,
        question_id: question.id,
        revision: 1,
        question_type: question_type.into(),
        stem: "鸦片战争爆发于哪一年？".into(),
        material_text: None,
        max_score: 2.0,
        content_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        source_artifact_id: None,
        source_anchor_json: None,
        supersedes_version_id: None,
        quality_level: "L3".into(),
        state: "published".into(),
        created_at: NOW.into(),
    };
    let old_answer = create_answer_key_version(
        &conn,
        &NewAnswerKeyVersion {
            question_version_id: version.id,
            revision: 1,
            answer_json: "{\"schema_version\":1,\"correct_labels\":[\"A\"]}",
            state: "confirmed",
            supersedes_answer_key_id: None,
            confirmed_by: Some("local_teacher"),
            slots: &[],
        },
    )
    .unwrap();
    let old_rubric = create_rubric_version(
        &conn,
        &NewRubricVersion {
            question_version_id: version.id,
            revision: 1,
            max_score: 2.0,
            state: "confirmed",
            supersedes_rubric_id: None,
            confirmed_by: Some("local_teacher"),
            points: &[NewRubricPoint {
                stable_id: None,
                order_index: 0,
                canonical_text: "选择正确年份",
                allowed_paraphrases_json: None,
                required_concepts_json: None,
                max_score: 2.0,
            }],
        },
    )
    .unwrap();
    let old_link = create_link_set(
        &conn,
        version.id,
        1,
        1,
        "confirmed",
        Some("local_teacher"),
        None,
    )
    .unwrap();
    let _new_answer = create_answer_key_version(
        &conn,
        &NewAnswerKeyVersion {
            question_version_id: version.id,
            revision: 2,
            answer_json: "{\"schema_version\":1,\"correct_labels\":[\"B\"]}",
            state: "confirmed",
            supersedes_answer_key_id: Some(old_answer.id),
            confirmed_by: Some("local_teacher"),
            slots: &[],
        },
    )
    .unwrap();
    let _new_rubric = create_rubric_version(
        &conn,
        &NewRubricVersion {
            question_version_id: version.id,
            revision: 2,
            max_score: 2.0,
            state: "confirmed",
            supersedes_rubric_id: Some(old_rubric.id),
            confirmed_by: Some("local_teacher"),
            points: &[NewRubricPoint {
                stable_id: None,
                order_index: 0,
                canonical_text: "选择修订后的正确年份",
                allowed_paraphrases_json: None,
                required_concepts_json: None,
                max_score: 2.0,
            }],
        },
    )
    .unwrap();
    let _new_link = create_link_set(
        &conn,
        version.id,
        1,
        2,
        "confirmed",
        Some("local_teacher"),
        Some(old_link.id),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO exam_assessments_v2
         (public_id,title,class_id,assessment_context,evidence_policy,state,
          created_by,created_at,updated_at)
         VALUES ('assessment-1','第一单元检测',1,'quiz','include','active',
                 'local_teacher',?1,?1)",
        [NOW],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO exam_assessment_versions_v2
         (public_id,assessment_id,revision,item_set_hash,state,created_at,confirmed_by,confirmed_at)
         VALUES ('assessment-version-1',1,1,
         'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
         'confirmed',?1,'local_teacher',?1)",
        [NOW],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO exam_assessment_items_v2
         (public_id,assessment_version_id,question_version_id,answer_key_version_id,
          rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,state,created_at)
         VALUES ('assessment-item-1',1,?1,?2,?3,?4,0,2,
                 '{\"schema_version\":1}','active',?5)",
        params![version.id, old_answer.id, old_rubric.id, old_link.id, NOW],
    )
    .unwrap();
    for (student_id, state, publication) in [
        (1_i64, "ready_to_publish", false),
        (2_i64, "published", true),
    ] {
        conn.execute(
            "INSERT INTO exam_attempts_v2
             (public_id,assessment_version_id,student_id,attempt_no,source_kind,
              attempt_kind,state,created_at,updated_at)
             VALUES (?1,1,?2,1,'image','first',?3,?4,?4)",
            params![format!("attempt-{student_id}"), student_id, state, NOW],
        )
        .unwrap();
        let attempt_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO exam_grade_decisions_v2
             (public_id,attempt_id,assessment_item_id,revision,teacher_score,
              point_results_json,confirmation_level,state,decided_by,decided_at,created_at)
             VALUES (?1,?2,1,1,2,'{\"schema_version\":1}',
                     'teacher_accepted','active','local_teacher',?3,?3)",
            params![format!("decision-{student_id}"), attempt_id, NOW],
        )
        .unwrap();
        if publication {
            let decision_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO exam_grade_publications_v2
                 (public_id,assessment_version_id,revision,state,published_by,published_at,created_at)
                 VALUES ('publication-1',1,1,'published','local_teacher',?1,?1)",
                [NOW],
            )
            .unwrap();
            let publication_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO exam_grade_publication_items_v2
                 (publication_id,attempt_id,grade_decision_set_hash,total_score,created_at)
                 VALUES (?1,?2,
                 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',2,?3)",
                params![publication_id, attempt_id, NOW],
            )
            .unwrap();
            let publication_item_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO exam_grade_publication_decisions_v2
                 (publication_item_id,attempt_id,grade_decision_id,created_at)
                 VALUES (?1,?2,?3,?4)",
                params![publication_item_id, attempt_id, decision_id, NOW],
            )
            .unwrap();
            conn.execute(
                "UPDATE exam_attempts_v2 SET active_publication_id=?1 WHERE id=?2",
                params![publication_id, attempt_id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO learning_evidence
                 (public_id,idempotency_key,student_id,source_module,source_type,
                  source_ref_type,source_ref_id,source_revision,decision_ref_type,
                  decision_ref_id,decision_revision,evidence_kind,value,confirmation_level,
                  evidence_quality,assessment_context,occurred_at,rule_version,
                  knowledge_map_version,state)
                 VALUES ('00000000-0000-7000-8000-000000000001','evidence-1',2,'grading',
                  'objective_question','assessment_item','assessment-item-1',1,
                  'grade_decision','decision-2',1,'accuracy',1,'teacher_accepted',1,
                  'closed_book',?1,'objective-grading-v1','map-v1','active')",
                [NOW],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO profile_evidence_links(snapshot_id,node_metric_id,learning_evidence_id)
                 VALUES (1,1,1)",
                [],
            )
            .unwrap();
        }
    }
    Fixture {
        conn,
        question_version_public_id: version.public_id,
        question_version_id: version.id,
        old_answer_id: old_answer.id,
        old_rubric_id: old_rubric.id,
        old_link_id: old_link.id,
    }
}

fn fixture() -> Fixture {
    fixture_with_question_type("single")
}

#[test]
fn performance_uses_only_current_published_teacher_decisions() {
    let fixture = fixture();
    fixture
        .conn
        .execute_batch(
            "INSERT INTO k1_questions
             (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
             VALUES ('unused-question','personal','local_teacher','cleared',0,
                     '2026-07-19T10:00:00Z');
             INSERT INTO k1_question_versions
             (public_id,question_id,revision,question_type,stem,max_score,content_hash,
              quality_level,state,created_at)
             VALUES ('unused-version',2,1,'true_false','未使用候选题',1,
                     'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                     'C0','candidate','2026-07-19T10:00:00Z');",
        )
        .unwrap();
    let catalog = list_question_performance(&fixture.conn, "local_teacher", 50).unwrap();
    assert_eq!(catalog.items.len(), 1, "未被作业使用的候选题不进入表现页");
    let item = catalog
        .items
        .iter()
        .find(|item| item.question_version_public_id == fixture.question_version_public_id)
        .unwrap();
    assert_eq!(item.assessment_usage_count, 1);
    assert_eq!(item.published_response_count, 1);
    assert_eq!(item.full_credit_count, 1);
    assert_eq!(item.average_score_rate, Some(1.0));
    assert_eq!(item.context_breakdown[0].assessment_context, "quiz");
    assert!(item.has_version_update_impact);
}

#[test]
fn read_models_are_query_only_and_preserve_owner_and_history_boundaries() {
    fn history_counts(conn: &Connection) -> [i64; 10] {
        conn.query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_grade_decisions_v2),
               (SELECT COUNT(*) FROM exam_grade_publications_v2),
               (SELECT COUNT(*) FROM learning_evidence),
               (SELECT COUNT(*) FROM profile_evidence_links),
               (SELECT COUNT(*) FROM exam_question_version_impact_plans_v2),
               (SELECT COUNT(*) FROM exam_question_version_impact_tasks_v2),
               (SELECT COUNT(*) FROM exam_question_version_review_cases_v2),
               (SELECT COUNT(*) FROM exam_question_version_review_resolutions_v2),
               (SELECT COUNT(*) FROM outbox_events),
               (SELECT COUNT(*) FROM audit_events)",
            [],
            |row| {
                Ok([
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ])
            },
        )
        .unwrap()
    }

    let mut fixture = fixture();
    let review_case = prepared_case(
        &mut fixture,
        "recalculate_unpublished",
        "read-model-query-only-plan",
    );
    let before_changes: i64 = fixture
        .conn
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    let before_history = history_counts(&fixture.conn);

    let performance = list_question_performance(&fixture.conn, "local_teacher", 50).unwrap();
    assert_eq!(performance.items.len(), 1);
    assert_eq!(performance.items[0].published_response_count, 1);
    let preview = preview_question_version_impact(
        &fixture.conn,
        "local_teacher",
        &fixture.question_version_public_id,
    )
    .unwrap();
    assert_eq!(preview.affected_assessment_count, 1);
    assert_eq!(preview.published_attempt_count, 1);
    let cases = list_question_impact_review_cases(
        &fixture.conn,
        "local_teacher",
        &review_case.plan_public_id,
    )
    .unwrap();
    assert_eq!(cases.cases.len(), 1);
    assert_eq!(cases.cases[0].state, "open");

    let other_owner = list_question_performance(&fixture.conn, "other_teacher", 50).unwrap();
    assert!(other_owner.items.is_empty());
    assert!(preview_question_version_impact(
        &fixture.conn,
        "other_teacher",
        &fixture.question_version_public_id,
    )
    .is_err());
    assert!(list_question_impact_review_cases(
        &fixture.conn,
        "other_teacher",
        &review_case.plan_public_id,
    )
    .is_err());

    let after_changes: i64 = fixture
        .conn
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(after_changes, before_changes);
    assert_eq!(history_counts(&fixture.conn), before_history);
}

#[test]
fn impact_preview_and_plans_never_change_grades_or_evidence() {
    let mut fixture = fixture();
    let preview = preview_question_version_impact(
        &fixture.conn,
        "local_teacher",
        &fixture.question_version_public_id,
    )
    .unwrap();
    assert_eq!(preview.affected_assessment_count, 1);
    assert_eq!(preview.unpublished_attempt_count, 1);
    assert_eq!(preview.published_attempt_count, 1);
    assert_eq!(preview.active_learning_evidence_count, 1);
    assert_eq!(preview.profile_snapshot_count, 1);
    let before: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
             (SELECT COUNT(*) FROM exam_grade_decisions_v2),
             (SELECT COUNT(*) FROM exam_grade_publications_v2),
             (SELECT COUNT(*) FROM learning_evidence)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let request = ConfirmQuestionImpactPlanRequest {
        request_key: "impact-unpublished".into(),
        question_version_public_id: fixture.question_version_public_id.clone(),
        expected_preview_hash: preview.preview_hash.clone(),
        action: "recalculate_unpublished".into(),
        planned_by: "local_teacher".into(),
    };
    let plan =
        confirm_question_impact_plan(&mut fixture.conn, "local_teacher", &request).unwrap();
    assert_eq!(plan.task_count, 1);
    assert!(!plan.changes_grade);
    let repeated =
        confirm_question_impact_plan(&mut fixture.conn, "local_teacher", &request).unwrap();
    assert_eq!(plan.public_id, repeated.public_id);
    let prepare = PrepareQuestionImpactReviewCasesRequest {
        plan_public_id: plan.public_id.clone(),
        expected_task_count: 1,
        prepared_by: "local_teacher".into(),
    };
    let prepared =
        prepare_question_impact_review_cases(&mut fixture.conn, "local_teacher", &prepare)
            .unwrap();
    assert_eq!(prepared.created_count, 1);
    assert_eq!(prepared.existing_count, 0);
    assert_eq!(prepared.cases.len(), 1);
    assert_eq!(prepared.cases[0].case_kind, "unpublished_recalculation");
    assert_eq!(
        prepared.cases[0].source_grade_decision_public_id.as_deref(),
        Some("decision-1")
    );
    assert_eq!(prepared.cases[0].source_teacher_score, Some(2.0));
    assert!(!prepared.changes_grade);
    let retried =
        prepare_question_impact_review_cases(&mut fixture.conn, "local_teacher", &prepare)
            .unwrap();
    assert_eq!(retried.created_count, 0);
    assert_eq!(retried.existing_count, 1);
    assert_eq!(retried.cases[0].public_id, prepared.cases[0].public_id);
    assert!(fixture
        .conn
        .execute(
            "UPDATE exam_question_version_review_cases_v2 SET state='open'",
            [],
        )
        .is_err());
    assert!(fixture
        .conn
        .execute("DELETE FROM exam_question_version_review_cases_v2", [])
        .is_err());
    let after: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
             (SELECT COUNT(*) FROM exam_grade_decisions_v2),
             (SELECT COUNT(*) FROM exam_grade_publications_v2),
             (SELECT COUNT(*) FROM learning_evidence)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(before, after);
    let source_ids: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT source_answer_key_version_id,source_rubric_version_id,source_link_set_id
             FROM exam_question_version_impact_tasks_v2",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        source_ids,
        (
            fixture.old_answer_id,
            fixture.old_rubric_id,
            fixture.old_link_id
        )
    );
}

#[test]
fn impact_plan_outbox_failure_rolls_back_plan_tasks_audit_and_history() {
    fn counts(conn: &Connection) -> [i64; 8] {
        conn.query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_question_version_impact_plans_v2),
               (SELECT COUNT(*) FROM exam_question_version_impact_tasks_v2),
               (SELECT COUNT(*) FROM outbox_events),
               (SELECT COUNT(*) FROM audit_events),
               (SELECT COUNT(*) FROM exam_grade_decisions_v2),
               (SELECT COUNT(*) FROM exam_grade_publications_v2),
               (SELECT COUNT(*) FROM learning_evidence),
               (SELECT COUNT(*) FROM profile_evidence_links)",
            [],
            |row| {
                Ok([
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ])
            },
        )
        .unwrap()
    }

    let mut fixture = fixture();
    let preview = preview_question_version_impact(
        &fixture.conn,
        "local_teacher",
        &fixture.question_version_public_id,
    )
    .unwrap();
    let request = ConfirmQuestionImpactPlanRequest {
        request_key: "impact-plan-outbox-rollback".into(),
        question_version_public_id: fixture.question_version_public_id.clone(),
        expected_preview_hash: preview.preview_hash,
        action: "recalculate_unpublished".into(),
        planned_by: "local_teacher".into(),
    };
    let before = counts(&fixture.conn);

    fixture
        .conn
        .execute_batch(
            "CREATE TRIGGER inject_impact_plan_outbox_failure
             BEFORE INSERT ON outbox_events
             WHEN NEW.event_type='k1.question_version_impact.planned'
             BEGIN
               SELECT RAISE(ABORT, 'injected impact plan outbox failure');
             END;",
        )
        .unwrap();
    let failed = confirm_question_impact_plan(&mut fixture.conn, "local_teacher", &request);
    assert!(failed.is_err());
    fixture
        .conn
        .execute_batch("DROP TRIGGER inject_impact_plan_outbox_failure;")
        .unwrap();

    assert_eq!(counts(&fixture.conn), before);

    let retried =
        confirm_question_impact_plan(&mut fixture.conn, "local_teacher", &request).unwrap();
    assert_eq!(retried.task_count, 1);
    let after_retry = counts(&fixture.conn);
    assert_eq!(after_retry[0], before[0] + 1);
    assert_eq!(after_retry[1], before[1] + 1);
    assert_eq!(after_retry[2], before[2] + 1);
    assert_eq!(after_retry[3], before[3] + 1);
    assert_eq!(after_retry[4], before[4]);
    assert_eq!(after_retry[5], before[5]);
    assert_eq!(after_retry[6], before[6]);
    assert_eq!(after_retry[7], before[7]);
}

#[test]
fn review_case_outbox_failure_rolls_back_case_audit_and_history() {
    fn counts(conn: &Connection) -> [i64; 8] {
        conn.query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_question_version_review_cases_v2),
               (SELECT COUNT(*) FROM exam_question_version_impact_tasks_v2),
               (SELECT COUNT(*) FROM outbox_events),
               (SELECT COUNT(*) FROM audit_events),
               (SELECT COUNT(*) FROM exam_grade_decisions_v2),
               (SELECT COUNT(*) FROM exam_grade_publications_v2),
               (SELECT COUNT(*) FROM learning_evidence),
               (SELECT COUNT(*) FROM profile_evidence_links)",
            [],
            |row| {
                Ok([
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ])
            },
        )
        .unwrap()
    }

    let mut fixture = fixture();
    let preview = preview_question_version_impact(
        &fixture.conn,
        "local_teacher",
        &fixture.question_version_public_id,
    )
    .unwrap();
    let plan = confirm_question_impact_plan(
        &mut fixture.conn,
        "local_teacher",
        &ConfirmQuestionImpactPlanRequest {
            request_key: "review-case-outbox-rollback-plan".into(),
            question_version_public_id: fixture.question_version_public_id.clone(),
            expected_preview_hash: preview.preview_hash,
            action: "recalculate_unpublished".into(),
            planned_by: "local_teacher".into(),
        },
    )
    .unwrap();
    assert_eq!(plan.task_count, 1);
    let request = PrepareQuestionImpactReviewCasesRequest {
        plan_public_id: plan.public_id,
        expected_task_count: 1,
        prepared_by: "local_teacher".into(),
    };
    let before = counts(&fixture.conn);

    fixture
        .conn
        .execute_batch(
            "CREATE TRIGGER inject_review_case_outbox_failure
             BEFORE INSERT ON outbox_events
             WHEN NEW.event_type='k1.question_version_impact.review_cases_prepared'
             BEGIN
               SELECT RAISE(ABORT, 'injected review case outbox failure');
             END;",
        )
        .unwrap();
    let failed = prepare_question_impact_review_cases(&mut fixture.conn, "local_teacher", &request);
    assert!(failed.is_err());
    fixture
        .conn
        .execute_batch("DROP TRIGGER inject_review_case_outbox_failure;")
        .unwrap();

    assert_eq!(counts(&fixture.conn), before);

    let retried =
        prepare_question_impact_review_cases(&mut fixture.conn, "local_teacher", &request).unwrap();
    assert_eq!(retried.created_count, 1);
    assert_eq!(retried.existing_count, 0);
    assert_eq!(retried.cases.len(), 1);
    let after_retry = counts(&fixture.conn);
    assert_eq!(after_retry[0], before[0] + 1);
    assert_eq!(after_retry[1], before[1]);
    assert_eq!(after_retry[2], before[2] + 1);
    assert_eq!(after_retry[3], before[3] + 1);
    assert_eq!(after_retry[4], before[4]);
    assert_eq!(after_retry[5], before[5]);
    assert_eq!(after_retry[6], before[6]);
    assert_eq!(after_retry[7], before[7]);
}

#[test]
fn published_review_is_separate_and_preview_drift_is_rejected() {
    let mut fixture = fixture();
    let preview = preview_question_version_impact(
        &fixture.conn,
        "local_teacher",
        &fixture.question_version_public_id,
    )
    .unwrap();
    let new_answer = create_answer_key_version(
        &fixture.conn,
        &NewAnswerKeyVersion {
            question_version_id: fixture.question_version_id,
            revision: 3,
            answer_json: "{\"schema_version\":1,\"correct_labels\":[\"C\"]}",
            state: "confirmed",
            supersedes_answer_key_id: None,
            confirmed_by: Some("local_teacher"),
            slots: &[],
        },
    )
    .unwrap();
    assert!(new_answer.id > fixture.old_answer_id);
    let stale = ConfirmQuestionImpactPlanRequest {
        request_key: "impact-stale".into(),
        question_version_public_id: fixture.question_version_public_id.clone(),
        expected_preview_hash: preview.preview_hash,
        action: "review_published".into(),
        planned_by: "local_teacher".into(),
    };
    assert!(confirm_question_impact_plan(&mut fixture.conn, "local_teacher", &stale).is_err());
    let refreshed = preview_question_version_impact(
        &fixture.conn,
        "local_teacher",
        &fixture.question_version_public_id,
    )
    .unwrap();
    let request = ConfirmQuestionImpactPlanRequest {
        request_key: "impact-published".into(),
        question_version_public_id: fixture.question_version_public_id.clone(),
        expected_preview_hash: refreshed.preview_hash,
        action: "review_published".into(),
        planned_by: "local_teacher".into(),
    };
    let plan =
        confirm_question_impact_plan(&mut fixture.conn, "local_teacher", &request).unwrap();
    assert_eq!(plan.task_count, 1);
    let kind: (String, Option<i64>) = fixture
        .conn
        .query_row(
            "SELECT task_kind,publication_id
             FROM exam_question_version_impact_tasks_v2",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(kind.0, "review_published");
    assert!(kind.1.is_some());
    let prepared = prepare_question_impact_review_cases(
        &mut fixture.conn,
        "local_teacher",
        &PrepareQuestionImpactReviewCasesRequest {
            plan_public_id: plan.public_id,
            expected_task_count: 1,
            prepared_by: "local_teacher".into(),
        },
    )
    .unwrap();
    assert_eq!(prepared.cases[0].case_kind, "published_review");
    assert_eq!(
        prepared.cases[0].publication_public_id.as_deref(),
        Some("publication-1")
    );
    assert_eq!(
        prepared.cases[0].source_grade_decision_public_id.as_deref(),
        Some("decision-2")
    );
    assert!(prepared.cases[0]
        .next_step_note
        .contains("新的发布 revision"));
}

fn prepared_case(
    fixture: &mut Fixture,
    action: &str,
    request_key: &str,
) -> QuestionImpactReviewCase {
    let preview = preview_question_version_impact(
        &fixture.conn,
        "local_teacher",
        &fixture.question_version_public_id,
    )
    .unwrap();
    let plan = confirm_question_impact_plan(
        &mut fixture.conn,
        "local_teacher",
        &ConfirmQuestionImpactPlanRequest {
            request_key: request_key.into(),
            question_version_public_id: fixture.question_version_public_id.clone(),
            expected_preview_hash: preview.preview_hash,
            action: action.into(),
            planned_by: "local_teacher".into(),
        },
    )
    .unwrap();
    prepare_question_impact_review_cases(
        &mut fixture.conn,
        "local_teacher",
        &PrepareQuestionImpactReviewCasesRequest {
            plan_public_id: plan.public_id,
            expected_task_count: 1,
            prepared_by: "local_teacher".into(),
        },
    )
    .unwrap()
    .cases
    .remove(0)
}

#[test]
fn unpublished_case_confirms_new_grade_then_requires_explicit_publication() {
    let mut fixture = fixture();
    let review_case = prepared_case(
        &mut fixture,
        "recalculate_unpublished",
        "impact-unpublished-resolution-plan",
    );
    let before: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_grade_publications_v2),
               (SELECT COUNT(*) FROM learning_evidence WHERE state='active'),
               (SELECT COUNT(*) FROM exam_question_version_review_resolutions_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let request = ResolveQuestionImpactReviewCaseRequest {
        request_key: "impact-unpublished-resolution".into(),
        case_public_id: review_case.public_id.clone(),
        expected_source_snapshot_hash: review_case.source_snapshot_hash.clone(),
        teacher_score: Some(0.0),
        components: vec![],
        teacher_note: "按修订后的标准答案确认不得分".into(),
        resolved_by: "local_teacher".into(),
    };
    let resolved =
        resolve_question_impact_review_case(&mut fixture.conn, "local_teacher", &request)
            .unwrap();
    assert_eq!(resolved.grade_decision.revision, 2);
    assert_eq!(resolved.grade_decision.teacher_score, 0.0);
    assert!(resolved.old_publication_unchanged);
    assert!(resolved.learning_evidence_unchanged);
    assert!(resolved.requires_explicit_publication);
    let after_resolution: (i64, i64, i64, Option<i64>, String) = fixture
        .conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_grade_publications_v2),
               (SELECT COUNT(*) FROM learning_evidence WHERE state='active'),
               (SELECT COUNT(*) FROM exam_question_version_review_resolutions_v2),
               attempt.active_publication_id,attempt.state
             FROM exam_attempts_v2 attempt WHERE attempt.public_id='attempt-1'",
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
        (before.0, before.1, 1),
        (after_resolution.0, after_resolution.1, after_resolution.2)
    );
    assert_eq!(after_resolution.3, None);
    assert_eq!(after_resolution.4, "ready_to_publish");
    let repeated =
        resolve_question_impact_review_case(&mut fixture.conn, "local_teacher", &request)
            .unwrap();
    assert_eq!(
        repeated.grade_decision.public_id,
        resolved.grade_decision.public_id
    );
    let catalog = list_question_impact_review_cases(
        &fixture.conn,
        "local_teacher",
        &review_case.plan_public_id,
    )
    .unwrap();
    assert_eq!(catalog.cases[0].state, "grade_confirmed");
    assert!(!catalog.cases[0].changes_publication);
    let published = publish_question_impact_review_case(
        &fixture.conn,
        "local_teacher",
        &PublishQuestionImpactReviewCaseRequest {
            case_public_id: review_case.public_id.clone(),
            expected_grade_decision_public_id: resolved.grade_decision.public_id.clone(),
            published_by: "local_teacher".into(),
        },
    )
    .unwrap();
    assert_eq!(published.publication.total_score, 0.0);
    assert!(!published.prior_publication_superseded);
    let final_catalog = list_question_impact_review_cases(
        &fixture.conn,
        "local_teacher",
        &review_case.plan_public_id,
    )
    .unwrap();
    assert_eq!(final_catalog.cases[0].state, "republished");
    assert!(final_catalog.cases[0].changes_publication);
}

#[test]
fn published_case_keeps_old_evidence_until_republication_switches_it() {
    let mut fixture = fixture();
    let review_case = prepared_case(
        &mut fixture,
        "review_published",
        "impact-published-resolution-plan",
    );
    let request = ResolveQuestionImpactReviewCaseRequest {
        request_key: "impact-published-resolution".into(),
        case_public_id: review_case.public_id.clone(),
        expected_source_snapshot_hash: review_case.source_snapshot_hash.clone(),
        teacher_score: Some(0.0),
        components: vec![],
        teacher_note: "复核正式成绩后按新答案改为不得分".into(),
        resolved_by: "local_teacher".into(),
    };
    let resolved =
        resolve_question_impact_review_case(&mut fixture.conn, "local_teacher", &request)
            .unwrap();
    let after_resolution: (String, String, i64, String) = fixture
        .conn
        .query_row(
            "SELECT publication.public_id,publication.state,
                    (SELECT COUNT(*) FROM learning_evidence
                     WHERE decision_ref_id='decision-2' AND state='active'),
                    attempt.state
             FROM exam_attempts_v2 attempt
             JOIN exam_grade_publications_v2 publication
               ON publication.id=attempt.active_publication_id
             WHERE attempt.public_id='attempt-2'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(after_resolution.0, "publication-1");
    assert_eq!(after_resolution.1, "published");
    assert_eq!(after_resolution.2, 1);
    assert_eq!(after_resolution.3, "ready_to_publish");
    let published = publish_question_impact_review_case(
        &fixture.conn,
        "local_teacher",
        &PublishQuestionImpactReviewCaseRequest {
            case_public_id: review_case.public_id.clone(),
            expected_grade_decision_public_id: resolved.grade_decision.public_id.clone(),
            published_by: "local_teacher".into(),
        },
    )
    .unwrap();
    assert!(published.prior_publication_superseded);
    assert!(published.learning_evidence_switched);
    let after_publication: (String, String, i64, String) = fixture
        .conn
        .query_row(
            "SELECT old_publication.state,new_publication.state,
                    (SELECT COUNT(*) FROM learning_evidence
                     WHERE decision_ref_id='decision-2' AND state='reverted'),
                    attempt.state
             FROM exam_attempts_v2 attempt
             JOIN exam_grade_publications_v2 new_publication
               ON new_publication.id=attempt.active_publication_id
             JOIN exam_grade_publications_v2 old_publication
               ON old_publication.public_id='publication-1'
             WHERE attempt.public_id='attempt-2'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(after_publication.0, "superseded");
    assert_eq!(after_publication.1, "published");
    assert_eq!(after_publication.2, 1);
    assert_eq!(after_publication.3, "published");
    let repeated = publish_question_impact_review_case(
        &fixture.conn,
        "local_teacher",
        &PublishQuestionImpactReviewCaseRequest {
            case_public_id: review_case.public_id,
            expected_grade_decision_public_id: resolved.grade_decision.public_id,
            published_by: "local_teacher".into(),
        },
    )
    .unwrap();
    assert_eq!(
        repeated.publication.public_id,
        published.publication.public_id
    );
}

#[test]
fn subjective_case_requires_complete_components_and_student_evidence_for_credit() {
    let mut fixture = fixture_with_question_type("short_answer");
    let review_case = prepared_case(
        &mut fixture,
        "recalculate_unpublished",
        "impact-subjective-resolution-plan",
    );
    assert_eq!(review_case.target_components.len(), 1);
    assert_eq!(review_case.target_components[0].source_type, "rubric_point");
    let component = review_case.target_components[0].clone();
    let unsupported_credit = resolve_question_impact_review_case(
        &mut fixture.conn,
        "local_teacher",
        &ResolveQuestionImpactReviewCaseRequest {
            request_key: "impact-subjective-without-evidence".into(),
            case_public_id: review_case.public_id.clone(),
            expected_source_snapshot_hash: review_case.source_snapshot_hash.clone(),
            teacher_score: None,
            components: vec![ResolveQuestionImpactComponentInput {
                source_public_id: component.source_public_id.clone(),
                teacher_score: component.max_score,
                evidence_text: None,
                teacher_note: None,
            }],
            teacher_note: "尝试无证据给分".into(),
            resolved_by: "local_teacher".into(),
        },
    );
    assert!(unsupported_credit.is_err());
    let resolution_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_question_version_review_resolutions_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(resolution_count, 0);
    let resolved = resolve_question_impact_review_case(
        &mut fixture.conn,
        "local_teacher",
        &ResolveQuestionImpactReviewCaseRequest {
            request_key: "impact-subjective-zero".into(),
            case_public_id: review_case.public_id,
            expected_source_snapshot_hash: review_case.source_snapshot_hash,
            teacher_score: None,
            components: vec![ResolveQuestionImpactComponentInput {
                source_public_id: component.source_public_id,
                teacher_score: 0.0,
                evidence_text: None,
                teacher_note: Some("未找到该评分点".into()),
            }],
            teacher_note: "逐项核对后确认不得分".into(),
            resolved_by: "local_teacher".into(),
        },
    )
    .unwrap();
    assert_eq!(resolved.grade_decision.teacher_score, 0.0);
    let point_results: Value =
        serde_json::from_str(&resolved.grade_decision.point_results_json).unwrap();
    assert_eq!(
        point_results["component_results"]["components"][0]["result_status"],
        "incorrect"
    );
}

#[test]
fn review_case_preparation_rejects_attempt_scope_drift() {
    let mut fixture = fixture();
    let preview = preview_question_version_impact(
        &fixture.conn,
        "local_teacher",
        &fixture.question_version_public_id,
    )
    .unwrap();
    let plan = confirm_question_impact_plan(
        &mut fixture.conn,
        "local_teacher",
        &ConfirmQuestionImpactPlanRequest {
            request_key: "impact-drift-before-case".into(),
            question_version_public_id: fixture.question_version_public_id.clone(),
            expected_preview_hash: preview.preview_hash,
            action: "recalculate_unpublished".into(),
            planned_by: "local_teacher".into(),
        },
    )
    .unwrap();
    fixture
        .conn
        .execute(
            "UPDATE exam_attempts_v2 SET state='voided' WHERE public_id='attempt-1'",
            [],
        )
        .unwrap();
    let result = prepare_question_impact_review_cases(
        &mut fixture.conn,
        "local_teacher",
        &PrepareQuestionImpactReviewCasesRequest {
            plan_public_id: plan.public_id,
            expected_task_count: 1,
            prepared_by: "local_teacher".into(),
        },
    );
    assert!(result.is_err());
    let case_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_question_version_review_cases_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(case_count, 0);
}

#[test]
fn future_default_upgrade_clones_version_without_rebinding_history() {
    let mut fixture = fixture();
    let preview = preview_question_version_impact(
        &fixture.conn,
        "local_teacher",
        &fixture.question_version_public_id,
    )
    .unwrap();
    assert!(preview.rows[0].is_current_default);
    assert_eq!(preview.rows[0].assessment_version_revision, 1);
    let plan = confirm_question_impact_plan(
        &mut fixture.conn,
        "local_teacher",
        &ConfirmQuestionImpactPlanRequest {
            request_key: "future-default-plan".into(),
            question_version_public_id: fixture.question_version_public_id.clone(),
            expected_preview_hash: preview.preview_hash,
            action: "future_only".into(),
            planned_by: "local_teacher".into(),
        },
    )
    .unwrap();
    let before: (i64, i64, i64, i64, i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_attempts_v2),
               (SELECT COUNT(*) FROM exam_grade_decisions_v2),
               (SELECT COUNT(*) FROM exam_grade_publications_v2),
               (SELECT COUNT(*) FROM learning_evidence),
               (SELECT COUNT(*) FROM exam_assessment_versions_v2),
               (SELECT COUNT(*) FROM exam_assessment_items_v2),
               (SELECT COUNT(*) FROM exam_assessment_default_version_selections_v2)",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .unwrap();
    let request = UpgradeAssessmentDefaultRequest {
        request_key: "future-default-upgrade".into(),
        plan_public_id: plan.public_id,
        source_assessment_version_public_id: "assessment-version-1".into(),
        expected_current_default_version_public_id: "assessment-version-1".into(),
        upgraded_by: "local_teacher".into(),
    };
    let upgraded =
        upgrade_assessment_default_from_impact(&mut fixture.conn, "local_teacher", &request)
            .unwrap();
    assert_eq!(upgraded.source_revision, 1);
    assert_eq!(upgraded.default_revision, 2);
    assert_eq!(upgraded.upgraded_item_count, 1);
    assert!(upgraded.default_for_future_intake);
    assert!(!upgraded.changes_historical_attempts);
    assert!(!upgraded.changes_grade);
    assert!(!upgraded.changes_publication);
    assert!(!upgraded.changes_learning_evidence);
    let after: (i64, i64, i64, i64, i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_attempts_v2),
               (SELECT COUNT(*) FROM exam_grade_decisions_v2),
               (SELECT COUNT(*) FROM exam_grade_publications_v2),
               (SELECT COUNT(*) FROM learning_evidence),
               (SELECT COUNT(*) FROM exam_assessment_versions_v2),
               (SELECT COUNT(*) FROM exam_assessment_items_v2),
               (SELECT COUNT(*) FROM exam_assessment_default_version_selections_v2)",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(before.0, after.0);
    assert_eq!(before.1, after.1);
    assert_eq!(before.2, after.2);
    assert_eq!(before.3, after.3);
    assert_eq!(after.4, before.4 + 1);
    assert_eq!(after.5, before.5 + 1);
    assert_eq!(after.6, before.6 + 1);
    let history_versions: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_attempts_v2
             WHERE assessment_version_id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(history_versions, 2);
    let default_binding: (String, i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT version.public_id,item.answer_key_version_id,
                    item.rubric_version_id,item.link_set_id
             FROM exam_assessment_current_defaults_v2 current
             JOIN exam_assessment_versions_v2 version
               ON version.id=current.assessment_version_id
             JOIN exam_assessment_items_v2 item
               ON item.assessment_version_id=version.id
             WHERE current.assessment_id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        default_binding.0,
        upgraded.default_assessment_version_public_id
    );
    let targets: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT target_answer_key_version_id,target_rubric_version_id,target_link_set_id
             FROM exam_question_version_impact_plans_v2
             WHERE public_id=?1",
            [&request.plan_public_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (default_binding.1, default_binding.2, default_binding.3),
        targets
    );
    let repeated =
        upgrade_assessment_default_from_impact(&mut fixture.conn, "local_teacher", &request)
            .unwrap();
    assert_eq!(repeated.selection_public_id, upgraded.selection_public_id);
    assert_eq!(repeated.default_revision, 2);
    assert!(fixture
        .conn
        .execute(
            "UPDATE exam_assessment_default_version_selections_v2
             SET selected_by='other' WHERE public_id=?1",
            [&upgraded.selection_public_id],
        )
        .is_err());
    assert!(fixture
        .conn
        .execute(
            "DELETE FROM exam_assessment_default_version_selections_v2
             WHERE public_id=?1",
            [&upgraded.selection_public_id],
        )
        .is_err());
}

#[test]
fn future_default_upgrade_failure_rolls_back_version_items_selection_outbox_and_audit() {
    fn counts(conn: &Connection) -> [i64; 9] {
        conn.query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_attempts_v2),
               (SELECT COUNT(*) FROM exam_grade_decisions_v2),
               (SELECT COUNT(*) FROM exam_grade_publications_v2),
               (SELECT COUNT(*) FROM learning_evidence),
               (SELECT COUNT(*) FROM exam_assessment_versions_v2),
               (SELECT COUNT(*) FROM exam_assessment_items_v2),
               (SELECT COUNT(*) FROM exam_assessment_default_version_selections_v2),
               (SELECT COUNT(*) FROM outbox_events),
               (SELECT COUNT(*) FROM audit_events)",
            [],
            |row| {
                Ok([
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ])
            },
        )
        .unwrap()
    }

    fn current_default_public_id(conn: &Connection) -> String {
        conn.query_row(
            "SELECT version.public_id
             FROM exam_assessment_versions_v2 version
             WHERE version.id=COALESCE(
               (SELECT selection.selected_assessment_version_id
                FROM exam_assessment_default_version_selections_v2 selection
                WHERE selection.assessment_id=1
                ORDER BY selection.revision DESC,selection.id DESC
                LIMIT 1),
               (SELECT latest.id
                FROM exam_assessment_versions_v2 latest
                WHERE latest.assessment_id=1 AND latest.state='confirmed'
                ORDER BY latest.revision DESC,latest.id DESC
                LIMIT 1)
             )",
            [],
            |row| row.get(0),
        )
        .unwrap()
    }

    let mut fixture = fixture();
    let preview = preview_question_version_impact(
        &fixture.conn,
        "local_teacher",
        &fixture.question_version_public_id,
    )
    .unwrap();
    let plan = confirm_question_impact_plan(
        &mut fixture.conn,
        "local_teacher",
        &ConfirmQuestionImpactPlanRequest {
            request_key: "future-default-rollback-plan".into(),
            question_version_public_id: fixture.question_version_public_id.clone(),
            expected_preview_hash: preview.preview_hash,
            action: "future_only".into(),
            planned_by: "local_teacher".into(),
        },
    )
    .unwrap();
    let before_counts = counts(&fixture.conn);
    let before_updated_at: String = fixture
        .conn
        .query_row(
            "SELECT updated_at FROM exam_assessments_v2 WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let before_default = current_default_public_id(&fixture.conn);

    fixture
        .conn
        .execute_batch(
            "CREATE TRIGGER inject_future_default_selection_failure
             BEFORE INSERT ON exam_assessment_default_version_selections_v2
             BEGIN
               SELECT RAISE(ABORT, 'injected future default selection failure');
             END;",
        )
        .unwrap();
    let failed = upgrade_assessment_default_from_impact(
        &mut fixture.conn,
        "local_teacher",
        &UpgradeAssessmentDefaultRequest {
            request_key: "future-default-rollback-failure".into(),
            plan_public_id: plan.public_id.clone(),
            source_assessment_version_public_id: "assessment-version-1".into(),
            expected_current_default_version_public_id: "assessment-version-1".into(),
            upgraded_by: "local_teacher".into(),
        },
    );
    assert!(failed.is_err());
    fixture
        .conn
        .execute_batch("DROP TRIGGER inject_future_default_selection_failure;")
        .unwrap();

    assert_eq!(counts(&fixture.conn), before_counts);
    assert_eq!(current_default_public_id(&fixture.conn), before_default);
    let after_failure_updated_at: String = fixture
        .conn
        .query_row(
            "SELECT updated_at FROM exam_assessments_v2 WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(after_failure_updated_at, before_updated_at);

    let retried = upgrade_assessment_default_from_impact(
        &mut fixture.conn,
        "local_teacher",
        &UpgradeAssessmentDefaultRequest {
            request_key: "future-default-rollback-retry".into(),
            plan_public_id: plan.public_id,
            source_assessment_version_public_id: "assessment-version-1".into(),
            expected_current_default_version_public_id: "assessment-version-1".into(),
            upgraded_by: "local_teacher".into(),
        },
    )
    .unwrap();
    assert_eq!(retried.default_revision, 2);
    assert_eq!(retried.upgraded_item_count, 1);
    let after_retry_counts = counts(&fixture.conn);
    assert_eq!(after_retry_counts[0], before_counts[0]);
    assert_eq!(after_retry_counts[1], before_counts[1]);
    assert_eq!(after_retry_counts[2], before_counts[2]);
    assert_eq!(after_retry_counts[3], before_counts[3]);
    assert_eq!(after_retry_counts[4], before_counts[4] + 1);
    assert_eq!(after_retry_counts[5], before_counts[5] + 1);
    assert_eq!(after_retry_counts[6], before_counts[6] + 1);
    assert_eq!(after_retry_counts[7], before_counts[7] + 1);
    assert_eq!(after_retry_counts[8], before_counts[8] + 1);
}

#[test]
fn future_default_upgrade_rejects_stale_or_unreviewed_source() {
    let mut fixture = fixture();
    let preview = preview_question_version_impact(
        &fixture.conn,
        "local_teacher",
        &fixture.question_version_public_id,
    )
    .unwrap();
    let plan = confirm_question_impact_plan(
        &mut fixture.conn,
        "local_teacher",
        &ConfirmQuestionImpactPlanRequest {
            request_key: "future-default-stale-plan".into(),
            question_version_public_id: fixture.question_version_public_id.clone(),
            expected_preview_hash: preview.preview_hash,
            action: "future_only".into(),
            planned_by: "local_teacher".into(),
        },
    )
    .unwrap();
    let first = UpgradeAssessmentDefaultRequest {
        request_key: "future-default-first".into(),
        plan_public_id: plan.public_id.clone(),
        source_assessment_version_public_id: "assessment-version-1".into(),
        expected_current_default_version_public_id: "assessment-version-1".into(),
        upgraded_by: "local_teacher".into(),
    };
    upgrade_assessment_default_from_impact(&mut fixture.conn, "local_teacher", &first).unwrap();
    let stale = UpgradeAssessmentDefaultRequest {
        request_key: "future-default-stale".into(),
        plan_public_id: plan.public_id,
        source_assessment_version_public_id: "assessment-version-1".into(),
        expected_current_default_version_public_id: "assessment-version-1".into(),
        upgraded_by: "local_teacher".into(),
    };
    assert!(
        upgrade_assessment_default_from_impact(&mut fixture.conn, "local_teacher", &stale)
            .is_err()
    );
    let counts: (i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM exam_assessment_versions_v2),
               (SELECT COUNT(*) FROM exam_assessment_default_version_selections_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (2, 1));
}
