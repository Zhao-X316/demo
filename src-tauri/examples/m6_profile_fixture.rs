//! M6 个人报告与班级教学输入的隔离真机验收夹具。
//!
//! 夹具只允许写入 `jiaofu-m6-fixture-*` 根下、且 bundle id 为
//! `com.jiaofu.suite.m6fixture` 的全新数据目录。seed 通过正式学习证据、
//! 个人快照、老师补充判断和班级快照服务建立可操作数据；个人报告保存和
//! 班级教学重点确认留给真实 `.app` 的 Tauri IPC，verify 只读检查持久化、
//! 审计事件和“没有创建作业、成绩或额外学习证据”等业务不变量。

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use module_profile::class_profile::{
    self, ClassProfileScope, GenerateClassProfileInput,
};
use module_profile::profile::{
    self, GenerateStudentProfileInput, StudentProfileScope, StudentProfileSnapshot,
};
use module_profile::teacher_assessments::{
    save_profile_teacher_assessment, SaveProfileTeacherAssessmentInput,
};
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use suite_core::db::repo::learning_evidence::{create_or_get, NewLearningEvidence};
use suite_core::models::{
    AssessmentContext, ConfirmationLevel, EvidenceKind, EvidenceSourceModule,
};

const FIXTURE_BUNDLE_ID: &str = "com.jiaofu.suite.m6fixture";
const FIXTURE_ROOT_PREFIX: &str = "jiaofu-m6-fixture-";
const FIXTURE_MARKER: &str = "m6-profile-fixture.json";
const FIXTURE_SCHEMA: i64 = 1;
const TEACHER: &str = "local_teacher";
const RANGE_START: &str = "2026-06-20";
const RANGE_END: &str = "2026-07-19";
const KNOWLEDGE_TITLE: &str = "洋务运动失败原因";
const ABILITY_TITLE: &str = "因果分析";

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Serialize)]
struct FixtureReport {
    schema_version: i64,
    phase: String,
    data_dir: String,
    migration_count: i64,
    integrity_check: String,
    foreign_key_violations: i64,
    classes: i64,
    students: i64,
    active_learning_evidence: i64,
    student_profile_snapshots: i64,
    class_profile_snapshots: i64,
    teacher_assessment_revisions: i64,
    report_snapshots: i64,
    teaching_input_drafts: i64,
    teaching_input_items: i64,
    report_audit_events: i64,
    report_outbox_events: i64,
    teaching_input_audit_events: i64,
    teaching_input_outbox_events: i64,
    recitation_tasks: i64,
    recitation_submissions: i64,
    exam_assessment_versions: i64,
    exam_attempts: i64,
    exam_grade_decisions: i64,
    exam_publications: i64,
    current_common_support_nodes: i64,
    saved_teaching_titles: String,
}

#[derive(Debug, Serialize)]
struct FixtureMarker {
    schema_version: i64,
    bundle_id: &'static str,
    class_id: i64,
    student_ids: Vec<i64>,
    first_student_snapshot_public_id: String,
    class_snapshot_public_id: String,
}

struct SeededIds {
    class_id: i64,
    student_ids: Vec<i64>,
    student_snapshots: Vec<StudentProfileSnapshot>,
    class_snapshot_public_id: String,
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

fn expected_migration_count() -> i64 {
    (suite_core::db::CORE_MIGRATIONS.len()
        + module_knowledge::knowledge_migrations().len()
        + module_recitation::recitation_migrations().len()
        + module_exam::exam_migrations().len()
        + module_wrongbook::wrongbook_migrations().len()
        + module_profile::profile_migrations().len()) as i64
}

#[allow(clippy::too_many_arguments)]
fn add_evidence(
    conn: &Connection,
    student_id: i64,
    map_public_id: &str,
    knowledge_public_id: &str,
    ability_public_id: &str,
    key: &str,
    occurred_at: &str,
    value: f64,
) -> AppResult<()> {
    create_or_get(
        conn,
        &NewLearningEvidence {
            idempotency_key: key,
            student_id,
            source_module: EvidenceSourceModule::Grading,
            source_type: "question_rubric_point",
            source_ref_type: "rubric_point",
            source_ref_id: key,
            source_revision: 1,
            decision_ref_type: Some("grade_decision"),
            decision_ref_id: Some(key),
            decision_revision: Some(1),
            knowledge_node_id: Some(knowledge_public_id),
            ability_dimension_id: Some(ability_public_id),
            evidence_kind: EvidenceKind::Accuracy,
            value,
            confirmation_level: ConfirmationLevel::TeacherCorrected,
            evidence_quality: 1.0,
            assessment_context: AssessmentContext::ClosedBook,
            occurred_at,
            rule_version: "m6-fixture-v1",
            knowledge_map_version: &format!("{map_public_id}:r1"),
        },
    )?;
    Ok(())
}

fn seed_business(conn: &mut Connection) -> AppResult<SeededIds> {
    conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])?;
    let subject_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO classes(name,term,textbook)
         VALUES ('八年级一班','2026','中国历史八上')",
        [],
    )?;
    let class_id = conn.last_insert_rowid();
    let mut student_ids = Vec::new();
    for (student_no, name) in [("01", "小周"), ("02", "小林"), ("03", "小陈")] {
        conn.execute(
            "INSERT INTO students(student_no,name,class_id,enabled)
             VALUES (?1,?2,?3,1)",
            params![student_no, name, class_id],
        )?;
        student_ids.push(conn.last_insert_rowid());
    }

    conn.execute(
        "INSERT INTO k1_textbook_editions
          (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
         VALUES ('edition-m6-fixture',?1,'pep','2026','八上历史','8','upper','active',
                 '2026-06-20T00:00:00.000Z')",
        [subject_id],
    )?;
    let edition_id = conn.last_insert_rowid();
    let map_public_id = "map-m6-fixture";
    conn.execute(
        "INSERT INTO k1_knowledge_maps
          (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
         VALUES (?1,?2,1,'confirmed','2026-06-20T00:00:00.000Z',
                 '2026-06-20T00:00:00.000Z')",
        params![map_public_id, edition_id],
    )?;
    let map_id = conn.last_insert_rowid();
    let knowledge_public_id = "knowledge-m6-fixture";
    conn.execute(
        "INSERT INTO k1_knowledge_nodes
          (public_id,stable_id,knowledge_map_id,title,order_index,state,created_at)
         VALUES (?1,'stable-m6-fixture',?2,?3,1,'active',
                 '2026-06-20T00:00:00.000Z')",
        params![knowledge_public_id, map_id, KNOWLEDGE_TITLE],
    )?;
    conn.execute(
        "INSERT INTO k1_knowledge_nodes
          (public_id,stable_id,knowledge_map_id,title,order_index,state,created_at)
         VALUES ('knowledge-m6-unassessed','stable-m6-unassessed',?1,'未考查知识点',2,
                 'active','2026-06-20T00:00:00.000Z')",
        [map_id],
    )?;
    let ability_public_id = "ability-m6-fixture";
    conn.execute(
        "INSERT INTO k1_ability_dimensions
          (public_id,stable_id,subject_id,revision,code,title,state,created_at)
         VALUES (?1,'ability-stable-m6-fixture',?2,1,'cause_analysis',?3,'active',
                 '2026-06-20T00:00:00.000Z')",
        params![ability_public_id, subject_id, ABILITY_TITLE],
    )?;

    let mut student_snapshots = Vec::new();
    for (student_index, student_id) in student_ids.iter().enumerate() {
        for (date_index, occurred_at) in [
            "2026-07-05T00:00:00.000Z",
            "2026-07-12T00:00:00.000Z",
            "2026-07-16T00:00:00.000Z",
        ]
        .iter()
        .enumerate()
        {
            add_evidence(
                conn,
                *student_id,
                map_public_id,
                knowledge_public_id,
                ability_public_id,
                &format!("m6-fixture-{student_index}-{date_index}"),
                occurred_at,
                0.0,
            )?;
        }
        student_snapshots.push(profile::generate_student_profile(
            conn,
            &GenerateStudentProfileInput {
                scope: StudentProfileScope {
                    class_id,
                    student_id: *student_id,
                    range_start: RANGE_START,
                    range_end: RANGE_END,
                },
                confirmed_by: TEACHER,
            },
        )?);
    }

    let first_metric = student_snapshots[0]
        .knowledge_metrics
        .first()
        .ok_or_else(|| invalid("首名学生必须生成知识节点指标"))?;
    save_profile_teacher_assessment(
        conn,
        &SaveProfileTeacherAssessmentInput {
            snapshot_public_id: &student_snapshots[0].public_id,
            node_metric_public_id: &first_metric.public_id,
            expected_revision: 0,
            assessment: Some("observe"),
            note: Some("课堂口头回答仍需继续观察；报告只供老师内部核对。"),
            actor_id: TEACHER,
        },
    )?;

    let scope = ClassProfileScope {
        class_id,
        range_start: RANGE_START,
        range_end: RANGE_END,
    };
    let preview = class_profile::preview_class_profile(conn, &scope)?;
    let class_snapshot = class_profile::generate_class_profile(
        conn,
        &GenerateClassProfileInput {
            scope,
            expected_source_watermark: &preview.source_watermark,
            confirmed_by: TEACHER,
        },
    )?;
    expect(
        class_snapshot
            .knowledge_metrics
            .iter()
            .any(|metric| metric.class_status == "common_needs_support"),
        "合成班级快照必须包含共同需要支持的知识点",
    )?;
    expect(
        class_snapshot
            .ability_metrics
            .iter()
            .any(|metric| metric.class_status == "common_needs_support"),
        "合成班级快照必须包含共同需要支持的能力项",
    )?;

    Ok(SeededIds {
        class_id,
        student_ids,
        student_snapshots,
        class_snapshot_public_id: class_snapshot.public_id,
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
        serde_json::to_vec_pretty(&FixtureMarker {
            schema_version: FIXTURE_SCHEMA,
            bundle_id: FIXTURE_BUNDLE_ID,
            class_id: ids.class_id,
            student_ids: ids.student_ids,
            first_student_snapshot_public_id: ids.student_snapshots[0].public_id.clone(),
            class_snapshot_public_id: ids.class_snapshot_public_id,
        })?,
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
    let saved_teaching_titles = conn
        .query_row(
            "SELECT COALESCE(GROUP_CONCAT(title, ' | '),'')
             FROM (SELECT title FROM class_teaching_input_drafts ORDER BY id)",
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
        classes: scalar_i64(&conn, "SELECT COUNT(*) FROM classes")?,
        students: scalar_i64(&conn, "SELECT COUNT(*) FROM students")?,
        active_learning_evidence: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM learning_evidence WHERE state='active'",
        )?,
        student_profile_snapshots: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM profile_snapshots",
        )?,
        class_profile_snapshots: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM class_profile_snapshots",
        )?,
        teacher_assessment_revisions: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM profile_teacher_assessment_revisions",
        )?,
        report_snapshots: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM student_profile_report_snapshots",
        )?,
        teaching_input_drafts: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM class_teaching_input_drafts",
        )?,
        teaching_input_items: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM class_teaching_input_items",
        )?,
        report_audit_events: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM audit_events
             WHERE action='profile.student_report.created'",
        )?,
        report_outbox_events: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM outbox_events
             WHERE event_type='student_profile_internal_report_created'",
        )?,
        teaching_input_audit_events: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM audit_events
             WHERE action='profile.class_teaching_input.confirmed'",
        )?,
        teaching_input_outbox_events: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM outbox_events
             WHERE event_type='class_teaching_input_confirmed'",
        )?,
        recitation_tasks: scalar_i64(&conn, "SELECT COUNT(*) FROM tasks")?,
        recitation_submissions: scalar_i64(&conn, "SELECT COUNT(*) FROM submissions")?,
        exam_assessment_versions: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM exam_assessment_versions_v2",
        )?,
        exam_attempts: scalar_i64(&conn, "SELECT COUNT(*) FROM exam_attempts_v2")?,
        exam_grade_decisions: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM exam_grade_decisions_v2",
        )?,
        exam_publications: scalar_i64(
            &conn,
            "SELECT COUNT(*) FROM exam_grade_publications_v2",
        )?,
        current_common_support_nodes: scalar_i64(
            &conn,
            "SELECT COUNT(*)
             FROM class_profile_node_metrics metric
             JOIN class_profile_snapshots snapshot ON snapshot.id=metric.snapshot_id
             WHERE snapshot.revision=(
               SELECT MAX(latest.revision) FROM class_profile_snapshots latest
               WHERE latest.class_id=snapshot.class_id
             )
             AND metric.class_status='common_needs_support'
             AND metric.sample_sufficient=1",
        )?,
        saved_teaching_titles,
    })
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
    expect(report.classes == 1, "夹具必须只有 1 个班级")?;
    expect(report.students == 3, "夹具必须只有 3 名虚构学生")?;
    expect(
        report.active_learning_evidence == 9,
        "夹具必须固定为 9 条合成正式学习证据",
    )?;
    expect(
        report.student_profile_snapshots == 3,
        "夹具必须固定为 3 份个人快照",
    )?;
    expect(
        report.class_profile_snapshots == 1,
        "夹具必须固定为 1 份班级快照",
    )?;
    expect(
        report.teacher_assessment_revisions == 1,
        "夹具必须固定为 1 条老师补充判断",
    )?;
    expect(
        report.current_common_support_nodes == 2,
        "当前班级快照必须含 1 个知识点和 1 个能力项",
    )?;
    expect(report.recitation_tasks == 0, "夹具不能创建背诵任务")?;
    expect(
        report.recitation_submissions == 0,
        "夹具不能创建背诵提交",
    )?;
    expect(
        report.exam_assessment_versions == 0,
        "夹具不能创建作业版本",
    )?;
    expect(report.exam_attempts == 0, "夹具不能创建学生作答")?;
    expect(report.exam_grade_decisions == 0, "夹具不能创建成绩")?;
    expect(report.exam_publications == 0, "夹具不能发布成绩")?;
    match phase {
        "seeded" => {
            expect(report.report_snapshots == 0, "seeded 不能预置个人报告")?;
            expect(
                report.teaching_input_drafts == 0 && report.teaching_input_items == 0,
                "seeded 不能预置班级教学输入",
            )?;
            expect(
                report.report_audit_events == 0 && report.report_outbox_events == 0,
                "seeded 不能有报告审计或 outbox",
            )?;
            expect(
                report.teaching_input_audit_events == 0
                    && report.teaching_input_outbox_events == 0,
                "seeded 不能有教学输入审计或 outbox",
            )?;
        }
        "confirmed" | "restarted" => {
            expect(report.report_snapshots == 1, "真实导出后必须只有 1 份报告快照")?;
            expect(
                report.teaching_input_drafts == 1,
                "真实确认后必须只有 1 份班级教学输入",
            )?;
            expect(
                (1..=2).contains(&report.teaching_input_items),
                "教学输入必须保留老师确认的 1 至 2 个节点",
            )?;
            expect(
                report.report_audit_events == 1 && report.report_outbox_events == 1,
                "报告确认必须各形成 1 条审计和 outbox",
            )?;
            expect(
                report.teaching_input_audit_events == 1
                    && report.teaching_input_outbox_events == 1,
                "教学输入确认必须各形成 1 条审计和 outbox",
            )?;
            expect(
                !report.saved_teaching_titles.trim().is_empty(),
                "教学输入必须保存老师确认后的标题",
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
            "用法：m6_profile_fixture <seed|verify> --data-dir <绝对路径> \
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
    use module_profile::class_teaching_inputs::{
        confirm_class_teaching_input, preview_class_teaching_input, ConfirmClassTeachingInput,
        PreviewClassTeachingInput,
    };
    use module_profile::student_reports::{
        create_report_snapshot, CreateStudentProfileReportInput, LOCAL_TEACHER_ACTOR_ID,
    };

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
    fn seed_then_real_services_keep_business_history_empty() {
        let (root, data_dir) = test_data_dir("contract");
        seed_fixture(&data_dir).unwrap();
        let marker: serde_json::Value =
            serde_json::from_slice(&fs::read(data_dir.join(FIXTURE_MARKER)).unwrap()).unwrap();
        let mut conn = suite_core::db::open(&data_dir.join("data.db")).unwrap();
        let student_snapshot = profile::get_student_profile(
            &conn,
            marker["first_student_snapshot_public_id"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
        .unwrap();
        create_report_snapshot(
            &mut conn,
            &CreateStudentProfileReportInput {
                request_key: "m6-fixture-test-report",
                snapshot_public_id: &student_snapshot.public_id,
                expected_snapshot_payload_sha256: &student_snapshot.payload_sha256,
                report_kind: "student_learning_summary",
                purpose: "teacher_internal_feedback",
                actor_role: "local_teacher",
                actor_id: LOCAL_TEACHER_ACTOR_ID,
            },
        )
        .unwrap();
        let class_snapshot = class_profile::get_class_profile(
            &conn,
            marker["class_snapshot_public_id"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap();
        let preview = preview_class_teaching_input(
            &conn,
            &PreviewClassTeachingInput {
                snapshot_public_id: &class_snapshot.public_id,
            },
        )
        .unwrap();
        let selected = preview
            .items
            .iter()
            .map(|item| item.node_metric_public_id.clone())
            .collect::<Vec<_>>();
        confirm_class_teaching_input(
            &mut conn,
            &ConfirmClassTeachingInput {
                request_key: "m6-fixture-test-teaching",
                snapshot_public_id: &class_snapshot.public_id,
                expected_snapshot_payload_sha256: &class_snapshot.payload_sha256,
                title: "八年级一班阶段复习重点",
                teaching_note: "先核对洋务运动失败原因，再做一次因果归纳。",
                estimated_minutes: 20,
                selected_node_metric_public_ids: &selected,
                confirmed_by: TEACHER,
            },
        )
        .unwrap();
        drop(conn);
        verify_fixture(&data_dir, "confirmed").unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
