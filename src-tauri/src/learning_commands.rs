//! M3 错题事实 / M6 掌握分析教师入口命令。

use module_wrongbook::correction::{self, CorrectionAssignment, CreateCorrectionInput};
use module_wrongbook::error_cause::{self, ConfirmErrorCausesInput, ErrorCauseReview};
use module_wrongbook::read_model::{self, ClassWrongbookDashboard};
use serde::Deserialize;
use tauri::State;

use crate::state::AppState;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmWrongbookErrorCausesRequest {
    class_id: i64,
    student_id: i64,
    question_version_public_id: String,
    grade_decision_public_id: String,
    publication_public_id: String,
    cause_codes: Vec<String>,
    teacher_note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWrongbookCorrectionRequest {
    class_id: i64,
    student_id: i64,
    question_version_public_id: String,
    source_grade_decision_public_id: String,
    source_publication_public_id: String,
}

#[tauri::command]
pub fn class_wrongbook_dashboard(
    state: State<'_, AppState>,
    class_id: i64,
) -> Result<ClassWrongbookDashboard, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    read_model::class_wrongbook_dashboard(&connection, class_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn confirm_wrongbook_error_causes(
    state: State<'_, AppState>,
    input: ConfirmWrongbookErrorCausesRequest,
) -> Result<ErrorCauseReview, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    error_cause::confirm_error_causes(
        &mut connection,
        &ConfirmErrorCausesInput {
            class_id: input.class_id,
            student_id: input.student_id,
            question_version_public_id: &input.question_version_public_id,
            grade_decision_public_id: &input.grade_decision_public_id,
            publication_public_id: &input.publication_public_id,
            cause_codes: &input.cause_codes,
            teacher_note: input.teacher_note.as_deref(),
            confirmed_by: "local_teacher",
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_wrongbook_single_correction(
    state: State<'_, AppState>,
    input: CreateWrongbookCorrectionRequest,
) -> Result<CorrectionAssignment, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    correction::create_single_correction(
        &mut connection,
        &CreateCorrectionInput {
            class_id: input.class_id,
            student_id: input.student_id,
            question_version_public_id: &input.question_version_public_id,
            source_grade_decision_public_id: &input.source_grade_decision_public_id,
            source_publication_public_id: &input.source_publication_public_id,
            created_by: "local_teacher",
        },
    )
    .map_err(|error| error.to_string())
}
