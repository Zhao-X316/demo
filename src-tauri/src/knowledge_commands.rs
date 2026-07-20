//! K1 题库组卷教师入口。

use module_exam::answer_source_recognition::{
    AnswerSourceRecognizer, AnswerSourceRecognizerDescriptor,
};
use module_exam::service::blueprint_assembly::{
    self, BlueprintAssembly, BlueprintOptions, BlueprintPreview, BlueprintPreviewRequest,
    BlueprintQuestionTypeTarget, ConfirmBlueprintRequest,
};
use module_exam::service::blueprint_paper::{
    self, BlueprintPaperEdition, BlueprintPaperEditor, BlueprintPaperItemInput,
    ConfirmBlueprintPaperRequest, WrittenBlueprintPaper,
};
use module_exam::service::question_candidate_review::{
    self, CandidateReviewDecision, CandidateReviewInbox, DiscardCandidateRequest,
    PromoteCandidateRequest,
};
use module_exam::service::question_performance::{
    self, AssessmentDefaultUpgrade, ConfirmQuestionImpactPlanRequest,
    PrepareQuestionImpactReviewCasesRequest, PrepareQuestionImpactReviewCasesResult,
    PublishQuestionImpactReviewCaseRequest, PublishQuestionImpactReviewCaseResult,
    QuestionImpactPlan, QuestionImpactReviewCaseCatalog, QuestionPerformanceCatalog,
    QuestionVersionImpactPreview, ResolveQuestionImpactReviewCaseRequest,
    ResolveQuestionImpactReviewCaseResult, UpgradeAssessmentDefaultRequest,
};
use module_knowledge::db::answer_sources::{
    self, AnswerMatchReview, AnswerSourceInboxItem, AnswerTargetSet, ConfirmAnswerMatchRequest,
};
use module_knowledge::db::link_reviews::{
    self, ConfirmLinkReviewRequest, LinkReviewCatalog, LinkReviewInboxItem, LinkReviewResult,
};
use module_knowledge::db::search::{
    self, DuplicateReviewDecision, QuestionSearchRequest, QuestionSearchResponse,
    ReviewDuplicateRequest,
};
use module_knowledge::db::source_documents::{
    self, CorrectedSourceQuestion, DiscardSourceDraftRequest, ReviewSourceDraftRequest,
    SourceDraftReview, SourceInboxItem,
};
use module_knowledge::link_suggestion::{LinkSuggester, LinkSuggestionInput};
use module_knowledge::semantic_search::{
    validate_output as validate_semantic_output, SemanticQuestionSearcher, SemanticSearchCandidate,
    SemanticSearchInput,
};
use module_knowledge::source_import::{SourceOptionDraft, SourceQuestionRecognizer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::answer_source_provider::ArkAnswerSourceRecognizer;
use crate::knowledge_answer_run::{
    self, BeginKnowledgeAnswerRun, ImportKnowledgeAnswerRequest, KnowledgeAnswerAnalysisResult,
};
use crate::knowledge_link_provider::ArkKnowledgeLinkSuggester;
use crate::knowledge_link_run::{self, BeginLinkRun, LinkSuggestionAnalysisResult};
use crate::knowledge_semantic_provider::ArkSemanticQuestionSearcher;
use crate::knowledge_semantic_run::{
    self, BeginSemanticRun, SemanticRunFailure, SemanticSearchAnalysisResult,
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
pub struct BlueprintPaperItemRequest {
    source_slot_order_index: i64,
    question_version_public_id: String,
    page_break_before: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmBlueprintPaperInput {
    request_key: String,
    assembly_public_id: String,
    expected_source_assessment_version_public_id: String,
    title: String,
    items: Vec<BlueprintPaperItemRequest>,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestKnowledgeLinksInput {
    question_version_public_id: String,
    knowledge_map_public_id: String,
    request_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticQuestionSearchInput {
    request_key: String,
    search: QuestionSearchRequest,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticQuestionSearchItem {
    candidate: SemanticSearchCandidate,
    score: f64,
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticQuestionSearchResponse {
    ai_run_id: i64,
    status: String,
    state: String,
    confidence: Option<f64>,
    issue_codes: Vec<String>,
    items: Vec<SemanticQuestionSearchItem>,
    failure: Option<SemanticRunFailure>,
    catalog_snapshot_hash: String,
    boundary_note: String,
}

fn semantic_search_response(
    input: &SemanticSearchInput,
    result: SemanticSearchAnalysisResult,
) -> suite_core::error::CoreResult<SemanticQuestionSearchResponse> {
    let SemanticSearchAnalysisResult {
        ai_run_id,
        status,
        output,
        failure,
    } = result;
    let Some(output) = output else {
        return Ok(SemanticQuestionSearchResponse {
            ai_run_id,
            status,
            state: "failed".into(),
            confidence: None,
            issue_codes: Vec::new(),
            items: Vec::new(),
            failure,
            catalog_snapshot_hash: input.catalog_snapshot_hash.clone(),
            boundary_note:
                "本机关键词查找仍可使用；失败的 AI 结果不会修改题库、作业、成绩或学习证据。".into(),
        });
    };
    validate_semantic_output(input, &output)?;
    let mut items = Vec::with_capacity(output.matches.len());
    for matched in &output.matches {
        let candidate = input
            .candidates
            .iter()
            .find(|candidate| {
                candidate.question_version_public_id == matched.question_version_public_id
            })
            .cloned()
            .ok_or_else(|| {
                suite_core::error::CoreError::Invalid("语义找题结果不属于冻结候选清单".into())
            })?;
        items.push(SemanticQuestionSearchItem {
            candidate,
            score: matched.score,
            reason: matched.reason.clone(),
        });
    }
    Ok(SemanticQuestionSearchResponse {
        ai_run_id,
        status,
        state: output.state,
        confidence: Some(output.confidence),
        issue_codes: output.issue_codes,
        items,
        failure,
        catalog_snapshot_hash: input.catalog_snapshot_hash.clone(),
        boundary_note: format!(
            "本机先按老师权限和结构化条件冻结 {} 道候选；AI 只在该清单内按意思排序。结果仅供选题，不会自动合并、改答案、发布、重算历史成绩或形成学习证据。",
            input.candidates.len()
        ),
    })
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
pub fn k1_blueprint_paper_editor(
    state: State<'_, AppState>,
    assembly_public_id: String,
) -> Result<BlueprintPaperEditor, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    blueprint_paper::load_blueprint_paper_editor(&connection, &assembly_public_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_blueprint_paper_confirm(
    state: State<'_, AppState>,
    input: ConfirmBlueprintPaperInput,
) -> Result<BlueprintPaperEdition, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    blueprint_paper::confirm_blueprint_paper(
        &mut connection,
        &ConfirmBlueprintPaperRequest {
            request_key: input.request_key,
            assembly_public_id: input.assembly_public_id,
            expected_source_assessment_version_public_id: input
                .expected_source_assessment_version_public_id,
            title: input.title,
            items: input
                .items
                .into_iter()
                .map(|item| BlueprintPaperItemInput {
                    source_slot_order_index: item.source_slot_order_index,
                    question_version_public_id: item.question_version_public_id,
                    page_break_before: item.page_break_before,
                })
                .collect(),
            confirmed_by: LOCAL_TEACHER_ACTOR_ID.into(),
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_blueprint_paper_write(
    state: State<'_, AppState>,
    edition_public_id: String,
    export_kind: String,
    output_path: String,
) -> Result<WrittenBlueprintPaper, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    blueprint_paper::write_blueprint_paper_html(
        &connection,
        &edition_public_id,
        &export_kind,
        &output_path,
    )
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

/// 在本地有权访问的有限题目清单内按意思排序。
///
/// 候选清单先在短锁内冻结，外部调用不持 SQLite 锁；模型只能返回清单内版本 ID。
#[tauri::command]
pub async fn k1_question_semantic_search(
    state: State<'_, AppState>,
    input: SemanticQuestionSearchInput,
) -> Result<SemanticQuestionSearchResponse, String> {
    let semantic_input = {
        let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        search::prepare_semantic_search(&connection, LOCAL_TEACHER_ACTOR_ID, &input.search)
            .map_err(|error| error.to_string())?
    };
    let creds = secrets::load(&state.data_dir).map_err(|error| error.to_string())?;
    let searcher = ArkSemanticQuestionSearcher::from_creds(&creds);
    let begin = {
        let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        knowledge_semantic_run::begin(&connection, &semantic_input, &searcher, &input.request_key)
            .map_err(|error| error.to_string())?
    };
    let result = match begin {
        BeginSemanticRun::Completed(result) => *result,
        BeginSemanticRun::Execute { ai_run_id } => {
            let worker_input = std::sync::Arc::new(semantic_input.clone());
            let thread_input = std::sync::Arc::clone(&worker_input);
            let result =
                tauri::async_runtime::spawn_blocking(move || searcher.search(&thread_input))
                    .await
                    .unwrap_or_else(|_| {
                        Err(suite_core::error::CoreError::Invalid(
                            "语义找题任务意外中断，可稍后重试或改用关键词查找".into(),
                        ))
                    });
            let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
            knowledge_semantic_run::finish(&connection, &worker_input, ai_run_id, result)
                .map_err(|error| error.to_string())?
        }
    };
    semantic_search_response(&semantic_input, result).map_err(|error| error.to_string())
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
pub fn k1_question_performance(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<QuestionPerformanceCatalog, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    question_performance::list_question_performance(
        &connection,
        LOCAL_TEACHER_ACTOR_ID,
        limit.unwrap_or(200),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_question_impact_preview(
    state: State<'_, AppState>,
    question_version_public_id: String,
) -> Result<QuestionVersionImpactPreview, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    question_performance::preview_question_version_impact(
        &connection,
        LOCAL_TEACHER_ACTOR_ID,
        &question_version_public_id,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_question_impact_confirm(
    state: State<'_, AppState>,
    input: ConfirmQuestionImpactPlanRequest,
) -> Result<QuestionImpactPlan, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    question_performance::confirm_question_impact_plan(
        &mut connection,
        LOCAL_TEACHER_ACTOR_ID,
        &input,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_question_impact_cases(
    state: State<'_, AppState>,
    plan_public_id: String,
) -> Result<QuestionImpactReviewCaseCatalog, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    question_performance::list_question_impact_review_cases(
        &connection,
        LOCAL_TEACHER_ACTOR_ID,
        &plan_public_id,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_question_impact_prepare(
    state: State<'_, AppState>,
    input: PrepareQuestionImpactReviewCasesRequest,
) -> Result<PrepareQuestionImpactReviewCasesResult, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    question_performance::prepare_question_impact_review_cases(
        &mut connection,
        LOCAL_TEACHER_ACTOR_ID,
        &input,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_question_impact_resolve(
    state: State<'_, AppState>,
    input: ResolveQuestionImpactReviewCaseRequest,
) -> Result<ResolveQuestionImpactReviewCaseResult, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    question_performance::resolve_question_impact_review_case(
        &mut connection,
        LOCAL_TEACHER_ACTOR_ID,
        &input,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_question_impact_publish(
    state: State<'_, AppState>,
    input: PublishQuestionImpactReviewCaseRequest,
) -> Result<PublishQuestionImpactReviewCaseResult, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    question_performance::publish_question_impact_review_case(
        &connection,
        LOCAL_TEACHER_ACTOR_ID,
        &input,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_question_impact_upgrade_assessment(
    state: State<'_, AppState>,
    input: UpgradeAssessmentDefaultRequest,
) -> Result<AssessmentDefaultUpgrade, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    question_performance::upgrade_assessment_default_from_impact(
        &mut connection,
        LOCAL_TEACHER_ACTOR_ID,
        &input,
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

#[tauri::command]
pub fn k1_link_review_catalog(state: State<'_, AppState>) -> Result<LinkReviewCatalog, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    link_reviews::list_catalog(&connection).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_link_review_inbox(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<LinkReviewInboxItem>, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    link_reviews::list_review_inbox(&connection, LOCAL_TEACHER_ACTOR_ID, limit.unwrap_or(100))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_link_review_editor(
    state: State<'_, AppState>,
    question_version_public_id: String,
    knowledge_map_public_id: String,
) -> Result<LinkSuggestionInput, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    link_reviews::build_suggestion_input(
        &connection,
        LOCAL_TEACHER_ACTOR_ID,
        &question_version_public_id,
        &knowledge_map_public_id,
    )
    .map_err(|error| error.to_string())
}

/// 为当前 L2 题目生成知识/能力链接草稿。
///
/// 输入只包含题目、已确认答案槽位/评分点和已确认知识目录；模型调用不持 SQLite 锁，
/// 成功后也只创建草稿，老师确认前不晋级、不进入图谱。
#[tauri::command]
pub async fn k1_link_suggest(
    state: State<'_, AppState>,
    input: SuggestKnowledgeLinksInput,
) -> Result<LinkSuggestionAnalysisResult, String> {
    let suggestion_input = {
        let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        link_reviews::build_suggestion_input(
            &connection,
            LOCAL_TEACHER_ACTOR_ID,
            &input.question_version_public_id,
            &input.knowledge_map_public_id,
        )
        .map_err(|error| error.to_string())?
    };
    let creds = secrets::load(&state.data_dir).map_err(|error| error.to_string())?;
    let suggester = ArkKnowledgeLinkSuggester::from_creds(&creds);
    let begin = {
        let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
        knowledge_link_run::begin(
            &connection,
            &suggestion_input,
            &suggester,
            &input.request_key,
        )
        .map_err(|error| error.to_string())?
    };
    let result = match begin {
        BeginLinkRun::Completed(result) => *result,
        BeginLinkRun::Execute { ai_run_id } => {
            let worker_input = std::sync::Arc::new(suggestion_input.clone());
            let thread_input = std::sync::Arc::clone(&worker_input);
            let result =
                tauri::async_runtime::spawn_blocking(move || suggester.suggest(&thread_input))
                    .await
                    .unwrap_or_else(|_| {
                        Err(suite_core::error::CoreError::Invalid(
                            "知识链接建议任务意外中断，可稍后重试或手工关联".into(),
                        ))
                    });
            let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
            knowledge_link_run::finish(&connection, &worker_input, ai_run_id, result)
                .map_err(|error| error.to_string())?
        }
    };
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    knowledge_link_run::materialize(
        &mut connection,
        LOCAL_TEACHER_ACTOR_ID,
        &suggestion_input,
        result,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn k1_link_confirm(
    state: State<'_, AppState>,
    input: ConfirmLinkReviewRequest,
) -> Result<LinkReviewResult, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    link_reviews::confirm_links(&mut connection, LOCAL_TEACHER_ACTOR_ID, &input)
        .map_err(|error| error.to_string())
}
