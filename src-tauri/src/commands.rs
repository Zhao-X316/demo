//! Tauri 命令层（薄封装，调用已测好的 core/模块服务）。

use chrono::{Duration, NaiveDate, Utc};
use serde::Serialize;
use tauri::State;

use module_recitation::config::RecitationConfig;
use module_recitation::db::contents::{self, ContentInput, RecContent};
use module_recitation::service::{import, matching, scoring, tasks as task_svc};
use suite_core::db::repo::students::{self, StudentInput};
use suite_core::db::repo::{file_ledger, submissions, tasks, verdicts};
use suite_core::models::{MediaType, ModuleKey, Student, TaskKind, TaskStatus};

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
    file_path: String,
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
                    file_path: s.file_path,
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

// ───────────────────────── 导入 + ASR 评分 ─────────────────────────

#[derive(Serialize)]
pub struct ImportResultDto {
    file: String,
    status: String, // imported | duplicate | anomaly | error
    detail: String,
    submission_id: Option<i64>,
}

/// 批量导入音频文件：计算哈希 → 解析文件名 → 入库/去重/异常。
#[tauri::command]
pub fn import_paths(state: State<'_, AppState>, paths: Vec<String>) -> R<Vec<ImportResultDto>> {
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let mut out = Vec::with_capacity(paths.len());
    for p in paths {
        let path = std::path::Path::new(&p);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let hash = match suite_core::domain::hashing::sha256_file(path) {
            Ok(h) => h,
            Err(err) => {
                out.push(ImportResultDto { file: p.clone(), status: "error".into(), detail: format!("读取/哈希失败: {err}"), submission_id: None });
                continue;
            }
        };
        let item = import::ImportItem { file_path: &p, file_stem: &stem, file_hash: &hash, duration_ms: None };
        let dto = match import::import_one(&conn, &item) {
            Ok(import::ImportOutcome::Imported { submission_id, warning, .. }) => ImportResultDto {
                file: p.clone(),
                status: "imported".into(),
                detail: warning.unwrap_or_else(|| "已导入".into()),
                submission_id: Some(submission_id),
            },
            Ok(import::ImportOutcome::Duplicate { existing_submission_id }) => ImportResultDto {
                file: p.clone(),
                status: "duplicate".into(),
                detail: "重复文件（已跳过）".into(),
                submission_id: existing_submission_id,
            },
            Ok(import::ImportOutcome::Anomaly { submission_id, anomaly_type }) => ImportResultDto {
                file: p.clone(),
                status: "anomaly".into(),
                detail: anomaly_label(&anomaly_type),
                submission_id: Some(submission_id),
            },
            Err(err) => ImportResultDto { file: p.clone(), status: "error".into(), detail: err.to_string(), submission_id: None },
        };
        out.push(dto);
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

// ───────────────────────── 智能识别导入（按录音内容自动命名） ─────────────────────────

#[derive(Serialize)]
pub struct AutonameDto {
    file: String,
    status: String, // scored | duplicate | unmatched | error
    detail: String,
    new_name: Option<String>,
    student: Option<String>,
    content: Option<String>,
    accuracy: Option<f64>,
    pass: Option<bool>,
}

/// 内容匹配阈值（覆盖率%）：低于此值视为未能识别内容。
const MATCH_MIN: f64 = 50.0;

/// 分析录音 → 识别学生/内容 → 自动重命名为 `日期_学号_姓名_内容编号` → 落库评分。
/// 录音内容约定为「姓名 + 日期 + 背诵内容」。日期暂用当天（后续可解析口述日期）。
#[tauri::command]
pub async fn import_autoname(state: State<'_, AppState>, paths: Vec<String>) -> R<Vec<AutonameDto>> {
    let creds = secrets::load(&state.data_dir).map_err(e)?;
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
        let dup = {
            let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
            file_ledger::get(&conn, &hash).map_err(e)?.is_some()
        };
        if dup {
            out.push(AutonameDto {
                file: p.clone(),
                status: "duplicate".into(),
                detail: "重复文件（已跳过）".into(),
                new_name: None,
                student: None,
                content: None,
                accuracy: None,
                pass: None,
            });
            continue;
        }

        // 转码 + ASR（异步段，不持锁）
        let tmp = std::env::temp_dir();
        let transcoded = crate::audio::transcode_to_wav16k(&p, &tmp);
        let asr_path = transcoded.as_ref().and_then(|x| x.to_str()).unwrap_or(&p).to_string();
        let asr = crate::asr::recognize(&creds, &asr_path, &hash).await;
        if let Some(t) = transcoded {
            let _ = std::fs::remove_file(t);
        }
        let asr = match asr {
            Ok(a) => a,
            Err(err) => {
                out.push(fail(format!("识别失败: {err}")));
                continue;
            }
        };
        let dur = if asr.duration_ms > 0 {
            Some(asr.duration_ms as i64)
        } else {
            crate::audio::ffprobe_duration_ms(&p).map(|d| d as i64)
        };

        // 匹配 + 落库 + 评分（同步段）
        let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        let rcfg = RecitationConfig::load(&conn).map_err(e)?;
        let scfg = rcfg.to_score_cfg();
        let student = matching::find_student(&conn, &asr.text).map_err(e)?;
        let content = matching::best_content(&conn, &asr.text, &scfg.normalize, &scfg.accuracy).map_err(e)?;

        match (student, content) {
            (Some(s), Some((c, score))) if score >= MATCH_MIN => {
                let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("m4a");
                let new_name =
                    format!("{}_{}_{}_{}.{}", today_str().replace('-', ""), s.student_no, s.name, c.content_no, ext);
                let final_path = match path.parent() {
                    Some(dir) => {
                        let np = dir.join(&new_name);
                        if std::fs::rename(&p, &np).is_ok() {
                            np.to_string_lossy().to_string()
                        } else {
                            p.clone()
                        }
                    }
                    None => p.clone(),
                };
                let (sub_id, _t) =
                    import::import_resolved(&conn, &final_path, &hash, s.id, c.id, dur).map_err(e)?;
                submissions::set_recognition(&conn, sub_id, Some(&asr.text), "ok", None, dur).map_err(e)?;
                let r = scoring::score_submission(&conn, sub_id, &asr.words, today_naive(), &scfg).map_err(e)?;
                out.push(AutonameDto {
                    file: p.clone(),
                    status: "scored".into(),
                    detail: format!("匹配度 {score:.0}%"),
                    new_name: Some(new_name),
                    student: Some(format!("{} {}", s.student_no, s.name)),
                    content: Some(format!("{} {}", c.content_no, c.title)),
                    accuracy: Some(r.accuracy),
                    pass: Some(r.pass),
                });
            }
            _ => {
                let meta = serde_json::json!({ "asr": asr.text }).to_string();
                let sub_id = submissions::insert(
                    &conn,
                    &submissions::NewSubmission {
                        module: MODULE,
                        task_id: None,
                        student_id: None,
                        ref_id: None,
                        media_type: MediaType::Audio,
                        file_path: &p,
                        file_hash: &hash,
                        duration_ms: dur,
                        parsed_meta: Some(&meta),
                        anomaly_type: Some("autoname_unmatched"),
                        status: "anomaly",
                    },
                )
                .map_err(e)?;
                file_ledger::record(&conn, &hash, &p, sub_id).map_err(e)?;
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
        .map(|s| AnomalyDto {
            submission_id: s.id,
            file_path: s.file_path,
            anomaly_type: anomaly_label(&s.anomaly_type.unwrap_or_default()),
            parsed_meta: s.parsed_meta,
            student_id: s.student_id,
            ref_id: s.ref_id,
        })
        .collect())
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
}

/// 对一条提交跑火山 ASR 并评分（异步）。识别文本与词级时间戳回写后入评分编排。
#[tauri::command]
pub async fn asr_and_score(state: State<'_, AppState>, submission_id: i64) -> R<ScoreOutcomeDto> {
    // 同步段：取音频路径 + 凭据（不可跨 await 持锁）
    let (audio_path, req_id, creds) = {
        let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        let sub = submissions::get(&conn, submission_id)
            .map_err(e)?
            .ok_or_else(|| "提交不存在".to_string())?;
        let creds = secrets::load(&state.data_dir).map_err(e)?;
        (sub.file_path.clone(), sub.file_hash.clone(), creds)
    };

    // 可选 ffmpeg 转码（提升火山兼容性），失败则用原文件
    let tmp = std::env::temp_dir();
    let transcoded = crate::audio::transcode_to_wav16k(&audio_path, &tmp);
    let asr_path = transcoded
        .as_ref()
        .and_then(|p| p.to_str())
        .unwrap_or(&audio_path)
        .to_string();

    // 异步段：调用火山
    let mut out = crate::asr::recognize(&creds, &asr_path, &req_id).await?;
    if out.duration_ms == 0 {
        if let Some(d) = crate::audio::ffprobe_duration_ms(&audio_path) {
            out.duration_ms = d;
        }
    }
    if let Some(p) = transcoded {
        let _ = std::fs::remove_file(p);
    }

    // 同步段：回写识别 + 评分
    let conn = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    submissions::set_recognition(&conn, submission_id, Some(&out.text), "ok", None, Some(out.duration_ms as i64)).map_err(e)?;
    let cfg = RecitationConfig::load(&conn).map_err(e)?.to_score_cfg();
    let r = scoring::score_submission(&conn, submission_id, &out.words, today_naive(), &cfg).map_err(e)?;
    Ok(ScoreOutcomeDto {
        verdict_id: r.verdict_id,
        accuracy: r.accuracy,
        pass: r.pass,
        fluency: r.fluency,
        quality: r.quality,
        text: out.text,
        next: format!("{:?}", r.next),
    })
}
