//! 改作业模块 Tauri 命令：知识点树 / 题库 / 豆包视觉预分析。

use serde::{Deserialize, Serialize};
use tauri::State;

use module_exam::db::knowledge_points::{self as kp, KnowledgePoint, KpInput};
use module_exam::db::questions::{self, NewOption, NewQuestion, Question, QuestionOption};
use module_exam::service::assessment::{self, GradeDecision, Publication};
use module_exam::service::grading::{self, AnswerDetail};
use module_exam::service::objective::{
    self, ObjectiveReviewBatch, ObjectiveWorkbench, StrictBatchReview,
};
use module_exam::vlm::{self as exam_vlm, AnalyzedQuestion};

use crate::secrets;
use crate::state::AppState;
use crate::vlm;

type R<T> = Result<T, String>;
const LOCAL_TEACHER_ACTOR: &str = "teacher";

fn e<E: ToString>(err: E) -> String {
    err.to_string()
}
fn lock<'a>(state: &'a State<'a, AppState>) -> R<std::sync::MutexGuard<'a, rusqlite::Connection>> {
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
        &KpInput {
            subject_id,
            parent_id,
            code: code.as_deref(),
            name: &name,
        },
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
    #[serde(default = "default_max_score")]
    pub max_score: f64,
    pub enabled: bool,
    pub options: Vec<OptionInput>,
}

fn default_max_score() -> f64 {
    1.0
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
    questions::create_with_options(
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
            max_score: q.max_score,
            enabled: q.enabled,
        },
        &map_options(&q.options),
    )
    .map_err(e)
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

// ───────────────────────── 客观题批改 ─────────────────────────

/// 生成机器建议，不产生错题/掌握度副作用。
#[tauri::command]
pub fn exam_answer_suggest(
    state: State<'_, AppState>,
    student_id: i64,
    question_id: i64,
    picked: String,
) -> R<AnswerDetail> {
    let conn = lock(&state)?;
    grading::suggest_answer(&conn, student_id, question_id, &picked).map_err(e)
}

/// 老师确认或改判；确认后才更新最终得分、错题本和掌握度。
#[tauri::command]
pub fn exam_answer_human_decide(
    state: State<'_, AppState>,
    answer_id: i64,
    is_correct: bool,
    note: Option<String>,
) -> R<AnswerDetail> {
    let conn = lock(&state)?;
    grading::human_decide(&conn, answer_id, is_correct, note.as_deref()).map_err(e)
}

#[tauri::command]
pub fn exam_answers_list(state: State<'_, AppState>, limit: Option<i64>) -> R<Vec<AnswerDetail>> {
    let conn = lock(&state)?;
    grading::list_answers(&conn, limit.unwrap_or(50)).map_err(e)
}

// ───────────────────── T6 标准卷按题终审 ─────────────────────

/// 只读工作台：聚合当前 observation、机器建议、老师 revision 和发布预览。
#[tauri::command]
pub fn exam_objective_workbench(
    state: State<'_, AppState>,
    assessment_version_id: Option<i64>,
    limit: Option<i64>,
) -> R<ObjectiveWorkbench> {
    let conn = lock(&state)?;
    objective::list_objective_workbench(&conn, assessment_version_id, limit.unwrap_or(500))
        .map_err(e)
}

/// 老师逐条接受当前客观题建议；不会自动发布成绩。
#[tauri::command]
pub fn exam_objective_accept(state: State<'_, AppState>, suggestion_id: i64) -> R<GradeDecision> {
    let conn = lock(&state)?;
    objective::accept_objective_suggestion(&conn, suggestion_id, LOCAL_TEACHER_ACTOR).map_err(e)
}

/// 老师对异常记录人工记分；写 teacher_corrected revision，仍不自动发布。
#[tauri::command]
pub fn exam_objective_correct(
    state: State<'_, AppState>,
    suggestion_id: i64,
    teacher_score: f64,
    teacher_note: Option<String>,
) -> R<GradeDecision> {
    let conn = lock(&state)?;
    objective::correct_objective_suggestion(
        &conn,
        suggestion_id,
        teacher_score,
        teacher_note.as_deref(),
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)
}

/// 严格批量确认：服务层会逐条记录纳入/排除原因，且整批原子提交。
#[tauri::command]
pub fn exam_objective_strict_batch_accept(
    state: State<'_, AppState>,
    suggestion_ids: Vec<i64>,
    confidence_threshold: Option<f64>,
    idempotency_key: String,
) -> R<ObjectiveReviewBatch> {
    let conn = lock(&state)?;
    objective::strict_batch_accept(
        &conn,
        &StrictBatchReview {
            suggestion_ids: &suggestion_ids,
            confidence_threshold: confidence_threshold
                .unwrap_or(objective::DEFAULT_STRICT_BATCH_CONFIDENCE),
            reviewed_by: LOCAL_TEACHER_ACTOR,
            idempotency_key: &idempotency_key,
        },
    )
    .map_err(e)
}

/// 老师显式发布单份 attempt；只有全部题目已终审时服务层才放行。
#[tauri::command]
pub fn exam_objective_publish_attempt(
    state: State<'_, AppState>,
    attempt_id: i64,
) -> R<Publication> {
    let conn = lock(&state)?;
    assessment::publish_attempt(&conn, attempt_id, LOCAL_TEACHER_ACTOR).map_err(e)
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
