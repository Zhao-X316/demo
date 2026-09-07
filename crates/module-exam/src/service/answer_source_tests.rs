use super::*;
use crate::answer_source_recognition::{AnswerSourceEntry, AnswerSourceRecognizerDescriptor};
use crate::service::fixed_paper::{register_fixed_input_document, NewFixedInputDocument};
use suite_core::db::repo::artifacts;
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

struct Fixture {
    conn: Connection,
    batch_id: i64,
    run_id: i64,
}

fn fixture(candidate_correct: bool) -> Fixture {
    fixture_with_candidate(
        "true_false",
        serde_json::json!({"schema_version":1,"correct":candidate_correct}),
    )
}

fn fixture_with_candidate(question_type: &str, candidate_answer: Value) -> Fixture {
    let conn = open_in_memory().unwrap();
    run_migrations(&conn, CORE_MIGRATIONS).unwrap();
    run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
    run_migrations(&conn, crate::exam_migrations()).unwrap();
    let hash = "a".repeat(64);
    conn.execute_batch(&format!(
            r#"INSERT INTO subjects(name) VALUES ('历史');
               INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
               INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('answer-source-edition',1,'PEP','2024','中国历史八上','8','upper',
                         'active','2026-07-15T10:00:00Z');
               INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('answer-source-map',1,1,'confirmed','2026-07-15T10:00:00Z',
                         '2026-07-15T10:00:00Z');
               INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('answer-source-q','personal','teacher','unknown',0,'2026-07-15T10:00:00Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES ('answer-source-qv',1,1,'{question_type}','鸦片战争爆发于1840年。',1,
                         '{hash}','L2','published','2026-07-15T10:00:00Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-source-ak',1,1,'{{"schema_version":1,"correct":true}}',
                         'confirmed','2026-07-15T10:00:00Z','teacher','2026-07-15T10:00:00Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-source-r',1,1,1,'confirmed','2026-07-15T10:00:00Z',
                         'teacher','2026-07-15T10:00:00Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-source-links',1,1,1,'confirmed','2026-07-15T10:00:00Z',
                         'teacher','2026-07-15T10:00:00Z');
               INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,
                  created_by,created_at,updated_at)
                 VALUES ('answer-source-a','答案资料作业',1,'quiz','include','active','teacher',
                         '2026-07-15T10:00:00Z','2026-07-15T10:00:00Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,
                  created_at,confirmed_by,confirmed_at)
                 VALUES ('answer-source-av',1,1,'{hash}','answer-source-template','confirmed',
                         '2026-07-15T10:00:00Z','teacher','2026-07-15T10:00:00Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,state,created_at)
                 VALUES ('answer-source-item',1,1,1,1,1,0,1,
                         '{{"schema_version":1,"question_no":"1"}}','active',
                         '2026-07-15T10:00:00Z');
               INSERT INTO exam_ingest_batches_v2
                 (public_id,assessment_version_id,source_kind,idempotency_key,state,
                  created_by,created_at,updated_at)
                 VALUES ('answer-source-batch',1,'image_folder','answer-source-batch','processing',
                         'teacher','2026-07-15T10:00:00Z','2026-07-15T10:00:00Z');"#
        ))
        .unwrap();
    if question_type == "short_answer" {
        conn.execute_batch(
                r#"INSERT INTO k1_rubric_points
                     (public_id,stable_id,rubric_version_id,order_index,canonical_text,
                      allowed_paraphrases_json,required_concepts_json,max_score,created_at)
                   VALUES ('answer-source-rp','stable-rp',1,0,'旧评分点',
                           '["旧允许表达"]','["旧概念"]',1,'2026-07-15T10:00:00Z');
                   INSERT INTO k1_knowledge_nodes
                     (public_id,stable_id,knowledge_map_id,code,title,order_index,state,created_at)
                   VALUES ('answer-source-kn','stable-kn',1,'KN-1','鸦片战争影响',0,'active',
                           '2026-07-15T10:00:00Z');
                   INSERT INTO k1_ability_dimensions
                     (public_id,stable_id,subject_id,revision,code,title,state,created_at)
                   VALUES ('answer-source-ad','stable-ad',1,1,'CAUSE','因果分析','active',
                           '2026-07-15T10:00:00Z');
                   INSERT INTO k1_knowledge_links
                     (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,
                      relation_type,confirmation_level,verified_by,verified_at,created_at)
                   VALUES ('answer-source-kl',1,'rubric_point','answer-source-rp',1,
                           'rubric_basis','teacher_confirmed','teacher',
                           '2026-07-15T10:00:00Z','2026-07-15T10:00:00Z');
                   INSERT INTO k1_ability_links
                     (public_id,link_set_id,source_type,source_public_id,ability_dimension_id,
                      evidence_strength,response_mode,confirmation_level,verified_by,verified_at,created_at)
                   VALUES ('answer-source-al',1,'rubric_point','answer-source-rp',1,0.8,
                           'structured_response','teacher_confirmed','teacher',
                           '2026-07-15T10:00:00Z','2026-07-15T10:00:00Z');"#,
            )
            .unwrap();
    }
    let artifact = artifacts::create_or_get(
        &conn,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Document,
            sha256: &"b".repeat(64),
            mime_type: "text/plain",
            byte_size: 4,
            original_name: Some("answer.txt"),
            original_path: None,
            archived_path: "/tmp/answer-source.txt",
            parent_artifact_id: None,
            derivative_type: None,
            processing_version: "answer-source-test-v1",
            privacy_class: PrivacyClass::TeachingContent,
            archive_status: ArchiveStatus::Ready,
        },
    )
    .unwrap();
    register_fixed_input_document(
        &conn,
        &NewFixedInputDocument {
            ingest_batch_id: 1,
            source_artifact_id: artifact.id,
            document_role: "answer_source",
            source_format: "text",
            import_index: 0,
            page_count: 1,
            idempotency_key: "answer-source-doc",
            created_by: "teacher",
        },
    )
    .unwrap();
    let descriptor = AnswerSourceRecognizerDescriptor {
        provider: "fixture".into(),
        model_name: "fixture".into(),
        model_version: "v1".into(),
        config_version: "v1".into(),
        rule_version: "v1".into(),
    };
    let output = AnswerSourceRecognitionOutput {
        schema_version: 1,
        ingest_batch_id: 1,
        source_artifact_id: artifact.id,
        source_artifact_sha256: artifact.sha256.clone(),
        input_hash: "c".repeat(64),
        descriptor,
        state: AnswerSourceState::Ready,
        entries: vec![AnswerSourceEntry {
            assessment_item_id: 1,
            answer_json: candidate_answer,
            source_anchor: serde_json::json!({"schema_version":1,"line":1}),
            confidence: 0.99,
        }],
        confidence: 0.99,
        issue_codes: vec![],
    };
    let run = ai_runs::create_or_get(
        &conn,
        &ai_runs::NewAiRun {
            idempotency_key: "answer-source-run",
            run_type: "answer_source_structure",
            source_module: "exam",
            business_ref_type: "fixed_answer_source",
            business_ref_id: "1",
            input_artifact_id: Some(artifact.id),
            provider: "fixture",
            model_name: "fixture",
            model_version: "v1",
            config_version: "v1",
            prompt_or_rule_version: "v1",
            input_hash: &output.input_hash,
            retry_of_ai_run_id: None,
        },
    )
    .unwrap();
    ai_runs::start(&conn, run.id, "2026-07-15T10:00:01Z", None).unwrap();
    let output_json = serde_json::to_string(&output).unwrap();
    ai_runs::finalize_succeeded(
        &conn,
        run.id,
        &hashing::sha256_hex(output_json.as_bytes()),
        Some(0.99),
        &output_json,
        "2026-07-15T10:00:02Z",
    )
    .unwrap();
    materialize_ai_drafts(&conn, &output, run.id).unwrap();
    Fixture {
        conn,
        batch_id: 1,
        run_id: run.id,
    }
}

#[test]
fn matching_source_requires_one_teacher_confirmation_then_reuses_bound_k1() {
    let mut fixture = fixture(true);
    let summary = review_summary(&fixture.conn, fixture.batch_id, fixture.run_id).unwrap();
    assert_eq!(summary.route, "ready_to_confirm");
    assert_eq!(
        latest_preflight_gate(&fixture.conn, fixture.batch_id)
            .unwrap()
            .0,
        AnswerSourcePreflightGate::ReviewRequired
    );
    let confirmed = confirm_matches(
        &mut fixture.conn,
        fixture.batch_id,
        fixture.run_id,
        "teacher",
    )
    .unwrap();
    assert_eq!(confirmed.route, "confirmed");
    let official: i64 = fixture
        .conn
        .query_row(
            "SELECT COUNT(*) FROM exam_answer_authority_candidates_v2
                 WHERE source_kind='uploaded_official' AND teacher_confirmed=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(official, 1);
    assert_eq!(
        latest_preflight_gate(&fixture.conn, fixture.batch_id)
            .unwrap()
            .0,
        AnswerSourcePreflightGate::Ready
    );
    let effects: (i64, i64) = fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_publications_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(effects, (0, 0));
}

#[test]
fn conflict_stays_blocked_until_teacher_explicitly_keeps_bound_answer() {
    let mut fixture = fixture(false);
    let summary = review_summary(&fixture.conn, fixture.batch_id, fixture.run_id).unwrap();
    assert_eq!(summary.route, "blocked");
    assert_eq!(summary.conflict_count, 1);
    assert!(confirm_matches(
        &mut fixture.conn,
        fixture.batch_id,
        fixture.run_id,
        "teacher"
    )
    .is_err());
    let resolved = keep_bound_answers(
        &mut fixture.conn,
        fixture.batch_id,
        fixture.run_id,
        "teacher",
    )
    .unwrap();
    assert_eq!(resolved.route, "kept_bound");
    assert_eq!(
        latest_preflight_gate(&fixture.conn, fixture.batch_id)
            .unwrap()
            .0,
        AnswerSourcePreflightGate::Ready
    );
}

#[test]
fn conflict_can_create_new_k1_and_assessment_version_without_rebinding_current_batch() {
    let mut fixture = fixture(false);
    let adopted = adopt_conflicts_as_new_version(
        &mut fixture.conn,
        fixture.batch_id,
        fixture.run_id,
        "teacher",
    )
    .unwrap();
    assert_eq!(adopted.route, "adopted_new_version");
    let adoption = adopted.adoption.clone().unwrap();
    assert_eq!(adoption.source_assessment_version_id, 1);
    assert_eq!(adoption.adopted_assessment_revision, 2);
    assert_eq!(adoption.changed_item_count, 1);
    assert!(adoption.current_batch_unchanged);
    assert_eq!(adopted.resolution.as_deref(), Some("kept_bound"));

    let batch_version: i64 = fixture
        .conn
        .query_row(
            "SELECT assessment_version_id FROM exam_ingest_batches_v2 WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(batch_version, 1);
    let adopted_state: (String, bool) = fixture
        .conn
        .query_row(
            "SELECT state,item_set_hash IS NOT NULL FROM exam_assessment_versions_v2 WHERE id=?1",
            [adoption.adopted_assessment_version_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(adopted_state, ("confirmed".into(), true));
    let (new_answer_id, new_answer_json, supersedes): (i64, String, i64) = fixture
        .conn
        .query_row(
            "SELECT i.answer_key_version_id,a.answer_json,a.supersedes_answer_key_id
                 FROM exam_assessment_items_v2 i
                 JOIN k1_answer_key_versions a ON a.id=i.answer_key_version_id
                 WHERE i.assessment_version_id=?1",
            [adoption.adopted_assessment_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_ne!(new_answer_id, 1);
    assert_eq!(supersedes, 1);
    assert_eq!(
        serde_json::from_str::<Value>(&new_answer_json).unwrap(),
        serde_json::json!({"schema_version":1,"correct":false})
    );
    let old_answer_json: String = fixture
        .conn
        .query_row(
            "SELECT answer_json FROM k1_answer_key_versions WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&old_answer_json).unwrap(),
        serde_json::json!({"schema_version":1,"correct":true})
    );
    let effects: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_publications_v2),
                        (SELECT COUNT(*) FROM learning_evidence)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(effects, (0, 0, 0));
    assert_eq!(
        latest_preflight_gate(&fixture.conn, fixture.batch_id)
            .unwrap()
            .0,
        AnswerSourcePreflightGate::Ready
    );

    let repeated = adopt_conflicts_as_new_version(
        &mut fixture.conn,
        fixture.batch_id,
        fixture.run_id,
        "teacher",
    )
    .unwrap();
    assert_eq!(
        repeated
            .adoption
            .as_ref()
            .unwrap()
            .adopted_assessment_version_id,
        adoption.adopted_assessment_version_id
    );
    let counts: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                        (SELECT COUNT(*) FROM k1_answer_key_versions),
                        (SELECT COUNT(*) FROM exam_answer_source_adoptions_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (2, 2, 1));
    assert!(fixture
        .conn
        .execute(
            "UPDATE exam_answer_source_adoptions_v2 SET changed_item_count=2 WHERE id=1",
            [],
        )
        .is_err());
}

#[test]
fn short_answer_conflict_creates_confirmed_rubric_and_carries_verified_links() {
    let mut fixture = fixture_with_candidate(
        "short_answer",
        serde_json::json!({
            "schema_version": 1,
            "reference_answer": "需要新的参考答案",
            "rubric_points": [{
                "order_index": 0,
                "canonical_text":"新评分点一",
                "allowed_paraphrases":["等价表述"],
                "required_concepts":["核心概念"],
                "max_score":1
            }],
        }),
    );
    let adopted = adopt_conflicts_as_new_version(
        &mut fixture.conn,
        fixture.batch_id,
        fixture.run_id,
        "teacher",
    )
    .unwrap();
    let adoption = adopted.adoption.unwrap();
    assert_eq!(adoption.changed_rubric_count, 1);
    assert_eq!(adoption.carried_knowledge_link_count, 1);
    assert_eq!(adoption.carried_ability_link_count, 1);
    let new_version_id = adoption.adopted_assessment_version_id;
    let adopted_versions: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT answer_key_version_id,rubric_version_id,link_set_id
                 FROM exam_assessment_items_v2 WHERE assessment_version_id=?1",
            [new_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_ne!(adopted_versions.0, 1);
    assert_ne!(adopted_versions.1, 1);
    assert_ne!(adopted_versions.2, 1);
    let point: (String, String, String, String, f64) = fixture
        .conn
        .query_row(
            "SELECT stable_id,canonical_text,allowed_paraphrases_json,
                        required_concepts_json,max_score
                 FROM k1_rubric_points WHERE rubric_version_id=?1",
            [adopted_versions.1],
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
    assert_eq!(point.0, "stable-rp");
    assert_eq!(point.1, "新评分点一");
    assert_eq!(point.2, "[\"等价表述\"]");
    assert_eq!(point.3, "[\"核心概念\"]");
    assert_eq!(point.4, 1.0);
    let carried: (i64, i64, String, String) = fixture
        .conn
        .query_row(
            "SELECT
                   (SELECT COUNT(*) FROM k1_knowledge_links WHERE link_set_id=?1),
                   (SELECT COUNT(*) FROM k1_ability_links WHERE link_set_id=?1),
                   (SELECT source_public_id FROM k1_knowledge_links WHERE link_set_id=?1),
                   (SELECT confirmation_level FROM k1_knowledge_links WHERE link_set_id=?1)",
            [adopted_versions.2],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (carried.0, carried.1, carried.3.as_str()),
        (1, 1, "teacher_confirmed")
    );
    assert_ne!(carried.2, "answer-source-rp");
    let effects: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_publications_v2),
                        (SELECT COUNT(*) FROM learning_evidence)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(effects, (0, 0, 0));
    let current_batch_version: i64 = fixture
        .conn
        .query_row(
            "SELECT assessment_version_id FROM exam_ingest_batches_v2 WHERE id=?1",
            [fixture.batch_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(current_batch_version, 1);
}

#[test]
fn short_answer_point_shape_change_rolls_back_every_new_version() {
    let mut fixture = fixture_with_candidate(
        "short_answer",
        serde_json::json!({
            "schema_version": 1,
            "reference_answer": "需要新的参考答案",
            "rubric_points": [
                {"order_index":0,"canonical_text":"评分点一","max_score":0.5},
                {"order_index":1,"canonical_text":"评分点二","max_score":0.5}
            ],
        }),
    );
    let error = adopt_conflicts_as_new_version(
        &mut fixture.conn,
        fixture.batch_id,
        fixture.run_id,
        "teacher",
    )
    .unwrap_err();
    assert!(error.to_string().contains("数量"));
    let counts: (i64, i64, i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                        (SELECT COUNT(*) FROM k1_answer_key_versions),
                        (SELECT COUNT(*) FROM k1_rubric_versions),
                        (SELECT COUNT(*) FROM k1_link_sets),
                        (SELECT COUNT(*) FROM exam_answer_source_adoptions_v2)",
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
    assert_eq!(counts, (1, 1, 1, 1, 0));
}

#[test]
fn teacher_mapping_can_reuse_one_point_and_add_one_without_guessing_links() {
    let mut fixture = fixture_with_candidate(
        "short_answer",
        serde_json::json!({
            "schema_version": 1,
            "reference_answer": "需要新的参考答案",
            "rubric_points": [
                {"order_index":0,"canonical_text":"沿用旧知识点的新表述","max_score":0.5},
                {"order_index":1,"canonical_text":"新增评分点","max_score":0.5}
            ],
        }),
    );
    let adopted = adopt_conflicts_as_new_version_with_mappings(
        &mut fixture.conn,
        fixture.batch_id,
        fixture.run_id,
        "teacher",
        &[
            RubricPointMappingInput {
                assessment_item_id: 1,
                candidate_order_index: 0,
                action: RubricPointMappingAction::ReuseExisting,
                previous_stable_id: Some("stable-rp".into()),
            },
            RubricPointMappingInput {
                assessment_item_id: 1,
                candidate_order_index: 1,
                action: RubricPointMappingAction::NewPoint,
                previous_stable_id: None,
            },
        ],
    )
    .unwrap();
    let adoption = adopted.adoption.unwrap();
    assert_eq!(adoption.changed_rubric_count, 1);
    assert_eq!(adoption.new_rubric_point_count, 1);
    assert_eq!(adoption.retired_rubric_point_count, 0);
    assert_eq!(adoption.unlinked_new_rubric_point_count, 1);
    assert_eq!(adoption.carried_knowledge_link_count, 1);
    assert_eq!(adoption.carried_ability_link_count, 1);
    assert_eq!(adoption.dropped_knowledge_link_count, 0);
    assert_eq!(adoption.dropped_ability_link_count, 0);

    let new_rubric_version_id: i64 = fixture
        .conn
        .query_row(
            "SELECT rubric_version_id FROM exam_assessment_items_v2
                 WHERE assessment_version_id=?1",
            [adoption.adopted_assessment_version_id],
            |row| row.get(0),
        )
        .unwrap();
    let (point_count, reused_count, generated_count): (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT COUNT(*),
                        SUM(CASE WHEN stable_id='stable-rp' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN stable_id LIKE 'rubric-point-%' THEN 1 ELSE 0 END)
                 FROM k1_rubric_points WHERE rubric_version_id=?1",
            [new_rubric_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!((point_count, reused_count, generated_count), (2, 1, 1));

    let mapping_counts: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT COUNT(*),
                        SUM(CASE WHEN mapping_action='reuse_existing' THEN 1 ELSE 0 END),
                        SUM(CASE WHEN mapping_action='new_point' THEN 1 ELSE 0 END)
                 FROM exam_rubric_point_mappings_v2",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(mapping_counts, (2, 1, 1));
    assert!(fixture
        .conn
        .execute(
            "UPDATE exam_rubric_point_mappings_v2 SET created_by='other' WHERE id=1",
            [],
        )
        .is_err());
    let effects: (i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_publications_v2),
                        (SELECT COUNT(*) FROM learning_evidence)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(effects, (0, 0, 0));
}

#[test]
fn incomplete_or_duplicate_teacher_mapping_rolls_back_every_new_version() {
    let mut fixture = fixture_with_candidate(
        "short_answer",
        serde_json::json!({
            "schema_version": 1,
            "reference_answer": "需要新的参考答案",
            "rubric_points": [
                {"order_index":0,"canonical_text":"评分点一","max_score":0.5},
                {"order_index":1,"canonical_text":"评分点二","max_score":0.5}
            ],
        }),
    );
    let incomplete = adopt_conflicts_as_new_version_with_mappings(
        &mut fixture.conn,
        fixture.batch_id,
        fixture.run_id,
        "teacher",
        &[RubricPointMappingInput {
            assessment_item_id: 1,
            candidate_order_index: 0,
            action: RubricPointMappingAction::ReuseExisting,
            previous_stable_id: Some("stable-rp".into()),
        }],
    )
    .unwrap_err();
    assert!(incomplete
        .to_string()
        .contains("必须覆盖上传答案的每一个评分点"));

    let duplicate = adopt_conflicts_as_new_version_with_mappings(
        &mut fixture.conn,
        fixture.batch_id,
        fixture.run_id,
        "teacher",
        &[
            RubricPointMappingInput {
                assessment_item_id: 1,
                candidate_order_index: 0,
                action: RubricPointMappingAction::ReuseExisting,
                previous_stable_id: Some("stable-rp".into()),
            },
            RubricPointMappingInput {
                assessment_item_id: 1,
                candidate_order_index: 1,
                action: RubricPointMappingAction::ReuseExisting,
                previous_stable_id: Some("stable-rp".into()),
            },
        ],
    )
    .unwrap_err();
    assert!(duplicate
        .to_string()
        .contains("同一个旧评分点不能对应多个新评分点"));

    let counts: (i64, i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                        (SELECT COUNT(*) FROM k1_rubric_versions),
                        (SELECT COUNT(*) FROM exam_answer_source_adoptions_v2),
                        (SELECT COUNT(*) FROM exam_rubric_point_mappings_v2)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 1, 0, 0));
}

#[test]
fn teacher_can_retire_an_old_point_without_carrying_its_graph_links() {
    let mut fixture = fixture_with_candidate(
        "short_answer",
        serde_json::json!({
            "schema_version": 1,
            "reference_answer": "新的参考答案",
            "rubric_points": [{
                "order_index":0,
                "canonical_text":"完全新增的评分点",
                "max_score":1
            }],
        }),
    );
    let adopted = adopt_conflicts_as_new_version_with_mappings(
        &mut fixture.conn,
        fixture.batch_id,
        fixture.run_id,
        "teacher",
        &[RubricPointMappingInput {
            assessment_item_id: 1,
            candidate_order_index: 0,
            action: RubricPointMappingAction::NewPoint,
            previous_stable_id: None,
        }],
    )
    .unwrap();
    let adoption = adopted.adoption.unwrap();
    assert_eq!(adoption.new_rubric_point_count, 1);
    assert_eq!(adoption.retired_rubric_point_count, 1);
    assert_eq!(adoption.unlinked_new_rubric_point_count, 1);
    assert_eq!(adoption.carried_knowledge_link_count, 0);
    assert_eq!(adoption.carried_ability_link_count, 0);
    assert_eq!(adoption.dropped_knowledge_link_count, 1);
    assert_eq!(adoption.dropped_ability_link_count, 1);

    let (new_link_set_id, new_rubric_version_id): (i64, i64) = fixture
        .conn
        .query_row(
            "SELECT link_set_id,rubric_version_id FROM exam_assessment_items_v2
                 WHERE assessment_version_id=?1",
            [adoption.adopted_assessment_version_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let (knowledge_links, ability_links, new_points, mapping_rows): (i64, i64, i64, i64) = fixture
        .conn
        .query_row(
            "SELECT
                       (SELECT COUNT(*) FROM k1_knowledge_links WHERE link_set_id=?1),
                       (SELECT COUNT(*) FROM k1_ability_links WHERE link_set_id=?1),
                       (SELECT COUNT(*) FROM k1_rubric_points WHERE rubric_version_id=?2),
                       (SELECT COUNT(*) FROM exam_rubric_point_mappings_v2)",
            (new_link_set_id, new_rubric_version_id),
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (knowledge_links, ability_links, new_points, mapping_rows),
        (0, 0, 1, 2)
    );
    let actions = fixture
        .conn
        .prepare("SELECT mapping_action FROM exam_rubric_point_mappings_v2 ORDER BY mapping_action")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(actions, vec!["new_point", "retire_existing"]);
}

#[test]
fn fill_blank_adoption_preserves_slot_identity_and_scoring_rules() {
    let mut fixture = fixture_with_candidate(
        "fill_blank",
        serde_json::json!({
            "schema_version": 1,
            "slots": [{
                "order_index": 0,
                "canonical_answers": ["1842年"],
                "accepted_variants": ["一八四二年"]
            }]
        }),
    );
    fixture
        .conn
        .execute(
            "INSERT INTO k1_answer_slots
                 (public_id,stable_id,answer_key_version_id,order_index,
                  canonical_answers_json,normalization_rules_json,max_score,created_at)
                 VALUES ('old-slot-public','stable-slot',1,0,
                         '{\"schema_version\":1,\"answers\":[\"1842\"]}',
                         '{\"schema_version\":1,\"trim\":true}',1,'2026-07-15T10:00:00Z')",
            [],
        )
        .unwrap();
    let adopted = adopt_conflicts_as_new_version(
        &mut fixture.conn,
        fixture.batch_id,
        fixture.run_id,
        "teacher",
    )
    .unwrap();
    let new_version = adopted.adoption.unwrap().adopted_assessment_version_id;
    let (stable_id, canonical, normalization, max_score): (String, String, String, f64) = fixture
        .conn
        .query_row(
            "SELECT s.stable_id,s.canonical_answers_json,s.normalization_rules_json,s.max_score
                     FROM exam_assessment_items_v2 i
                     JOIN k1_answer_slots s ON s.answer_key_version_id=i.answer_key_version_id
                     WHERE i.assessment_version_id=?1",
            [new_version],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(stable_id, "stable-slot");
    assert_eq!(
        serde_json::from_str::<Value>(&canonical).unwrap(),
        serde_json::json!({
            "schema_version":1,
            "answers":["1842年"],
            "accepted_variants":["一八四二年"]
        })
    );
    assert_eq!(
        serde_json::from_str::<Value>(&normalization).unwrap(),
        serde_json::json!({"schema_version":1,"trim":true})
    );
    assert_eq!(max_score, 1.0);
}
