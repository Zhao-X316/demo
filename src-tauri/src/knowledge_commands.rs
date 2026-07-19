//! K1 题库组卷教师入口。

use module_exam::service::blueprint_assembly::{
    self, BlueprintAssembly, BlueprintOptions, BlueprintPreview, BlueprintPreviewRequest,
    BlueprintQuestionTypeTarget, ConfirmBlueprintRequest,
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
