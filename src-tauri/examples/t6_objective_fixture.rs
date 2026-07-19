//! T6 客观题工作台的隔离业务夹具。
//!
//! 只允许写入 `jiaofu-t6-fixture-*` 根下、且 bundle id 为
//! `com.jiaofu.suite.t6fixture` 的全新数据目录。它通过正式 service API
//! 建立作业、页面、配准、题区和 observation，不复制终审/发布业务逻辑。

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use module_exam::service::assessment::{
    add_assessment_item, confirm_assessment_version, create_assessment_draft, create_attempt,
    NewAssessmentDraft, NewAssessmentItem,
};
use module_exam::service::objective::{
    list_objective_workbench, record_objective_observation, NewObjectiveObservation,
};
use module_exam::service::papers::{
    create_or_get_ingest_batch, decide_page_match, record_answer_region, record_page_alignment,
    record_page_quality, register_ingest_page, NewAnswerRegionRevision, NewIngestBatch,
    NewIngestPage, NewPageAlignmentRevision, NewPageMatchRevision, NewPageQualityRevision,
};
use module_knowledge::db::content::{
    add_ability_link, add_knowledge_link, create_answer_key_version, create_link_set,
    create_question, create_question_version, create_rubric_version, promote_question_version,
    NewAbilityLink, NewAnswerKeyVersion, NewKnowledgeLink, NewQuestion, NewQuestionVersion,
    NewRubricPoint, NewRubricVersion,
};
use module_knowledge::db::taxonomy::{
    create_ability_dimension, create_knowledge_map, create_knowledge_node, create_textbook_edition,
    NewAbilityDimension, NewKnowledgeMap, NewKnowledgeNode, NewTextbookEdition,
};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use suite_core::db::repo::artifacts::{create_or_get, NewArtifact};
use suite_core::db::repo::classes;
use suite_core::db::repo::students::{self, StudentInput};
use suite_core::domain::hashing;
use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

const FIXTURE_BUNDLE_ID: &str = "com.jiaofu.suite.t6fixture";
const FIXTURE_TITLE: &str = "T6 客观题隔离验收";
const FIXTURE_SCHEMA: i64 = 1;
const TEACHER: &str = "t6-fixture-teacher";

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Copy)]
struct Scenario {
    student_no: &'static str,
    student_name: &'static str,
    label: &'static str,
    result_state: &'static str,
    observed_answer_json: Option<&'static str>,
    confidence: Option<f64>,
    failure_meta_json: Option<&'static str>,
}

const SCENARIOS: [Scenario; 6] = [
    Scenario {
        student_no: "T6F001",
        student_name: "高置信正确",
        label: "HIGH / CORRECT / 99%",
        result_state: "recognized",
        observed_answer_json: Some(r#"{"schema_version":1,"selected":true}"#),
        confidence: Some(0.99),
        failure_meta_json: None,
    },
    Scenario {
        student_no: "T6F002",
        student_name: "高置信错误",
        label: "HIGH / INCORRECT / 98%",
        result_state: "recognized",
        observed_answer_json: Some(r#"{"schema_version":1,"selected":false}"#),
        confidence: Some(0.98),
        failure_meta_json: None,
    },
    Scenario {
        student_no: "T6F003",
        student_name: "低置信答案",
        label: "LOW CONFIDENCE / 72%",
        result_state: "low_confidence",
        observed_answer_json: Some(r#"{"schema_version":1,"selected":true}"#),
        confidence: Some(0.72),
        failure_meta_json: None,
    },
    Scenario {
        student_no: "T6F004",
        student_name: "空白答案",
        label: "BLANK ANSWER",
        result_state: "blank",
        observed_answer_json: None,
        confidence: None,
        failure_meta_json: None,
    },
    Scenario {
        student_no: "T6F005",
        student_name: "涂改答案",
        label: "ALTERED ANSWER / 88%",
        result_state: "altered",
        observed_answer_json: Some(r#"{"schema_version":1,"selected":true}"#),
        confidence: Some(0.88),
        failure_meta_json: None,
    },
    Scenario {
        student_no: "T6F006",
        student_name: "识别失败",
        label: "RECOGNITION FAILED",
        result_state: "failed",
        observed_answer_json: None,
        confidence: None,
        failure_meta_json: Some(
            r#"{"schema_version":1,"error_code":"FIXTURE_RECOGNITION_FAILED","error_message":"isolated acceptance fixture","retryable":true}"#,
        ),
    },
];

#[derive(Debug, Serialize)]
struct FixtureReport {
    schema_version: i64,
    phase: String,
    data_dir: String,
    migration_count: i64,
    integrity_check: String,
    foreign_key_violations: i64,
    workbench_rows: usize,
    attempts: usize,
    batch_eligible: usize,
    confirmed_rows: usize,
    observation_states: BTreeMap<String, i64>,
    attempt_states: BTreeMap<String, i64>,
    active_grade_decisions: i64,
    teacher_corrected_decisions: i64,
    strict_batch_decisions: i64,
    published_publications: i64,
    active_learning_evidence: i64,
    missing_artifact_files: i64,
}

fn invalid(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

fn validate_fixture_data_dir(data_dir: &Path) -> AppResult<()> {
    if !data_dir.is_absolute() {
        return Err(invalid("--data-dir 必须是绝对路径"));
    }
    if data_dir.file_name().and_then(|value| value.to_str()) != Some(FIXTURE_BUNDLE_ID) {
        return Err(invalid(format!("隔离目录末级必须是 {FIXTURE_BUNDLE_ID}")));
    }
    let has_isolated_root = data_dir.components().any(|component| match component {
        Component::Normal(value) => value
            .to_str()
            .is_some_and(|value| value.starts_with("jiaofu-t6-fixture-")),
        _ => false,
    });
    if !has_isolated_root {
        return Err(invalid(
            "隔离目录必须位于名称以 jiaofu-t6-fixture- 开头的根目录下",
        ));
    }
    Ok(())
}

fn run_all_migrations(conn: &Connection) -> AppResult<()> {
    suite_core::db::run_migrations(conn, suite_core::db::CORE_MIGRATIONS)?;
    suite_core::db::run_migrations(conn, module_knowledge::knowledge_migrations())?;
    suite_core::db::run_migrations(conn, module_recitation::recitation_migrations())?;
    suite_core::db::run_migrations(conn, module_exam::exam_migrations())?;
    Ok(())
}

fn write_svg(path: &Path, title: &str, subtitle: &str, color: &str) -> AppResult<Vec<u8>> {
    let body = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="900" height="240" viewBox="0 0 900 240">
<rect width="900" height="240" rx="24" fill="#f8f7f2"/>
<rect x="18" y="18" width="12" height="204" rx="6" fill="{color}"/>
<text x="60" y="100" font-family="Arial, sans-serif" font-size="42" font-weight="700" fill="#172019">{title}</text>
<text x="60" y="160" font-family="Arial, sans-serif" font-size="28" fill="#526057">{subtitle}</text>
<rect x="720" y="70" width="110" height="100" rx="16" fill="none" stroke="{color}" stroke-width="8"/>
</svg>"##
    );
    let bytes = body.into_bytes();
    fs::write(path, &bytes)?;
    Ok(bytes)
}

fn artifact(
    conn: &Connection,
    archive_dir: &Path,
    scenario: Scenario,
    kind: ArtifactKind,
    suffix: &str,
    parent_artifact_id: Option<i64>,
    derivative_type: Option<&str>,
) -> AppResult<i64> {
    let path = archive_dir.join(format!("{}-{suffix}.svg", scenario.student_no));
    let color = match scenario.result_state {
        "recognized" => "#228b57",
        "low_confidence" => "#c28b16",
        "blank" => "#8b8f93",
        "altered" => "#d56c2a",
        _ => "#b63838",
    };
    let bytes = write_svg(&path, scenario.student_no, scenario.label, color)?;
    let archived_path = path
        .to_str()
        .ok_or_else(|| invalid("夹具归档路径不是有效 UTF-8"))?;
    let original_name =
        (parent_artifact_id.is_none()).then(|| format!("{}-page.svg", scenario.student_no));
    Ok(create_or_get(
        conn,
        &NewArtifact {
            kind,
            sha256: &hashing::sha256_hex(&bytes),
            mime_type: "image/svg+xml",
            byte_size: bytes.len() as i64,
            original_name: original_name.as_deref(),
            original_path: None,
            archived_path,
            parent_artifact_id,
            derivative_type,
            processing_version: match suffix {
                "source" => "t6-objective-fixture-source-v1",
                "aligned" => "t6-objective-fixture-alignment-v1",
                _ => "t6-objective-fixture-crop-v1",
            },
            privacy_class: PrivacyClass::StudentSensitive,
            archive_status: ArchiveStatus::Ready,
        },
    )?
    .id)
}

fn seed_fixture(data_dir: &Path) -> AppResult<FixtureReport> {
    validate_fixture_data_dir(data_dir)?;
    let db_path = data_dir.join("data.db");
    if db_path.exists() {
        return Err(invalid(format!(
            "拒绝覆盖已有数据库：{}",
            db_path.display()
        )));
    }
    if data_dir.exists() && fs::read_dir(data_dir)?.next().is_some() {
        return Err(invalid(format!(
            "seed 只接受全新空目录：{}",
            data_dir.display()
        )));
    }
    let archive_dir = data_dir.join("archive/t6-objective-fixture");
    fs::create_dir_all(&archive_dir)?;

    let conn = suite_core::db::open(&db_path)?;
    run_all_migrations(&conn)?;
    conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])?;
    let subject_id = conn.last_insert_rowid();
    let class = classes::create(
        &conn,
        "T6 隔离验收班",
        Some("人教版八年级上册"),
        Some("2026秋"),
    )?;
    let students = SCENARIOS
        .iter()
        .map(|scenario| {
            students::upsert(
                &conn,
                &StudentInput {
                    student_no: scenario.student_no,
                    name: scenario.student_name,
                    class_id: Some(class.id),
                    enabled: true,
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;

    let edition = create_textbook_edition(
        &conn,
        &NewTextbookEdition {
            subject_id,
            publisher_code: "PEP",
            edition_code: "2024",
            title: "中国历史八年级上册",
            grade: "8",
            volume: "upper",
            curriculum_region: Some("CN"),
        },
    )?;
    let knowledge_map = create_knowledge_map(
        &conn,
        &NewKnowledgeMap {
            textbook_edition_id: edition.id,
            revision: 1,
            state: "confirmed",
            supersedes_map_id: None,
        },
    )?;
    let knowledge_node = create_knowledge_node(
        &conn,
        &NewKnowledgeNode {
            stable_id: Some("t6.fixture.opium-war.year"),
            knowledge_map_id: knowledge_map.id,
            curriculum_node_id: None,
            parent_id: None,
            code: Some("T6-FIXTURE-K1"),
            title: "鸦片战争爆发时间",
            description: Some("隔离验收知识节点"),
            order_index: 1,
        },
    )?;
    let ability = create_ability_dimension(
        &conn,
        &NewAbilityDimension {
            stable_id: Some("t6.fixture.fact-recall"),
            subject_id,
            revision: 1,
            code: "fact_recall",
            title: "事实识记与提取",
            description: Some("隔离验收能力节点"),
            supersedes_dimension_id: None,
        },
    )?;
    let question = create_question(
        &conn,
        &NewQuestion {
            owner_scope: "personal",
            owner_id: TEACHER,
            question_family_id: None,
            rights_status: "unknown",
            sharing_allowed: false,
        },
    )?;
    let question_version = create_question_version(
        &conn,
        &NewQuestionVersion {
            question_id: question.id,
            revision: 1,
            question_type: "true_false",
            stem: "鸦片战争爆发于1840年。",
            material_text: None,
            max_score: 1.0,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "L0",
            state: "draft",
            options: &[],
        },
    )?;
    let answer_key = create_answer_key_version(
        &conn,
        &NewAnswerKeyVersion {
            question_version_id: question_version.id,
            revision: 1,
            answer_json: r#"{"schema_version":1,"correct":true}"#,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_answer_key_id: None,
            slots: &[],
        },
    )?;
    let rubric_points = [NewRubricPoint {
        stable_id: Some("t6.fixture.point.correct"),
        order_index: 0,
        canonical_text: "判断作答与标准答案一致",
        allowed_paraphrases_json: None,
        required_concepts_json: None,
        max_score: 1.0,
    }];
    let rubric = create_rubric_version(
        &conn,
        &NewRubricVersion {
            question_version_id: question_version.id,
            revision: 1,
            max_score: 1.0,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_rubric_id: None,
            points: &rubric_points,
        },
    )?;
    let link_set = create_link_set(
        &conn,
        question_version.id,
        knowledge_map.id,
        1,
        "confirmed",
        Some(TEACHER),
        None,
    )?;
    add_knowledge_link(
        &conn,
        &NewKnowledgeLink {
            link_set_id: link_set.id,
            source_type: "question",
            source_public_id: &question_version.public_id,
            knowledge_node_id: knowledge_node.id,
            relation_type: "direct_assessment",
            confirmation_level: "teacher_confirmed",
            verified_by: Some(TEACHER),
        },
    )?;
    add_ability_link(
        &conn,
        &NewAbilityLink {
            link_set_id: link_set.id,
            source_type: "question",
            source_public_id: &question_version.public_id,
            ability_dimension_id: ability.id,
            evidence_strength: 0.6,
            response_mode: "recognition",
            confirmation_level: "teacher_confirmed",
            verified_by: Some(TEACHER),
        },
    )?;
    promote_question_version(
        &conn,
        question_version.id,
        "L3",
        TEACHER,
        Some("T6 隔离验收需要正式批改与图谱证据"),
    )?;

    let assessment = create_assessment_draft(
        &conn,
        &NewAssessmentDraft {
            title: FIXTURE_TITLE,
            class_id: class.id,
            assessment_context: "quiz",
            evidence_policy: "include",
            created_by: TEACHER,
            template_version: Some("t6-objective-fixture-v1"),
        },
    )?;
    let item = add_assessment_item(
        &conn,
        assessment.assessment_version_id,
        &NewAssessmentItem {
            question_version_id: question_version.id,
            answer_key_version_id: answer_key.id,
            rubric_version_id: rubric.id,
            link_set_id: link_set.id,
            order_index: 1,
            score: 1.0,
            option_order_json: None,
            presentation_snapshot_json: r#"{"schema_version":1,"question_no":"1"}"#,
        },
    )?;
    confirm_assessment_version(&conn, assessment.assessment_version_id, TEACHER)?;
    let batch = create_or_get_ingest_batch(
        &conn,
        &NewIngestBatch {
            assessment_version_id: assessment.assessment_version_id,
            source_kind: "fixed_fixture",
            idempotency_key: "t6-objective-fixture-batch-v1",
            created_by: TEACHER,
        },
    )?;

    for (index, (scenario, student)) in SCENARIOS.iter().zip(students.iter()).enumerate() {
        let attempt = create_attempt(
            &conn,
            assessment.assessment_version_id,
            student.id,
            "image",
            "first",
        )?;
        let page_artifact = artifact(
            &conn,
            &archive_dir,
            *scenario,
            ArtifactKind::Page,
            "source",
            None,
            None,
        )?;
        let aligned_artifact = artifact(
            &conn,
            &archive_dir,
            *scenario,
            ArtifactKind::Page,
            "aligned",
            Some(page_artifact),
            Some("page_alignment"),
        )?;
        let crop_artifact = artifact(
            &conn,
            &archive_dir,
            *scenario,
            ArtifactKind::Crop,
            "crop",
            Some(aligned_artifact),
            Some("answer_region"),
        )?;
        let page = register_ingest_page(
            &conn,
            &NewIngestPage {
                batch_id: batch.id,
                source_artifact_id: page_artifact,
                import_index: index as i64,
                expected_page_no: Some(1),
            },
        )?;
        record_page_quality(
            &conn,
            &NewPageQualityRevision {
                page_id: page.id,
                blur_score: 0.02,
                glare_score: 0.01,
                brightness_score: 0.95,
                perspective_score: 0.98,
                rotation_degrees: 0.0,
                crop_complete: true,
                result: "pass",
                issue_codes_json: r#"{"schema_version":1,"codes":[]}"#,
                checked_by_type: "fixture",
                checked_by: None,
            },
        )?;
        decide_page_match(
            &conn,
            &NewPageMatchRevision {
                page_id: page.id,
                attempt_id: Some(attempt.id),
                page_no: Some(1),
                student_confidence: Some(0.99),
                page_no_confidence: Some(0.99),
                template_confidence: Some(0.99),
                decision: "teacher_confirmed",
                reason_code: None,
                confirmed_by: Some(TEACHER),
            },
        )?;
        record_page_alignment(
            &conn,
            &NewPageAlignmentRevision {
                page_id: page.id,
                template_version: "t6-objective-fixture-v1",
                transform_json: r#"{"schema_version":1,"matrix":[1,0,0,0,1,0,0,0,1]}"#,
                confidence: 0.99,
                aligned_artifact_id: Some(aligned_artifact),
                decision: "teacher_confirmed",
                reason_code: None,
                confirmed_by: Some(TEACHER),
            },
        )?;
        let region = record_answer_region(
            &conn,
            &NewAnswerRegionRevision {
                page_id: page.id,
                assessment_item_id: item.id,
                region_index: 0,
                bbox_json: r#"{"schema_version":1,"x":0.1,"y":0.1,"width":0.8,"height":0.2}"#,
                crop_artifact_id: Some(crop_artifact),
                mapping_confidence: Some(0.99),
                decision: "teacher_confirmed",
                reason_code: None,
                confirmed_by: Some(TEACHER),
            },
        )?;
        record_objective_observation(
            &conn,
            &NewObjectiveObservation {
                answer_region_revision_id: region.id,
                source_kind: "fixed_fixture",
                result_state: scenario.result_state,
                observed_answer_json: scenario.observed_answer_json,
                confidence: scenario.confidence,
                ai_run_id: None,
                failure_meta_json: scenario.failure_meta_json,
                idempotency_key: &format!("t6-objective-{}-v1", scenario.student_no),
            },
        )?;
    }

    fs::write(
        data_dir.join("t6-objective-fixture.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": FIXTURE_SCHEMA,
            "bundle_id": FIXTURE_BUNDLE_ID,
            "assessment_title": FIXTURE_TITLE,
            "assessment_version_id": assessment.assessment_version_id,
            "scenario_count": SCENARIOS.len()
        }))?,
    )?;
    drop(conn);
    verify_fixture(data_dir, "seeded")
}

fn count(conn: &Connection, sql: &str) -> AppResult<i64> {
    Ok(conn.query_row(sql, [], |row| row.get(0))?)
}

fn grouped_counts(conn: &Connection, sql: &str) -> AppResult<BTreeMap<String, i64>> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<Result<BTreeMap<String, i64>, _>>()?)
}

fn inspect_fixture(data_dir: &Path, phase: &str) -> AppResult<FixtureReport> {
    validate_fixture_data_dir(data_dir)?;
    if !data_dir.join("t6-objective-fixture.json").is_file() {
        return Err(invalid(
            "缺少 t6-objective-fixture.json，拒绝检查未知数据库",
        ));
    }
    let db_path = data_dir.join("data.db");
    // FTS5 的 integrity_check 需要可写句柄；路径已由隔离夹具目录和 marker 守卫，
    // 并且不授予 SQLITE_OPEN_CREATE。
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let workbench = list_objective_workbench(&conn, None, 500)?;
    let integrity_check: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    let foreign_key_violations = count(&conn, "SELECT COUNT(*) FROM pragma_foreign_key_check")?;
    let missing_artifact_files = {
        let mut statement = conn.prepare(
            "SELECT archived_path FROM artifacts WHERE processing_version LIKE 't6-objective-fixture-%'",
        )?;
        let paths = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut missing = 0;
        for path in paths {
            if !Path::new(&path?).is_file() {
                missing += 1;
            }
        }
        missing
    };
    Ok(FixtureReport {
        schema_version: FIXTURE_SCHEMA,
        phase: phase.to_owned(),
        data_dir: data_dir.display().to_string(),
        migration_count: count(&conn, "SELECT COUNT(*) FROM schema_migrations")?,
        integrity_check,
        foreign_key_violations,
        workbench_rows: workbench.rows.len(),
        attempts: workbench.attempts.len(),
        batch_eligible: workbench.rows.iter().filter(|row| row.batch_eligible).count(),
        confirmed_rows: workbench
            .rows
            .iter()
            .filter(|row| row.current_suggestion_confirmed)
            .count(),
        observation_states: grouped_counts(
            &conn,
            "SELECT result_state,COUNT(*) FROM exam_objective_observation_revisions_v2 WHERE state='active' GROUP BY result_state ORDER BY result_state",
        )?,
        attempt_states: grouped_counts(
            &conn,
            "SELECT state,COUNT(*) FROM exam_attempts_v2 GROUP BY state ORDER BY state",
        )?,
        active_grade_decisions: count(
            &conn,
            "SELECT COUNT(*) FROM exam_grade_decisions_v2 WHERE state='active'",
        )?,
        teacher_corrected_decisions: count(
            &conn,
            "SELECT COUNT(*) FROM exam_grade_decisions_v2 WHERE state='active' AND confirmation_level='teacher_corrected'",
        )?,
        strict_batch_decisions: count(
            &conn,
            "SELECT COUNT(*) FROM exam_grade_decisions_v2 d JOIN exam_grade_decision_objective_sources_v2 s ON s.grade_decision_id=d.id WHERE d.state='active' AND s.review_mode='strict_batch'",
        )?,
        published_publications: count(
            &conn,
            "SELECT COUNT(*) FROM exam_grade_publications_v2 WHERE state='published'",
        )?,
        active_learning_evidence: count(
            &conn,
            "SELECT COUNT(*) FROM learning_evidence WHERE state='active' AND source_module='grading'",
        )?,
        missing_artifact_files,
    })
}

fn expect(condition: bool, message: impl Into<String>) -> AppResult<()> {
    if condition {
        Ok(())
    } else {
        Err(invalid(message))
    }
}

fn verify_fixture(data_dir: &Path, phase: &str) -> AppResult<FixtureReport> {
    let report = inspect_fixture(data_dir, phase)?;
    let expected_migrations = (suite_core::db::CORE_MIGRATIONS.len()
        + module_knowledge::knowledge_migrations().len()
        + module_recitation::recitation_migrations().len()
        + module_exam::exam_migrations().len()) as i64;
    expect(
        report.migration_count == expected_migrations,
        format!("夹具必须包含全部 {expected_migrations} 个迁移"),
    )?;
    expect(
        report.integrity_check == "ok",
        format!(
            "integrity_check 必须为 ok，实际为：{}",
            report.integrity_check
        ),
    )?;
    expect(report.foreign_key_violations == 0, "夹具不能包含外键违规")?;
    expect(report.workbench_rows == 6, "工作台必须显示 6 条题区证据")?;
    expect(report.attempts == 6, "工作台必须显示 6 份整卷")?;
    expect(report.batch_eligible == 2, "必须恰有 2 条严格批量候选")?;
    expect(
        report.missing_artifact_files == 0,
        "所有夹具 artifact 必须真实可读",
    )?;
    let expected_observations = BTreeMap::from([
        ("altered".to_owned(), 1),
        ("blank".to_owned(), 1),
        ("failed".to_owned(), 1),
        ("low_confidence".to_owned(), 1),
        ("recognized".to_owned(), 2),
    ]);
    expect(
        report.observation_states == expected_observations,
        "观察状态分布不符合固定夹具",
    )?;
    match phase {
        "seeded" => {
            expect(report.confirmed_rows == 0, "seeded 阶段不能有老师终审")?;
            expect(
                report.active_grade_decisions == 0,
                "seeded 阶段不能有评分 revision",
            )?;
            expect(
                report.published_publications == 0 && report.active_learning_evidence == 0,
                "seeded 阶段不能发布或生成正式证据",
            )?;
            expect(
                report.attempt_states == BTreeMap::from([("ingesting".to_owned(), 6)]),
                "seeded 阶段 attempt 应全部为 ingesting",
            )?;
        }
        "reviewed" => {
            expect(report.confirmed_rows == 6, "reviewed 阶段必须完成 6 条终审")?;
            expect(
                report.active_grade_decisions == 6
                    && report.teacher_corrected_decisions == 4
                    && report.strict_batch_decisions == 2,
                "reviewed 阶段必须是 2 条严格批量 + 4 条人工修正",
            )?;
            expect(
                report.published_publications == 0 && report.active_learning_evidence == 0,
                "终审完成仍不能自动发布或生成正式证据",
            )?;
            expect(
                report.attempt_states == BTreeMap::from([("ready_to_publish".to_owned(), 6)]),
                "reviewed 阶段 attempt 应全部待发布",
            )?;
        }
        "published" => {
            expect(
                report.confirmed_rows == 6,
                "published 阶段必须保留 6 条终审",
            )?;
            expect(
                report.active_grade_decisions == 6
                    && report.teacher_corrected_decisions == 4
                    && report.strict_batch_decisions == 2,
                "published 阶段评分来源分布必须保持不变",
            )?;
            expect(report.published_publications == 6, "必须显式发布 6 份整卷")?;
            expect(
                report.active_learning_evidence == 12,
                "每份整卷应生成 1 条知识 + 1 条能力正式证据",
            )?;
            expect(
                report.attempt_states == BTreeMap::from([("published".to_owned(), 6)]),
                "published 阶段 attempt 应全部已发布",
            )?;
        }
        _ => return Err(invalid("--phase 只能是 seeded、reviewed 或 published")),
    }
    Ok(report)
}

fn parse_args() -> AppResult<(String, PathBuf, String)> {
    let mut args = std::env::args().skip(1);
    let command = args
        .next()
        .ok_or_else(|| invalid("用法：t6_objective_fixture <seed|verify> --data-dir <绝对路径> [--phase seeded|reviewed|published]"))?;
    let mut data_dir = None;
    let mut phase = "seeded".to_owned();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--phase" => phase = args.next().ok_or_else(|| invalid("--phase 缺少值"))?,
            _ => return Err(invalid(format!("未知参数：{argument}"))),
        }
    }
    let data_dir = data_dir.ok_or_else(|| invalid("缺少 --data-dir"))?;
    Ok((command, data_dir, phase))
}

fn main() -> AppResult<()> {
    let (command, data_dir, phase) = parse_args()?;
    let report = match command.as_str() {
        "seed" => seed_fixture(&data_dir)?,
        "verify" => verify_fixture(&data_dir, &phase)?,
        _ => return Err(invalid("首个参数只能是 seed 或 verify")),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_rejects_production_data_dir() {
        assert!(validate_fixture_data_dir(Path::new(
            "/Users/teacher/Library/Application Support/com.jiaofu.suite"
        ))
        .is_err());
    }

    #[test]
    fn seeded_fixture_is_complete_and_has_no_teacher_effects() {
        let unique = format!(
            "jiaofu-t6-fixture-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let data_dir = root
            .join("Library/Application Support")
            .join(FIXTURE_BUNDLE_ID);
        let report = seed_fixture(&data_dir).unwrap();
        assert_eq!(report.workbench_rows, 6);
        assert_eq!(report.confirmed_rows, 0);
        fs::remove_dir_all(root).unwrap();
    }
}
