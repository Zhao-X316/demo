//! Tauri 命令层（薄封装，调用已测好的 core/模块服务）。

use chrono::{Duration, NaiveDate, Utc};
use serde::Serialize;
use tauri::State;

use module_recitation::config::RecitationConfig;
use module_recitation::db::contents::{self, ContentInput, RecContent};
use module_recitation::service::{import, scoring, tasks as task_svc};
use suite_core::db::repo::students::{self, StudentInput};
use suite_core::db::repo::{submissions, tasks, verdicts};
use suite_core::models::{ModuleKey, Student, TaskKind, TaskStatus};

use crate::secrets::{self, VolcanoCreds};
use crate::state::AppState;

const MODULE: ModuleKey = ModuleKey::Recitation;

type R<T> = Result<T, String>;
fn e<E: ToString>(err: E) -> String {
    err.to_string()
}

/// 日切基准：Asia/Shanghai (UTC+8)。
fn today_str() -> String {
    (Utc::now() + Duration::hours(8)).format("%Y-%m-%d").to_string()
}
fn today_naive() -> NaiveDate {
    (Utc::now() + Duration::hours(8)).date_naive()
}

fn kind_str(k: TaskKind) -> &'static str {
    match k {
        TaskKind::Normal => "normal",
        TaskKind::Makeup => "makeup",
        TaskKind::Review => "review",
    }
}
fn status_str(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Open => "open",
        TaskStatus::Submitted => "submitted",
        TaskStatus::Passed => "passed",
        TaskStatus::Failed => "failed",
        TaskStatus::Reopened => "reopened",
        TaskStatus::Closed => "closed",
        TaskStatus::Expired => "expired",
    }
}

// ───────────────────────── DTO ─────────────────────────

#[derive(Serialize)]
pub struct SubmissionCard {
    submission_id: i64,
    status: String,
    recognized_text: Option<String>,
    accuracy: Option<f64>,
    pass: Option<bool>,
    fluency: Option<f64>,
    quality: Option<String>,
    human_result: Option<String>,
    machine_note: Option<String>,
}

#[derive(Serialize)]
pub struct TaskCard {
    task_id: i64,
    kind: String,
    status: String,
    student_no: String,
    student_name: String,
    content_no: String,
    content_title: String,
    submission: Option<SubmissionCard>,
}

#[derive(Serialize)]
pub struct TodayView {
    date: String,
    normal: Vec<TaskCard>,
    makeup: Vec<TaskCard>,
    review: Vec<TaskCard>,
}

// ───────────────────────── 命令 ─────────────────────────

/// 今日看板：按 新背/补背/复习 分组，含提交与判定。
#[tauri::command]
pub fn dashboard_today(state: State<'_, AppState>) -> R<TodayView> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let date = today_str();
    let tasks_today = tasks::list_by_date(&conn, MODULE, &date).map_err(e)?;

    let mut view = TodayView { date, normal: vec![], makeup: vec![], review: vec![] };
    for t in tasks_today {
        let student = students::get_by_id(&conn, t.student_id).map_err(e)?;
        let content = contents::get_by_id(&conn, t.ref_id).map_err(e)?;
        let sub = submissions::find_by_task(&conn, t.id).map_err(e)?;
        let submission = match sub {
            Some(s) => {
                let v = verdicts::get_by_submission(&conn, s.id).map_err(e)?;
                Some(SubmissionCard {
                    submission_id: s.id,
                    status: s.status,
                    recognized_text: s.recognized_text,
                    accuracy: v.as_ref().and_then(|x| x.primary_score),
                    pass: v.as_ref().and_then(|x| x.pass),
                    fluency: v.as_ref().and_then(|x| x.secondary_score),
                    quality: v.as_ref().and_then(|x| x.quality.clone()),
                    human_result: v.as_ref().and_then(|x| x.human_result.clone()),
                    machine_note: v.as_ref().and_then(|x| x.machine_note.clone()),
                })
            }
            None => None,
        };
        let card = TaskCard {
            task_id: t.id,
            kind: kind_str(t.kind).to_string(),
            status: status_str(t.status).to_string(),
            student_no: student.as_ref().map(|s| s.student_no.clone()).unwrap_or_default(),
            student_name: student.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
            content_no: content.as_ref().map(|c| c.content_no.clone()).unwrap_or_default(),
            content_title: content.as_ref().map(|c| c.title.clone()).unwrap_or_default(),
            submission,
        };
        match t.kind {
            TaskKind::Normal => view.normal.push(card),
            TaskKind::Makeup => view.makeup.push(card),
            TaskKind::Review => view.review.push(card),
        }
    }
    Ok(view)
}

/// 人工最终判定：pass | fail | reopen。
#[tauri::command]
pub fn verdict_human_decide(
    state: State<'_, AppState>,
    submission_id: i64,
    result: String,
    note: Option<String>,
) -> R<String> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let cfg = RecitationConfig::load(&conn).map_err(e)?.to_score_cfg();
    let next = scoring::human_decide(
        &conn,
        submission_id,
        &result,
        note.as_deref(),
        Some("teacher"),
        today_naive(),
        &cfg,
    )
    .map_err(e)?;
    Ok(format!("{next:?}"))
}

#[tauri::command]
pub fn students_list(state: State<'_, AppState>) -> R<Vec<Student>> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    students::list(&conn, false).map_err(e)
}

#[tauri::command]
pub fn contents_list(state: State<'_, AppState>) -> R<Vec<RecContent>> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    contents::list(&conn, false).map_err(e)
}

/// 生成示例数据（无需 ASR/真实录音即可体验看板全流程）。
#[tauri::command]
pub fn seed_demo(state: State<'_, AppState>) -> R<String> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let date = today_str();
    let ymd = date.replace('-', "");

    let s1 = students::upsert(&conn, &StudentInput { student_no: "2023001", name: "张三", class_id: None, enabled: true }).map_err(e)?;
    let s2 = students::upsert(&conn, &StudentInput { student_no: "2023002", name: "李四", class_id: None, enabled: true }).map_err(e)?;
    let answer = "床前明月光，疑是地上霜。举头望明月，低头思故乡。";
    let c = contents::upsert(&conn, &ContentInput { content_no: "C012", title: "静夜思", answer_text: answer, subject_id: None, enabled: true }).map_err(e)?;

    task_svc::generate_normal(&conn, &date, &[(s1.id, c.id), (s2.id, c.id)]).map_err(e)?;

    let t = today_naive();
    let cfg = RecitationConfig::load(&conn).map_err(e)?.to_score_cfg();
    // 张三完美背诵 → 通过；李四只背前两句 → 未通过(补背)
    let rows = [
        ("2023001", "张三", answer, "seed-h1"),
        ("2023002", "李四", "床前明月光，疑是地上霜。", "seed-h2"),
    ];
    for (no, name, text, hash) in rows {
        let stem = format!("{ymd}_{no}_{name}_C012");
        let item = import::ImportItem { file_path: &stem, file_stem: &stem, file_hash: hash, duration_ms: Some(8000) };
        if let import::ImportOutcome::Imported { submission_id, .. } = import::import_one(&conn, &item).map_err(e)? {
            submissions::set_recognition(&conn, submission_id, Some(text), "ok", None, Some(8000)).map_err(e)?;
            scoring::score_submission(&conn, submission_id, &[], t, &cfg).map_err(e)?;
        }
    }
    Ok("已生成示例数据：2 名学生 + 静夜思，张三通过、李四待补背".to_string())
}

// ───────────────────────── 设置：评分配置 ─────────────────────────

#[tauri::command]
pub fn config_get(state: State<'_, AppState>) -> R<RecitationConfig> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    RecitationConfig::load(&conn).map_err(e)
}

#[tauri::command]
pub fn config_set(state: State<'_, AppState>, cfg: RecitationConfig) -> R<()> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    cfg.save(&conn).map_err(e)
}

// ───────────────────────── 设置：火山凭据（本地文件） ─────────────────────────

#[tauri::command]
pub fn secrets_get(state: State<'_, AppState>) -> R<VolcanoCreds> {
    secrets::load(&state.data_dir).map_err(e)
}

#[tauri::command]
pub fn secrets_set(state: State<'_, AppState>, creds: VolcanoCreds) -> R<()> {
    secrets::save(&state.data_dir, &creds).map_err(e)
}

// ───────────────────────── 管理：学生 / 内容 / 任务 ─────────────────────────

#[tauri::command]
pub fn students_upsert(
    state: State<'_, AppState>,
    student_no: String,
    name: String,
    enabled: bool,
) -> R<Student> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    students::upsert(
        &conn,
        &StudentInput { student_no: &student_no, name: &name, class_id: None, enabled },
    )
    .map_err(e)
}

#[tauri::command]
pub fn contents_upsert(
    state: State<'_, AppState>,
    content_no: String,
    title: String,
    answer_text: String,
    enabled: bool,
) -> R<RecContent> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    contents::upsert(
        &conn,
        &ContentInput {
            content_no: &content_no,
            title: &title,
            answer_text: &answer_text,
            subject_id: None,
            enabled,
        },
    )
    .map_err(e)
}

/// 为今日批量生成"新背"任务。返回生成数量。
#[tauri::command]
pub fn tasks_generate(state: State<'_, AppState>, pairs: Vec<(i64, i64)>) -> R<usize> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let date = today_str();
    let ids = task_svc::generate_normal(&conn, &date, &pairs).map_err(e)?;
    Ok(ids.len())
}
