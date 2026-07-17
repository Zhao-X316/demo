//! M3 错题事实 / M6 掌握分析教师入口命令。

use module_wrongbook::correction::{self, CorrectionAssignment, CreateCorrectionInput};
use module_wrongbook::error_cause::{self, ConfirmErrorCausesInput, ErrorCauseReview};
use module_wrongbook::read_model::{self, ClassWrongbookDashboard};
use module_wrongbook::reinforcement::{
    self, ConfirmReinforcementInput, ReinforcementAssignment, ReinforcementScopeInput,
    ReinforcementSuggestion,
};
use serde::Deserialize;
use suite_core::services::scheduling::{ScheduleHoliday, SchedulePolicy, SchedulePolicyUpdate};
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrongbookReinforcementScopeRequest {
    class_id: i64,
    student_id: i64,
    question_version_public_id: String,
    source_grade_decision_public_id: String,
    source_publication_public_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmWrongbookReinforcementRequest {
    class_id: i64,
    student_id: i64,
    question_version_public_id: String,
    source_grade_decision_public_id: String,
    source_publication_public_id: String,
    expected_policy_public_id: String,
    expected_due_date: String,
    previewed_as_of_date: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWrongbookSchedulePolicyRequest {
    default_delay_days: i64,
    daily_limit_per_student: i64,
    weekend_policy: String,
    holiday_policy: String,
    max_shift_days: i64,
    holidays: Vec<ScheduleHoliday>,
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

#[tauri::command]
pub fn wrongbook_schedule_policy(state: State<'_, AppState>) -> Result<SchedulePolicy, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    reinforcement::load_schedule_policy(&connection).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_wrongbook_schedule_policy(
    state: State<'_, AppState>,
    input: UpdateWrongbookSchedulePolicyRequest,
) -> Result<SchedulePolicy, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    reinforcement::update_schedule_policy(
        &mut connection,
        &SchedulePolicyUpdate {
            default_delay_days: input.default_delay_days,
            daily_limit_per_student: input.daily_limit_per_student,
            weekend_policy: input.weekend_policy,
            holiday_policy: input.holiday_policy,
            max_shift_days: input.max_shift_days,
            holidays: input.holidays,
        },
        "local_teacher",
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_wrongbook_reinforcement(
    state: State<'_, AppState>,
    input: WrongbookReinforcementScopeRequest,
) -> Result<ReinforcementSuggestion, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    reinforcement::preview_reinforcement(
        &connection,
        &ReinforcementScopeInput {
            class_id: input.class_id,
            student_id: input.student_id,
            question_version_public_id: &input.question_version_public_id,
            source_grade_decision_public_id: &input.source_grade_decision_public_id,
            source_publication_public_id: &input.source_publication_public_id,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn confirm_wrongbook_reinforcement(
    state: State<'_, AppState>,
    input: ConfirmWrongbookReinforcementRequest,
) -> Result<ReinforcementAssignment, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    reinforcement::confirm_reinforcement(
        &mut connection,
        &ConfirmReinforcementInput {
            scope: ReinforcementScopeInput {
                class_id: input.class_id,
                student_id: input.student_id,
                question_version_public_id: &input.question_version_public_id,
                source_grade_decision_public_id: &input.source_grade_decision_public_id,
                source_publication_public_id: &input.source_publication_public_id,
            },
            expected_policy_public_id: &input.expected_policy_public_id,
            expected_due_date: &input.expected_due_date,
            previewed_as_of_date: &input.previewed_as_of_date,
            created_by: "local_teacher",
        },
    )
    .map_err(|error| error.to_string())
}
