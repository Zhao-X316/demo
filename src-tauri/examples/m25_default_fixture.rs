//! M2.5 未来作业默认版本升级的隔离真机验收夹具。
//!
//! 夹具只允许写入 `jiaofu-m25-fixture-*` 根下、且 bundle id 为
//! `com.jiaofu.suite.m25fixture` 的全新数据目录。seed 通过正式 service
//! 建立题目、两版答案/rubric/link 和一份已发布历史作业；future_only 影响计划与
//! 默认版本升级留给真实 `.app` 的 Tauri IPC，verify 只读检查持久化和历史不变式。

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use module_exam::service::assessment::{
    add_assessment_item, confirm_assessment_version, create_assessment_draft, create_attempt,
    decide_grade, publish_attempt, NewAssessmentDraft, NewAssessmentItem, NewGradeDecision,
};
#[cfg(test)]
use module_exam::service::question_performance::{
    confirm_question_impact_plan, upgrade_assessment_default_from_impact,
    ConfirmQuestionImpactPlanRequest, UpgradeAssessmentDefaultRequest,
};
use module_exam::service::question_performance::{
    list_question_performance, preview_question_version_impact,
};
use module_knowledge::db::content::{
    add_ability_link, add_knowledge_link, create_answer_key_version, create_link_set,
    create_question, create_question_version, create_rubric_version, promote_question_version,
    NewAbilityLink, NewAnswerKeyVersion, NewKnowledgeLink, NewQuestion, NewQuestionOption,
    NewQuestionVersion, NewRubricPoint, NewRubricVersion,
};
use module_knowledge::db::taxonomy::{
    create_ability_dimension, create_knowledge_map, create_knowledge_node, create_textbook_edition,
    NewAbilityDimension, NewKnowledgeMap, NewKnowledgeNode, NewTextbookEdition,
};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use suite_core::db::repo::classes;
use suite_core::db::repo::students::{self, StudentInput};

const FIXTURE_BUNDLE_ID: &str = "com.jiaofu.suite.m25fixture";
const FIXTURE_SCHEMA: i64 = 1;
const FIXTURE_MARKER: &str = "m25-default-fixture.json";
const TEACHER: &str = "local_teacher";
const ASSESSMENT_TITLE: &str = "第一单元默认换版验收";
const QUESTION_STEM: &str = "鸦片战争爆发于哪一年？";

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Serialize)]
struct FixtureReport {
    schema_version: i64,
    phase: String,
    data_dir: String,
    migration_count: i64,
    integrity_check: String,
    foreign_key_violations: i64,
    performance_items: usize,
    impact_plans: i64,
    assessment_versions: i64,
    assessment_items: i64,
    default_selections: i64,
    current_default_revision: i64,
    current_default_is_target: bool,
    historical_attempts: i64,
    historical_grade_decisions: i64,
    historical_publications: i64,
    historical_active_learning_evidence: i64,
    upgrade_audit_events: i64,
    upgrade_outbox_events: i64,
}

struct SeededIds {
    question_version_public_id: String,
    source_assessment_version_public_id: String,
}

fn invalid(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

fn expect(condition: bool, message: impl Into<String>) -> AppResult<()> {
    if condition {
        Ok(())
    } else {
        Err(invalid(message))
    }
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
            .is_some_and(|value| value.starts_with("jiaofu-m25-fixture-")),
        _ => false,
    });
    if !has_isolated_root {
        return Err(invalid(
            "隔离目录必须位于名称以 jiaofu-m25-fixture- 开头的根目录下",
        ));
    }
    Ok(())
}

fn run_all_migrations(conn: &Connection) -> AppResult<()> {
    suite_core::db::run_migrations(conn, suite_core::db::CORE_MIGRATIONS)?;
    suite_core::db::run_migrations(conn, module_knowledge::knowledge_migrations())?;
    suite_core::db::run_migrations(conn, module_recitation::recitation_migrations())?;
    suite_core::db::run_migrations(conn, module_exam::exam_migrations())?;
    suite_core::db::run_migrations(conn, module_wrongbook::wrongbook_migrations())?;
    suite_core::db::run_migrations(conn, module_profile::profile_migrations())?;
    Ok(())
}

fn add_confirmed_links(
    conn: &Connection,
    link_set_id: i64,
    question_version_public_id: &str,
    knowledge_node_id: i64,
    ability_dimension_id: i64,
) -> AppResult<()> {
    add_knowledge_link(
        conn,
        &NewKnowledgeLink {
            link_set_id,
            source_type: "question",
            source_public_id: question_version_public_id,
            knowledge_node_id,
            relation_type: "direct_assessment",
            confirmation_level: "teacher_confirmed",
            verified_by: Some(TEACHER),
        },
    )?;
    add_ability_link(
        conn,
        &NewAbilityLink {
            link_set_id,
            source_type: "question",
            source_public_id: question_version_public_id,
            ability_dimension_id,
            evidence_strength: 0.7,
            response_mode: "recognition",
            confirmation_level: "teacher_confirmed",
            verified_by: Some(TEACHER),
        },
    )?;
    Ok(())
}

fn seed_business(conn: &mut Connection) -> AppResult<SeededIds> {
    conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])?;
    let subject_id = conn.last_insert_rowid();
    let class = classes::create(
        conn,
        "M2.5 默认换版隔离班",
        Some("人教版八年级上册"),
        Some("2026秋"),
    )?;
    let student = students::upsert(
        conn,
        &StudentInput {
            student_no: "M25001",
            name: "历史哨兵学生",
            class_id: Some(class.id),
            enabled: true,
        },
    )?;
    let edition = create_textbook_edition(
        conn,
        &NewTextbookEdition {
            subject_id,
            publisher_code: "pep",
            edition_code: "2026",
            title: "中国历史八年级上册",
            grade: "八年级",
            volume: "upper",
            curriculum_region: None,
        },
    )?;
    let knowledge_map = create_knowledge_map(
        conn,
        &NewKnowledgeMap {
            textbook_edition_id: edition.id,
            revision: 1,
            state: "confirmed",
            supersedes_map_id: None,
        },
    )?;
    let knowledge_node = create_knowledge_node(
        conn,
        &NewKnowledgeNode {
            stable_id: Some("m25-opium-war-year"),
            knowledge_map_id: knowledge_map.id,
            curriculum_node_id: None,
            parent_id: None,
            code: Some("M25-K01"),
            title: "鸦片战争爆发时间",
            description: Some("隔离验收知识点"),
            order_index: 0,
        },
    )?;
    let ability_dimension = create_ability_dimension(
        conn,
        &NewAbilityDimension {
            stable_id: Some("m25-fact-recall"),
            subject_id,
            revision: 1,
            code: "fact_recall",
            title: "史实识记与提取",
            description: Some("隔离验收能力维度"),
            supersedes_dimension_id: None,
        },
    )?;

    let question = create_question(
        conn,
        &NewQuestion {
            owner_scope: "personal",
            owner_id: TEACHER,
            question_family_id: None,
            rights_status: "cleared",
            sharing_allowed: false,
        },
    )?;
    let question_version = create_question_version(
        conn,
        &NewQuestionVersion {
            question_id: question.id,
            revision: 1,
            question_type: "single",
            stem: QUESTION_STEM,
            material_text: None,
            max_score: 2.0,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "L0",
            state: "draft",
            options: &[
                NewQuestionOption {
                    label: "A",
                    content: "1840年",
                    order_index: 0,
                },
                NewQuestionOption {
                    label: "B",
                    content: "1842年",
                    order_index: 1,
                },
            ],
        },
    )?;
    let old_answer = create_answer_key_version(
        conn,
        &NewAnswerKeyVersion {
            question_version_id: question_version.id,
            revision: 1,
            answer_json: r#"{"schema_version":1,"correct_labels":["A"]}"#,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_answer_key_id: None,
            slots: &[],
        },
    )?;
    let old_rubric = create_rubric_version(
        conn,
        &NewRubricVersion {
            question_version_id: question_version.id,
            revision: 1,
            max_score: 2.0,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_rubric_id: None,
            points: &[NewRubricPoint {
                stable_id: Some("m25-year"),
                order_index: 0,
                canonical_text: "选择旧版标准答案",
                allowed_paraphrases_json: None,
                required_concepts_json: None,
                max_score: 2.0,
            }],
        },
    )?;
    let old_link = create_link_set(
        conn,
        question_version.id,
        knowledge_map.id,
        1,
        "confirmed",
        Some(TEACHER),
        None,
    )?;
    add_confirmed_links(
        conn,
        old_link.id,
        &question_version.public_id,
        knowledge_node.id,
        ability_dimension.id,
    )?;
    promote_question_version(
        conn,
        question_version.id,
        "L3",
        TEACHER,
        Some("M2.5 隔离验收题"),
    )?;

    let assessment = create_assessment_draft(
        conn,
        &NewAssessmentDraft {
            title: ASSESSMENT_TITLE,
            class_id: class.id,
            assessment_context: "quiz",
            evidence_policy: "include",
            created_by: TEACHER,
            template_version: Some("m25-default-fixture-v1"),
        },
    )?;
    let assessment_item = add_assessment_item(
        conn,
        assessment.assessment_version_id,
        &NewAssessmentItem {
            question_version_id: question_version.id,
            answer_key_version_id: old_answer.id,
            rubric_version_id: old_rubric.id,
            link_set_id: old_link.id,
            order_index: 0,
            score: 2.0,
            option_order_json: Some(r#"{"schema_version":1,"labels":["A","B"]}"#),
            presentation_snapshot_json: r#"{"schema_version":1,"fixture":"m25-default"}"#,
        },
    )?;
    confirm_assessment_version(conn, assessment.assessment_version_id, TEACHER)?;

    let attempt = create_attempt(
        conn,
        assessment.assessment_version_id,
        student.id,
        "image",
        "first",
    )?;
    decide_grade(
        conn,
        &NewGradeDecision {
            attempt_id: attempt.id,
            assessment_item_id: assessment_item.id,
            machine_grade_ai_run_id: None,
            teacher_score: 2.0,
            point_results_json: r#"{"schema_version":1,"fixture":"historical-sentinel"}"#,
            teacher_note: Some("升级未来默认版本前的正式历史成绩"),
            confirmation_level: "teacher_accepted",
            decided_by: TEACHER,
        },
    )?;
    publish_attempt(conn, attempt.id, TEACHER)?;

    let new_answer = create_answer_key_version(
        conn,
        &NewAnswerKeyVersion {
            question_version_id: question_version.id,
            revision: 2,
            answer_json: r#"{"schema_version":1,"correct_labels":["B"]}"#,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_answer_key_id: Some(old_answer.id),
            slots: &[],
        },
    )?;
    let new_rubric = create_rubric_version(
        conn,
        &NewRubricVersion {
            question_version_id: question_version.id,
            revision: 2,
            max_score: 2.0,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_rubric_id: Some(old_rubric.id),
            points: &[NewRubricPoint {
                stable_id: Some("m25-year"),
                order_index: 0,
                canonical_text: "选择老师修订后的标准答案",
                allowed_paraphrases_json: None,
                required_concepts_json: None,
                max_score: 2.0,
            }],
        },
    )?;
    let new_link = create_link_set(
        conn,
        question_version.id,
        knowledge_map.id,
        2,
        "confirmed",
        Some(TEACHER),
        Some(old_link.id),
    )?;
    add_confirmed_links(
        conn,
        new_link.id,
        &question_version.public_id,
        knowledge_node.id,
        ability_dimension.id,
    )?;
    expect(
        new_answer.id != old_answer.id,
        "答案版本必须形成新 revision",
    )?;
    expect(new_rubric.id != old_rubric.id, "rubric 必须形成新 revision")?;

    let preview = preview_question_version_impact(conn, TEACHER, &question_version.public_id)?;
    expect(preview.affected_assessment_count == 1, "必须影响一份作业")?;
    expect(
        preview.published_attempt_count == 1,
        "必须保留一份已发布历史作答",
    )?;
    expect(
        preview.active_learning_evidence_count > 0,
        "历史发布必须已经形成正式学习证据",
    )?;
    Ok(SeededIds {
        question_version_public_id: question_version.public_id,
        source_assessment_version_public_id: assessment.assessment_version_public_id,
    })
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
    fs::create_dir_all(data_dir)?;
    let mut conn = suite_core::db::open(&db_path)?;
    run_all_migrations(&conn)?;
    let ids = seed_business(&mut conn)?;
    expect(
        !ids.question_version_public_id.is_empty()
            && !ids.source_assessment_version_public_id.is_empty(),
        "夹具业务标识不能为空",
    )?;
    drop(conn);
    fs::write(
        data_dir.join(FIXTURE_MARKER),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": FIXTURE_SCHEMA,
            "bundle_id": FIXTURE_BUNDLE_ID,
            "question_version_public_id": ids.question_version_public_id,
            "source_assessment_version_public_id": ids.source_assessment_version_public_id
        }))?,
    )?;
    verify_fixture(data_dir, "seeded")
}

fn scalar_i64(conn: &Connection, sql: &str) -> AppResult<i64> {
    Ok(conn.query_row(sql, [], |row| row.get(0))?)
}

fn inspect_fixture(data_dir: &Path, phase: &str) -> AppResult<FixtureReport> {
    validate_fixture_data_dir(data_dir)?;
    let db_path = data_dir.join("data.db");
    if !db_path.is_file() {
        return Err(invalid(format!("未找到夹具数据库：{}", db_path.display())));
    }
    if !data_dir.join(FIXTURE_MARKER).is_file() {
        return Err(invalid(format!(
            "缺少 {FIXTURE_MARKER}，拒绝检查未知数据库"
        )));
    }
    // FTS5 的 integrity_check 需要可写句柄；路径和 marker 已把范围锁定在隔离夹具，
    // 且这里不授予 SQLITE_OPEN_CREATE。
    let conn = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let integrity_check: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    let foreign_key_violations = {
        let mut statement = conn.prepare("PRAGMA foreign_key_check")?;
        let rows = statement.query_map([], |_| Ok(()))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?.len() as i64
    };
    let performance = list_question_performance(&conn, TEACHER, 20)?;
    let (current_default_revision, current_default_is_target): (i64, bool) = conn.query_row(
        "SELECT version.revision,
                item.answer_key_version_id=(
                  SELECT latest_answer.id FROM k1_answer_key_versions latest_answer
                  WHERE latest_answer.question_version_id=item.question_version_id
                    AND latest_answer.state='confirmed'
                  ORDER BY latest_answer.revision DESC,latest_answer.id DESC LIMIT 1
                )
                AND item.rubric_version_id=(
                  SELECT latest_rubric.id FROM k1_rubric_versions latest_rubric
                  WHERE latest_rubric.question_version_id=item.question_version_id
                    AND latest_rubric.state='confirmed'
                  ORDER BY latest_rubric.revision DESC,latest_rubric.id DESC LIMIT 1
                )
                AND item.link_set_id=(
                  SELECT latest_link.id FROM k1_link_sets latest_link
                  WHERE latest_link.question_version_id=item.question_version_id
                    AND latest_link.state='confirmed'
                  ORDER BY latest_link.revision DESC,latest_link.id DESC LIMIT 1
                )
         FROM exam_assessments_v2 assessment
         JOIN exam_assessment_versions_v2 version
           ON version.id=COALESCE(
             (SELECT selection.selected_assessment_version_id
              FROM exam_assessment_default_version_selections_v2 selection
              WHERE selection.assessment_id=assessment.id
              ORDER BY selection.revision DESC,selection.id DESC
              LIMIT 1),
             (SELECT latest.id
              FROM exam_assessment_versions_v2 latest
              WHERE latest.assessment_id=assessment.id AND latest.state='confirmed'
              ORDER BY latest.revision DESC,latest.id DESC
              LIMIT 1)
           )
         JOIN exam_assessment_items_v2 item
           ON item.assessment_version_id=version.id AND item.state='active'
         WHERE assessment.title=?1",
        [ASSESSMENT_TITLE],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(FixtureReport {
        schema_version: FIXTURE_SCHEMA,
        phase: phase.into(),
        data_dir: data_dir.display().to_string(),
        migration_count: scalar_i64(&conn, "SELECT COUNT(*) FROM schema_migrations")?,
        integrity_check,
        foreign_key_violations,
        performance_items: performance.items.len(),
        impact_plans: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM exam_question_version_impact_plans_v2",
        )?,
        assessment_versions: scalar_i64(&conn, "SELECT COUNT(*) FROM exam_assessment_versions_v2")?,
        assessment_items: scalar_i64(&conn, "SELECT COUNT(*) FROM exam_assessment_items_v2")?,
        default_selections: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM exam_assessment_default_version_selections_v2",
        )?,
        current_default_revision,
        current_default_is_target,
        historical_attempts: scalar_i64(&conn, "SELECT COUNT(*) FROM exam_attempts_v2")?,
        historical_grade_decisions: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM exam_grade_decisions_v2",
        )?,
        historical_publications: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM exam_grade_publications_v2 WHERE state='published'",
        )?,
        historical_active_learning_evidence: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM learning_evidence WHERE state='active'",
        )?,
        upgrade_audit_events: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM audit_events
             WHERE action='exam.assessment.future_default_upgraded'",
        )?,
        upgrade_outbox_events: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM outbox_events
             WHERE event_type='exam.assessment.future_default_upgraded'",
        )?,
    })
}

fn expected_migration_count() -> i64 {
    (suite_core::db::CORE_MIGRATIONS.len()
        + module_knowledge::knowledge_migrations().len()
        + module_recitation::recitation_migrations().len()
        + module_exam::exam_migrations().len()
        + module_wrongbook::wrongbook_migrations().len()
        + module_profile::profile_migrations().len()) as i64
}

fn verify_fixture(data_dir: &Path, phase: &str) -> AppResult<FixtureReport> {
    let report = inspect_fixture(data_dir, phase)?;
    expect(
        report.migration_count == expected_migration_count(),
        format!(
            "夹具必须包含全部 {} 个迁移，实际 {}",
            expected_migration_count(),
            report.migration_count
        ),
    )?;
    expect(
        report.integrity_check == "ok",
        format!(
            "integrity_check 必须为 ok，实际为：{}",
            report.integrity_check
        ),
    )?;
    expect(report.foreign_key_violations == 0, "夹具不能包含外键违规")?;
    expect(report.performance_items == 1, "表现页必须只有一题")?;
    expect(report.historical_attempts == 1, "历史 attempt 数量不能变化")?;
    expect(
        report.historical_grade_decisions == 1,
        "历史 grade decision 数量不能变化",
    )?;
    expect(
        report.historical_publications == 1,
        "历史 publication 数量不能变化",
    )?;
    expect(
        report.historical_active_learning_evidence == 2,
        "历史正式学习证据必须保持 2 条",
    )?;
    match phase {
        "seeded" => {
            expect(report.impact_plans == 0, "seeded 不能预置影响计划")?;
            expect(report.assessment_versions == 1, "seeded 只能有作业第 1 版")?;
            expect(
                report.assessment_items == 1,
                "seeded 只能有一个作业题目实例",
            )?;
            expect(
                report.default_selections == 0,
                "seeded 不能已有显式默认选择",
            )?;
            expect(
                report.current_default_revision == 1 && !report.current_default_is_target,
                "seeded 当前默认必须是仍引用旧规则的第 1 版",
            )?;
            expect(
                report.upgrade_audit_events == 0 && report.upgrade_outbox_events == 0,
                "seeded 不能有默认换版审计或 outbox",
            )?;
        }
        "upgraded" | "restarted" => {
            expect(
                report.impact_plans == 1,
                "升级后必须只有一个由真机冻结的 future_only 影响计划",
            )?;
            expect(report.assessment_versions == 2, "升级后必须新增作业第 2 版")?;
            expect(
                report.assessment_items == 2,
                "升级后必须复制一个新版题目实例",
            )?;
            expect(
                report.default_selections == 1,
                "升级后必须只有一个显式默认选择",
            )?;
            expect(
                report.current_default_revision == 2 && report.current_default_is_target,
                "升级后未来上传默认必须是完整引用目标规则的第 2 版",
            )?;
            expect(
                report.upgrade_audit_events == 1 && report.upgrade_outbox_events == 1,
                "升级必须各形成一条审计和 outbox 事件",
            )?;
        }
        _ => return Err(invalid("phase 只能是 seeded、upgraded 或 restarted")),
    }
    Ok(report)
}

fn parse_args() -> AppResult<(String, PathBuf, String)> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or_else(|| {
        invalid(
            "用法：m25_default_fixture <seed|verify> --data-dir <绝对路径> \
             [--phase seeded|upgraded|restarted]",
        )
    })?;
    let mut data_dir = None;
    let mut phase = "seeded".to_owned();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--phase" => phase = args.next().ok_or_else(|| invalid("--phase 缺少值"))?,
            _ => return Err(invalid(format!("未知参数：{argument}"))),
        }
    }
    Ok((
        command,
        data_dir.ok_or_else(|| invalid("缺少 --data-dir"))?,
        phase,
    ))
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

    fn test_data_dir(label: &str) -> (PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("jiaofu-m25-fixture-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let data_dir = root
            .join("Library/Application Support")
            .join(FIXTURE_BUNDLE_ID);
        (root, data_dir)
    }

    #[test]
    fn rejects_non_fixture_directory() {
        let wrong = std::env::temp_dir().join("com.jiaofu.suite");
        assert!(validate_fixture_data_dir(&wrong).is_err());
    }

    #[test]
    fn seed_and_upgrade_keep_historical_sentinels() {
        let (root, data_dir) = test_data_dir("contract");
        seed_fixture(&data_dir).unwrap();
        let mut conn = suite_core::db::open(&data_dir.join("data.db")).unwrap();
        let (question_version_public_id, source_version_public_id): (String, String) = conn
            .query_row(
                "SELECT question.public_id,version.public_id
                 FROM exam_assessment_items_v2 item
                 JOIN k1_question_versions question
                   ON question.id=item.question_version_id
                 JOIN exam_assessment_versions_v2 version
                   ON version.id=item.assessment_version_id
                 WHERE version.revision=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let preview =
            preview_question_version_impact(&conn, TEACHER, &question_version_public_id).unwrap();
        let plan = confirm_question_impact_plan(
            &mut conn,
            TEACHER,
            &ConfirmQuestionImpactPlanRequest {
                request_key: "m25-fixture-test-plan".into(),
                question_version_public_id,
                expected_preview_hash: preview.preview_hash,
                action: "future_only".into(),
                planned_by: TEACHER.into(),
            },
        )
        .unwrap();
        upgrade_assessment_default_from_impact(
            &mut conn,
            TEACHER,
            &UpgradeAssessmentDefaultRequest {
                request_key: "m25-fixture-test-upgrade".into(),
                plan_public_id: plan.public_id,
                source_assessment_version_public_id: source_version_public_id.clone(),
                expected_current_default_version_public_id: source_version_public_id,
                upgraded_by: TEACHER.into(),
            },
        )
        .unwrap();
        drop(conn);
        verify_fixture(&data_dir, "upgraded").unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
