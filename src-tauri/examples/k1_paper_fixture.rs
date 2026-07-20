//! K1 蓝图换题、排版与打印的隔离真机验收夹具。
//!
//! 夹具只允许写入 `jiaofu-k1-paper-fixture-*` 根下、且 bundle id 为
//! `com.jiaofu.suite.k1paperfixture` 的全新数据目录。seed 通过正式 service
//! 建立 3 道同题型同分值 L3 题和一份 2 题蓝图；换题、排序、分页与打印快照确认
//! 留给真实 `.app` 的 Tauri IPC，verify 只读检查持久化和业务不变量。

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use module_exam::service::blueprint_assembly::{
    confirm_blueprint, preview_blueprint, BlueprintPreviewRequest, BlueprintQuestionTypeTarget,
    ConfirmBlueprintRequest,
};
#[cfg(test)]
use module_exam::service::blueprint_paper::{
    confirm_blueprint_paper, BlueprintPaperItemInput, ConfirmBlueprintPaperRequest,
};
use module_knowledge::db::content::{
    add_ability_link, add_knowledge_link, create_answer_key_version, create_link_set,
    create_question, create_question_version, create_rubric_version, promote_question_version,
    NewAbilityLink, NewAnswerKeyVersion, NewKnowledgeLink, NewQuestion, NewQuestionOption,
    NewQuestionVersion, NewRubricPoint, NewRubricVersion,
};
use module_knowledge::db::taxonomy::{
    create_ability_dimension, create_curriculum_node, create_knowledge_map, create_knowledge_node,
    create_textbook_edition, NewAbilityDimension, NewCurriculumNode, NewKnowledgeMap,
    NewKnowledgeNode, NewTextbookEdition,
};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use suite_core::db::repo::classes;

const FIXTURE_BUNDLE_ID: &str = "com.jiaofu.suite.k1paperfixture";
const FIXTURE_ROOT_PREFIX: &str = "jiaofu-k1-paper-fixture-";
const FIXTURE_MARKER: &str = "k1-paper-fixture.json";
const FIXTURE_SCHEMA: i64 = 1;
const TEACHER: &str = "local_teacher";
const ASSEMBLY_TITLE: &str = "鸦片战争课堂练习";
const ORIGINAL_STEM: &str = "鸦片战争爆发于哪一年？";
const SECOND_STEM: &str = "鸦片战争签订结束条约是哪一年？";
const REPLACEMENT_STEM: &str = "中国近代史开端对应哪次战争？";

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Serialize)]
struct FixtureReport {
    schema_version: i64,
    phase: String,
    data_dir: String,
    migration_count: i64,
    integrity_check: String,
    foreign_key_violations: i64,
    blueprint_assemblies: i64,
    paper_editions: i64,
    latest_paper_revision: i64,
    paper_items: i64,
    assessment_versions: i64,
    assessment_items: i64,
    default_selections: i64,
    attempts: i64,
    grade_decisions: i64,
    publications: i64,
    active_learning_evidence: i64,
    replacement_items: i64,
    page_break_items: i64,
    source_slot_order: String,
    paper_audit_events: i64,
    paper_outbox_events: i64,
}

struct SeededIds {
    assembly_public_id: String,
    base_assessment_version_public_id: String,
    replacement_question_public_id: String,
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
    let isolated = data_dir.components().any(|component| match component {
        Component::Normal(value) => value
            .to_str()
            .is_some_and(|value| value.starts_with(FIXTURE_ROOT_PREFIX)),
        _ => false,
    });
    if !isolated {
        return Err(invalid(format!(
            "隔离目录必须位于名称以 {FIXTURE_ROOT_PREFIX} 开头的根目录下"
        )));
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

fn create_question_fixture(
    conn: &Connection,
    map_id: i64,
    knowledge_id: i64,
    ability_id: i64,
    stem: &str,
    correct_label: &str,
) -> AppResult<String> {
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
    let version = create_question_version(
        conn,
        &NewQuestionVersion {
            question_id: question.id,
            revision: 1,
            question_type: "single",
            stem,
            material_text: None,
            max_score: 1.0,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "L0",
            state: "draft",
            options: &[
                NewQuestionOption {
                    label: "A",
                    content: "1840年 / 鸦片战争",
                    order_index: 0,
                },
                NewQuestionOption {
                    label: "B",
                    content: "1842年 / 第二次鸦片战争",
                    order_index: 1,
                },
            ],
        },
    )?;
    create_answer_key_version(
        conn,
        &NewAnswerKeyVersion {
            question_version_id: version.id,
            revision: 1,
            answer_json: &serde_json::json!({
                "schema_version": 1,
                "correct_labels": [correct_label]
            })
            .to_string(),
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_answer_key_id: None,
            slots: &[],
        },
    )?;
    create_rubric_version(
        conn,
        &NewRubricVersion {
            question_version_id: version.id,
            revision: 1,
            max_score: 1.0,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_rubric_id: None,
            points: &[NewRubricPoint {
                stable_id: None,
                order_index: 0,
                canonical_text: "选择正确选项",
                allowed_paraphrases_json: None,
                required_concepts_json: None,
                max_score: 1.0,
            }],
        },
    )?;
    let links = create_link_set(
        conn,
        version.id,
        map_id,
        1,
        "confirmed",
        Some(TEACHER),
        None,
    )?;
    add_knowledge_link(
        conn,
        &NewKnowledgeLink {
            link_set_id: links.id,
            source_type: "question",
            source_public_id: &version.public_id,
            knowledge_node_id: knowledge_id,
            relation_type: "direct_assessment",
            confirmation_level: "teacher_confirmed",
            verified_by: Some(TEACHER),
        },
    )?;
    add_ability_link(
        conn,
        &NewAbilityLink {
            link_set_id: links.id,
            source_type: "question",
            source_public_id: &version.public_id,
            ability_dimension_id: ability_id,
            evidence_strength: 0.5,
            response_mode: "recognition",
            confirmation_level: "teacher_confirmed",
            verified_by: Some(TEACHER),
        },
    )?;
    promote_question_version(conn, version.id, "L3", TEACHER, Some("K1 打印隔离验收题"))?;
    Ok(version.public_id)
}

fn seed_business(conn: &mut Connection) -> AppResult<SeededIds> {
    conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])?;
    let subject_id = conn.last_insert_rowid();
    let class = classes::create(
        conn,
        "K1 换题打印隔离班",
        Some("人教版八年级上册"),
        Some("2026秋"),
    )?;
    let edition = create_textbook_edition(
        conn,
        &NewTextbookEdition {
            subject_id,
            publisher_code: "PEP",
            edition_code: "2024",
            title: "中国历史八年级上册",
            grade: "八年级",
            volume: "upper",
            curriculum_region: None,
        },
    )?;
    let map = create_knowledge_map(
        conn,
        &NewKnowledgeMap {
            textbook_edition_id: edition.id,
            revision: 1,
            state: "confirmed",
            supersedes_map_id: None,
        },
    )?;
    let curriculum = create_curriculum_node(
        conn,
        &NewCurriculumNode {
            stable_id: Some("k1-paper-lesson-opium-war"),
            knowledge_map_id: map.id,
            parent_id: None,
            node_type: "lesson",
            code: Some("L1"),
            title: "鸦片战争",
            description: Some("K1 换题打印隔离验收课"),
            order_index: 1,
        },
    )?;
    let knowledge = create_knowledge_node(
        conn,
        &NewKnowledgeNode {
            stable_id: Some("k1-paper-opium-war-time"),
            knowledge_map_id: map.id,
            curriculum_node_id: Some(curriculum.id),
            parent_id: None,
            code: Some("K1"),
            title: "鸦片战争时间与历史地位",
            description: Some("K1 换题打印隔离验收知识点"),
            order_index: 1,
        },
    )?;
    let ability = create_ability_dimension(
        conn,
        &NewAbilityDimension {
            stable_id: Some("k1-paper-fact-recall"),
            subject_id,
            revision: 1,
            code: "fact_recall",
            title: "史实识记与提取",
            description: Some("K1 换题打印隔离验收能力"),
            supersedes_dimension_id: None,
        },
    )?;

    let original =
        create_question_fixture(conn, map.id, knowledge.id, ability.id, ORIGINAL_STEM, "A")?;
    let second = create_question_fixture(conn, map.id, knowledge.id, ability.id, SECOND_STEM, "B")?;
    let replacement = create_question_fixture(
        conn,
        map.id,
        knowledge.id,
        ability.id,
        REPLACEMENT_STEM,
        "A",
    )?;
    let preview_request = BlueprintPreviewRequest {
        class_id: class.id,
        knowledge_map_public_id: map.public_id,
        curriculum_node_public_id: Some(curriculum.public_id),
        total_score: 2.0,
        question_type_targets: vec![BlueprintQuestionTypeTarget {
            question_type: "single".into(),
            count: 2,
        }],
        required_knowledge_node_public_ids: vec![knowledge.public_id],
    };
    let preview = preview_blueprint(conn, &preview_request)?;
    let assembly = confirm_blueprint(
        conn,
        &ConfirmBlueprintRequest {
            request_key: "k1-paper-fixture-blueprint".into(),
            title: ASSEMBLY_TITLE.into(),
            preview_request,
            expected_preview_hash: preview.preview_hash,
            selected_question_version_public_ids: vec![original, second],
            confirmed_by: TEACHER.into(),
        },
    )?;
    Ok(SeededIds {
        assembly_public_id: assembly.public_id,
        base_assessment_version_public_id: assembly.assessment_version_public_id,
        replacement_question_public_id: replacement,
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
    drop(conn);
    fs::write(
        data_dir.join(FIXTURE_MARKER),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": FIXTURE_SCHEMA,
            "bundle_id": FIXTURE_BUNDLE_ID,
            "assembly_public_id": ids.assembly_public_id,
            "base_assessment_version_public_id": ids.base_assessment_version_public_id,
            "replacement_question_public_id": ids.replacement_question_public_id
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
    let source_slot_order = conn
        .query_row(
            "SELECT COALESCE(GROUP_CONCAT(source_slot_order_index, ','),'')
             FROM (
               SELECT item.source_slot_order_index
               FROM exam_blueprint_paper_items_v2 item
               JOIN exam_blueprint_paper_editions_v2 edition ON edition.id=item.edition_id
               ORDER BY edition.revision DESC,item.order_index
             )",
            [],
            |row| row.get(0),
        )
        .unwrap_or_default();
    Ok(FixtureReport {
        schema_version: FIXTURE_SCHEMA,
        phase: phase.into(),
        data_dir: data_dir.display().to_string(),
        migration_count: scalar_i64(&conn, "SELECT COUNT(*) FROM schema_migrations")?,
        integrity_check,
        foreign_key_violations,
        blueprint_assemblies: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM exam_blueprint_assemblies_v2",
        )?,
        paper_editions: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM exam_blueprint_paper_editions_v2",
        )?,
        latest_paper_revision: scalar_i64(
            &conn,
            "SELECT COALESCE(MAX(revision),0) FROM exam_blueprint_paper_editions_v2",
        )?,
        paper_items: scalar_i64(&conn, "SELECT COUNT(*) FROM exam_blueprint_paper_items_v2")?,
        assessment_versions: scalar_i64(&conn, "SELECT COUNT(*) FROM exam_assessment_versions_v2")?,
        assessment_items: scalar_i64(&conn, "SELECT COUNT(*) FROM exam_assessment_items_v2")?,
        default_selections: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM exam_assessment_default_version_selections_v2",
        )?,
        attempts: scalar_i64(&conn, "SELECT COUNT(*) FROM exam_attempts_v2")?,
        grade_decisions: scalar_i64(&conn, "SELECT COUNT(*) FROM exam_grade_decisions_v2")?,
        publications: scalar_i64(&conn, "SELECT COUNT(*) FROM exam_grade_publications_v2")?,
        active_learning_evidence: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM learning_evidence WHERE state='active'",
        )?,
        replacement_items: scalar_i64(
            &conn,
            &format!(
                "SELECT COUNT(*)
                 FROM exam_blueprint_paper_items_v2 item
                 JOIN k1_question_versions question ON question.id=item.question_version_id
                 WHERE question.stem='{}'",
                REPLACEMENT_STEM.replace('\'', "''")
            ),
        )?,
        page_break_items: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM exam_blueprint_paper_items_v2 WHERE page_break_before=1",
        )?,
        source_slot_order,
        paper_audit_events: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM audit_events
             WHERE action='exam.blueprint_paper_edition.confirmed'",
        )?,
        paper_outbox_events: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM outbox_events
             WHERE event_type='k1_blueprint_paper_edition_confirmed'",
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
    expect(report.integrity_check == "ok", "integrity_check 必须为 ok")?;
    expect(report.foreign_key_violations == 0, "夹具不能包含外键违规")?;
    expect(report.blueprint_assemblies == 1, "必须只有一份已确认蓝图")?;
    expect(
        report.default_selections == 0,
        "换题不能改变未来上传默认版本",
    )?;
    expect(report.attempts == 0, "夹具不能创建学生作答")?;
    expect(report.grade_decisions == 0, "夹具不能创建成绩")?;
    expect(report.publications == 0, "夹具不能发布成绩")?;
    expect(
        report.active_learning_evidence == 0,
        "夹具不能创建正式学习证据",
    )?;
    match phase {
        "seeded" => {
            expect(report.paper_editions == 0, "seeded 不能预置打印版本")?;
            expect(report.paper_items == 0, "seeded 不能预置打印题目")?;
            expect(report.assessment_versions == 1, "seeded 只能有作业第 1 版")?;
            expect(report.assessment_items == 2, "seeded 必须有 2 个题目实例")?;
            expect(
                report.paper_audit_events == 0 && report.paper_outbox_events == 0,
                "seeded 不能有换题打印审计或 outbox",
            )?;
        }
        "confirmed" | "restarted" => {
            expect(report.paper_editions == 1, "确认后必须只有一版打印快照")?;
            expect(
                report.latest_paper_revision == 1,
                "首版打印 revision 必须为 1",
            )?;
            expect(report.paper_items == 2, "打印快照必须固定 2 道题")?;
            expect(report.assessment_versions == 2, "确认后必须新增作业第 2 版")?;
            expect(report.assessment_items == 4, "确认后必须累计 4 个题目实例")?;
            expect(report.replacement_items == 1, "必须换入且只换入 1 道目标题")?;
            expect(report.page_break_items == 1, "必须且只能有 1 个分页标记")?;
            expect(
                report.source_slot_order == "1,0",
                format!(
                    "老师排序必须持久化为源槽位 1,0，实际为 {}",
                    report.source_slot_order
                ),
            )?;
            expect(
                report.paper_audit_events == 1 && report.paper_outbox_events == 1,
                "确认必须各形成一条审计和 outbox 事件",
            )?;
        }
        _ => return Err(invalid("phase 只能是 seeded、confirmed 或 restarted")),
    }
    Ok(report)
}

fn parse_args() -> AppResult<(String, PathBuf, String)> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or_else(|| {
        invalid(
            "用法：k1_paper_fixture <seed|verify> --data-dir <绝对路径> \
             [--phase seeded|confirmed|restarted]",
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
        let root = std::env::temp_dir().join(format!(
            "{FIXTURE_ROOT_PREFIX}{label}-{}",
            std::process::id()
        ));
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
    fn seed_and_confirm_keep_history_empty() {
        let (root, data_dir) = test_data_dir("contract");
        seed_fixture(&data_dir).unwrap();
        let marker: serde_json::Value =
            serde_json::from_slice(&fs::read(data_dir.join(FIXTURE_MARKER)).unwrap()).unwrap();
        let mut conn = suite_core::db::open(&data_dir.join("data.db")).unwrap();
        let second_question_public_id: String = conn
            .query_row(
                "SELECT question.public_id
                 FROM exam_blueprint_items_v2 item
                 JOIN k1_question_versions question
                   ON question.id=item.question_version_id
                 WHERE item.order_index=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        confirm_blueprint_paper(
            &mut conn,
            &ConfirmBlueprintPaperRequest {
                request_key: "k1-paper-fixture-test-confirm".into(),
                assembly_public_id: marker["assembly_public_id"].as_str().unwrap().into(),
                expected_source_assessment_version_public_id: marker
                    ["base_assessment_version_public_id"]
                    .as_str()
                    .unwrap()
                    .into(),
                title: "鸦片战争课堂练习（新版）".into(),
                items: vec![
                    BlueprintPaperItemInput {
                        source_slot_order_index: 1,
                        question_version_public_id: second_question_public_id,
                        page_break_before: false,
                    },
                    BlueprintPaperItemInput {
                        source_slot_order_index: 0,
                        question_version_public_id: marker["replacement_question_public_id"]
                            .as_str()
                            .unwrap()
                            .into(),
                        page_break_before: true,
                    },
                ],
                confirmed_by: TEACHER.into(),
            },
        )
        .unwrap();
        drop(conn);
        verify_fixture(&data_dir, "confirmed").unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
