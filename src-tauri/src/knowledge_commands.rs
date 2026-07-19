//! K1 题库组卷教师入口。

use module_exam::answer_source_recognition::{
    AnswerSourceRecognizer, AnswerSourceRecognizerDescriptor,
};
use module_exam::service::blueprint_assembly::{
    self, BlueprintAssembly, BlueprintOptions, BlueprintPreview, BlueprintPreviewRequest,
    BlueprintQuestionTypeTarget, ConfirmBlueprintRequest,
};
use module_exam::service::question_candidate_review::{
    self, CandidateReviewDecision, CandidateReviewInbox, DiscardCandidateRequest,
    PromoteCandidateRequest,
};
use module_knowledge::db::answer_sources::{
    self, AnswerMatchReview, AnswerSourceInboxItem, AnswerTargetSet, ConfirmAnswerMatchRequest,
};
use module_knowledge::db::search::{
    self, DuplicateReviewDecision, QuestionSearchRequest, QuestionSearchResponse,
    ReviewDuplicateRequest,
};
use module_knowledge::db::source_documents::{
    self, CorrectedSourceQuestion, DiscardSourceDraftRequest, ReviewSourceDraftRequest,
    SourceDraftReview, SourceInboxItem,
};
use module_knowledge::source_import::{SourceOptionDraft, SourceQuestionRecognizer};
use serde::Deserialize;
use serde_json::Value;
use tauri::State;

use crate::answer_source_provider::ArkAnswerSourceRecognizer;
use crate::knowledge_answer_run::{
    self, BeginKnowledgeAnswerRun, ImportKnowledgeAnswerRequest, KnowledgeAnswerAnalysisResult,
};
use crate::knowledge_source_provider::ArkSourceQuestionRecognizer;
use crate::knowledge_source_run::{
    self, BeginSourceRun, ImportSourceRequest, SourceImportAnalysisResult,
};
use crate::secrets;
use crate::state::AppState;

const LOCAL_TEACHER_ACTOR_ID: &str = "local_teacher";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintQuestionTypeTargetRequest {
    question_type: String,
    count: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlueprintPreviewInput {
    class_id: i64,
    knowledge_map_public_id: String,
    curriculum_node_public_id: Option<String>,
    total_score: f64,
    question_type_targets: Vec<BlueprintQuestionTypeTargetRequest>,
    required_knowledge_node_public_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmBlueprintInput {
    request_key: String,
    title: String,
    preview: BlueprintPreviewInput,
    expected_preview_hash: String,
    selected_question_version_public_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDuplicateInput {
    request_key: String,
    left_question_version_public_id: String,
    right_question_version_public_id: String,
    decision: String,
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceOptionInput {
    label: String,
    content: String,
    order_index: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectedSourceQuestionInput {
    question_type: String,
    stem: String,
    material_text: Option<String>,
    max_score: f64,
    options: Vec<SourceOptionInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptSourceDraftInput {
    request_key: String,
    draft_public_id: String,
    expected_content_hash: String,
    corrected: Option<CorrectedSourceQuestionInput>,
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardSourceDraftInput {
    request_key: String,
    draft_public_id: String,
    expected_content_hash: String,
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmKnowledgeAnswerInput {
    request_key: String,
    match_draft_public_id: String,
    expected_content_hash: String,
    corrected_answer_json: Value,
    note: Option<String>,
}

impl From<BlueprintPreviewInput> for BlueprintPreviewRequest {
    fn from(value: BlueprintPreviewInput) -> Self {
        Self {
            class_id: value.class_id,
            knowledge_map_public_id: value.knowledge_map_public_id,
            curriculum_node_public_id: value.curriculum_node_public_id,
            total_score: value.total_score,
            question_type_targets: value
                .question_type_targets
                .into_iter()
                .map(|target| BlueprintQuestionTypeTarget {
                    question_type: target.question_type,
                    count: target.count,
                })
                .collect(),
            required_knowledge_node_public_ids: value.required_knowledge_node_public_ids,
        }
    }
}

#[tauri::command]
pub fn k1_blueprint_options(state: State<'_, AppState>) -> Result<BlueprintOptions, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    blueprint_assembly::list_blueprint_options(&connection).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_blueprint_preview(
    state: State<'_, AppState>,
    input: BlueprintPreviewInput,
) -> Result<BlueprintPreview, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    blueprint_assembly::preview_blueprint(&connection, &input.into())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_blueprint_confirm(
    state: State<'_, AppState>,
    input: ConfirmBlueprintInput,
) -> Result<BlueprintAssembly, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    blueprint_assembly::confirm_blueprint(
        &mut connection,
        &ConfirmBlueprintRequest {
            request_key: input.request_key,
            title: input.title,
            preview_request: input.preview.into(),
            expected_preview_hash: input.expected_preview_hash,
            selected_question_version_public_ids: input.selected_question_version_public_ids,
            confirmed_by: LOCAL_TEACHER_ACTOR_ID.into(),
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_blueprint_list(
    state: State<'_, AppState>,
    class_id: i64,
    limit: Option<i64>,
) -> Result<Vec<BlueprintAssembly>, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    blueprint_assembly::list_blueprint_assemblies(&connection, class_id, limit.unwrap_or(20))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_question_search(
    state: State<'_, AppState>,
    input: QuestionSearchRequest,
) -> Result<QuestionSearchResponse, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    search::search_questions(&connection, LOCAL_TEACHER_ACTOR_ID, &input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_duplicate_review(
    state: State<'_, AppState>,
    input: ReviewDuplicateInput,
) -> Result<DuplicateReviewDecision, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    search::review_duplicate(
        &mut connection,
        LOCAL_TEACHER_ACTOR_ID,
        &ReviewDuplicateRequest {
            request_key: input.request_key,
            left_question_version_public_id: input.left_question_version_public_id,
            right_question_version_public_id: input.right_question_version_public_id,
            decision: input.decision,
            note: input.note,
            decided_by: LOCAL_TEACHER_ACTOR_ID.into(),
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_candidate_review_list(
    state: State<'_, AppState>,
    include_reviewed: Option<bool>,
    limit: Option<i64>,
) -> Result<CandidateReviewInbox, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    question_candidate_review::list_candidate_review_inbox(
        &connection,
        LOCAL_TEACHER_ACTOR_ID,
        include_reviewed.unwrap_or(false),
        limit.unwrap_or(100),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_candidate_promote_l1(
    state: State<'_, AppState>,
    input: PromoteCandidateRequest,
) -> Result<CandidateReviewDecision, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    question_candidate_review::promote_candidate_to_l1(
        &mut connection,
        LOCAL_TEACHER_ACTOR_ID,
        &input,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_candidate_discard(
    state: State<'_, AppState>,
    input: DiscardCandidateRequest,
) -> Result<CandidateReviewDecision, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    question_candidate_review::discard_candidate(&mut connection, LOCAL_TEACHER_ACTOR_ID, &input)
        .map_err(|error| error.to_string())
}

/// 一键导入空白卷或电子题目文件并提取题面。
///
/// 本地解析、PDF 渲染和外部调用均不持 SQLite 锁；来源先归档为 teaching_content。
/// 成功结果只进入独立来源草稿箱，不创建答案、作业或正式可批改题。
#[tauri::command]
pub async fn k1_source_import_analyze(
    state: State<'_, AppState>,
    input: ImportSourceRequest,
) -> Result<SourceImportAnalysisResult, String> {
    let prepared = knowledge_source_run::prepare_source(&input.path, &input.source_type)
        .map_err(|error| error.to_string())?;
    let document = {
        let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        knowledge_source_run::persist_source(
            &mut connection,
            &state.data_dir,
            &prepared,
            &input.source_type,
            &input.request_key,
        )
        .map_err(|error| error.to_string())?
    };
    let extraction_input = std::sync::Arc::new(prepared.extraction_input(&document));
    let creds = secrets::load(&state.data_dir).map_err(|error| error.to_string())?;
    let recognizer = ArkSourceQuestionRecognizer::from_creds(&creds);
    let begin = {
        let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        knowledge_source_run::begin(
            &connection,
            &extraction_input,
            &recognizer,
            &input.request_key,
            document.source_artifact_id,
        )
        .map_err(|error| error.to_string())?
    };
    let ai_run_id = match begin {
        BeginSourceRun::Completed { ai_run_id } => ai_run_id,
        BeginSourceRun::Execute { ai_run_id } => {
            let worker_input = std::sync::Arc::clone(&extraction_input);
            let result =
                tauri::async_runtime::spawn_blocking(move || recognizer.recognize(&worker_input))
                    .await
                    .unwrap_or_else(|_| {
                        Err(suite_core::error::CoreError::Invalid(
                            "题目提取任务意外中断，来源已保留，可稍后重试".into(),
                        ))
                    });
            let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
            knowledge_source_run::finish(&connection, &extraction_input, ai_run_id, result)
                .map_err(|error| error.to_string())?;
            ai_run_id
        }
    };
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    knowledge_source_run::materialize_result(&mut connection, document, ai_run_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_source_inbox(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<SourceInboxItem>, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    source_documents::list_source_inbox(&connection, LOCAL_TEACHER_ACTOR_ID, limit.unwrap_or(50))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_source_accept(
    state: State<'_, AppState>,
    input: AcceptSourceDraftInput,
) -> Result<SourceDraftReview, String> {
    let corrected = input.corrected.map(|question| CorrectedSourceQuestion {
        question_type: question.question_type,
        stem: question.stem,
        material_text: question.material_text,
        max_score: question.max_score,
        options: question
            .options
            .into_iter()
            .map(|option| SourceOptionDraft {
                label: option.label,
                content: option.content,
                order_index: option.order_index,
            })
            .collect(),
    });
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    source_documents::accept_source_draft(
        &mut connection,
        LOCAL_TEACHER_ACTOR_ID,
        &ReviewSourceDraftRequest {
            request_key: input.request_key,
            draft_public_id: input.draft_public_id,
            expected_content_hash: input.expected_content_hash,
            corrected,
            reviewed_by: LOCAL_TEACHER_ACTOR_ID.into(),
            note: input.note,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_source_discard(
    state: State<'_, AppState>,
    input: DiscardSourceDraftInput,
) -> Result<SourceDraftReview, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    source_documents::discard_source_draft(
        &mut connection,
        LOCAL_TEACHER_ACTOR_ID,
        &DiscardSourceDraftRequest {
            request_key: input.request_key,
            draft_public_id: input.draft_public_id,
            expected_content_hash: input.expected_content_hash,
            reviewed_by: LOCAL_TEACHER_ACTOR_ID.into(),
            note: input.note,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_answer_targets(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<AnswerTargetSet>, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    answer_sources::list_answer_target_sets(
        &connection,
        LOCAL_TEACHER_ACTOR_ID,
        limit.unwrap_or(50),
    )
    .map_err(|error| error.to_string())
}

/// 为一批已确认题面上传独立答案图片/PDF/文本/Office 文件并生成逐题匹配草稿。
///
/// 文件解析与模型调用均在 SQLite 锁外；成功后仍需老师显式确认每题或高置信度批次。
#[tauri::command]
pub async fn k1_answer_import_analyze(
    state: State<'_, AppState>,
    input: ImportKnowledgeAnswerRequest,
) -> Result<KnowledgeAnswerAnalysisResult, String> {
    let prepared = knowledge_answer_run::prepare(&input.path).map_err(|error| error.to_string())?;
    let document = {
        let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        knowledge_answer_run::persist(
            &mut connection,
            &state.data_dir,
            &prepared,
            &input.question_source_document_public_id,
            &input.request_key,
        )
        .map_err(|error| error.to_string())?
    };
    let run_input = {
        let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        knowledge_answer_run::build_input(&connection, &prepared, document.clone())
            .map_err(|error| error.to_string())?
    };
    let creds = secrets::load(&state.data_dir).map_err(|error| error.to_string())?;
    let recognizer = ArkAnswerSourceRecognizer::from_creds(&creds);
    let descriptor: AnswerSourceRecognizerDescriptor = recognizer.descriptor();
    let run_input = std::sync::Arc::new(run_input);
    let begin = {
        let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        knowledge_answer_run::begin(&connection, &run_input, &descriptor, &input.request_key)
            .map_err(|error| error.to_string())?
    };
    let ai_run_id = match begin {
        BeginKnowledgeAnswerRun::Completed { ai_run_id } => ai_run_id,
        BeginKnowledgeAnswerRun::Execute { ai_run_id } => {
            let worker_input = std::sync::Arc::clone(&run_input);
            let result = tauri::async_runtime::spawn_blocking(move || {
                knowledge_answer_run::recognize(&recognizer, &worker_input)
            })
            .await
            .unwrap_or_else(|_| {
                let failure = module_exam::answer_source_recognition::AnswerSourceFailure {
                    schema_version: 1,
                    code: module_exam::answer_source_recognition::AnswerSourceErrorCode::Internal,
                    safe_message: "答案识别任务意外中断，来源已保留，可稍后重试".into(),
                    retryable: true,
                };
                Err(failure)
            });
            let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
            knowledge_answer_run::finish(&connection, &run_input, ai_run_id, result)
                .map_err(|error| error.to_string())?;
            ai_run_id
        }
    };
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    knowledge_answer_run::materialize_result(&mut connection, document, ai_run_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_answer_inbox(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<AnswerSourceInboxItem>, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    answer_sources::list_answer_source_inbox(
        &connection,
        LOCAL_TEACHER_ACTOR_ID,
        limit.unwrap_or(50),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_answer_confirm(
    state: State<'_, AppState>,
    input: ConfirmKnowledgeAnswerInput,
) -> Result<AnswerMatchReview, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    answer_sources::confirm_answer_match(
        &mut connection,
        LOCAL_TEACHER_ACTOR_ID,
        &ConfirmAnswerMatchRequest {
            request_key: input.request_key,
            match_draft_public_id: input.match_draft_public_id,
            expected_content_hash: input.expected_content_hash,
            corrected_answer_json: input.corrected_answer_json,
            reviewed_by: LOCAL_TEACHER_ACTOR_ID.into(),
            note: input.note,
        },
    )
    .map_err(|error| error.to_string())
}
