//! Tauri 命令层（薄封装，调用已测好的 core/模块服务）。

use chrono::{Duration, NaiveDate, Utc};
use serde::Serialize;
use tauri::{AppHandle, State};

use module_recitation::config::RecitationConfig;
use module_recitation::db::contents::{self, ContentInput, RecContent};
use module_recitation::service::{
    ai_pipeline, import, matching, recognition, scoring, structured_scoring, tasks as task_svc,
};
use suite_core::db::repo::students::{self, StudentInput};
use suite_core::db::repo::{classes, file_ledger, submissions, tasks, verdicts};
use suite_core::models::{Class, ModuleKey, Student, Submission, TaskKind, TaskStatus, Verdict};

use crate::backup::{self, BackupCatalog, BackupInfo, BackupKind};
use crate::secrets::{self, MaskedVolcanoCreds, VolcanoCreds};
use crate::state::{self, AppState};

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

/// 待老师确认的唯一口径：最新机器判定已存在，但老师还没有落最终结论。
fn is_pending_teacher_review(verdict: Option<&Verdict>) -> bool {
    verdict.is_some_and(|value| value.human_result.is_none())
}

fn preferred_playback_path(file_path: &str, archived_path: Option<&str>) -> String {
    archived_path
        .filter(|path| std::path::Path::new(path).is_file())
        .unwrap_or(file_path)
        .to_string()
}

fn playback_path(submission: &Submission) -> String {
    preferred_playback_path(&submission.file_path, submission.archived_path.as_deref())
}

// ───────────────────────── DTO ─────────────────────────

#[derive(Serialize)]
pub struct SubmissionCard {
    submission_id: i64,
    status: String,
    recognize_status: String,
    pending_review: bool,
    file_path: String,
    recognized_text: Option<String>,
    answer_text: Option<String>,
    answer_version: Option<i64>,
    scored_answer_version: Option<i64>,
    accuracy: Option<f64>,
    pass: Option<bool>,
    fluency: Option<f64>,
    quality: Option<String>,
    human_result: Option<String>,
    human_note: Option<String>,
    machine_note: Option<String>,
}

#[derive(Serialize)]
pub struct TaskCard {
    task_id: i64,
    kind: String,
    status: String,
    due_date: String,
    student_no: String,
    student_name: String,
    content_no: String,
    content_title: String,
    submission: Option<SubmissionCard>,
}

/// 今日单个背诵内容的统计（看板汇总下钻用）。
#[derive(Serialize)]
pub struct TodayContentStat {
    content_no: String,
    content_title: String,
    should: i64,     // 应背 = 该内容今日任务数
    submitted: i64,  // 实背 = 已交
    passed: i64,     // 通过
    failed: i64,     // 不通过
}

/// 今日看板顶部汇总。
#[derive(Serialize, Default)]
pub struct TodaySummary {
    should: i64,     // 应背人数 = 今日任务总数
    submitted: i64,  // 实背人数 = 有提交的任务数
    passed: i64,     // 通过人数
    failed: i64,     // 不通过人数
    pending: i64,    // 待确认 = 最新判定存在且未终审
    makeup: i64,     // 补背人数 = 今日补背任务数
    contents: Vec<TodayContentStat>,
}

#[derive(Serialize)]
pub struct TodayView {
    date: String,
    summary: TodaySummary,
    normal: Vec<TaskCard>,
    makeup: Vec<TaskCard>,
    review: Vec<TaskCard>,
    overdue_review: Vec<TaskCard>,
}

fn task_card(conn: &rusqlite::Connection, task: &suite_core::models::Task) -> R<TaskCard> {
    let student = students::get_by_id(conn, task.student_id).map_err(e)?;
    let content = contents::get_by_id(conn, task.ref_id).map_err(e)?;
    let sub = submissions::find_by_task(conn, task.id).map_err(e)?;
    let submission = match sub {
        Some(submission) => {
            let verdict = verdicts::get_by_submission(conn, submission.id).map_err(e)?;
            let audio_path = playback_path(&submission);
            Some(SubmissionCard {
                submission_id: submission.id,
                status: submission.status,
                recognize_status: submission.recognize_status,
                pending_review: is_pending_teacher_review(verdict.as_ref()),
                file_path: audio_path,
                recognized_text: submission.recognized_text,
                answer_text: content.as_ref().map(|value| value.answer_text.clone()),
                answer_version: content.as_ref().map(|value| value.answer_version),
                scored_answer_version: verdict.as_ref().map(|value| value.answer_version),
                accuracy: verdict.as_ref().and_then(|value| value.primary_score),
                pass: verdict.as_ref().and_then(|value| value.pass),
                fluency: verdict.as_ref().and_then(|value| value.secondary_score),
                quality: verdict.as_ref().and_then(|value| value.quality.clone()),
                human_result: verdict.as_ref().and_then(|value| value.human_result.clone()),
                human_note: verdict.as_ref().and_then(|value| value.human_note.clone()),
                machine_note: verdict.as_ref().and_then(|value| value.machine_note.clone()),
            })
        }
        None => None,
    };

    Ok(TaskCard {
        task_id: task.id,
        kind: kind_str(task.kind).to_string(),
        status: status_str(task.status).to_string(),
        due_date: task.due_date.clone(),
        student_no: student.as_ref().map(|value| value.student_no.clone()).unwrap_or_default(),
        student_name: student.as_ref().map(|value| value.name.clone()).unwrap_or_default(),
        content_no: content.as_ref().map(|value| value.content_no.clone()).unwrap_or_default(),
        content_title: content.as_ref().map(|value| value.title.clone()).unwrap_or_default(),
        submission,
    })
}

// ───────────────────────── 命令 ─────────────────────────

/// 今日看板：按 新背/补背/复习 分组，含提交与判定。
#[tauri::command]
pub fn dashboard_today(state: State<'_, AppState>) -> R<TodayView> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let date = today_str();
    let tasks_today = tasks::list_by_date(&conn, MODULE, &date).map_err(e)?;

    let mut view = TodayView {
        date: date.clone(),
        summary: TodaySummary::default(),
        normal: vec![],
        makeup: vec![],
        review: vec![],
        overdue_review: vec![],
    };
    for t in tasks_today {
        // 已关闭（撤销/覆盖/删除已布置）的任务不再显示
        if t.status == TaskStatus::Closed {
            continue;
        }
        let card = task_card(&conn, &t)?;

        // —— 顶部汇总统计（不额外查库，顺手 tally）——
        let st = status_str(t.status);
        let has_sub = card.submission.is_some();
        let pending_review = card.submission.as_ref().is_some_and(|item| item.pending_review);
        let is_pass = st == "passed";
        let is_fail = st == "failed";
        let c_no = card.content_no.clone();
        let c_title = card.content_title.clone();
        view.summary.should += 1;
        if matches!(t.kind, TaskKind::Makeup) {
            view.summary.makeup += 1;
        }
        if has_sub {
            view.summary.submitted += 1;
        }
        if is_pass {
            view.summary.passed += 1;
        } else if is_fail {
            view.summary.failed += 1;
        }
        // 独立按 verdict 派生，不能被历史 task 状态或 ASR/submission 状态替代。
        if pending_review {
            view.summary.pending += 1;
        }
        let idx = view.summary.contents.iter().position(|x| x.content_no == c_no);
        let cs = match idx {
            Some(i) => &mut view.summary.contents[i],
            None => {
                view.summary.contents.push(TodayContentStat {
                    content_no: c_no.clone(),
                    content_title: c_title.clone(),
                    should: 0,
                    submitted: 0,
                    passed: 0,
                    failed: 0,
                });
                view.summary.contents.last_mut().unwrap()
            }
        };
        cs.should += 1;
        if has_sub {
            cs.submitted += 1;
        }
        if is_pass {
            cs.passed += 1;
        } else if is_fail {
            cs.failed += 1;
        }

        match t.kind {
            TaskKind::Normal => view.normal.push(card),
            TaskKind::Makeup => view.makeup.push(card),
            TaskKind::Review => view.review.push(card),
        }
    }

    // 机器建议跨天后不能从老师视野消失。日切不会把 submitted/reopened 当未交，
    // 这里再按同一个 pending_review 派生口径收进“逾期待老师处理”。
    let overdue = tasks::list_review_candidates_before(&conn, MODULE, &date).map_err(e)?;
    for task in overdue {
        let card = task_card(&conn, &task)?;
        if card.submission.as_ref().is_some_and(|item| item.pending_review) {
            view.summary.pending += 1;
            view.overdue_review.push(card);
        }
    }
    Ok(view)
}

// ───────────────────────── 日切（没交→补背 / 到期→复习 自动上看板）─────────────────────────

#[derive(Serialize)]
pub struct RolloverDto {
    rolled: usize,  // 结转为补背的任务数（昨天 open/未交/过期）
    reviews: usize, // 推上看板的到期复习数
}

/// 跑一次"日切"：① 把昨天仍 open 的非补背任务标过期并结转为今日补背；② 把到期的复习卡生成今日复习任务。
/// 两个子步骤内部都幂等（去重），可安全重复调用。
pub fn run_day_rollover(conn: &rusqlite::Connection) -> R<(usize, usize)> {
    let (rolled, reviews) = task_svc::run_day_rollover(conn, today_naive()).map_err(e)?;
    Ok((rolled, reviews.len()))
}

/// 看板兜底命令：前端加载/检测到跨天时调一次，保证 没交→补背、到期→复习 自动出现（App 常驻不关也不漏）。
#[tauri::command]
pub fn day_rollover(state: State<'_, AppState>) -> R<RolloverDto> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let (rolled, reviews) = run_day_rollover(&conn)?;
    Ok(RolloverDto { rolled, reviews })
}

// ───────────────────────── 异常池候选建议（Top-N）─────────────────────────

#[derive(Serialize)]
pub struct ContentCandidate {
    content_no: String,
    title: String,
    score: f64, // 匹配度 0-100
}

#[derive(Serialize)]
pub struct SuggestDto {
    student_no: Option<String>,
    student_name: Option<String>,
    contents: Vec<ContentCandidate>,
}

/// 给一段识别文本算"最可能的学生 + Top-3 内容候选"，供异常池一键采纳。
#[tauri::command]
pub fn suggest_match(state: State<'_, AppState>, text: String) -> R<SuggestDto> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let rcfg = RecitationConfig::load(&conn).map_err(e)?;
    let scfg = rcfg.to_score_cfg();
    let student = matching::find_student(&conn, &text).map_err(e)?;
    let tops = matching::top_contents(&conn, &text, &scfg.normalize, &scfg.accuracy, 3).map_err(e)?;
    Ok(SuggestDto {
        student_no: student.as_ref().map(|s| s.student_no.clone()),
        student_name: student.as_ref().map(|s| s.name.clone()),
        contents: tops
            .into_iter()
            .map(|(c, score)| ContentCandidate { content_no: c.content_no, title: c.title, score })
            .collect(),
    })
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
            scoring::finish_recognition_and_score(
                &conn,
                submission_id,
                text,
                Some(8000),
                &[],
                &cfg,
            )
            .map_err(e)?;
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

// ───────────────────────── 本地数据库备份/恢复 ─────────────────────────

#[tauri::command]
pub fn backups_list(state: State<'_, AppState>) -> R<BackupCatalog> {
    backup::list_backups(&state.data_dir.join("backups")).map_err(e)
}

#[tauri::command]
pub fn backup_create(state: State<'_, AppState>) -> R<BackupInfo> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    backup::create_backup(
        &conn,
        &state.data_dir.join("backups"),
        BackupKind::Manual,
    )
    .map_err(e)
}

#[tauri::command]
pub fn backup_restore(
    app: AppHandle,
    state: State<'_, AppState>,
    file_name: String,
) -> R<BackupInfo> {
    let mut conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let protective = backup::restore_with(
        &mut conn,
        &state.data_dir.join("backups"),
        &file_name,
        state::run_all_migrations,
    )
    .map_err(e)?;
    if let Err(err) = state::allow_existing_media(&app, &conn) {
        // 数据库已经成功恢复，不能把白名单刷新失败伪装成“恢复失败”。
        // 下次启动还会重新按数据库精确放行，先保留可审计日志。
        eprintln!("[媒体白名单] 恢复后刷新失败，重启后将重试：{err}");
    }
    Ok(protective)
}

// ───────────────────────── 设置：火山凭据（本地文件） ─────────────────────────

#[tauri::command]
pub fn secrets_get(state: State<'_, AppState>) -> R<MaskedVolcanoCreds> {
    secrets::load(&state.data_dir)
        .map(|creds| secrets::masked(&creds))
        .map_err(e)
}

#[tauri::command]
pub fn secrets_set(state: State<'_, AppState>, creds: VolcanoCreds) -> R<()> {
    secrets::merge_and_save(&state.data_dir, &creds).map_err(e)
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

// ───────────────────────── 批量导入 / 删减 ─────────────────────────

#[derive(serde::Deserialize)]
pub struct StudentRow {
    student_no: String,
    name: String,
}

#[derive(serde::Deserialize)]
pub struct ContentRow {
    content_no: String,
    title: String,
    answer_text: String,
}

#[derive(Serialize)]
pub struct BatchImport {
    ok: usize,
    failed: usize,
    errors: Vec<String>,
}

#[derive(Serialize)]
pub struct DeleteResult {
    deleted: usize,
    blocked: Vec<String>, // 有历史记录删不掉的（返回编号，提示改用停用）
}

/// 批量导入学生（每行 学号 + 姓名）。逐行 upsert。
#[tauri::command]
pub fn students_import(state: State<'_, AppState>, rows: Vec<StudentRow>) -> R<BatchImport> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let mut ok = 0;
    let mut errors = Vec::new();
    for r in &rows {
        let no = r.student_no.trim();
        let name = r.name.trim();
        if no.is_empty() || name.is_empty() {
            errors.push(format!("跳过空行: 学号='{no}' 姓名='{name}'"));
            continue;
        }
        match students::upsert(
            &conn,
            &StudentInput { student_no: no, name, class_id: None, enabled: true },
        ) {
            Ok(_) => ok += 1,
            Err(err) => errors.push(format!("{no} {name}: {err}")),
        }
    }
    Ok(BatchImport { ok, failed: errors.len(), errors })
}

#[tauri::command]
pub fn students_set_enabled(state: State<'_, AppState>, ids: Vec<i64>, enabled: bool) -> R<usize> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    students::set_enabled(&conn, &ids, enabled).map_err(e)
}

// ───────────────────────── 班级（分班） ─────────────────────────

#[tauri::command]
pub fn classes_list(state: State<'_, AppState>) -> R<Vec<Class>> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    classes::list(&conn).map_err(e)
}

/// 建班（同名去重；可绑教材）。
#[tauri::command]
pub fn class_create(
    state: State<'_, AppState>,
    name: String,
    textbook: Option<String>,
    term: Option<String>,
) -> R<Class> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    classes::create(&conn, &name, textbook.as_deref(), term.as_deref()).map_err(e)
}

/// 改班：改名 / 调教材 / 学期。
#[tauri::command]
pub fn class_update(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    textbook: Option<String>,
    term: Option<String>,
) -> R<Class> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    classes::update(&conn, id, &name, textbook.as_deref(), term.as_deref()).map_err(e)
}

/// 删班：该班学生自动移出（class_id=NULL），再删班级。
#[tauri::command]
pub fn class_delete(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    classes::delete(&conn, id).map_err(e)
}

/// 批量分班；`class_id = None` 即移出班级。
#[tauri::command]
pub fn students_set_class(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    class_id: Option<i64>,
) -> R<usize> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    students::set_class(&conn, &ids, class_id).map_err(e)
}

#[tauri::command]
pub fn students_delete(state: State<'_, AppState>, ids: Vec<i64>) -> R<DeleteResult> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let (deleted, blocked_ids) = students::delete(&conn, &ids).map_err(e)?;
    let mut blocked = Vec::new();
    for id in blocked_ids {
        if let Some(s) = students::get_by_id(&conn, id).map_err(e)? {
            blocked.push(format!("{} {}", s.student_no, s.name));
        }
    }
    Ok(DeleteResult { deleted, blocked })
}

/// 批量导入背诵内容（每行 编号 + 标题 + 答案）。逐行 upsert。
#[tauri::command]
pub fn contents_import(state: State<'_, AppState>, rows: Vec<ContentRow>) -> R<BatchImport> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let mut ok = 0;
    let mut errors = Vec::new();
    for r in &rows {
        let no = r.content_no.trim();
        let title = r.title.trim();
        let ans = r.answer_text.trim();
        if no.is_empty() || title.is_empty() || ans.is_empty() {
            errors.push(format!("跳过不完整行: 编号='{no}'（需 编号/标题/答案 三项）"));
            continue;
        }
        match contents::upsert(
            &conn,
            &ContentInput { content_no: no, title, answer_text: ans, subject_id: None, enabled: true },
        ) {
            Ok(_) => ok += 1,
            Err(err) => errors.push(format!("{no}: {err}")),
        }
    }
    Ok(BatchImport { ok, failed: errors.len(), errors })
}

#[tauri::command]
pub fn contents_set_enabled(state: State<'_, AppState>, ids: Vec<i64>, enabled: bool) -> R<usize> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    contents::set_enabled(&conn, &ids, enabled).map_err(e)
}

#[tauri::command]
pub fn contents_delete(state: State<'_, AppState>, ids: Vec<i64>) -> R<DeleteResult> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let (deleted, blocked_ids) = contents::delete(&conn, &ids).map_err(e)?;
    let mut blocked = Vec::new();
    for id in blocked_ids {
        if let Some(c) = contents::get_by_id(&conn, id).map_err(e)? {
            blocked.push(format!("{} {}", c.content_no, c.title));
        }
    }
    Ok(DeleteResult { deleted, blocked })
}

/// 解析背诵清单文本 → 一条条 content 候选（老师预览/编辑确认后再调 contents_import 入库）。
/// `prefix` = 学科册（如 "道法8上"），用于拼 content_no。不落库、无副作用。
#[tauri::command]
pub fn parse_syllabus(
    text: String,
    prefix: String,
) -> R<Vec<module_recitation::domain::syllabus::ParsedContent>> {
    Ok(module_recitation::domain::syllabus::parse_syllabus(&text, &prefix))
}

#[derive(Serialize)]
pub struct TaskGenerateDto {
    created: usize,
    revived: usize,
    skipped_open: usize,
    skipped_completed: usize,
    total_effective: usize,
}

/// 为今日批量生成"新背"任务。返回真实生成/跳过数量。
#[tauri::command]
pub fn tasks_generate(state: State<'_, AppState>, pairs: Vec<(i64, i64)>) -> R<TaskGenerateDto> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let date = today_str();
    let report = task_svc::generate_normal(&conn, &date, &pairs).map_err(e)?;
    Ok(TaskGenerateDto {
        created: report.created,
        revived: report.revived,
        skipped_open: report.skipped_open,
        skipped_completed: report.skipped_completed,
        total_effective: report.created + report.revived,
    })
}

/// 撤销一条「已布置」任务：设为 closed（不删行，避开外键）。
/// 只允许撤销尚未开始的任务（status=open）；已交/已判的不能撤。
#[tauri::command]
pub fn task_cancel(state: State<'_, AppState>, task_id: i64) -> R<()> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let t = tasks::get(&conn, task_id).map_err(e)?.ok_or_else(|| "任务不存在".to_string())?;
    if t.status != TaskStatus::Open {
        return Err("该任务已提交或已完成，不能撤销".to_string());
    }
    tasks::set_status(&conn, task_id, TaskStatus::Closed).map_err(e)
}

/// 覆盖已布置：把给定学生今日**未开始**的「新背」任务关闭，再按新内容重新布置。
/// 已开始（已交/已判）的旧任务不动，保留学生已完成的工作。返回新建任务数。
#[tauri::command]
pub fn tasks_reassign(
    state: State<'_, AppState>,
    student_ids: Vec<i64>,
    content_ids: Vec<i64>,
) -> R<usize> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let date = today_str();
    let sset: std::collections::HashSet<i64> = student_ids.iter().copied().collect();
    for t in tasks::list_by_date(&conn, MODULE, &date).map_err(e)? {
        if t.kind == TaskKind::Normal && t.status == TaskStatus::Open && sset.contains(&t.student_id) {
            tasks::set_status(&conn, t.id, TaskStatus::Closed).map_err(e)?;
        }
    }
    let pairs: Vec<(i64, i64)> = student_ids
        .iter()
        .flat_map(|&s| content_ids.iter().map(move |&c| (s, c)))
        .collect();
    let report = task_svc::generate_normal(&conn, &date, &pairs).map_err(e)?;
    Ok(report.created + report.revived)
}

/// 删除已布置内容：把给定学生今日、指定内容的「新背」任务全部关闭（含已开始的）。
/// 关闭只标 closed，不删提交/判定记录。返回关闭数。
#[tauri::command]
pub fn tasks_remove(
    state: State<'_, AppState>,
    student_ids: Vec<i64>,
    content_ids: Vec<i64>,
) -> R<usize> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let date = today_str();
    let sset: std::collections::HashSet<i64> = student_ids.iter().copied().collect();
    let cset: std::collections::HashSet<i64> = content_ids.iter().copied().collect();
    let mut n = 0;
    for t in tasks::list_by_date(&conn, MODULE, &date).map_err(e)? {
        if t.kind == TaskKind::Normal
            && t.status != TaskStatus::Closed
            && sset.contains(&t.student_id)
            && cset.contains(&t.ref_id)
        {
            tasks::set_status(&conn, t.id, TaskStatus::Closed).map_err(e)?;
            n += 1;
        }
    }
    Ok(n)
}

// ───────────────────────── 导入 + ASR 评分 ─────────────────────────

/// 导入历史一行（来自 submissions 表，含全部状态；切走再回来不丢）。
#[derive(Serialize)]
pub struct ImportHistoryRow {
    submission_id: i64,
    file_name: String,         // 当前文件名（智能识别改名后即「改后名」）
    student: Option<String>,   // 张明 (2023001)
    content: Option<String>,   // 道法8上-04课-05 为什么要以礼待人
    status: String,            // 已评分 / 未识别·待改派 / 待分析 …
    recognized: Option<String>,
}

fn history_status(s: &str) -> String {
    match s {
        "scored" => "已评分".into(),
        "confirmed" => "已确认".into(),
        "anomaly" => "未识别·待改派".into(),
        "staged" => "待分析".into(),
        "pending" => "待识别".into(),
        other => other.to_string(),
    }
}

/// 导入历史：最近的提交（默认 200 条），enrich 学生/内容/状态。复用现有表，不改表。
#[tauri::command]
pub fn import_history(state: State<'_, AppState>, limit: Option<i64>) -> R<Vec<ImportHistoryRow>> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let subs = submissions::list_recent(&conn, MODULE, limit.unwrap_or(200)).map_err(e)?;
    let mut out = Vec::with_capacity(subs.len());
    for s in subs {
        let student = match s.student_id {
            Some(id) => students::get_by_id(&conn, id)
                .map_err(e)?
                .map(|st| format!("{} ({})", st.name, st.student_no)),
            None => None,
        };
        let content = match s.ref_id {
            Some(id) => contents::get_by_id(&conn, id)
                .map_err(e)?
                .map(|c| format!("{} {}", c.content_no, c.title)),
            None => None,
        };
        let file_name = std::path::Path::new(&s.file_path)
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or(&s.file_path)
            .to_string();
        out.push(ImportHistoryRow {
            submission_id: s.id,
            file_name,
            student,
            content,
            status: if s.recognize_status == "failed" {
                "识别失败·待处理".into()
            } else if s.recognize_status == "processing" {
                "正在识别".into()
            } else {
                history_status(&s.status)
            },
            recognized: s.recognized_text,
        });
    }
    Ok(out)
}

fn anomaly_label(t: &str) -> String {
    match t {
        "parse_error" => "文件名无法解析".into(),
        "student_not_found" => "找不到对应学生".into(),
        "content_not_found" => "找不到对应内容".into(),
        "no_task" => "当日无对应任务".into(),
        other => other.to_string(),
    }
}

// ───────────────────────── 暂存导入（先导入、后分析）─────────────────────────

#[derive(Serialize)]
pub struct StageDto {
    file: String,
    status: String, // staged | duplicate | error
    detail: String,
}

/// 第一步「导入」：只做哈希 + 去重检查，**不调 ASR、不落库**。
/// 返回每个文件能否进入「待分析」队列；真正的识别评分由前端随后对 staged 文件调用
/// `import_autoname` 完成（即「开始分析」）。
#[tauri::command]
pub fn import_stage(state: State<'_, AppState>, paths: Vec<String>, force: Option<bool>) -> R<Vec<StageDto>> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let force = force.unwrap_or(false);
    let mut out = Vec::with_capacity(paths.len());
    for p in paths {
        let path = std::path::Path::new(&p);
        let hash = match suite_core::domain::hashing::sha256_file(path) {
            Ok(h) => h,
            Err(err) => {
                out.push(StageDto {
                    file: p.clone(),
                    status: "error".into(),
                    detail: format!("读取/哈希失败: {err}"),
                });
                continue;
            }
        };
        let dup = !force && file_ledger::get(&conn, &hash).map_err(e)?.is_some();
        out.push(if dup {
            StageDto {
                file: p.clone(),
                status: "duplicate".into(),
                detail: "重复文件（已导入过，跳过）".into(),
            }
        } else {
            StageDto { file: p.clone(), status: "staged".into(), detail: "已加入待分析队列".into() }
        });
    }
    Ok(out)
}

// ───────────────────────── 智能识别导入（按录音内容自动命名） ─────────────────────────

#[derive(Serialize)]
pub struct AutonameDto {
    file: String,
    status: String, // scored | rescored | duplicate | unmatched | error
    detail: String,
    new_name: Option<String>,
    student: Option<String>,
    content: Option<String>,
    accuracy: Option<f64>,
    pass: Option<bool>,
}

/// 内容匹配阈值（覆盖率%）：低于此值视为未能识别内容。
const MATCH_MIN: f64 = 50.0;

async fn run_recitation_asr(
    state: &State<'_, AppState>,
    submission_id: i64,
    audio_path: &str,
    input_hash: &str,
    creds: &VolcanoCreds,
    preclaimed: bool,
) -> R<crate::asr::AsrOutput> {
    let descriptor = ai_pipeline::AsrRunDescriptor {
        provider: "volcano",
        model_name: "bigmodel-recording-asr",
        model_version: crate::asr::resource_id(creds),
        config_version: "punc-itn-utterances-v1",
        prompt_or_rule_version: "recitation-asr-output-v1",
    };
    let begin = {
        let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        ai_pipeline::begin_asr_run(&conn, submission_id, input_hash, &descriptor, preclaimed)
            .map_err(e)?
    };
    let (ai_run_id, request_id) = match begin {
        ai_pipeline::BeginAsrRun::Cached { transcript } => {
            let words = ai_pipeline::transcript_words(&transcript).map_err(e)?;
            return Ok(crate::asr::AsrOutput {
                text: transcript.raw_transcript,
                words,
                duration_ms: transcript.duration_ms.max(0) as u64,
            });
        }
        ai_pipeline::BeginAsrRun::Execute {
            ai_run_id,
            request_id,
        } => (ai_run_id, request_id),
    };

    let tmp = std::env::temp_dir();
    let transcoded = crate::audio::transcode_to_wav16k(audio_path, &tmp);
    let asr_path = transcoded
        .as_ref()
        .and_then(|path| path.to_str())
        .unwrap_or(audio_path)
        .to_string();
    let recognized = crate::asr::recognize(creds, &asr_path, &request_id).await;
    if let Some(path) = transcoded.as_ref() {
        let _ = std::fs::remove_file(path);
    }

    let mut output = match recognized {
        Ok(output) => output,
        Err(error) => {
            let message = format!("识别失败: {error}");
            return match state.db.lock() {
                Ok(conn) => {
                    match ai_pipeline::finish_asr_failure(&conn, ai_run_id, submission_id, &message)
                    {
                        Ok(()) => Err(message),
                        Err(persist_error) => {
                            Err(format!("{message}；失败状态写入异常: {persist_error}"))
                        }
                    }
                }
                Err(_) => Err(format!("{message}；数据库忙，失败状态未能写入")),
            };
        }
    };
    if output.duration_ms == 0 {
        if let Some(duration_ms) = crate::audio::ffprobe_duration_ms(audio_path) {
            output.duration_ms = duration_ms;
        }
    }
    let normalized = suite_core::domain::normalize::normalize(
        &output.text,
        &suite_core::domain::normalize::NormalizeCfg::default(),
    );
    let finalization = {
        let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        ai_pipeline::finish_asr_success(
            &conn,
            ai_run_id,
            &ai_pipeline::FinishAsrSuccessInput {
                submission_id,
                raw_transcript: &output.text,
                normalized_transcript: &normalized,
                normalization_version: "recitation-normalize-v1",
                words: &output.words,
                duration_ms: output.duration_ms as i64,
            },
        )
    };
    if let Err(error) = finalization {
        let message = format!("识别结果账本写入失败: {error}");
        if let Ok(conn) = state.db.lock() {
            let _ = ai_pipeline::finish_asr_failure(&conn, ai_run_id, submission_id, &message);
        }
        return Err(message);
    }
    Ok(output)
}

/// 分析录音 → 识别学生/内容 → 自动重命名为 `日期_学号_姓名_内容编号` → 落库评分。
/// 录音内容约定为「姓名 + 日期 + 背诵内容」。日期暂用当天（后续可解析口述日期）。
#[tauri::command]
pub async fn import_autoname(state: State<'_, AppState>, paths: Vec<String>, force: Option<bool>) -> R<Vec<AutonameDto>> {
    let creds = secrets::load(&state.data_dir);
    let force = force.unwrap_or(false);
    let mut out = Vec::with_capacity(paths.len());

    for p in paths {
        let path = std::path::Path::new(&p);
        let fail = |detail: String| AutonameDto {
            file: p.clone(),
            status: "error".into(),
            detail,
            new_name: None,
            student: None,
            content: None,
            accuracy: None,
            pass: None,
        };

        // 哈希 + 去重（同步段）
        let hash = match suite_core::domain::hashing::sha256_file(path) {
            Ok(h) => h,
            Err(err) => {
                out.push(fail(format!("哈希失败: {err}")));
                continue;
            }
        };
        let existing_submission_id = {
            let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
            if let Some(hit) = file_ledger::get(&conn, &hash).map_err(e)? {
                hit.submission_id
            } else {
                submissions::get_by_hash(&conn, &hash).map_err(e)?.map(|s| s.id)
            }
        };
        if existing_submission_id.is_some() && !force {
            out.push(AutonameDto {
                file: p.clone(),
                status: "duplicate".into(),
                detail: "重复文件（已跳过；勾选忽略重复可重新分析既有记录）".into(),
                new_name: None,
                student: None,
                content: None,
                accuracy: None,
                pass: None,
            });
            continue;
        }

        let archived = match crate::archive::archive_audio(
            path,
            &hash,
            &state.data_dir.join("archive"),
        ) {
            Ok(archived) => archived,
            Err(err) => {
                out.push(fail(format!("录音归档失败: {err}")));
                continue;
            }
        };
        let archived_path = match archived.path.to_str() {
            Some(path) => path.to_string(),
            None => {
                archived.rollback_new_file();
                out.push(fail("录音归档路径不是有效 UTF-8".into()));
                continue;
            }
        };

        // 先创建/抢占可追踪 submission，再释放锁调用外部 ASR。
        let tracking_id = {
            let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
            match import::prepare_tracking_submission(
                &conn,
                existing_submission_id,
                &p,
                &hash,
                Some(&archived_path),
            ) {
                Ok(id) => id,
                Err(err) => {
                    archived.rollback_new_file();
                    out.push(fail(err.to_string()));
                    continue;
                }
            }
        };

        let creds = match &creds {
            Ok(creds) => creds,
            Err(err) => {
                let message = format!("读取本机 ASR 凭据失败: {err}");
                if let Ok(conn) = state.db.lock() {
                    let _ = recognition::mark_failed(&conn, tracking_id, &message);
                }
                out.push(fail(message));
                continue;
            }
        };

        // ASR 网络调用在锁外执行；运行账本与 submission 只在前后短事务中写入。
        let asr = match run_recitation_asr(&state, tracking_id, &archived_path, &hash, creds, true)
            .await
        {
            Ok(a) => a,
            Err(message) => {
                out.push(fail(message));
                continue;
            }
        };
        let dur = if asr.duration_ms > 0 {
            Some(asr.duration_ms as i64)
        } else {
            crate::audio::ffprobe_duration_ms(&archived_path).map(|d| d as i64)
        };

        // 匹配 + 落库 + 评分（同步段）
        let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        let rcfg = RecitationConfig::load(&conn).map_err(e)?;
        let scfg = rcfg.to_score_cfg();
        let student = matching::find_student(&conn, &asr.text).map_err(e)?;
        let content = matching::best_content(&conn, &asr.text, &scfg.normalize, &scfg.accuracy).map_err(e)?;

        match (student, content) {
            (Some(s), Some((c, score))) if score >= MATCH_MIN => {
                let reused = existing_submission_id.is_some();
                let r = match import::finish_resolved_recognition(
                    &conn,
                    &import::ResolvedRecognition {
                        submission_id: tracking_id,
                        student_id: s.id,
                        content_id: c.id,
                        recognized_text: &asr.text,
                        duration_ms: dur,
                        words: &asr.words,
                        cfg: &scfg,
                    },
                ) {
                    Ok(result) => result,
                    Err(err) => {
                        let message = format!("识别结果入库失败: {err}");
                        let _ = recognition::mark_failed(&conn, tracking_id, &message);
                        out.push(fail(message));
                        continue;
                    }
                };
                let structured_warning =
                    structured_scoring::record_score_if_ready(&conn, tracking_id, &scfg)
                        .err()
                        .map(|error| format!("逐点评分待重试：{error}"));
                let (new_name, mut detail) = if reused {
                    (None, format!("重新分析既有记录 · 匹配度 {score:.0}%"))
                } else {
                    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("m4a");
                    let name = format!(
                        "{}_{}_{}_{}.{}",
                        today_str().replace('-', ""),
                        s.student_no,
                        s.name,
                        c.content_no,
                        ext
                    );
                    let detail = match path.parent() {
                        Some(dir) => {
                            let np = dir.join(&name);
                            match std::fs::rename(&p, &np) {
                                Ok(()) => {
                                    let final_path = np.to_string_lossy().to_string();
                                    submissions::set_file_path(&conn, tracking_id, &final_path).map_err(e)?;
                                    format!("匹配度 {score:.0}%")
                                }
                                Err(err) => format!("匹配度 {score:.0}%；文件重命名失败，已保留原路径：{err}"),
                            }
                        }
                        None => format!("匹配度 {score:.0}%；文件无父目录，已保留原路径"),
                    };
                    (Some(name), detail)
                };
                if let Some(warning) = structured_warning {
                    detail.push_str(&format!("；{warning}"));
                }
                out.push(AutonameDto {
                    file: p.clone(),
                    status: if reused { "rescored".into() } else { "scored".into() },
                    detail,
                    new_name,
                    student: Some(format!("{} {}", s.student_no, s.name)),
                    content: Some(format!("{} {}", c.content_no, c.title)),
                    accuracy: Some(r.accuracy),
                    pass: Some(r.pass),
                });
            }
            _ => {
                if let Err(err) = import::finish_unmatched_recognition(
                    &conn,
                    tracking_id,
                    &asr.text,
                    dur,
                    existing_submission_id.is_none(),
                ) {
                    let message = format!("识别结果入库失败: {err}");
                    let _ = recognition::mark_failed(&conn, tracking_id, &message);
                    out.push(fail(message));
                    continue;
                }
                if existing_submission_id.is_some() {
                    out.push(AutonameDto {
                        file: p.clone(),
                        status: "unmatched".into(),
                        detail: "重新识别后仍未能匹配学生/内容，既有记录未新增副本".into(),
                        new_name: None,
                        student: None,
                        content: None,
                        accuracy: None,
                        pass: None,
                    });
                    continue;
                }
                out.push(AutonameDto {
                    file: p.clone(),
                    status: "unmatched".into(),
                    detail: "未能识别学生/内容，已入异常池待人工".into(),
                    new_name: None,
                    student: None,
                    content: None,
                    accuracy: None,
                    pass: None,
                });
            }
        }
    }
    Ok(out)
}

// ───────────────────────── 异常池 ─────────────────────────

#[derive(Serialize)]
pub struct AnomalyDto {
    submission_id: i64,
    file_path: String,
    anomaly_type: String,
    parsed_meta: Option<String>,
    student_id: Option<i64>,
    ref_id: Option<i64>,
}

#[tauri::command]
pub fn anomalies_list(state: State<'_, AppState>) -> R<Vec<AnomalyDto>> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let subs = submissions::list_anomalies(&conn, MODULE).map_err(e)?;
    Ok(subs
        .into_iter()
        .map(|s| {
            let audio_path = playback_path(&s);
            AnomalyDto {
                submission_id: s.id,
                file_path: audio_path,
                anomaly_type: anomaly_label(&s.anomaly_type.unwrap_or_default()),
                parsed_meta: s.parsed_meta,
                student_id: s.student_id,
                ref_id: s.ref_id,
            }
        })
        .collect())
}

#[derive(Serialize)]
pub struct RecognitionFailureDto {
    submission_id: i64,
    file_path: String,
    error_code: String,
    error_message: String,
    retryable: bool,
    failed_at: String,
    attempts: u32,
    has_task: bool,
    file_missing: bool,
}

#[tauri::command]
pub fn recognition_failures_list(state: State<'_, AppState>) -> R<Vec<RecognitionFailureDto>> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let failures = submissions::list_recognition_failures(&conn, MODULE).map_err(e)?;
    Ok(failures
        .into_iter()
        .map(|submission| {
            let audio_path = playback_path(&submission);
            let meta = recognition::parse_failure_meta(submission.recognize_meta.as_deref())
                .unwrap_or_else(|| recognition::RecognitionFailureMeta {
                    error_code: "unknown".into(),
                    error_message: "识别失败，未找到结构化错误信息".into(),
                    retryable: false,
                    failed_at: "未知".into(),
                    attempts: 0,
                });
            RecognitionFailureDto {
                submission_id: submission.id,
                file_missing: !std::path::Path::new(&audio_path).is_file(),
                file_path: audio_path,
                error_code: meta.error_code,
                error_message: meta.error_message,
                retryable: meta.retryable,
                failed_at: meta.failed_at,
                attempts: meta.attempts,
                has_task: submission.task_id.is_some(),
            }
        })
        .collect())
}

#[tauri::command]
pub fn recognition_relocate(
    state: State<'_, AppState>,
    submission_id: i64,
    new_path: String,
) -> R<()> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    recognition::relocate_same_hash(&conn, submission_id, std::path::Path::new(&new_path))
        .map_err(e)
}

#[tauri::command]
pub fn recognition_void(state: State<'_, AppState>, submission_id: i64) -> R<bool> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    recognition::void_unconfirmed(&conn, submission_id).map_err(e)
}

/// 人工改派：传正确学号/内容编号（任一可空，沿用原值）。返回挂到的任务 id。
#[tauri::command]
pub fn anomaly_reassign(
    state: State<'_, AppState>,
    submission_id: i64,
    student_no: Option<String>,
    content_no: Option<String>,
) -> R<Option<i64>> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let sid = match student_no.as_deref() {
        Some(no) if !no.is_empty() => Some(
            students::get_by_no(&conn, no)
                .map_err(e)?
                .ok_or_else(|| format!("找不到学号 {no}"))?
                .id,
        ),
        _ => None,
    };
    let rid = match content_no.as_deref() {
        Some(no) if !no.is_empty() => Some(
            contents::get_by_no(&conn, no)
                .map_err(e)?
                .ok_or_else(|| format!("找不到内容编号 {no}"))?
                .id,
        ),
        _ => None,
    };
    import::reassign(&conn, submission_id, sid, rid).map_err(e)
}

#[derive(Serialize)]
pub struct ScoreOutcomeDto {
    verdict_id: i64,
    accuracy: f64,
    pass: bool,
    fluency: f64,
    quality: String,
    text: String,
    next: String,
    structured_score_run_id: Option<i64>,
    structured_warning: Option<String>,
}

fn persist_asr_failure(state: &State<'_, AppState>, submission_id: i64, message: &str) -> String {
    match state.db.lock() {
        Ok(conn) => match recognition::mark_failed(&conn, submission_id, message) {
            Ok(_) => message.to_string(),
            Err(err) => format!("{message}；失败状态写入异常: {err}"),
        },
        Err(_) => format!("{message}；数据库忙，失败状态未能写入"),
    }
}

/// 对一条提交跑火山 ASR 并评分（异步）。识别文本与词级时间戳回写后入评分编排。
#[tauri::command]
pub async fn asr_and_score(state: State<'_, AppState>, submission_id: i64) -> R<ScoreOutcomeDto> {
    let (audio_path, input_hash) = {
        let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        let sub = submissions::get(&conn, submission_id)
            .map_err(e)?
            .ok_or_else(|| "提交不存在".to_string())?;
        (playback_path(&sub), sub.file_hash.clone())
    };
    let creds = secrets::load(&state.data_dir)
        .map_err(|err| persist_asr_failure(&state, submission_id, &err.to_string()))?;

    let out = run_recitation_asr(
        &state,
        submission_id,
        &audio_path,
        &input_hash,
        &creds,
        false,
    )
    .await?;
    // 同步段：回写识别 + 评分
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let cfg = RecitationConfig::load(&conn).map_err(e)?.to_score_cfg();
    let r = match scoring::finish_recognition_and_score(
        &conn,
        submission_id,
        &out.text,
        Some(out.duration_ms as i64),
        &out.words,
        &cfg,
    ) {
        Ok(result) => result,
        Err(err) => {
            let message = format!("识别结果入库失败: {err}");
            let _ = recognition::mark_failed(&conn, submission_id, &message);
            return Err(message);
        }
    };
    let (structured_score_run_id, structured_warning) =
        match structured_scoring::record_score_if_ready(&conn, submission_id, &cfg) {
            Ok(score) => (score.map(|value| value.id), None),
            Err(error) => (None, Some(format!("逐点评分待重试：{error}"))),
        };
    Ok(ScoreOutcomeDto {
        verdict_id: r.verdict_id,
        accuracy: r.accuracy,
        pass: r.pass,
        fluency: r.fluency,
        quality: r.quality,
        text: out.text,
        next: format!("{:?}", r.next),
        structured_score_run_id,
        structured_warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine_verdict(pass: bool, human_result: Option<&str>) -> Verdict {
        Verdict {
            id: 1,
            submission_id: 1,
            module: MODULE,
            primary_score: Some(if pass { 95.0 } else { 60.0 }),
            pass: Some(pass),
            secondary_score: None,
            quality: None,
            confidence: None,
            answer_version: 1,
            metrics_json: None,
            machine_note: None,
            human_result: human_result.map(str::to_string),
            human_note: None,
        }
    }

    #[test]
    fn pending_review_only_counts_unconfirmed_machine_verdicts() {
        let machine_pass = machine_verdict(true, None);
        let machine_fail = machine_verdict(false, None);
        let confirmed = machine_verdict(true, Some("pass"));

        let cases = [
            ("机器 pass 未确认", Some(&machine_pass), true),
            ("机器 fail 未确认", Some(&machine_fail), true),
            ("人工已确认", Some(&confirmed), false),
            ("ASR failed 无 verdict", None, false),
            ("待评分无 verdict", None, false),
        ];

        assert_eq!(
            cases
                .iter()
                .filter(|(_, verdict, _)| is_pending_teacher_review(*verdict))
                .count(),
            2
        );
        for (label, verdict, expected) in cases {
            assert_eq!(is_pending_teacher_review(verdict), expected, "{label}");
        }
    }

    #[test]
    fn playback_prefers_archive_and_falls_back_to_original() {
        let root = std::env::temp_dir().join(format!("jiaofu-playback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let original = root.join("original.m4a");
        let archived = root.join("archived.m4a");
        std::fs::write(&original, b"original").unwrap();
        std::fs::write(&archived, b"archived").unwrap();

        assert_eq!(
            preferred_playback_path(
                original.to_str().unwrap(),
                Some(archived.to_str().unwrap())
            ),
            archived.to_string_lossy()
        );
        std::fs::remove_file(&archived).unwrap();
        assert_eq!(
            preferred_playback_path(
                original.to_str().unwrap(),
                Some(archived.to_str().unwrap())
            ),
            original.to_string_lossy()
        );
    }
}
