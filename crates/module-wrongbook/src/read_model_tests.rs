use super::*;

const NOW: &str = "2026-07-16T08:00:00Z";

struct Fixture {
    conn: Connection,
    class_one: i64,
    class_two: i64,
    students: Vec<i64>,
    assessment_version_id: i64,
    assessment_item_id: i64,
    next_publication_revision: i64,
}

impl Fixture {
    fn new() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        suite_core::db::run_migrations(&conn, suite_core::db::CORE_MIGRATIONS).unwrap();
        suite_core::db::run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        suite_core::db::run_migrations(&conn, module_exam::exam_migrations()).unwrap();
        suite_core::db::run_migrations(&conn, crate::wrongbook_migrations()).unwrap();
        conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])
            .unwrap();
        let subject_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO classes(name,term,textbook) VALUES ('八年级一班','2026秋','中国历史八上')",
            [],
        )
        .unwrap();
        let class_one = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO classes(name,term,textbook) VALUES ('八年级二班','2026秋','中国历史八上')",
            [],
        )
        .unwrap();
        let class_two = conn.last_insert_rowid();
        let mut students = Vec::new();
        for (student_no, name, class_id) in [
            ("10", "小十", class_one),
            ("02", "小二", class_one),
            ("03", "小三", class_one),
            ("04", "小四", class_one),
            ("05", "外班", class_two),
        ] {
            conn.execute(
                "INSERT INTO students(student_no,name,class_id,enabled) VALUES (?1,?2,?3,1)",
                (student_no, name, class_id),
            )
            .unwrap();
            students.push(conn.last_insert_rowid());
        }
        conn.execute(
                "INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('edition-1',?1,'PEP','2026','中国历史八上','八年级','upper','active',?2)",
                (subject_id, NOW),
            )
            .unwrap();
        let edition_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('map-1',?1,1,'confirmed',?2,?2)",
            (edition_id, NOW),
        )
        .unwrap();
        let map_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO k1_knowledge_nodes
                 (public_id,stable_id,knowledge_map_id,title,created_at)
                 VALUES ('knowledge-1','knowledge-stable-1',?1,'洋务运动失败原因',?2)",
            (map_id, NOW),
        )
        .unwrap();
        let knowledge_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO k1_ability_dimensions
                 (public_id,stable_id,subject_id,revision,code,title,state,created_at)
                 VALUES ('ability-1','ability-stable-1',?1,1,'CAUSE','因果分析','active',?2)",
            (subject_id, NOW),
        )
        .unwrap();
        let ability_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('question-1','personal','teacher','cleared',0,?1)",
            [NOW],
        )
        .unwrap();
        let question_id = conn.last_insert_rowid();
        let question_version_public_id = "question-version-1".to_string();
        conn.execute(
            "INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES (?1,?2,1,'single','洋务运动失败的根本原因是？',2,
                  'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                  'L3','published',?3)",
            (&question_version_public_id, question_id, NOW),
        )
        .unwrap();
        let question_version_id = conn.last_insert_rowid();
        conn.execute(
                "INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('answer-1',?1,1,'{\"schema_version\":1}','confirmed',?2,'teacher',?2)",
                (question_version_id, NOW),
            )
            .unwrap();
        let answer_key_version_id = conn.last_insert_rowid();
        conn.execute(
                "INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('rubric-1',?1,1,2,'confirmed',?2,'teacher',?2)",
                (question_version_id, NOW),
            )
            .unwrap();
        let rubric_version_id = conn.last_insert_rowid();
        conn.execute(
                "INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('links-1',?1,?2,1,'confirmed',?3,'teacher',?3)",
                (question_version_id, map_id, NOW),
            )
            .unwrap();
        let link_set_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO k1_knowledge_links
                 (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,
                  relation_type,confirmation_level,verified_by,verified_at,created_at)
                 VALUES ('knowledge-link-1',?1,'question',?2,?3,'direct_assessment',
                  'teacher_confirmed','teacher',?4,?4)",
            (link_set_id, &question_version_public_id, knowledge_id, NOW),
        )
        .unwrap();
        conn.execute(
                "INSERT INTO k1_ability_links
                 (public_id,link_set_id,source_type,source_public_id,ability_dimension_id,
                  evidence_strength,response_mode,confirmation_level,verified_by,verified_at,created_at)
                 VALUES ('ability-link-1',?1,'question',?2,?3,0.7,'recognition',
                  'teacher_confirmed','teacher',?4,?4)",
                (
                    link_set_id,
                    &question_version_public_id,
                    ability_id,
                    NOW,
                ),
            )
            .unwrap();
        conn.execute(
                "INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,created_by,created_at,updated_at)
                 VALUES ('assessment-1','近代化单元测验',?1,'quiz','include','active','teacher',?2,?2)",
                (class_one, NOW),
            )
            .unwrap();
        let assessment_id = conn.last_insert_rowid();
        conn.execute(
                "INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('assessment-version-1',?1,1,
                  'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                  'v1','confirmed',?2,'teacher',?2)",
                (assessment_id, NOW),
            )
            .unwrap();
        let assessment_version_id = conn.last_insert_rowid();
        conn.execute(
                "INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,state,created_at)
                 VALUES ('item-1',?1,?2,?3,?4,?5,0,2,'{\"schema_version\":1}','active',?6)",
                (
                    assessment_version_id,
                    question_version_id,
                    answer_key_version_id,
                    rubric_version_id,
                    link_set_id,
                    NOW,
                ),
            )
            .unwrap();
        let assessment_item_id = conn.last_insert_rowid();
        Self {
            conn,
            class_one,
            class_two,
            students,
            assessment_version_id,
            assessment_item_id,
            next_publication_revision: 1,
        }
    }

    fn publish_response(
        &mut self,
        student_id: i64,
        attempt_no: i64,
        attempt_kind: &str,
        score: f64,
        published_at: &str,
    ) -> (String, String) {
        let attempt_public_id = format!("attempt-{student_id}-{attempt_no}");
        self.conn
            .execute(
                "INSERT INTO exam_attempts_v2
                     (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                      attempt_kind,state,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,'image',?5,'published',?6,?6)",
                (
                    &attempt_public_id,
                    self.assessment_version_id,
                    student_id,
                    attempt_no,
                    attempt_kind,
                    published_at,
                ),
            )
            .unwrap();
        let attempt_id = self.conn.last_insert_rowid();
        let decision_public_id = format!("decision-{student_id}-{attempt_no}");
        self.conn
            .execute(
                "INSERT INTO exam_grade_decisions_v2
                     (public_id,attempt_id,assessment_item_id,revision,teacher_score,
                      point_results_json,confirmation_level,state,decided_by,decided_at,created_at)
                     VALUES (?1,?2,?3,1,?4,'{\"schema_version\":1}',
                      'teacher_accepted','active','teacher',?5,?5)",
                (
                    &decision_public_id,
                    attempt_id,
                    self.assessment_item_id,
                    score,
                    published_at,
                ),
            )
            .unwrap();
        let decision_id = self.conn.last_insert_rowid();
        let publication_public_id = format!("publication-{}", self.next_publication_revision);
        self.conn
                .execute(
                    "INSERT INTO exam_grade_publications_v2
                     (public_id,assessment_version_id,revision,state,published_by,published_at,created_at)
                     VALUES (?1,?2,?3,'published','teacher',?4,?4)",
                    (
                        &publication_public_id,
                        self.assessment_version_id,
                        self.next_publication_revision,
                        published_at,
                    ),
                )
                .unwrap();
        self.next_publication_revision += 1;
        let publication_id = self.conn.last_insert_rowid();
        self.conn
            .execute(
                "INSERT INTO exam_grade_publication_items_v2
                     (publication_id,attempt_id,grade_decision_set_hash,total_score,created_at)
                     VALUES (?1,?2,
                      'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
                      ?3,?4)",
                (publication_id, attempt_id, score, published_at),
            )
            .unwrap();
        let publication_item_id = self.conn.last_insert_rowid();
        self.conn
            .execute(
                "INSERT INTO exam_grade_publication_decisions_v2
                     (publication_item_id,attempt_id,grade_decision_id,created_at)
                     VALUES (?1,?2,?3,?4)",
                (publication_item_id, attempt_id, decision_id, published_at),
            )
            .unwrap();
        self.conn
            .execute(
                "UPDATE exam_attempts_v2
                     SET active_publication_id=?1 WHERE id=?2",
                (publication_id, attempt_id),
            )
            .unwrap();
        (decision_public_id, publication_public_id)
    }
}

#[test]
fn published_wrong_facts_have_explicit_correction_states_without_mastery_claims() {
    let mut fixture = Fixture::new();
    fixture.publish_response(fixture.students[0], 1, "first", 0.0, "2026-07-01T08:00:00Z");
    fixture.publish_response(
        fixture.students[0],
        2,
        "correction",
        2.0,
        "2026-07-02T08:00:00Z",
    );
    fixture.publish_response(fixture.students[1], 1, "first", 1.0, "2026-07-03T08:00:00Z");
    fixture.publish_response(fixture.students[1], 2, "retry", 2.0, "2026-07-04T08:00:00Z");
    fixture.publish_response(fixture.students[2], 1, "first", 0.0, "2026-07-05T08:00:00Z");
    fixture.publish_response(
        fixture.students[2],
        2,
        "correction",
        1.0,
        "2026-07-06T08:00:00Z",
    );
    fixture.publish_response(fixture.students[3], 1, "first", 2.0, "2026-07-07T08:00:00Z");

    let dashboard = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
    assert_eq!(dashboard.summary.wrong_question_count, 3);
    assert_eq!(dashboard.summary.affected_student_count, 3);
    assert_eq!(dashboard.summary.corrected_once_count, 1);
    assert_eq!(dashboard.summary.rechecked_correct_count, 1);
    assert_eq!(dashboard.summary.needs_correction_count, 1);
    assert_eq!(dashboard.summary.repeated_error_count, 1);
    assert!(dashboard
        .summary
        .denominator_note
        .contains("不等于知识点已掌握"));
    assert_eq!(
        dashboard
            .items
            .iter()
            .find(|item| item.student_name == "小十")
            .unwrap()
            .status,
        "corrected_once"
    );
    assert_eq!(
        dashboard
            .items
            .iter()
            .find(|item| item.student_name == "小二")
            .unwrap()
            .status,
        "rechecked_correct"
    );
    let repeated = dashboard
        .items
        .iter()
        .find(|item| item.student_name == "小三")
        .unwrap();
    assert_eq!(repeated.status, "needs_correction");
    assert!(repeated.repeated_error);
    assert_eq!(repeated.knowledge_nodes[0].title, "洋务运动失败原因");
    assert_eq!(repeated.ability_dimensions[0].title, "因果分析");
    assert!(dashboard
        .items
        .iter()
        .all(|item| item.student_name != "小四"));
}

#[test]
fn scope_uses_enabled_students_current_publications_and_student_number_order() {
    let mut fixture = Fixture::new();
    fixture.publish_response(fixture.students[0], 1, "first", 0.0, "2026-07-01T08:00:00Z");
    fixture.publish_response(fixture.students[1], 1, "first", 0.0, "2026-07-02T08:00:00Z");
    fixture.publish_response(fixture.students[4], 1, "first", 0.0, "2026-07-03T08:00:00Z");
    fixture
        .conn
        .execute(
            "UPDATE students SET enabled=0 WHERE id=?1",
            [fixture.students[2]],
        )
        .unwrap();

    let dashboard = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
    assert_eq!(
        dashboard
            .items
            .iter()
            .map(|item| item.student_no.as_str())
            .collect::<Vec<_>>(),
        vec!["02", "10"]
    );
    assert_eq!(
        class_wrongbook_dashboard(&fixture.conn, fixture.class_two)
            .unwrap()
            .summary
            .wrong_question_count,
        0
    );
}

#[test]
fn student_wrongbook_items_filter_before_building_facts() {
    let mut fixture = Fixture::new();
    fixture.publish_response(fixture.students[0], 1, "first", 0.0, "2026-07-01T08:00:00Z");
    fixture.publish_response(fixture.students[1], 1, "first", 0.0, "2026-07-02T08:00:00Z");
    let items =
        student_wrongbook_items(&fixture.conn, fixture.class_one, fixture.students[1]).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].student_id, fixture.students[1]);
    assert_eq!(items[0].student_no, "02");
    assert!(
        student_wrongbook_items(&fixture.conn, fixture.class_one, fixture.students[4]).is_err()
    );
}

#[test]
fn current_publication_snapshot_wins_over_superseded_history() {
    let mut fixture = Fixture::new();
    fixture.publish_response(fixture.students[0], 1, "first", 0.0, "2026-07-01T08:00:00Z");
    let attempt_id: i64 = fixture
        .conn
        .query_row(
            "SELECT id FROM exam_attempts_v2 WHERE student_id=?1 AND attempt_no=1",
            [fixture.students[0]],
            |row| row.get(0),
        )
        .unwrap();
    let old_publication_id: i64 = fixture
        .conn
        .query_row(
            "SELECT active_publication_id FROM exam_attempts_v2 WHERE id=?1",
            [attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    fixture
        .conn
        .execute(
            "UPDATE exam_grade_publications_v2 SET state='superseded' WHERE id=?1",
            [old_publication_id],
        )
        .unwrap();
    fixture
        .conn
        .execute(
            "UPDATE exam_attempts_v2 SET active_publication_id=NULL WHERE id=?1",
            [attempt_id],
        )
        .unwrap();

    assert_eq!(
        class_wrongbook_dashboard(&fixture.conn, fixture.class_one)
            .unwrap()
            .summary
            .wrong_question_count,
        0
    );
}

#[test]
fn legacy_wrong_item_and_mastery_tables_are_not_read() {
    let fixture = Fixture::new();
    fixture
        .conn
        .execute(
            "INSERT INTO exam_wrong_items
                 (student_id,question_id,times_wrong,status,first_seen,last_seen)
                 VALUES (?1,999,8,'open',?2,?2)",
            (fixture.students[0], NOW),
        )
        .unwrap();
    fixture
        .conn
        .execute(
            "INSERT INTO exam_knowledge_mastery
                 (student_id,knowledge_point_id,total,correct,updated_at)
                 VALUES (?1,999,10,0,?2)",
            (fixture.students[0], NOW),
        )
        .unwrap();

    let dashboard = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
    assert_eq!(dashboard.summary.wrong_question_count, 0);
    assert!(dashboard.items.is_empty());
}

#[test]
fn read_model_does_not_write_database() {
    let mut fixture = Fixture::new();
    fixture.publish_response(fixture.students[0], 1, "first", 0.0, "2026-07-01T08:00:00Z");
    let before: i64 = fixture
        .conn
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    let _ = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
    let after: i64 = fixture
        .conn
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(after, before);
}

#[test]
fn missing_class_is_rejected() {
    let fixture = Fixture::new();
    assert!(matches!(
        class_wrongbook_dashboard(&fixture.conn, 999),
        Err(CoreError::NotFound(_))
    ));
}

#[test]
fn teacher_confirmed_error_causes_are_versioned_and_same_input_is_noop() {
    use crate::error_cause::{confirm_error_causes, ConfirmErrorCausesInput};

    let mut fixture = Fixture::new();
    let student_id = fixture.students[0];
    let (decision_public_id, publication_public_id) =
        fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
    let causes = vec!["concept_confusion".to_string(), "fact_error".to_string()];
    let input = ConfirmErrorCausesInput {
        class_id: fixture.class_one,
        student_id,
        question_version_public_id: "question-version-1",
        grade_decision_public_id: &decision_public_id,
        publication_public_id: &publication_public_id,
        cause_codes: &causes,
        teacher_note: Some("把根本原因和直接原因混淆"),
        confirmed_by: "teacher",
    };
    let first = confirm_error_causes(&mut fixture.conn, &input).unwrap();
    let repeated = confirm_error_causes(&mut fixture.conn, &input).unwrap();
    assert_eq!(first, repeated);
    assert_eq!(first.revision, 1);
    assert_eq!(
        first.cause_codes,
        vec!["fact_error".to_string(), "concept_confusion".to_string()]
    );
    assert_eq!(
        fixture
            .conn
            .query_row("SELECT COUNT(*) FROM wb_error_cause_revisions", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        1
    );

    let changed_causes = vec!["fact_error".to_string()];
    let changed = confirm_error_causes(
        &mut fixture.conn,
        &ConfirmErrorCausesInput {
            cause_codes: &changed_causes,
            teacher_note: None,
            ..input
        },
    )
    .unwrap();
    assert_eq!(changed.revision, 2);
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM wb_error_cause_revisions WHERE state='active'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM wb_error_cause_revisions WHERE state='superseded'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    let dashboard = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
    assert_eq!(
        dashboard.items[0]
            .cause_review
            .as_ref()
            .unwrap()
            .cause_codes,
        vec!["fact_error"]
    );
    assert!(dashboard.items[0]
        .cause_options
        .iter()
        .any(|option| option.code == "misread_prompt"));
}

#[test]
fn error_cause_confirmation_rejects_stale_scope_without_partial_rows() {
    use crate::error_cause::{confirm_error_causes, ConfirmErrorCausesInput};

    let mut fixture = Fixture::new();
    let student_id = fixture.students[0];
    let (old_decision, old_publication) =
        fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
    let (_latest_decision, _latest_publication) =
        fixture.publish_response(student_id, 2, "correction", 0.0, "2026-07-02T08:00:00Z");
    let causes = vec!["fact_error".to_string()];
    let stale = confirm_error_causes(
        &mut fixture.conn,
        &ConfirmErrorCausesInput {
            class_id: fixture.class_one,
            student_id,
            question_version_public_id: "question-version-1",
            grade_decision_public_id: &old_decision,
            publication_public_id: &old_publication,
            cause_codes: &causes,
            teacher_note: None,
            confirmed_by: "teacher",
        },
    );
    assert!(matches!(stale, Err(CoreError::Invalid(_))));
    assert_eq!(
        fixture
            .conn
            .query_row("SELECT COUNT(*) FROM wb_error_cause_revisions", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
}

#[test]
fn other_error_cause_requires_note_and_revision_rows_are_immutable() {
    use crate::error_cause::{confirm_error_causes, ConfirmErrorCausesInput};

    let mut fixture = Fixture::new();
    let student_id = fixture.students[0];
    let (decision_public_id, publication_public_id) =
        fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
    let causes = vec!["other".to_string()];
    let base = ConfirmErrorCausesInput {
        class_id: fixture.class_one,
        student_id,
        question_version_public_id: "question-version-1",
        grade_decision_public_id: &decision_public_id,
        publication_public_id: &publication_public_id,
        cause_codes: &causes,
        teacher_note: None,
        confirmed_by: "teacher",
    };
    assert!(matches!(
        confirm_error_causes(&mut fixture.conn, &base),
        Err(CoreError::Invalid(_))
    ));
    let saved = confirm_error_causes(
        &mut fixture.conn,
        &ConfirmErrorCausesInput {
            teacher_note: Some("课堂用语理解偏差"),
            ..base
        },
    )
    .unwrap();
    assert_eq!(saved.revision, 1);
    assert!(fixture
        .conn
        .execute(
            "UPDATE wb_error_cause_revisions SET teacher_note='覆盖原备注' WHERE state='active'",
            [],
        )
        .is_err());
    assert!(fixture
        .conn
        .execute("DELETE FROM wb_error_cause_items", [])
        .is_err());
}

#[test]
fn single_correction_is_atomic_idempotent_and_does_not_precreate_attempt() {
    use crate::correction::{create_single_correction, CreateCorrectionInput};

    let mut fixture = Fixture::new();
    let student_id = fixture.students[0];
    let (decision_public_id, publication_public_id) =
        fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
    let source_attempt_count: i64 = fixture
        .conn
        .query_row("SELECT COUNT(*) FROM exam_attempts_v2", [], |row| {
            row.get(0)
        })
        .unwrap();
    let input = CreateCorrectionInput {
        class_id: fixture.class_one,
        student_id,
        question_version_public_id: "question-version-1",
        source_grade_decision_public_id: &decision_public_id,
        source_publication_public_id: &publication_public_id,
        created_by: "teacher",
    };

    let created = create_single_correction(&mut fixture.conn, &input).unwrap();
    let repeated = create_single_correction(&mut fixture.conn, &input).unwrap();
    assert_eq!(created, repeated);
    assert_eq!(created.status, "waiting_upload");
    assert_eq!(created.latest_attempt_public_id, None);
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM wb_correction_assignments",
                [],
                |row| { row.get::<_, i64>(0) }
            )
            .unwrap(),
        1
    );
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT COUNT(*)
                     FROM exam_assessment_targets_v2 target
                     JOIN exam_assessment_versions_v2 version
                       ON version.id=target.assessment_version_id
                     WHERE version.public_id=?1 AND target.student_id=?2",
                (&created.assessment_version_public_id, student_id),
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_assessments_v2
                     WHERE assessment_context='correction'
                       AND evidence_policy='progress_only' AND state='active'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT COUNT(*)
                     FROM exam_assessment_versions_v2 version
                     JOIN exam_assessment_items_v2 item
                       ON item.assessment_version_id=version.id AND item.state='active'
                     JOIN k1_question_versions question
                       ON question.id=item.question_version_id
                     WHERE version.public_id=?1 AND version.state='confirmed'
                       AND question.public_id='question-version-1'",
                [&created.assessment_version_public_id],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        fixture
            .conn
            .query_row("SELECT COUNT(*) FROM exam_attempts_v2", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        source_attempt_count
    );
    assert!(fixture
        .conn
        .execute(
            "UPDATE wb_correction_assignments SET created_by='other' WHERE public_id=?1",
            [&created.public_id],
        )
        .is_err());
    assert!(fixture
        .conn
        .execute(
            "DELETE FROM wb_correction_assignments WHERE public_id=?1",
            [&created.public_id],
        )
        .is_err());

    let dashboard = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
    assert_eq!(
        dashboard.items[0].correction_assignment.as_ref().unwrap(),
        &created
    );

    let mismatched = CreateCorrectionInput {
        student_id: fixture.students[1],
        ..input
    };
    assert!(matches!(
        create_single_correction(&mut fixture.conn, &mismatched),
        Err(CoreError::Invalid(_))
    ));
}

#[test]
fn correction_assignment_failure_rolls_back_m2_assessment_and_target() {
    use crate::correction::{create_single_correction, CreateCorrectionInput};

    let mut fixture = Fixture::new();
    let student_id = fixture.students[0];
    let (decision_public_id, publication_public_id) =
        fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
    fixture
        .conn
        .execute_batch(
            "CREATE TRIGGER fail_correction_assignment
                 BEFORE INSERT ON wb_correction_assignments
                 BEGIN SELECT RAISE(ABORT,'injected correction assignment failure'); END;",
        )
        .unwrap();
    let result = create_single_correction(
        &mut fixture.conn,
        &CreateCorrectionInput {
            class_id: fixture.class_one,
            student_id,
            question_version_public_id: "question-version-1",
            source_grade_decision_public_id: &decision_public_id,
            source_publication_public_id: &publication_public_id,
            created_by: "teacher",
        },
    );
    assert!(result.is_err());
    let counts: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
                   (SELECT COUNT(*) FROM exam_assessments_v2
                    WHERE assessment_context='correction'),
                   (SELECT COUNT(*) FROM exam_assessment_targets_v2),
                   (SELECT COUNT(*) FROM wb_correction_assignments)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[test]
fn stale_correction_source_rolls_back_assessment_and_assignment() {
    use crate::correction::{create_single_correction, CreateCorrectionInput};

    let mut fixture = Fixture::new();
    let student_id = fixture.students[0];
    let (old_decision, old_publication) =
        fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
    fixture.publish_response(student_id, 2, "correction", 2.0, "2026-07-02T08:00:00Z");
    let result = create_single_correction(
        &mut fixture.conn,
        &CreateCorrectionInput {
            class_id: fixture.class_one,
            student_id,
            question_version_public_id: "question-version-1",
            source_grade_decision_public_id: &old_decision,
            source_publication_public_id: &old_publication,
            created_by: "teacher",
        },
    );
    assert!(matches!(result, Err(CoreError::Invalid(_))));
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM wb_correction_assignments",
                [],
                |row| { row.get::<_, i64>(0) }
            )
            .unwrap(),
        0
    );
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_assessments_v2
                     WHERE assessment_context='correction'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn correction_attempt_guard_accepts_only_target_student_and_kind() {
    use crate::correction::{
        create_single_correction, load_for_source_decision, CreateCorrectionInput,
    };

    let mut fixture = Fixture::new();
    let student_id = fixture.students[0];
    let (decision_public_id, publication_public_id) =
        fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
    let created = create_single_correction(
        &mut fixture.conn,
        &CreateCorrectionInput {
            class_id: fixture.class_one,
            student_id,
            question_version_public_id: "question-version-1",
            source_grade_decision_public_id: &decision_public_id,
            source_publication_public_id: &publication_public_id,
            created_by: "teacher",
        },
    )
    .unwrap();
    let version_id: i64 = fixture
        .conn
        .query_row(
            "SELECT id FROM exam_assessment_versions_v2 WHERE public_id=?1",
            [&created.assessment_version_public_id],
            |row| row.get(0),
        )
        .unwrap();
    let insert_attempt =
        |conn: &Connection, public_id: &str, target_student_id: i64, attempt_kind: &str| {
            conn.execute(
                "INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES (?1,?2,?3,1,'image',?4,'ingesting',?5,?5)",
                (public_id, version_id, target_student_id, attempt_kind, NOW),
            )
        };

    assert!(insert_attempt(
        &fixture.conn,
        "wrong-student-attempt",
        fixture.students[1],
        "correction"
    )
    .is_err());
    assert!(insert_attempt(&fixture.conn, "wrong-kind-attempt", student_id, "retry").is_err());
    insert_attempt(
        &fixture.conn,
        "target-correction-attempt",
        student_id,
        "correction",
    )
    .unwrap();

    let loaded = load_for_source_decision(&fixture.conn, &decision_public_id)
        .unwrap()
        .unwrap();
    assert_eq!(loaded.status, "in_progress");
    assert_eq!(
        loaded.latest_attempt_public_id.as_deref(),
        Some("target-correction-attempt")
    );
}

#[test]
fn reinforcement_preview_is_read_only_and_teacher_confirmation_is_idempotent() {
    use chrono::NaiveDate;

    use crate::reinforcement::{
        confirm_reinforcement_at, preview_reinforcement_at, ConfirmReinforcementInput,
        ReinforcementScopeInput,
    };

    let mut fixture = Fixture::new();
    let student_id = fixture.students[0];
    let (decision_public_id, publication_public_id) =
        fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
    fixture.publish_response(student_id, 2, "correction", 2.0, "2026-07-02T08:00:00Z");
    let scope = ReinforcementScopeInput {
        class_id: fixture.class_one,
        student_id,
        question_version_public_id: "question-version-1",
        source_grade_decision_public_id: &decision_public_id,
        source_publication_public_id: &publication_public_id,
    };
    let today = NaiveDate::from_ymd_opt(2026, 7, 16).unwrap();
    let preview = preview_reinforcement_at(&fixture.conn, &scope, today).unwrap();
    assert_eq!(preview.priority, "normal");
    assert_eq!(preview.suggested_due_date, "2026-07-16");
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM wb_reinforcement_assignments",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );

    let input = ConfirmReinforcementInput {
        scope,
        expected_policy_public_id: &preview.policy_public_id,
        expected_due_date: &preview.suggested_due_date,
        previewed_as_of_date: &preview.previewed_as_of_date,
        created_by: "teacher",
    };
    let created = confirm_reinforcement_at(&mut fixture.conn, &input, today).unwrap();
    let repeated = confirm_reinforcement_at(&mut fixture.conn, &input, today).unwrap();
    assert_eq!(created, repeated);
    assert_eq!(created.status, "scheduled");
    assert_eq!(created.due_date, "2026-07-16");
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT status FROM tasks WHERE id=?1",
                [created.task_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "open"
    );
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_attempts_v2
                     WHERE assessment_version_id=(
                       SELECT id FROM exam_assessment_versions_v2 WHERE public_id=?1
                     )",
                [&created.assessment_version_public_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    let dashboard = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
    assert_eq!(
        dashboard.items[0]
            .reinforcement_assignment
            .as_ref()
            .unwrap(),
        &created
    );
}

#[test]
fn repeated_error_only_raises_reinforcement_priority_without_creating_work() {
    use chrono::NaiveDate;

    use crate::reinforcement::{preview_reinforcement_at, ReinforcementScopeInput};

    let mut fixture = Fixture::new();
    let student_id = fixture.students[0];
    fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
    let (latest_error, latest_error_publication) =
        fixture.publish_response(student_id, 2, "retry", 1.0, "2026-07-02T08:00:00Z");
    fixture.publish_response(student_id, 3, "correction", 2.0, "2026-07-03T08:00:00Z");
    let preview = preview_reinforcement_at(
        &fixture.conn,
        &ReinforcementScopeInput {
            class_id: fixture.class_one,
            student_id,
            question_version_public_id: "question-version-1",
            source_grade_decision_public_id: &latest_error,
            source_publication_public_id: &latest_error_publication,
        },
        NaiveDate::from_ymd_opt(2026, 7, 16).unwrap(),
    )
    .unwrap();
    assert_eq!(preview.priority, "high");
    assert!(preview.reason.contains("2 次非满分"));
    let counts: (i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
                   (SELECT COUNT(*) FROM wb_reinforcement_assignments),
                   (SELECT COUNT(*) FROM tasks WHERE module='wrongbook')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0));
}

#[test]
fn stale_reinforcement_preview_is_rejected_without_partial_writes() {
    use chrono::NaiveDate;
    use suite_core::services::scheduling::{SchedulePolicyUpdate, LEARNING_POLICY_KEY};

    use crate::reinforcement::{
        confirm_reinforcement_at, preview_reinforcement_at, ConfirmReinforcementInput,
        ReinforcementScopeInput,
    };

    let mut fixture = Fixture::new();
    let student_id = fixture.students[0];
    let (decision_public_id, publication_public_id) =
        fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
    fixture.publish_response(student_id, 2, "correction", 2.0, "2026-07-02T08:00:00Z");
    let scope = ReinforcementScopeInput {
        class_id: fixture.class_one,
        student_id,
        question_version_public_id: "question-version-1",
        source_grade_decision_public_id: &decision_public_id,
        source_publication_public_id: &publication_public_id,
    };
    let today = NaiveDate::from_ymd_opt(2026, 7, 16).unwrap();
    let preview = preview_reinforcement_at(&fixture.conn, &scope, today).unwrap();
    suite_core::services::scheduling::replace_active_policy(
        &mut fixture.conn,
        LEARNING_POLICY_KEY,
        &SchedulePolicyUpdate {
            default_delay_days: 8,
            daily_limit_per_student: 3,
            weekend_policy: "next_workday".into(),
            holiday_policy: "next_workday".into(),
            max_shift_days: 60,
            holidays: Vec::new(),
        },
        "teacher",
    )
    .unwrap();
    let result = confirm_reinforcement_at(
        &mut fixture.conn,
        &ConfirmReinforcementInput {
            scope,
            expected_policy_public_id: &preview.policy_public_id,
            expected_due_date: &preview.suggested_due_date,
            previewed_as_of_date: &preview.previewed_as_of_date,
            created_by: "teacher",
        },
        today,
    );
    assert!(matches!(result, Err(CoreError::Invalid(_))));
    let counts: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
                   (SELECT COUNT(*) FROM wb_reinforcement_assignments),
                   (SELECT COUNT(*) FROM tasks WHERE module='wrongbook'),
                   (SELECT COUNT(*) FROM exam_assessments_v2
                    WHERE assessment_context='homework' AND evidence_policy='include_low_weight')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[test]
fn reinforcement_attempt_updates_shared_task_lifecycle() {
    use chrono::NaiveDate;

    use crate::reinforcement::{
        confirm_reinforcement_at, preview_reinforcement_at, ConfirmReinforcementInput,
        ReinforcementScopeInput,
    };

    let mut fixture = Fixture::new();
    let student_id = fixture.students[0];
    let (decision_public_id, publication_public_id) =
        fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
    fixture.publish_response(student_id, 2, "correction", 2.0, "2026-07-02T08:00:00Z");
    let scope = ReinforcementScopeInput {
        class_id: fixture.class_one,
        student_id,
        question_version_public_id: "question-version-1",
        source_grade_decision_public_id: &decision_public_id,
        source_publication_public_id: &publication_public_id,
    };
    let today = NaiveDate::from_ymd_opt(2026, 7, 16).unwrap();
    let preview = preview_reinforcement_at(&fixture.conn, &scope, today).unwrap();
    let created = confirm_reinforcement_at(
        &mut fixture.conn,
        &ConfirmReinforcementInput {
            scope,
            expected_policy_public_id: &preview.policy_public_id,
            expected_due_date: &preview.suggested_due_date,
            previewed_as_of_date: &preview.previewed_as_of_date,
            created_by: "teacher",
        },
        today,
    )
    .unwrap();
    let version_id: i64 = fixture
        .conn
        .query_row(
            "SELECT id FROM exam_assessment_versions_v2 WHERE public_id=?1",
            [&created.assessment_version_public_id],
            |row| row.get(0),
        )
        .unwrap();
    fixture
        .conn
        .execute(
            "INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES ('reinforcement-attempt-1',?1,?2,1,'image','first','ingesting',?3,?3)",
            (version_id, student_id, NOW),
        )
        .unwrap();
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT status FROM tasks WHERE id=?1",
                [created.task_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "submitted"
    );
    fixture
            .conn
            .execute(
                "UPDATE exam_attempts_v2 SET state='published' WHERE public_id='reinforcement-attempt-1'",
                [],
            )
            .unwrap();
    assert_eq!(
        fixture
            .conn
            .query_row(
                "SELECT status FROM tasks WHERE id=?1",
                [created.task_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "closed"
    );
}

#[test]
fn statistics_filter_current_activity_and_only_aggregate_teacher_confirmed_causes() {
    use crate::error_cause::{confirm_error_causes, ConfirmErrorCausesInput};
    use crate::report::{wrongbook_statistics, WrongbookStatisticsScope};

    let mut fixture = Fixture::new();
    let student_one = fixture.students[0];
    let student_two = fixture.students[1];
    fixture.publish_response(student_one, 1, "first", 0.0, "2026-07-01T08:00:00Z");
    let (latest_decision, latest_publication) =
        fixture.publish_response(student_one, 2, "retry", 1.0, "2026-07-16T08:00:00Z");
    fixture.publish_response(student_two, 1, "first", 0.0, "2026-06-01T08:00:00Z");
    confirm_error_causes(
        &mut fixture.conn,
        &ConfirmErrorCausesInput {
            class_id: fixture.class_one,
            student_id: student_one,
            question_version_public_id: "question-version-1",
            grade_decision_public_id: &latest_decision,
            publication_public_id: &latest_publication,
            cause_codes: &["concept_confusion".into(), "fact_error".into()],
            teacher_note: Some("根本原因与直接原因混淆"),
            confirmed_by: "teacher",
        },
    )
    .unwrap();

    let before: i64 = fixture
        .conn
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    let statistics = wrongbook_statistics(
        &fixture.conn,
        &WrongbookStatisticsScope {
            class_id: fixture.class_one,
            student_id: None,
            range_start: "2026-07-10",
            range_end: "2026-07-17",
        },
    )
    .unwrap();
    assert_eq!(
        fixture
            .conn
            .query_row("SELECT total_changes()", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        before
    );
    assert_eq!(statistics.summary.student_count, 1);
    assert_eq!(statistics.summary.fact_count, 1);
    assert_eq!(statistics.summary.evidence_count, 2);
    assert_eq!(statistics.summary.repeated_error_count, 1);
    assert_eq!(statistics.summary.confirmed_cause_review_count, 1);
    assert_eq!(statistics.summary.confirmed_cause_item_count, 2);
    assert_eq!(statistics.students[0].student_id, student_one);
    assert_eq!(statistics.question_causes.len(), 1);
    assert_eq!(statistics.knowledge_causes.len(), 1);
    assert_eq!(
        statistics.question_causes[0]
            .causes
            .iter()
            .map(|cause| cause.cause_label.as_str())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["史实错误", "概念混淆"])
    );
    assert!(statistics.meta.activity_filter_rule.contains("最近一次"));
    assert!(statistics
        .meta
        .evidence_count_rule
        .contains("不等同于独立掌握"));
}

#[test]
fn report_snapshot_freezes_scope_and_student_export_excludes_classmates() {
    use crate::report::{
        create_report_snapshot, write_report_snapshot_csv, CreateWrongbookReportInput,
    };

    let mut fixture = Fixture::new();
    let student_one = fixture.students[0];
    let student_two = fixture.students[1];
    fixture.publish_response(student_one, 1, "first", 0.0, "2026-07-16T08:00:00Z");
    fixture.publish_response(student_two, 1, "first", 1.0, "2026-07-16T09:00:00Z");

    let snapshot = create_report_snapshot(
        &mut fixture.conn,
        &CreateWrongbookReportInput {
            report_kind: "student_parent",
            class_id: fixture.class_one,
            student_id: Some(student_one),
            range_start: "2026-07-01",
            range_end: "2026-07-17",
            generated_by: "teacher",
        },
    )
    .unwrap();
    assert_eq!(snapshot.student_id, Some(student_one));
    assert_eq!(snapshot.fact_count, 1);
    assert_eq!(snapshot.evidence_count, 1);
    assert!(snapshot.suggested_file_name.ends_with(".csv"));

    let payload: String = fixture
        .conn
        .query_row(
            "SELECT payload_json FROM wb_report_snapshots WHERE public_id=?1",
            [&snapshot.public_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(payload.contains("小十"));
    assert!(!payload.contains("小二"));

    let path = std::env::temp_dir().join(format!(
        "jiaofu-wrongbook-report-{}.csv",
        suite_core::domain::ids::new_public_id()
    ));
    let written =
        write_report_snapshot_csv(&fixture.conn, &snapshot.public_id, path.to_str().unwrap())
            .unwrap();
    assert_eq!(written.sha256, snapshot.csv_sha256);
    let csv = std::fs::read_to_string(&path).unwrap();
    assert!(csv.contains("小十"));
    assert!(!csv.contains("小二"));
    assert!(csv.contains("不含名次、班级排名或掌握总分"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    assert!(
        write_report_snapshot_csv(&fixture.conn, &snapshot.public_id, path.to_str().unwrap())
            .is_err()
    );
    std::fs::remove_file(path).unwrap();

    let immutable = fixture.conn.execute(
        "UPDATE wb_report_snapshots SET evidence_count=99 WHERE public_id=?1",
        [&snapshot.public_id],
    );
    assert!(immutable.is_err());
}

#[test]
fn report_scope_validation_is_fail_closed_without_snapshot_rows() {
    use crate::report::{create_report_snapshot, CreateWrongbookReportInput};

    let mut fixture = Fixture::new();
    assert!(create_report_snapshot(
        &mut fixture.conn,
        &CreateWrongbookReportInput {
            report_kind: "student_parent",
            class_id: fixture.class_one,
            student_id: None,
            range_start: "2026-07-01",
            range_end: "2026-07-17",
            generated_by: "teacher",
        },
    )
    .is_err());
    assert!(create_report_snapshot(
        &mut fixture.conn,
        &CreateWrongbookReportInput {
            report_kind: "class_summary",
            class_id: fixture.class_one,
            student_id: Some(fixture.students[0]),
            range_start: "2026-07-01",
            range_end: "2026-07-17",
            generated_by: "teacher",
        },
    )
    .is_err());
    assert_eq!(
        fixture
            .conn
            .query_row("SELECT COUNT(*) FROM wb_report_snapshots", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
}
