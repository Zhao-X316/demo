//! 对正式 SQLite 做 Online Backup 副本并在副本上验证全部迁移。
//!
//! 用法：
//! cargo run --example migrate_database_copy -- <source.db> <new-copy.db>
//!
//! 目标文件必须不存在，避免测试误覆盖老师数据。

use std::path::Path;

use rusqlite::{Connection, DatabaseName};

fn run_all(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    suite_core::db::run_migrations(conn, suite_core::db::CORE_MIGRATIONS)?;
    suite_core::db::run_migrations(conn, module_knowledge::knowledge_migrations())?;
    suite_core::db::run_migrations(conn, module_recitation::recitation_migrations())?;
    suite_core::db::run_migrations(conn, module_exam::exam_migrations())?;
    suite_core::db::run_migrations(conn, module_wrongbook::wrongbook_migrations())?;
    suite_core::db::run_migrations(conn, module_profile::profile_migrations())?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let source_path = args
        .next()
        .ok_or("缺少源数据库路径：migrate_database_copy <source.db> <new-copy.db>")?;
    let target_path = args
        .next()
        .ok_or("缺少副本路径：migrate_database_copy <source.db> <new-copy.db>")?;
    if args.next().is_some() {
        return Err("参数过多：只接受源数据库和新副本路径".into());
    }
    let source_path = Path::new(&source_path);
    let target_path = Path::new(&target_path);
    if !source_path.is_file() {
        return Err(format!("源数据库不存在：{}", source_path.display()).into());
    }
    if target_path.exists() {
        return Err(format!("副本路径已存在，拒绝覆盖：{}", target_path.display()).into());
    }

    let source = suite_core::db::open(source_path)?;
    let before_count: i64 = source.query_row(
        "SELECT COUNT(*) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    source.backup(DatabaseName::Main, target_path, None)?;

    let copy = suite_core::db::open(target_path)?;
    run_all(&copy)?;
    run_all(&copy)?;
    let after_count: i64 = copy.query_row(
        "SELECT COUNT(*) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;
    let integrity: String =
        copy.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    let foreign_key_violations: i64 = copy.query_row(
        "SELECT COUNT(*) FROM pragma_foreign_key_check",
        [],
        |row| row.get(0),
    )?;
    let new_table_count: i64 = copy.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='table' AND name='exam_rubric_evidence_promotions_v2'",
        [],
        |row| row.get(0),
    )?;
    let new_trigger_count: i64 = copy.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='trigger' AND name LIKE 'trg_exam_rubric_evidence_promotion_%'",
        [],
        |row| row.get(0),
    )?;
    let new_business_rows: i64 = copy.query_row(
        "SELECT COUNT(*) FROM exam_rubric_evidence_promotions_v2",
        [],
        |row| row.get(0),
    )?;
    let review_case_table_count: i64 = copy.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='table' AND name='exam_question_version_review_cases_v2'",
        [],
        |row| row.get(0),
    )?;
    let review_case_trigger_count: i64 = copy.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='trigger' AND name LIKE 'trg_exam_question_review_case_%'",
        [],
        |row| row.get(0),
    )?;
    let review_case_business_rows: i64 = copy.query_row(
        "SELECT COUNT(*) FROM exam_question_version_review_cases_v2",
        [],
        |row| row.get(0),
    )?;
    let default_selection_table_count: i64 = copy.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='table' AND name='exam_assessment_default_version_selections_v2'",
        [],
        |row| row.get(0),
    )?;
    let default_selection_view_count: i64 = copy.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='view' AND name='exam_assessment_current_defaults_v2'",
        [],
        |row| row.get(0),
    )?;
    let default_selection_trigger_count: i64 = copy.query_row(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='trigger' AND name LIKE 'trg_exam_assessment_default_selection_%'",
        [],
        |row| row.get(0),
    )?;
    let default_selection_business_rows: i64 = copy.query_row(
        "SELECT COUNT(*) FROM exam_assessment_default_version_selections_v2",
        [],
        |row| row.get(0),
    )?;

    println!("source_migrations={before_count}");
    println!("copy_migrations={after_count}");
    println!("integrity_check={integrity}");
    println!("foreign_key_violations={foreign_key_violations}");
    println!("new_table_count={new_table_count}");
    println!("new_trigger_count={new_trigger_count}");
    println!("new_business_rows={new_business_rows}");
    println!("review_case_table_count={review_case_table_count}");
    println!("review_case_trigger_count={review_case_trigger_count}");
    println!("review_case_business_rows={review_case_business_rows}");
    println!("default_selection_table_count={default_selection_table_count}");
    println!("default_selection_view_count={default_selection_view_count}");
    println!("default_selection_trigger_count={default_selection_trigger_count}");
    println!("default_selection_business_rows={default_selection_business_rows}");
    Ok(())
}
