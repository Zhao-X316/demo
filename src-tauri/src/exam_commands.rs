//! 改作业模块 Tauri 命令：知识点树 / 题库 / 豆包视觉预分析。

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use module_exam::answer_sheet_template_recognition::{
    AnswerSheetTemplateRecognitionErrorCode, AnswerSheetTemplateRecognitionFailure,
    AnswerSheetTemplateRecognizer, ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION,
};
use module_exam::answer_source_recognition::{
    AnswerSourceErrorCode, AnswerSourceFailure, AnswerSourceRecognizer,
    ANSWER_SOURCE_SCHEMA_VERSION,
};
use module_exam::class_dashboard::{self as class_dashboard_service, ClassOperationsDashboard};
use module_exam::db::knowledge_points::{self as kp, KnowledgePoint, KpInput};
use module_exam::db::questions::{self, NewOption, NewQuestion, Question, QuestionOption};
use module_exam::dictation_recognition::{
    DictationErrorCode, DictationFailure, DictationOcrRecognizer, DictationTemplateRecognizer,
    DICTATION_OCR_SCHEMA_VERSION, DICTATION_TEMPLATE_SCHEMA_VERSION,
};
use module_exam::objective_recognition::{
    ObjectiveRecognitionErrorCode, ObjectiveRecognitionFailure, ObjectiveRecognizer,
    OBJECTIVE_RECOGNITION_SCHEMA_VERSION,
};
use module_exam::ordinary_paper_recognition::{
    OrdinaryPaperRecognitionErrorCode, OrdinaryPaperRecognitionFailure, OrdinaryPaperRecognizer,
    ORDINARY_PAPER_SCHEMA_VERSION,
};
use module_exam::service::answer_source::{
    self, AnswerSourceReviewSummary, RubricPointMappingInput,
};
use module_exam::service::assessment::{self, GradeDecision, Publication};
use module_exam::service::dictation_pipeline::{
    self, DictationPageMaterializationResult, DictationReviewBatch, DictationTemplateConfirmation,
    DictationTranscriptionResult, DictationWorkbench, StrictDictationBatchReview,
};
use module_exam::service::grading::{self, AnswerDetail};
use module_exam::service::objective::{
    self, ObjectiveObservationResult, ObjectiveReviewBatch, ObjectiveWorkbench, StrictBatchReview,
};
use module_exam::service::ordinary_question_sync::{self, OrdinaryQuestionSyncSummary};
use module_exam::service::ordinary_structure::OrdinaryStructureConfirmationResult;
use module_exam::service::subjective::{
    AcceptedAnswerPromotionResult, RubricEvidencePromotionResult, ShortAnswerGradeAnalysis,
    SubjectiveComponentGradeInput, SubjectiveTranscriptionRevision, SubjectiveWorkbench,
};
use module_exam::service::subjective_links::{
    SubjectiveLinkEditResult, SubjectiveLinkEditor, SubjectiveSourceLinkInput,
};
use module_exam::short_answer_grading::{
    ShortAnswerGradeErrorCode, ShortAnswerGradeFailure, ShortAnswerGrader,
    SHORT_ANSWER_GRADE_SCHEMA_VERSION,
};
use module_exam::vlm::{self as exam_vlm, AnalyzedQuestion};

use crate::answer_sheet_materialization::{self, AnswerSheetPageProcessingResult};
use crate::answer_sheet_template_provider::ArkAnswerSheetTemplateRecognizer;
use crate::answer_sheet_template_run::{
    self, AnswerSheetTemplateRunResult, AnswerSheetTemplateStatus, BeginAnswerSheetTemplateRun,
};
use crate::answer_source_provider::ArkAnswerSourceRecognizer;
use crate::answer_source_run::{self, AnswerSourceAnalysisResult, BeginAnswerSourceRun};
use crate::dictation_materialization;
use crate::dictation_provider::{
    ArkDictationOcrRecognizer, ArkDictationTemplateRecognizer, ArkHandwritingOcrRecognizer,
};
use crate::dictation_run::{
    self, BeginDictationOcrRun, BeginDictationTemplateRun, DictationTemplateRunResult,
    DictationTemplateStatus,
};
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
use crate::short_answer_provider::ArkShortAnswerGrader;
use crate::short_answer_run::{self, BeginShortAnswerGradeRun};
use crate::state::AppState;
use crate::subjective_run::{self, BeginSubjectiveOcrRun};
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

// ───────────────────────── 班级运行仪表盘 ─────────────────────────

/// M6.1-1 只读运行事实：不计算掌握度，也不执行终审、发布或日切。
#[tauri::command]
pub fn class_operations_dashboard(
    state: State<'_, AppState>,
    class_id: i64,
    as_of_date: Option<String>,
) -> R<ClassOperationsDashboard> {
    let date = as_of_date.unwrap_or_else(|| {
        chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string()
    });
    let conn = lock(&state)?;
    class_dashboard_service::class_operations_dashboard(&conn, class_id, &date).map_err(e)
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

/// 结构化老师上传的答案图片、PDF、文本或 Office 文件，并逐题生成带来源锚点的 AI 草稿。
///
/// 请求不含学生作答或当前 K1 标准答案；外部调用期间不持 SQLite 锁。成功结果仍须
/// 老师一次确认，冲突/缺题在固定卷预检中保持 blocked。
#[tauri::command]
pub async fn exam_answer_source_analyze(
    state: State<'_, AppState>,
    batch_id: i64,
    idempotency_key: String,
) -> R<AnswerSourceAnalysisResult> {
    let creds = secrets::load(&state.data_dir).map_err(e)?;
    let recognizer = ArkAnswerSourceRecognizer::from_creds(&creds);
    let descriptor = recognizer.descriptor();
    let metadata = {
        let conn = lock(&state)?;
        answer_source_run::load_metadata(&conn, batch_id).map_err(e)?
    };
    let input = std::sync::Arc::new(answer_source_run::load_input(metadata).map_err(e)?);
    let begin = {
        let conn = lock(&state)?;
        answer_source_run::begin(&conn, &input, &descriptor, &idempotency_key).map_err(e)?
    };
    if let BeginAnswerSourceRun::Completed(result) = begin {
        let conn = lock(&state)?;
        return answer_source_run::materialize_and_review(&conn, *result).map_err(e);
    }
    let BeginAnswerSourceRun::Execute { ai_run_id } = begin else {
        unreachable!("completed returned above")
    };
    let worker_input = std::sync::Arc::clone(&input);
    let provider_result =
        tauri::async_runtime::spawn_blocking(move || recognizer.recognize(&worker_input.request()))
            .await
            .unwrap_or_else(|_| {
                Err(AnswerSourceFailure {
                    schema_version: ANSWER_SOURCE_SCHEMA_VERSION,
                    code: AnswerSourceErrorCode::Internal,
                    safe_message: "答案结构化任务意外中断，资料已保留等待重试".into(),
                    retryable: true,
                })
            });
    let conn = lock(&state)?;
    let result = answer_source_run::finish(&conn, &input, ai_run_id, provider_result).map_err(e)?;
    answer_source_run::materialize_and_review(&conn, result).map_err(e)
}

fn refresh_fixed_preflight(conn: &rusqlite::Connection, batch_id: i64) -> R<()> {
    let expected_pages: i64 = conn
        .query_row(
            "SELECT expected_pages_per_attempt FROM exam_ordered_grouping_revisions_v2
             WHERE ingest_batch_id=?1 AND state='active'",
            [batch_id],
            |row| row.get(0),
        )
        .map_err(e)?;
    module_exam::service::fixed_paper::preflight_fixed_paper_batch(
        conn,
        &module_exam::service::fixed_paper::FixedPaperPreflightInput {
            ingest_batch_id: batch_id,
            expected_pages_per_attempt: expected_pages,
            created_by_type: "teacher",
            created_by: Some(LOCAL_TEACHER_ACTOR),
        },
    )
    .map_err(e)?;
    Ok(())
}

/// 全部逐题候选与当前作业答案一致时，老师一次确认并复用现有 K1 版本。
#[tauri::command]
pub fn exam_answer_source_confirm_matches(
    state: State<'_, AppState>,
    batch_id: i64,
    source_ai_run_id: i64,
) -> R<AnswerSourceReviewSummary> {
    let mut conn = lock(&state)?;
    let result =
        answer_source::confirm_matches(&mut conn, batch_id, source_ai_run_id, LOCAL_TEACHER_ACTOR)
            .map_err(e)?;
    refresh_fixed_preflight(&conn, batch_id)?;
    Ok(result)
}

/// 有冲突或缺题时，老师明确选择沿用本次作业已绑定答案；上传草稿被拒绝但保留审计。
#[tauri::command]
pub fn exam_answer_source_keep_bound(
    state: State<'_, AppState>,
    batch_id: i64,
    source_ai_run_id: i64,
) -> R<AnswerSourceReviewSummary> {
    let mut conn = lock(&state)?;
    let result = answer_source::keep_bound_answers(
        &mut conn,
        batch_id,
        source_ai_run_id,
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)?;
    refresh_fixed_preflight(&conn, batch_id)?;
    Ok(result)
}

/// 将高置信冲突答案另存为新的 K1/作业版本；当前批次继续固定引用旧版本。
#[tauri::command]
pub fn exam_answer_source_adopt_new_version(
    state: State<'_, AppState>,
    batch_id: i64,
    source_ai_run_id: i64,
    rubric_mappings: Option<Vec<RubricPointMappingInput>>,
) -> R<AnswerSourceReviewSummary> {
    let mut conn = lock(&state)?;
    let result = answer_source::adopt_conflicts_as_new_version_with_mappings(
        &mut conn,
        batch_id,
        source_ai_run_id,
        LOCAL_TEACHER_ACTOR,
        rubric_mappings.as_deref().unwrap_or_default(),
    )
    .map_err(e)?;
    refresh_fixed_preflight(&conn, batch_id)?;
    Ok(result)
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

/// 页面结构确认后自动把安全的印刷题面文本送入 M2.5 私有候选管线。
///
/// 该命令与批改识别分开：题库同步失败只返回错误，不撤销已经确认的页面结构，
/// 也不阻断后续客观题识别和老师终审。
#[tauri::command]
pub fn exam_ordinary_paper_sync_questions(
    state: State<'_, AppState>,
    page_id: i64,
    ai_run_id: i64,
) -> R<OrdinaryQuestionSyncSummary> {
    let conn = lock(&state)?;
    ordinary_question_sync::sync_confirmed_printed_questions(
        &conn,
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
async fn recognize_answer_sheet_subjective_region(
    state: &State<'_, AppState>,
    answer_region_revision_id: i64,
    idempotency_key: String,
) -> R<SubjectiveTranscriptionRevision> {
    let creds = secrets::load(&state.data_dir).map_err(e)?;
    let recognizer = ArkHandwritingOcrRecognizer::from_creds(&creds);
    let descriptor = recognizer.descriptor();
    let metadata = {
        let conn = lock(state)?;
        subjective_run::load_metadata(&conn, answer_region_revision_id).map_err(e)?
    };
    let input = std::sync::Arc::new(subjective_run::load_input(metadata).map_err(e)?);
    let ai_run_id = {
        let mut conn = lock(state)?;
        match subjective_run::begin(&mut conn, &input, &descriptor, &idempotency_key).map_err(e)? {
            BeginSubjectiveOcrRun::Execute { ai_run_id } => ai_run_id,
            BeginSubjectiveOcrRun::Completed(result) => return Ok(*result),
        }
    };
    let worker_input = std::sync::Arc::clone(&input);
    let provider_result =
        tauri::async_runtime::spawn_blocking(move || recognizer.recognize(&worker_input.request()))
            .await
            .unwrap_or_else(|_| {
                Err(DictationFailure {
                    schema_version: DICTATION_OCR_SCHEMA_VERSION,
                    code: DictationErrorCode::Internal,
                    safe_message: "手写识别任务意外中断，已保留题区等待重试".into(),
                    retryable: true,
                })
            });
    let mut conn = lock(state)?;
    subjective_run::finish(&mut conn, &input, ai_run_id, provider_result).map_err(e)
}

/// 为一个 active recognized 简答转写生成逐评分点机器建议。
///
/// 输入在数据库锁内冻结，方舟调用在锁外执行；返回结果仍需老师显式终审。
async fn grade_answer_sheet_short_answer(
    state: &State<'_, AppState>,
    transcription_revision_id: i64,
    idempotency_key: String,
) -> R<ShortAnswerGradeAnalysis> {
    let creds = secrets::load(&state.data_dir).map_err(e)?;
    let grader = ArkShortAnswerGrader::from_creds(&creds);
    let descriptor = grader.descriptor();
    let input = {
        let conn = lock(state)?;
        short_answer_run::load_input(&conn, transcription_revision_id).map_err(e)?
    };
    let input = std::sync::Arc::new(input);
    let ai_run_id = {
        let mut conn = lock(state)?;
        match short_answer_run::begin(&mut conn, &input, &descriptor, &idempotency_key)
            .map_err(e)?
        {
            BeginShortAnswerGradeRun::Execute { ai_run_id } => ai_run_id,
            BeginShortAnswerGradeRun::Completed(result) => return Ok(*result),
        }
    };
    let worker_input = std::sync::Arc::clone(&input);
    let provider_result = tauri::async_runtime::spawn_blocking(move || grader.grade(&worker_input))
        .await
        .unwrap_or_else(|_| {
            Err(ShortAnswerGradeFailure {
                schema_version: SHORT_ANSWER_GRADE_SCHEMA_VERSION,
                code: ShortAnswerGradeErrorCode::Internal,
                safe_message: "简答题评分任务意外中断，已保留转写等待重试".into(),
                retryable: true,
            })
        });
    let mut conn = lock(state)?;
    short_answer_run::finish(&mut conn, &input, ai_run_id, provider_result).map_err(e)
}

/// 老师把当前 active 固定答题卡模板应用到一张已确认学生页面。
///
/// 本地完成四角校正、题区裁剪和客观题 OMR；主观区随后逐区调用只读手写 OCR。
/// 单区异常不会抹掉已经完成的结构/客观题结果，而会返回显式失败供老师重试。
#[tauri::command]
pub async fn exam_answer_sheet_process_page(
    state: State<'_, AppState>,
    page_id: i64,
) -> R<AnswerSheetPageProcessingResult> {
    let mut result = {
        let conn = lock(&state)?;
        answer_sheet_materialization::process_page(
            &conn,
            &state.data_dir,
            page_id,
            LOCAL_TEACHER_ACTOR,
        )
        .map_err(e)?
    };
    for region in result.subjective_regions.clone() {
        let key = format!(
            "answer-sheet:page:{page_id}:region:{}:handwriting-ocr-v1",
            region.answer_region_revision_id
        );
        match recognize_answer_sheet_subjective_region(
            &state,
            region.answer_region_revision_id,
            key,
        )
        .await
        {
            Ok(transcription) => {
                if transcription.question_type == "short_answer"
                    && transcription.result_state == "recognized"
                {
                    let grade_key = format!(
                        "answer-sheet:transcription:{}:answer-grade-v1",
                        transcription.id
                    );
                    if let Err(safe_message) =
                        grade_answer_sheet_short_answer(&state, transcription.id, grade_key).await
                    {
                        result.subjective_failures.push(
                            answer_sheet_materialization::AnswerSheetSubjectiveRegionFailure {
                                answer_region_revision_id: region.answer_region_revision_id,
                                safe_message: format!(
                                    "手写已识别，但简答评分建议未生成：{safe_message}"
                                ),
                            },
                        );
                    }
                }
                result.subjective_transcriptions.push(transcription);
            }
            Err(safe_message) => {
                result.subjective_failures.push(
                    answer_sheet_materialization::AnswerSheetSubjectiveRegionFailure {
                        answer_region_revision_id: region.answer_region_revision_id,
                        safe_message,
                    },
                );
            }
        }
    }
    Ok(result)
}

/// 对单个答题卡主观题区主动重试手写 OCR；新请求使用新幂等键，不覆盖旧转写。
#[tauri::command]
pub async fn exam_answer_sheet_recognize_subjective_region(
    state: State<'_, AppState>,
    answer_region_revision_id: i64,
    idempotency_key: String,
) -> R<SubjectiveTranscriptionRevision> {
    recognize_answer_sheet_subjective_region(&state, answer_region_revision_id, idempotency_key)
        .await
}

/// 老师只校正已存在的 OCR 文本；原始机器文本保留，新文本形成追加 revision。
#[tauri::command]
pub fn exam_answer_sheet_correct_subjective_transcription(
    state: State<'_, AppState>,
    answer_region_revision_id: i64,
    corrected_text: String,
) -> R<SubjectiveTranscriptionRevision> {
    let mut conn = lock(&state)?;
    module_exam::service::subjective::teacher_correct_transcription(
        &mut conn,
        answer_region_revision_id,
        &corrected_text,
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)
}

/// 老师校正转写后或失败重试时，主动重新生成简答题逐点评分建议。
#[tauri::command]
pub async fn exam_answer_sheet_grade_short_answer(
    state: State<'_, AppState>,
    transcription_revision_id: i64,
    idempotency_key: String,
) -> R<ShortAnswerGradeAnalysis> {
    grade_answer_sheet_short_answer(&state, transcription_revision_id, idempotency_key).await
}

/// 答题卡填空/简答题工作台：展示当前转写、已确认答案/评分点和老师终审 revision。
#[tauri::command]
pub fn exam_answer_sheet_subjective_workbench(
    state: State<'_, AppState>,
    assessment_version_id: Option<i64>,
    limit: Option<i64>,
) -> R<SubjectiveWorkbench> {
    let conn = lock(&state)?;
    module_exam::service::subjective::list_subjective_workbench(
        &conn,
        assessment_version_id,
        limit.unwrap_or(1000),
    )
    .map_err(e)
}

/// 老师显式接受填空题确定性建议或简答题逐点评分建议；两者都只做单条终审。
#[tauri::command]
pub fn exam_answer_sheet_subjective_accept(
    state: State<'_, AppState>,
    suggestion_id: i64,
) -> R<GradeDecision> {
    let conn = lock(&state)?;
    module_exam::service::subjective::accept_subjective_suggestion(
        &conn,
        suggestion_id,
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)
}

/// 老师查看原图后对填空分歧或简答题人工记分；必须填写判定依据。
#[tauri::command]
pub fn exam_answer_sheet_subjective_correct(
    state: State<'_, AppState>,
    suggestion_id: i64,
    teacher_score: f64,
    teacher_note: Option<String>,
) -> R<GradeDecision> {
    let conn = lock(&state)?;
    module_exam::service::subjective::correct_subjective_suggestion(
        &conn,
        suggestion_id,
        teacher_score,
        teacher_note.as_deref(),
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)
}

/// 老师按填空槽位或简答评分点逐项确认；总分由逐项得分自动汇总。
#[tauri::command]
pub fn exam_answer_sheet_subjective_correct_components(
    state: State<'_, AppState>,
    suggestion_id: i64,
    components: Vec<SubjectiveComponentGradeInput>,
    teacher_note: String,
) -> R<GradeDecision> {
    let conn = lock(&state)?;
    module_exam::service::subjective::correct_subjective_components(
        &conn,
        suggestion_id,
        &components,
        &teacher_note,
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)
}

/// 老师把一次已人工判满分的填空写法加入未来答案版本；当前成绩与发布保持不变。
#[tauri::command]
pub fn exam_answer_sheet_promote_accepted_answer(
    state: State<'_, AppState>,
    grade_decision_id: i64,
) -> R<AcceptedAnswerPromotionResult> {
    let mut conn = lock(&state)?;
    module_exam::service::subjective::promote_fill_accepted_answer(
        &mut conn,
        grade_decision_id,
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)
}

/// 老师把逐点评分中确认过的学生表述加入未来评分点规则；历史成绩保持不变。
#[tauri::command]
pub fn exam_answer_sheet_promote_rubric_evidence(
    state: State<'_, AppState>,
    grade_decision_id: i64,
    source_public_id: String,
) -> R<RubricEvidencePromotionResult> {
    let mut conn = lock(&state)?;
    module_exam::service::subjective::promote_short_answer_rubric_evidence(
        &mut conn,
        grade_decision_id,
        &source_public_id,
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)
}

/// 读取本题最新确认版本的答案槽位/评分点链接及同教材知识点、同学科能力候选。
#[tauri::command]
pub fn exam_subjective_link_editor(
    state: State<'_, AppState>,
    assessment_item_id: i64,
) -> R<SubjectiveLinkEditor> {
    let conn = lock(&state)?;
    module_exam::service::subjective_links::get_editor(&conn, assessment_item_id).map_err(e)
}

/// 老师确认主观题链接；另存 K1 link set 与未来作业版本，不改当前成绩和历史发布。
#[tauri::command]
pub fn exam_subjective_link_save(
    state: State<'_, AppState>,
    assessment_item_id: i64,
    sources: Vec<SubjectiveSourceLinkInput>,
) -> R<SubjectiveLinkEditResult> {
    let mut conn = lock(&state)?;
    module_exam::service::subjective_links::save_editor(
        &mut conn,
        assessment_item_id,
        &sources,
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)
}

/// 显式发布已完成全部题目终审的答题卡；评分建议本身永远不会自动发布。
#[tauri::command]
pub fn exam_answer_sheet_subjective_publish_attempt(
    state: State<'_, AppState>,
    attempt_id: i64,
) -> R<Publication> {
    let conn = lock(&state)?;
    assessment::publish_attempt(&conn, attempt_id, LOCAL_TEACHER_ACTOR).map_err(e)
}

/// 查询当前答题卡页是否已有老师确认的 active 空白模板。
#[tauri::command]
pub fn exam_answer_sheet_template_status(
    state: State<'_, AppState>,
    reference_page_id: i64,
) -> R<AnswerSheetTemplateStatus> {
    let conn = lock(&state)?;
    answer_sheet_template_run::status(&conn, reference_page_id).map_err(e)
}

/// 归档老师选择的首张空白答题卡并生成结构候选。
///
/// 外部视觉调用不持数据库锁；候选不会自动成为模板，也不会处理任何学生答案。
#[tauri::command]
pub async fn exam_answer_sheet_analyze_template(
    state: State<'_, AppState>,
    reference_page_id: i64,
    blank_path: String,
    idempotency_key: String,
) -> R<AnswerSheetTemplateRunResult> {
    let creds = secrets::load(&state.data_dir).map_err(e)?;
    let recognizer = ArkAnswerSheetTemplateRecognizer::from_creds(&creds);
    let descriptor = recognizer.descriptor();
    let prepared = answer_sheet_template_run::prepare_blank_template(&blank_path, &state.data_dir)
        .map_err(e)?;
    let blank_artifact_id = {
        let conn = lock(&state)?;
        answer_sheet_template_run::register_blank_template(&conn, &prepared)
            .map_err(e)?
            .id
    };
    let input = {
        let conn = lock(&state)?;
        answer_sheet_template_run::load_input(&conn, reference_page_id, blank_artifact_id)
            .map_err(e)?
    };
    let input = std::sync::Arc::new(input);
    let ai_run_id = {
        let conn = lock(&state)?;
        match answer_sheet_template_run::begin(&conn, &input, &descriptor, &idempotency_key)
            .map_err(e)?
        {
            BeginAnswerSheetTemplateRun::Execute { ai_run_id } => ai_run_id,
            BeginAnswerSheetTemplateRun::Completed(result) => return Ok(*result),
        }
    };
    let worker_input = std::sync::Arc::clone(&input);
    let provider_result =
        tauri::async_runtime::spawn_blocking(move || recognizer.recognize(&worker_input.request()))
            .await
            .unwrap_or_else(|_| {
                Err(AnswerSheetTemplateRecognitionFailure {
                    schema_version: ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION,
                    code: AnswerSheetTemplateRecognitionErrorCode::Internal,
                    safe_message: "答题卡模板分析任务意外中断，已保留空白卡等待重试".into(),
                    retryable: true,
                })
            });
    let conn = lock(&state)?;
    answer_sheet_template_run::finish(&conn, &input, ai_run_id, provider_result).map_err(e)
}

/// 老师一次确认 ready 空白答题卡候选，创建不可变 active 模板 revision。
#[tauri::command]
pub fn exam_answer_sheet_confirm_template(
    state: State<'_, AppState>,
    reference_page_id: i64,
    ai_run_id: i64,
) -> R<module_exam::service::answer_sheet::AnswerSheetTemplateRevision> {
    let mut conn = lock(&state)?;
    answer_sheet_template_run::confirm(&mut conn, reference_page_id, ai_run_id, LOCAL_TEACHER_ACTOR)
        .map_err(e)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationPageProcessingResult {
    pub structure: DictationPageMaterializationResult,
    pub transcriptions: Vec<DictationTranscriptionResult>,
}

/// 查询当前默写页是否已有老师一次确认的 active 空白模板。
#[tauri::command]
pub fn exam_dictation_template_status(
    state: State<'_, AppState>,
    reference_page_id: i64,
) -> R<DictationTemplateStatus> {
    let conn = lock(&state)?;
    dictation_run::template_status(&conn, reference_page_id).map_err(e)
}

/// 分析老师选择的固定格式默写空白页；模型只定位题号/行栏，不读取答案。
#[tauri::command]
pub async fn exam_dictation_analyze_template(
    state: State<'_, AppState>,
    reference_page_id: i64,
    blank_path: String,
    idempotency_key: String,
) -> R<DictationTemplateRunResult> {
    let creds = secrets::load(&state.data_dir).map_err(e)?;
    let recognizer = ArkDictationTemplateRecognizer::from_creds(&creds);
    let descriptor = recognizer.descriptor();
    let prepared =
        dictation_run::prepare_blank_template(&blank_path, &state.data_dir).map_err(e)?;
    let blank_artifact_id = {
        let conn = lock(&state)?;
        dictation_run::register_blank_template(&conn, &prepared)
            .map_err(e)?
            .id
    };
    let input = {
        let conn = lock(&state)?;
        dictation_run::load_template_input(&conn, reference_page_id, blank_artifact_id)
            .map_err(e)?
    };
    let input = std::sync::Arc::new(input);
    let ai_run_id = {
        let conn = lock(&state)?;
        match dictation_run::begin_template(&conn, &input, &descriptor, &idempotency_key)
            .map_err(e)?
        {
            BeginDictationTemplateRun::Execute { ai_run_id } => ai_run_id,
            BeginDictationTemplateRun::Completed(result) => return Ok(*result),
        }
    };
    let worker_input = std::sync::Arc::clone(&input);
    let provider_result =
        tauri::async_runtime::spawn_blocking(move || recognizer.recognize(&worker_input.request()))
            .await
            .unwrap_or_else(|_| {
                Err(DictationFailure {
                    schema_version: DICTATION_TEMPLATE_SCHEMA_VERSION,
                    code: DictationErrorCode::Internal,
                    safe_message: "默写模板分析任务意外中断，已保留空白页等待重试".into(),
                    retryable: true,
                })
            });
    let conn = lock(&state)?;
    dictation_run::finish_template(&conn, &input, ai_run_id, provider_result).map_err(e)
}

/// 老师一次确认 ready 默写空白模板，同时冻结本页每项答案/rubric 策略。
#[tauri::command]
pub fn exam_dictation_confirm_template(
    state: State<'_, AppState>,
    reference_page_id: i64,
    ai_run_id: i64,
) -> R<DictationTemplateConfirmation> {
    let mut conn = lock(&state)?;
    dictation_run::confirm_template(&mut conn, reference_page_id, ai_run_id, LOCAL_TEACHER_ACTOR)
        .map_err(e)
}

async fn recognize_dictation_region(
    state: &State<'_, AppState>,
    answer_region_revision_id: i64,
    idempotency_key: String,
) -> R<DictationTranscriptionResult> {
    let creds = secrets::load(&state.data_dir).map_err(e)?;
    let recognizer = ArkDictationOcrRecognizer::from_creds(&creds);
    let descriptor = recognizer.descriptor();
    let metadata = {
        let conn = lock(state)?;
        dictation_run::load_ocr_metadata(&conn, answer_region_revision_id).map_err(e)?
    };
    let input = std::sync::Arc::new(dictation_run::load_ocr_input(metadata).map_err(e)?);
    let ai_run_id = {
        let mut conn = lock(state)?;
        match dictation_run::begin_ocr(&mut conn, &input, &descriptor, &idempotency_key)
            .map_err(e)?
        {
            BeginDictationOcrRun::Execute { ai_run_id } => ai_run_id,
            BeginDictationOcrRun::Completed(result) => return Ok(*result),
        }
    };
    let worker_input = std::sync::Arc::clone(&input);
    let provider_result =
        tauri::async_runtime::spawn_blocking(move || recognizer.recognize(&worker_input.request()))
            .await
            .unwrap_or_else(|_| {
                Err(DictationFailure {
                    schema_version: DICTATION_OCR_SCHEMA_VERSION,
                    code: DictationErrorCode::Internal,
                    safe_message: "默写 OCR 任务意外中断，已保留题区等待重试".into(),
                    retryable: true,
                })
            });
    let mut conn = lock(state)?;
    dictation_run::finish_ocr(&mut conn, &input, ai_run_id, provider_result).map_err(e)
}

/// 单独重试一个默写题区；同一幂等键不会重复产生 OCR 调用或转写 revision。
#[tauri::command]
pub async fn exam_dictation_recognize_region(
    state: State<'_, AppState>,
    answer_region_revision_id: i64,
    idempotency_key: String,
) -> R<DictationTranscriptionResult> {
    recognize_dictation_region(&state, answer_region_revision_id, idempotency_key).await
}

/// 把当前固定模板应用到学生页，并逐题 OCR。任何机器结果都只是建议，不创建成绩。
#[tauri::command]
pub async fn exam_dictation_process_page(
    state: State<'_, AppState>,
    page_id: i64,
) -> R<DictationPageProcessingResult> {
    let structure = {
        let conn = lock(&state)?;
        dictation_materialization::materialize_page(
            &conn,
            &state.data_dir,
            page_id,
            LOCAL_TEACHER_ACTOR,
        )
        .map_err(e)?
    };
    let mut transcriptions = Vec::with_capacity(structure.regions.len());
    for region in &structure.regions {
        let key = format!("dictation:page:{page_id}:region:{}:ocr-v1", region.id);
        transcriptions.push(recognize_dictation_region(&state, region.id, key).await?);
    }
    Ok(DictationPageProcessingResult {
        structure,
        transcriptions,
    })
}

/// 默写异常工作台：精确命中可快速查看，其余全部明确交给老师。
#[tauri::command]
pub fn exam_dictation_workbench(
    state: State<'_, AppState>,
    assessment_version_id: Option<i64>,
    limit: Option<i64>,
) -> R<DictationWorkbench> {
    let conn = lock(&state)?;
    dictation_pipeline::list_dictation_workbench(
        &conn,
        assessment_version_id,
        limit.unwrap_or(1000),
    )
    .map_err(e)
}

/// 接受当前精确命中或老师已校正后的默写建议；只终审，不发布。
#[tauri::command]
pub fn exam_dictation_accept(
    state: State<'_, AppState>,
    transcription_revision_id: i64,
) -> R<GradeDecision> {
    let conn = lock(&state)?;
    dictation_pipeline::accept_dictation_suggestion(
        &conn,
        transcription_revision_id,
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)
}

/// 老师依据原图对分歧、未写或识别失败项人工记分。
#[tauri::command]
pub fn exam_dictation_correct_grade(
    state: State<'_, AppState>,
    transcription_revision_id: i64,
    teacher_score: f64,
    teacher_note: Option<String>,
    teacher_evidence_text: Option<String>,
) -> R<GradeDecision> {
    let conn = lock(&state)?;
    dictation_pipeline::correct_dictation_grade(
        &conn,
        transcription_revision_id,
        teacher_score,
        teacher_note.as_deref(),
        teacher_evidence_text.as_deref(),
        LOCAL_TEACHER_ACTOR,
    )
    .map_err(e)
}

/// 只批量接受当前精确、置信度不低于 0.95 的默写结果；逐条保存排除原因。
#[tauri::command]
pub fn exam_dictation_strict_batch_accept(
    state: State<'_, AppState>,
    transcription_revision_ids: Vec<i64>,
    idempotency_key: String,
) -> R<DictationReviewBatch> {
    let conn = lock(&state)?;
    dictation_pipeline::strict_batch_accept(
        &conn,
        &StrictDictationBatchReview {
            transcription_revision_ids: &transcription_revision_ids,
            reviewed_by: LOCAL_TEACHER_ACTOR,
            idempotency_key: &idempotency_key,
        },
    )
    .map_err(e)
}

/// 默写整份 attempt 显式发布；发布后才激活评分点学习证据。
#[tauri::command]
pub fn exam_dictation_publish_attempt(
    state: State<'_, AppState>,
    attempt_id: i64,
) -> R<Publication> {
    let conn = lock(&state)?;
    assessment::publish_attempt(&conn, attempt_id, LOCAL_TEACHER_ACTOR).map_err(e)
}

/// 老师校正 OCR 文本。原始 OCR 保留，新 revision 重新做可复现的精确比较。
#[tauri::command]
pub fn exam_dictation_correct_transcription(
    state: State<'_, AppState>,
    answer_region_revision_id: i64,
    corrected_text: String,
) -> R<DictationTranscriptionResult> {
    let mut conn = lock(&state)?;
    dictation_pipeline::teacher_correct_transcription(
        &mut conn,
        answer_region_revision_id,
        &corrected_text,
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
