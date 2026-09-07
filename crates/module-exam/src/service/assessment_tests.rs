use super::*;
use module_knowledge::db::content::{
    add_ability_link, add_knowledge_link, create_answer_key_version, create_link_set,
    create_question, create_question_version, create_rubric_version, promote_question_version,
    NewAbilityLink, NewAnswerKeyVersion, NewAnswerSlot, NewKnowledgeLink, NewQuestion,
    NewQuestionVersion, NewRubricPoint, NewRubricVersion,
};
use module_knowledge::db::taxonomy::{
    create_ability_dimension, create_curriculum_node, create_knowledge_map, create_knowledge_node,
    create_textbook_edition, NewAbilityDimension, NewCurriculumNode, NewKnowledgeMap,
    NewKnowledgeNode, NewTextbookEdition,
};
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

struct Fixture {
    conn: Connection,
    version_id: i64,
    item_id: i64,
    attempt_id: i64,
}

fn setup() -> Fixture {
    let conn = open_in_memory().unwrap();
    run_migrations(&conn, CORE_MIGRATIONS).unwrap();
    run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
    run_migrations(&conn, crate::exam_migrations()).unwrap();
    conn.execute_batch(
        "INSERT INTO subjects(name) VALUES ('历史');
             INSERT INTO classes(name, term) VALUES ('八年级一班', '2026秋');
             INSERT INTO students(student_no, name, class_id) VALUES ('S001','小林',1);",
    )
    .unwrap();

    let edition = create_textbook_edition(
        &conn,
        &NewTextbookEdition {
            subject_id: 1,
            publisher_code: "PEP",
            edition_code: "2024",
            title: "中国历史八年级上册",
            grade: "8",
            volume: "upper",
            curriculum_region: None,
        },
    )
    .unwrap();
    let map = create_knowledge_map(
        &conn,
        &NewKnowledgeMap {
            textbook_edition_id: edition.id,
            revision: 1,
            state: "confirmed",
            supersedes_map_id: None,
        },
    )
    .unwrap();
    let lesson = create_curriculum_node(
        &conn,
        &NewCurriculumNode {
            stable_id: None,
            knowledge_map_id: map.id,
            parent_id: None,
            node_type: "lesson",
            code: Some("L1"),
            title: "鸦片战争",
            description: None,
            order_index: 1,
        },
    )
    .unwrap();
    let knowledge = create_knowledge_node(
        &conn,
        &NewKnowledgeNode {
            stable_id: None,
            knowledge_map_id: map.id,
            curriculum_node_id: Some(lesson.id),
            parent_id: None,
            code: Some("K1"),
            title: "鸦片战争爆发时间",
            description: None,
            order_index: 1,
        },
    )
    .unwrap();
    let ability = create_ability_dimension(
        &conn,
        &NewAbilityDimension {
            stable_id: None,
            subject_id: 1,
            revision: 1,
            code: "fact_recall",
            title: "事实识记与提取",
            description: None,
            supersedes_dimension_id: None,
        },
    )
    .unwrap();
    let question = create_question(
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
    let question_version = create_question_version(
        &conn,
        &NewQuestionVersion {
            question_id: question.id,
            revision: 1,
            question_type: "true_false",
            stem: "鸦片战争爆发于 1840 年。",
            material_text: None,
            max_score: 1.0,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "L0",
            state: "draft",
            options: &[],
        },
    )
    .unwrap();
    let answer = create_answer_key_version(
        &conn,
        &NewAnswerKeyVersion {
            question_version_id: question_version.id,
            revision: 1,
            answer_json: r#"{"schema_version":1,"correct":true}"#,
            state: "confirmed",
            confirmed_by: Some("teacher"),
            supersedes_answer_key_id: None,
            slots: &[],
        },
    )
    .unwrap();
    let rubric = create_rubric_version(
        &conn,
        &NewRubricVersion {
            question_version_id: question_version.id,
            revision: 1,
            max_score: 1.0,
            state: "confirmed",
            confirmed_by: Some("teacher"),
            supersedes_rubric_id: None,
            points: &[NewRubricPoint {
                stable_id: None,
                order_index: 0,
                canonical_text: "判断为正确",
                allowed_paraphrases_json: None,
                required_concepts_json: None,
                max_score: 1.0,
            }],
        },
    )
    .unwrap();
    let link_set = create_link_set(
        &conn,
        question_version.id,
        map.id,
        1,
        "confirmed",
        Some("teacher"),
        None,
    )
    .unwrap();
    add_knowledge_link(
        &conn,
        &NewKnowledgeLink {
            link_set_id: link_set.id,
            source_type: "question",
            source_public_id: &question_version.public_id,
            knowledge_node_id: knowledge.id,
            relation_type: "direct_assessment",
            confirmation_level: "teacher_confirmed",
            verified_by: Some("teacher"),
        },
    )
    .unwrap();
    add_ability_link(
        &conn,
        &NewAbilityLink {
            link_set_id: link_set.id,
            source_type: "question",
            source_public_id: &question_version.public_id,
            ability_dimension_id: ability.id,
            evidence_strength: 0.4,
            response_mode: "recognition",
            confirmation_level: "teacher_confirmed",
            verified_by: Some("teacher"),
        },
    )
    .unwrap();
    promote_question_version(&conn, question_version.id, "L3", "teacher", None).unwrap();

    let assessment = create_assessment_draft(
        &conn,
        &NewAssessmentDraft {
            title: "第一课随堂测",
            class_id: 1,
            assessment_context: "quiz",
            evidence_policy: "include",
            created_by: "teacher",
            template_version: None,
        },
    )
    .unwrap();
    let item = add_assessment_item(
        &conn,
        assessment.assessment_version_id,
        &NewAssessmentItem {
            question_version_id: question_version.id,
            answer_key_version_id: answer.id,
            rubric_version_id: rubric.id,
            link_set_id: link_set.id,
            order_index: 0,
            score: 1.0,
            option_order_json: None,
            presentation_snapshot_json: r#"{"schema_version":1,"question_no":"1"}"#,
        },
    )
    .unwrap();
    confirm_assessment_version(&conn, assessment.assessment_version_id, "teacher").unwrap();
    let attempt = create_attempt(
        &conn,
        assessment.assessment_version_id,
        1,
        "manual",
        "first",
    )
    .unwrap();
    conn.execute(
        "INSERT INTO exam_questions(qtype, stem, correct_answer, max_score)
             VALUES ('judge','旧题','true',1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO exam_student_answers
             (student_id, question_id, picked, max_score, machine_correct, human_correct,
              is_correct, score, status, updated_at)
             VALUES (1,1,'true',1,1,1,1,1,'confirmed',datetime('now'))",
        [],
    )
    .unwrap();
    Fixture {
        conn,
        version_id: assessment.assessment_version_id,
        item_id: item.id,
        attempt_id: attempt.id,
    }
}

#[test]
fn correction_context_forces_correction_attempt_kind() {
    let fixture = setup();
    assert_eq!(
        attempt_kind_for_assessment_version(&fixture.conn, fixture.version_id, 1).unwrap(),
        "first"
    );
    assert_eq!(
        attempt_kind_for_assessment_version(&fixture.conn, fixture.version_id, 2).unwrap(),
        "retry"
    );
    let refs: (i64, i64, i64, i64, f64, Option<String>, String) = fixture
        .conn
        .query_row(
            "SELECT question_version_id,answer_key_version_id,rubric_version_id,
                        link_set_id,score,option_order_json,presentation_snapshot_json
                 FROM exam_assessment_items_v2 WHERE id=?1",
            [fixture.item_id],
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
    let transaction = fixture.conn.unchecked_transaction().unwrap();
    let correction = create_confirmed_single_item_correction_in_transaction(
        &transaction,
        &NewSingleItemCorrectionAssessment {
            title: "小林 · 单题订正",
            class_id: 1,
            target_student_id: 1,
            created_by: "teacher",
            question_version_id: refs.0,
            answer_key_version_id: refs.1,
            rubric_version_id: refs.2,
            link_set_id: refs.3,
            score: refs.4,
            option_order_json: refs.5.as_deref(),
            presentation_snapshot_json: &refs.6,
        },
    )
    .unwrap();
    assert_eq!(
        attempt_kind_for_assessment_version(&transaction, correction.assessment_version_id, 1)
            .unwrap(),
        "correction"
    );
    assert_eq!(
        attempt_kind_for_assessment_version(&transaction, correction.assessment_version_id, 2)
            .unwrap(),
        "correction"
    );
    transaction.rollback().unwrap();
}

#[test]
fn multi_item_targeted_assessment_freezes_explicit_scope_without_attempts() {
    let fixture = setup();
    fixture
        .conn
        .execute(
            "INSERT INTO students(student_no,name,class_id,enabled)
                 VALUES ('S002','小周',1,1)",
            [],
        )
        .unwrap();
    let second_student_id = fixture.conn.last_insert_rowid();
    let refs: (i64, i64, i64, i64, f64, Option<String>, String) = fixture
        .conn
        .query_row(
            "SELECT question_version_id,answer_key_version_id,rubric_version_id,
                        link_set_id,score,option_order_json,presentation_snapshot_json
                 FROM exam_assessment_items_v2 WHERE id=?1",
            [fixture.item_id],
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
    let item = NewTargetedAssessmentItem {
        question_version_id: refs.0,
        answer_key_version_id: refs.1,
        rubric_version_id: refs.2,
        link_set_id: refs.3,
        score: refs.4,
        option_order_json: refs.5.as_deref(),
        presentation_snapshot_json: &refs.6,
    };
    let transaction = fixture.conn.unchecked_transaction().unwrap();
    let created = create_confirmed_multi_item_targeted_in_transaction(
        &transaction,
        &NewMultiItemTargetedAssessment {
            title: "鸦片战争定向巩固",
            class_id: 1,
            target_student_ids: &[1, second_student_id],
            assessment_context: "classwork",
            evidence_policy: "include",
            template_version: "m6.1-class-action-practice-v1",
            created_by: "teacher",
            items: &[item],
        },
    )
    .unwrap();
    let scope: (String, String, String, i64, i64, i64) = transaction
        .query_row(
            "SELECT assessment.state,assessment.audience_kind,version.state,
                        (SELECT COUNT(*) FROM exam_assessment_targets_v2
                         WHERE assessment_version_id=version.id),
                        (SELECT COUNT(*) FROM exam_assessment_items_v2
                         WHERE assessment_version_id=version.id AND state='active'),
                        (SELECT COUNT(*) FROM exam_attempts_v2
                         WHERE assessment_version_id=version.id)
                 FROM exam_assessments_v2 assessment
                 JOIN exam_assessment_versions_v2 version
                   ON version.assessment_id=assessment.id
                 WHERE assessment.id=?1",
            [created.assessment_id],
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
        scope,
        (
            "active".into(),
            "explicit".into(),
            "confirmed".into(),
            2,
            1,
            0
        )
    );
    let public_id = created.assessment_public_id;
    transaction.rollback().unwrap();
    let persisted: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_assessments_v2 WHERE public_id=?1",
            [public_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(persisted, 0);
}

fn create_subjective_k1(conn: &Connection, question_type: &str) -> (i64, i64, i64, String) {
    let question = create_question(
        conn,
        &NewQuestion {
            owner_scope: "personal",
            owner_id: "teacher",
            question_family_id: None,
            rights_status: "unknown",
            sharing_allowed: false,
        },
    )
    .unwrap();
    let question_version = create_question_version(
        conn,
        &NewQuestionVersion {
            question_id: question.id,
            revision: 1,
            question_type,
            stem: "主观题证据测试",
            material_text: None,
            max_score: 2.0,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "L0",
            state: "draft",
            options: &[],
        },
    )
    .unwrap();
    let slots = if question_type == "fill_blank" {
        vec![NewAnswerSlot {
            stable_id: Some("slot-1"),
            order_index: 0,
            canonical_answers_json: r#"{"schema_version":1,"answers":["1842"]}"#,
            normalization_rules_json: None,
            max_score: 2.0,
        }]
    } else {
        vec![]
    };
    let answer = create_answer_key_version(
        conn,
        &NewAnswerKeyVersion {
            question_version_id: question_version.id,
            revision: 1,
            answer_json: r#"{"schema_version":1,"answer":"测试"}"#,
            state: "confirmed",
            confirmed_by: Some("teacher"),
            supersedes_answer_key_id: None,
            slots: &slots,
        },
    )
    .unwrap();
    let rubric = create_rubric_version(
        conn,
        &NewRubricVersion {
            question_version_id: question_version.id,
            revision: 1,
            max_score: 2.0,
            state: "confirmed",
            confirmed_by: Some("teacher"),
            supersedes_rubric_id: None,
            points: &[NewRubricPoint {
                stable_id: Some("point-1"),
                order_index: 0,
                canonical_text: "评分点一",
                allowed_paraphrases_json: None,
                required_concepts_json: None,
                max_score: 2.0,
            }],
        },
    )
    .unwrap();
    let point: (i64, String) = conn
        .query_row(
            "SELECT id,public_id FROM k1_rubric_points WHERE rubric_version_id=?1",
            [rubric.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    (answer.id, rubric.id, point.0, point.1)
}

fn subjective_evidence_decision(
    question_type: &str,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    confirmation_level: &str,
    result_json: Option<String>,
) -> EvidenceDecision {
    EvidenceDecision {
        id: 1,
        public_id: "decision-test".into(),
        revision: 1,
        teacher_score: 1.0,
        confirmation_level: confirmation_level.into(),
        decided_at: "2026-07-16T00:00:00Z".into(),
        item_public_id: "item-test".into(),
        item_score: 2.0,
        link_set_id: 1,
        dictation_rubric_point_public_id: None,
        question_type: question_type.into(),
        answer_key_version_id,
        rubric_version_id,
        subjective_source: true,
        short_answer_result_json: result_json,
    }
}

#[test]
fn subjective_evidence_uses_only_safe_slot_or_point_granularity() {
    let fixture = setup();
    let (fill_answer, fill_rubric, _, _) = create_subjective_k1(&fixture.conn, "fill_blank");
    let fill = subjective_evidence_decision(
        "fill_blank",
        fill_answer,
        fill_rubric,
        "teacher_corrected",
        None,
    );
    let fill_sources = evidence_sources(&fixture.conn, &fill).unwrap();
    assert_eq!(fill_sources.len(), 1);
    assert_eq!(fill_sources[0].source_ref_type, "answer_slot");
    assert!((fill_sources[0].value - 0.5).abs() < 0.000_001);

    let (short_answer, short_rubric, point_id, point_public_id) =
        create_subjective_k1(&fixture.conn, "short_answer");
    let corrected = subjective_evidence_decision(
        "short_answer",
        short_answer,
        short_rubric,
        "teacher_corrected",
        Some(
            serde_json::json!({
                "schema_version": 1,
                "point_results": [{"rubric_point_id": point_id, "suggested_score": 1.5}]
            })
            .to_string(),
        ),
    );
    assert!(evidence_sources(&fixture.conn, &corrected)
        .unwrap()
        .is_empty());

    let accepted = subjective_evidence_decision(
        "short_answer",
        short_answer,
        short_rubric,
        "teacher_accepted",
        Some(
            serde_json::json!({
                "schema_version": 1,
                "point_results": [{"rubric_point_id": point_id, "suggested_score": 1.5}]
            })
            .to_string(),
        ),
    );
    let short_sources = evidence_sources(&fixture.conn, &accepted).unwrap();
    assert_eq!(short_sources.len(), 1);
    assert_eq!(short_sources[0].source_ref_id, point_public_id);
    assert!((short_sources[0].value - 0.75).abs() < 0.000_001);
}

fn decision<'a>(fixture: &'a Fixture, score: f64, note: Option<&'a str>) -> NewGradeDecision<'a> {
    NewGradeDecision {
        attempt_id: fixture.attempt_id,
        assessment_item_id: fixture.item_id,
        machine_grade_ai_run_id: None,
        teacher_score: score,
        point_results_json: r#"{"schema_version":1,"result":"confirmed"}"#,
        teacher_note: note,
        confirmation_level: "teacher_corrected",
        decided_by: "teacher",
    }
}

#[test]
fn decision_is_idempotent_and_publication_freezes_the_decision_set() {
    let fixture = setup();
    let first = decide_grade(&fixture.conn, &decision(&fixture, 1.0, None)).unwrap();
    let repeated = decide_grade(&fixture.conn, &decision(&fixture, 1.0, None)).unwrap();
    assert_eq!(first.id, repeated.id);
    assert_eq!(
        get_attempt(&fixture.conn, fixture.attempt_id)
            .unwrap()
            .unwrap()
            .state,
        "ready_to_publish"
    );
    let publications: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_grade_publications_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(publications, 0, "单题终审不能自动发布");

    let published = publish_attempt(&fixture.conn, fixture.attempt_id, "teacher").unwrap();
    assert_eq!(published.revision, 1);
    assert_eq!(published.total_score, 1.0);
    let first_evidence: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='active'),
                   (SELECT COUNT(*) FROM outbox_events
                    WHERE event_type='learning_evidence_changed'),
                   (SELECT COUNT(*) FROM audit_events
                    WHERE action='exam.learning_evidence.activated')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(first_evidence, (2, 2, 2));
    let changed = decide_grade(&fixture.conn, &decision(&fixture, 0.0, Some("改判"))).unwrap();
    assert_eq!(changed.revision, 2);
    assert_eq!(
        get_attempt(&fixture.conn, fixture.attempt_id)
            .unwrap()
            .unwrap()
            .state,
        "ready_to_publish"
    );
    let old_state: String = fixture
        .conn
        .query_row(
            "SELECT state FROM exam_grade_publications_v2 WHERE id=?1",
            [published.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(old_state, "published", "改分但未重发时旧发布仍然有效");
    let evidence_after_change: (i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='active'),
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='reverted')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        evidence_after_change,
        (2, 0),
        "改分但未重发时，旧正式发布对应的学习证据必须继续有效"
    );

    let republished = publish_attempt(&fixture.conn, fixture.attempt_id, "teacher").unwrap();
    assert_eq!(republished.revision, 2);
    assert_eq!(republished.total_score, 0.0);
    let old_state: String = fixture
        .conn
        .query_row(
            "SELECT state FROM exam_grade_publications_v2 WHERE id=?1",
            [published.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(old_state, "superseded");
    let frozen_old_decision: i64 = fixture
        .conn
        .query_row(
            "SELECT grade_decision_id FROM exam_grade_publication_decisions_v2 pd
                 JOIN exam_grade_publication_items_v2 pi ON pi.id=pd.publication_item_id
                 WHERE pi.publication_id=?1",
            [published.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(frozen_old_decision, first.id);
    let evidence_after_republish: (i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='active'),
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='reverted')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(evidence_after_republish, (2, 2));
    assert!(fixture
        .conn
        .execute(
            "UPDATE exam_assessment_items_v2 SET score=2 WHERE id=?1",
            [fixture.item_id]
        )
        .is_err());
}

#[test]
fn decision_revision_rolls_back_if_insert_fails() {
    let fixture = setup();
    let first = decide_grade(&fixture.conn, &decision(&fixture, 1.0, None)).unwrap();
    publish_attempt(&fixture.conn, fixture.attempt_id, "teacher").unwrap();
    fixture
        .conn
        .execute_batch(
            "CREATE TRIGGER fail_second_decision
                 BEFORE INSERT ON exam_grade_decisions_v2 WHEN NEW.revision=2
                 BEGIN SELECT RAISE(ABORT, 'injected decision failure'); END;",
        )
        .unwrap();
    assert!(decide_grade(&fixture.conn, &decision(&fixture, 0.0, None)).is_err());
    let active: (i64, String) = fixture
        .conn
        .query_row(
            "SELECT id, state FROM exam_grade_decisions_v2
                 WHERE attempt_id=?1 AND assessment_item_id=?2 AND state='active'",
            (fixture.attempt_id, fixture.item_id),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(active, (first.id, "active".into()));
    let evidence_states: (i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='active'),
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='reverted')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(evidence_states, (2, 0));
}

#[test]
fn publication_and_formal_evidence_roll_back_together() {
    let fixture = setup();
    decide_grade(&fixture.conn, &decision(&fixture, 1.0, None)).unwrap();
    fixture
        .conn
        .execute_batch(
            "CREATE TRIGGER fail_publication_evidence
                 BEFORE INSERT ON learning_evidence
                 BEGIN SELECT RAISE(ABORT, 'injected evidence failure'); END;",
        )
        .unwrap();
    assert!(publish_attempt(&fixture.conn, fixture.attempt_id, "teacher").is_err());
    let counts: (i64, i64, i64, String, Option<i64>) = fixture
        .conn
        .query_row(
            "SELECT
                   (SELECT COUNT(*) FROM exam_grade_publications_v2),
                   (SELECT COUNT(*) FROM learning_evidence),
                   (SELECT COUNT(*) FROM outbox_events
                    WHERE event_type='learning_evidence_changed'),
                   state,active_publication_id
                 FROM exam_attempts_v2 WHERE id=?1",
            [fixture.attempt_id],
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
    assert_eq!(counts, (0, 0, 0, "ready_to_publish".into(), None));
}

#[test]
fn migration_keeps_old_answers_explicitly_unmapped() {
    let fixture = setup();
    let legacy = list_legacy_answer_compat(&fixture.conn, 10).unwrap();
    assert_eq!(legacy.len(), 1);
    assert_eq!(legacy[0].source_kind, "legacy_manual");
    assert_eq!(legacy[0].mapping_state, "legacy_unmapped");
    let mapping_count: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_legacy_answer_mappings_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(mapping_count, 0);
    assert!(fixture.version_id > 0);
}
