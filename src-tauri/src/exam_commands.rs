//! 改作业模块 Tauri 命令：知识点树 / 题库 / 豆包视觉预分析。

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use module_exam::db::knowledge_points::{self as kp, KnowledgePoint, KpInput};
use module_exam::db::questions::{self, NewOption, NewQuestion, Question, QuestionOption};
use module_exam::objective_recognition::{
    ObjectiveRecognitionErrorCode, ObjectiveRecognitionFailure, ObjectiveRecognizer,
    OBJECTIVE_RECOGNITION_SCHEMA_VERSION,
};
use module_exam::ordinary_paper_recognition::{
    OrdinaryPaperRecognitionErrorCode, OrdinaryPaperRecognitionFailure, OrdinaryPaperRecognizer,
    ORDINARY_PAPER_SCHEMA_VERSION,
};
use module_exam::service::assessment::{self, GradeDecision, Publication};
use module_exam::service::grading::{self, AnswerDetail};
use module_exam::service::objective::{
    self, ObjectiveObservationResult, ObjectiveReviewBatch, ObjectiveWorkbench, StrictBatchReview,
};
use module_exam::service::ordinary_structure::OrdinaryStructureConfirmationResult;
use module_exam::vlm::{self as exam_vlm, AnalyzedQuestion};

use crate::answer_sheet_materialization::{self, AnswerSheetPageProcessingResult};
use crate::exam_intake::{
    self, FixedIntakeOption, FixedIntakeRequest, FixedIntakeResult, GroupingConfirmationResult,
    GroupingQualityConfirmationResult, GroupingRetakeResult, MaterialTypeConfirmationResult,
    PageCycleSuggestion,
};
use crate::objective_provider::ArkObjectiveRecognizer;
use crate::objective_run::{self, BeginObjectiveRun};
use crate::ordinary_paper_materialization;
use crate::ordinary_paper_provider::ArkOrdinaryPaperRecognizer;
use crate::ordinary_paper_run::{self, BeginOrdinaryPaperRun, OrdinaryPaperRunResult};
use crate::secrets;
use crate::state::AppState;
use crate::vlm;
use module_exam::service::ordered_activation::GroupedPageEvidence;

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

// ───────────────────── T6.1b 固定卷一站式上传 ─────────────────────

/// 只返回已有的已确认客观题作业版本，不在上传页临时创建第二套作业状态。
#[tauri::command]
pub fn exam_fixed_intake_options(state: State<'_, AppState>) -> R<Vec<FixedIntakeOption>> {
    let conn = lock(&state)?;
    exam_intake::list_options(&conn).map_err(e)
}

/// 只读取待上传文件并比较重复版式，不持有数据库锁，也不创建任何批改事实。
#[tauri::command]
pub fn exam_fixed_intake_infer_page_cycle(student_paths: Vec<String>) -> R<PageCycleSuggestion> {
    exam_intake::infer_page_cycle_paths(&student_paths).map_err(e)
}

/// 将 JPG/PDF 学生卷和可选答案资料归档、拆页并登记到既有 B1/B3a 状态机。
#[tauri::command]
pub fn exam_fixed_intake_prepare(
    state: State<'_, AppState>,
    request: FixedIntakeRequest,
) -> R<FixedIntakeResult> {
    // PDF 解析/拆页先在数据库锁外完成，避免大文件处理冻结其他本地查询。
    let prepared = exam_intake::prepare_fixed_intake_files(&request).map_err(e)?;
    let conn = lock(&state)?;
    exam_intake::persist_fixed_intake(&conn, &state.data_dir, &request, &prepared).map_err(e)
}

#[tauri::command]
pub fn exam_fixed_intake_confirm_material_type(
    state: State<'_, AppState>,
    batch_id: i64,
    material_type: String,
) -> R<MaterialTypeConfirmationResult> {
    let conn = lock(&state)?;
    exam_intake::confirm_intake_material_type(&conn, batch_id, &material_type).map_err(e)
}

#[tauri::command]
pub fn exam_fixed_intake_confirm_grouping(
    state: State<'_, AppState>,
    batch_id: i64,
    first_student_no: String,
    absent_student_nos: Vec<String>,
) -> R<GroupingConfirmationResult> {
    let conn = lock(&state)?;
    exam_intake::confirm_intake_grouping(&conn, batch_id, &first_student_no, &absent_student_nos)
        .map_err(e)
}

/// 返回当前老师已确认页组的原图路径，并只把这些数据库登记过的精确路径加入 asset 白名单。
#[tauri::command]
pub fn exam_fixed_intake_grouping_evidence(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    batch_id: i64,
) -> R<Vec<GroupedPageEvidence>> {
    let evidence = {
        let conn = lock(&state)?;
        exam_intake::intake_grouping_evidence(&conn, batch_id).map_err(e)?
    };
    let scope = app.asset_protocol_scope();
    for group in &evidence {
        for page in &group.pages {
            let path = std::path::Path::new(&page.archived_path);
            if !path.is_file() {
                return Err(format!("页面原图不存在：page#{}", page.page_id));
            }
            scope.allow_file(path).map_err(e)?;
        }
    }
    Ok(evidence)
}

/// 老师看过联系表后一次确认质量；模糊页所在学生组被扣住，其他组原子建立正式归属。
#[tauri::command]
pub fn exam_fixed_intake_confirm_grouping_quality(
    state: State<'_, AppState>,
    batch_id: i64,
    rejected_page_ids: Vec<i64>,
) -> R<GroupingQualityConfirmationResult> {
    let conn = lock(&state)?;
    exam_intake::confirm_intake_grouping_quality(&conn, batch_id, &rejected_page_ids).map_err(e)
}

/// 只替换一个当前待重拍页；旧原图和旧确认快照保留，不移动其他学生的照片顺序。
#[tauri::command]
pub fn exam_fixed_intake_replace_rejected_page(
    state: State<'_, AppState>,
    batch_id: i64,
    rejected_page_id: i64,
    replacement_path: String,
) -> R<GroupingRetakeResult> {
    let conn = lock(&state)?;
    exam_intake::replace_intake_rejected_page(
        &conn,
        &state.data_dir,
        batch_id,
        rejected_page_id,
        &replacement_path,
    )
    .map_err(e)
}

/// 对一条已完成老师确认、且已有明确答题格坐标的客观题区域执行真实视觉识别。
///
/// 外部网络调用期间不持有 SQLite 锁；结果只形成 observation/机器评分建议，仍需
/// 老师终审并显式发布。失败也会写脱敏 run 与待复核记录，便于用新幂等键重试。
#[tauri::command]
pub async fn exam_objective_recognize_region(
    state: State<'_, AppState>,
    answer_region_revision_id: i64,
    idempotency_key: String,
) -> R<ObjectiveObservationResult> {
    let creds = secrets::load(&state.data_dir).map_err(e)?;
    let recognizer = ArkObjectiveRecognizer::from_creds(&creds);
    let descriptor = recognizer.descriptor();
    let metadata = {
        let conn = lock(&state)?;
        objective_run::load_metadata(&conn, answer_region_revision_id).map_err(e)?
    };
    let input = objective_run::load_input(metadata).map_err(e)?;
    let ai_run_id = {
        let conn = lock(&state)?;
        match objective_run::begin(&conn, &input, &descriptor, &idempotency_key).map_err(e)? {
            BeginObjectiveRun::Execute { ai_run_id } => ai_run_id,
            BeginObjectiveRun::Completed(result) => return Ok(*result),
        }
    };

    let provider_result =
        tauri::async_runtime::spawn_blocking(move || recognizer.recognize(&input.request()))
            .await
            .unwrap_or_else(|_| {
                Err(ObjectiveRecognitionFailure {
                    schema_version: OBJECTIVE_RECOGNITION_SCHEMA_VERSION,
                    code: ObjectiveRecognitionErrorCode::Internal,
                    safe_message: "客观题识别任务意外中断，已保留记录等待重试".into(),
                    retryable: true,
                })
            });
    let conn = lock(&state)?;
    objective_run::finish(&conn, ai_run_id, &idempotency_key, provider_result).map_err(e)
}

/// 对一张已完成评分前身份/质量确认、材料为普通试卷的页面执行整页结构分析。
///
/// 外部调用期间不持 SQLite 锁；结果只写不可变 ai_run，当前批不自动覆盖老师质量、
/// 不确认配准/题区，也不创建分数、发布或学习证据。
#[tauri::command]
pub async fn exam_ordinary_paper_analyze_page(
    state: State<'_, AppState>,
    page_id: i64,
    idempotency_key: String,
) -> R<OrdinaryPaperRunResult> {
    let creds = secrets::load(&state.data_dir).map_err(e)?;
    let recognizer = ArkOrdinaryPaperRecognizer::from_creds(&creds);
    let descriptor = recognizer.descriptor();
    let metadata = {
        let conn = lock(&state)?;
        ordinary_paper_run::load_metadata(&conn, page_id).map_err(e)?
    };
    let input = std::sync::Arc::new(ordinary_paper_run::load_input(metadata).map_err(e)?);
    let ai_run_id = {
        let conn = lock(&state)?;
        match ordinary_paper_run::begin(&conn, &input, &descriptor, &idempotency_key).map_err(e)? {
            BeginOrdinaryPaperRun::Execute { ai_run_id } => ai_run_id,
            BeginOrdinaryPaperRun::Completed(result) => return Ok(*result),
        }
    };
    let worker_input = std::sync::Arc::clone(&input);
    let provider_result =
        tauri::async_runtime::spawn_blocking(move || recognizer.recognize(&worker_input.request()))
            .await
            .unwrap_or_else(|_| {
                Err(OrdinaryPaperRecognitionFailure {
                    schema_version: ORDINARY_PAPER_SCHEMA_VERSION,
                    code: OrdinaryPaperRecognitionErrorCode::Internal,
                    safe_message: "普通试卷分析任务意外中断，已保留记录等待重试".into(),
                    retryable: true,
                })
            });
    let conn = lock(&state)?;
    ordinary_paper_run::finish(&conn, &input, ai_run_id, provider_result).map_err(e)
}

/// 老师确认一张 ready 普通试卷的整页结构，并一次性生成配准、题区和本地裁图。
///
/// 该动作不产生分数或发布；返回的题区仍需逐题视觉识别，再由老师在批改台终审。
#[tauri::command]
pub fn exam_ordinary_paper_confirm_page_structure(
    state: State<'_, AppState>,
    page_id: i64,
    ai_run_id: i64,
) -> R<OrdinaryStructureConfirmationResult> {
    let conn = lock(&state)?;
    ordinary_paper_materialization::confirm_page_structure(
        &conn,
        &state.data_dir,
        page_id,
        ai_run_id,
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)
}

/// 老师把当前 active 固定答题卡模板应用到一张已确认学生页面。
///
/// 本地完成四角校正、题区裁剪与像素差分 OMR；清晰结果和异常都只进入客观题建议，
/// 不自动确认分数，也不自动发布成绩。
#[tauri::command]
pub fn exam_answer_sheet_process_page(
    state: State<'_, AppState>,
    page_id: i64,
) -> R<AnswerSheetPageProcessingResult> {
    let conn = lock(&state)?;
    answer_sheet_materialization::process_page(
        &conn,
        &state.data_dir,
        page_id,
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)
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
