//! 改作业模块 Tauri 命令：知识点树 / 题库 / 豆包视觉预分析。

use serde::{Deserialize, Serialize};
use tauri::State;

use module_exam::db::knowledge_points::{self as kp, KnowledgePoint, KpInput};
use module_exam::db::questions::{self, NewOption, NewQuestion, Question, QuestionOption};
use module_exam::vlm::{self as exam_vlm, AnalyzedQuestion};

use crate::secrets;
use crate::state::AppState;
use crate::vlm;

type R<T> = Result<T, String>;
fn e<E: ToString>(err: E) -> String {
    err.to_string()
}
fn lock(state: &State<'_, AppState>) -> R<std::sync::MutexGuard<'_, rusqlite::Connection>> {
    state.db.lock().map_err(|_| "数据库忙".to_string())
}

// ───────────────────────── 知识点 ─────────────────────────

#[tauri::command]
pub fn kp_list(state: State<'_, AppState>) -> R<Vec<KnowledgePoint>> {
    let conn = lock(&state)?;
    kp::list(&conn).map_err(e)
}

#[tauri::command]
pub fn kp_create(
    state: State<'_, AppState>,
    subject_id: Option<i64>,
    parent_id: Option<i64>,
    code: Option<String>,
    name: String,
) -> R<KnowledgePoint> {
    let conn = lock(&state)?;
    kp::create(
        &conn,
        &KpInput { subject_id, parent_id, code: code.as_deref(), name: &name },
    )
    .map_err(e)
}

#[tauri::command]
pub fn kp_rename(state: State<'_, AppState>, id: i64, name: String) -> R<()> {
    let conn = lock(&state)?;
    kp::rename(&conn, id, &name).map_err(e)
}

#[tauri::command]
pub fn kp_delete(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = lock(&state)?;
    kp::delete(&conn, id).map_err(e)
}

// ───────────────────────── 题库 ─────────────────────────

#[derive(Deserialize)]
pub struct OptionInput {
    pub label: String,
    pub content: String,
    pub is_correct: bool,
    pub knowledge_point_id: Option<i64>,
    pub analysis: Option<String>,
    pub ord: i64,
}

#[derive(Deserialize)]
pub struct QuestionInput {
    pub subject_id: Option<i64>,
    pub question_no: Option<String>,
    pub qtype: String,
    pub stem: String,
    pub image_path: Option<String>,
    pub correct_answer: Option<String>,
    pub knowledge_point_id: Option<i64>,
    pub difficulty: Option<i64>,
    pub analysis: Option<String>,
    pub enabled: bool,
    pub options: Vec<OptionInput>,
}

#[derive(Serialize)]
pub struct QuestionDetail {
    pub question: Question,
    pub options: Vec<QuestionOption>,
}

fn map_options(input: &[OptionInput]) -> Vec<NewOption<'_>> {
    input
        .iter()
        .map(|o| NewOption {
            label: &o.label,
            content: &o.content,
            is_correct: o.is_correct,
            knowledge_point_id: o.knowledge_point_id,
            analysis: o.analysis.as_deref(),
            ord: o.ord,
        })
        .collect()
}

/// 新建题目（含选项），返回新 id。
#[tauri::command]
pub fn question_create(state: State<'_, AppState>, q: QuestionInput) -> R<i64> {
    let conn = lock(&state)?;
    let id = questions::create_question(
        &conn,
        &NewQuestion {
            subject_id: q.subject_id,
            question_no: q.question_no.as_deref(),
            qtype: &q.qtype,
            stem: &q.stem,
            image_path: q.image_path.as_deref(),
            correct_answer: q.correct_answer.as_deref(),
            knowledge_point_id: q.knowledge_point_id,
            difficulty: q.difficulty,
            analysis: q.analysis.as_deref(),
            enabled: q.enabled,
        },
    )
    .map_err(e)?;
    questions::set_options(&conn, id, &map_options(&q.options)).map_err(e)?;
    Ok(id)
}

/// 重设某题选项（教师审核 VLM 结果后保存）。
#[tauri::command]
pub fn question_set_options(
    state: State<'_, AppState>,
    question_id: i64,
    options: Vec<OptionInput>,
) -> R<()> {
    let conn = lock(&state)?;
    questions::set_options(&conn, question_id, &map_options(&options)).map_err(e)
}

#[tauri::command]
pub fn questions_list(state: State<'_, AppState>) -> R<Vec<Question>> {
    let conn = lock(&state)?;
    questions::list(&conn).map_err(e)
}

#[tauri::command]
pub fn question_get(state: State<'_, AppState>, id: i64) -> R<Option<QuestionDetail>> {
    let conn = lock(&state)?;
    Ok(questions::get(&conn, id)
        .map_err(e)?
        .map(|(question, options)| QuestionDetail { question, options }))
}

#[tauri::command]
pub fn question_delete(state: State<'_, AppState>, id: i64) -> R<()> {
    let conn = lock(&state)?;
    questions::delete(&conn, id).map_err(e)
}

// ───────────────────────── 豆包视觉：题目预分析 ─────────────────────────

/// 传一张题目图片，调用豆包视觉大模型预分析，返回结构化题目（供教师审核后入库）。
#[tauri::command]
pub async fn question_vlm_analyze(
    state: State<'_, AppState>,
    image_path: String,
) -> R<AnalyzedQuestion> {
    let creds = secrets::load(&state.data_dir).map_err(e)?;
    let content = vlm::chat_vision(&creds, &image_path, exam_vlm::QUESTION_ANALYSIS_PROMPT).await?;
    exam_vlm::parse_question_analysis(&content).map_err(e)
}
