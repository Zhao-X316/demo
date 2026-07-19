//! K1 题库组卷教师入口。

use module_exam::service::blueprint_assembly::{
    self, BlueprintAssembly, BlueprintOptions, BlueprintPreview, BlueprintPreviewRequest,
    BlueprintQuestionTypeTarget, ConfirmBlueprintRequest,
};
use module_exam::service::question_candidate_review::{
    self, CandidateReviewDecision, CandidateReviewInbox, DiscardCandidateRequest,
    PromoteCandidateRequest,
};
use module_knowledge::db::search::{
    self, DuplicateReviewDecision, QuestionSearchRequest, QuestionSearchResponse,
    ReviewDuplicateRequest,
};
use serde::Deserialize;
use tauri::State;

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
