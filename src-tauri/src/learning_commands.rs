//! M3 错题事实 / M6 掌握分析教师入口命令。

use module_profile::class_profile::{
    self as class_profile_service, ClassProfilePreview, ClassProfileScope, ClassProfileSnapshot,
    GenerateClassProfileInput,
};
use module_profile::profile::{
    self, GenerateStudentProfileInput, StudentProfilePreview, StudentProfileScope,
    StudentProfileSnapshot,
};
use module_profile::teaching_events::{
    self, ClassTeachingEvent, CreateTeachingEventInput, ReviseTeachingEventInput,
    VoidTeachingEventInput,
};
use module_wrongbook::correction::{self, CorrectionAssignment, CreateCorrectionInput};
use module_wrongbook::error_cause::{self, ConfirmErrorCausesInput, ErrorCauseReview};
use module_wrongbook::read_model::{self, ClassWrongbookDashboard};
use module_wrongbook::reinforcement::{
    self, ConfirmReinforcementInput, ReinforcementAssignment, ReinforcementScopeInput,
    ReinforcementSuggestion,
};
use module_wrongbook::report::{
    self, CreateWrongbookReportInput, WrittenWrongbookReport, WrongbookReportSnapshot,
    WrongbookStatistics, WrongbookStatisticsScope,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrongbookStatisticsRequest {
    class_id: i64,
    student_id: Option<i64>,
    range_start: String,
    range_end: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWrongbookReportRequest {
    report_kind: String,
    class_id: i64,
    student_id: Option<i64>,
    range_start: String,
    range_end: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentProfileScopeRequest {
    class_id: i64,
    student_id: i64,
    range_start: String,
    range_end: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassProfileScopeRequest {
    class_id: i64,
    range_start: String,
    range_end: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateClassProfileRequest {
    class_id: i64,
    range_start: String,
    range_end: String,
    expected_source_watermark: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTeachingEventRequest {
    request_key: String,
    class_id: i64,
    event_type: String,
    title: String,
    range_start: String,
    range_end: String,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviseTeachingEventRequest {
    request_key: String,
    event_key: String,
    expected_revision: i64,
    event_type: String,
    title: String,
    range_start: String,
    range_end: String,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoidTeachingEventRequest {
    request_key: String,
    event_key: String,
    expected_revision: i64,
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

#[tauri::command]
pub fn wrongbook_statistics(
    state: State<'_, AppState>,
    input: WrongbookStatisticsRequest,
) -> Result<WrongbookStatistics, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    report::wrongbook_statistics(
        &connection,
        &WrongbookStatisticsScope {
            class_id: input.class_id,
            student_id: input.student_id,
            range_start: &input.range_start,
            range_end: &input.range_end,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_wrongbook_report_snapshot(
    state: State<'_, AppState>,
    input: CreateWrongbookReportRequest,
) -> Result<WrongbookReportSnapshot, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    report::create_report_snapshot(
        &mut connection,
        &CreateWrongbookReportInput {
            report_kind: &input.report_kind,
            class_id: input.class_id,
            student_id: input.student_id,
            range_start: &input.range_start,
            range_end: &input.range_end,
            generated_by: "local_teacher",
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn write_wrongbook_report_snapshot(
    state: State<'_, AppState>,
    snapshot_public_id: String,
    output_path: String,
) -> Result<WrittenWrongbookReport, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    report::write_report_snapshot_csv(&connection, &snapshot_public_id, &output_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_student_profile(
    state: State<'_, AppState>,
    input: StudentProfileScopeRequest,
) -> Result<StudentProfilePreview, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    profile::preview_student_profile(
        &connection,
        &StudentProfileScope {
            class_id: input.class_id,
            student_id: input.student_id,
            range_start: &input.range_start,
            range_end: &input.range_end,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn generate_student_profile(
    state: State<'_, AppState>,
    input: StudentProfileScopeRequest,
) -> Result<StudentProfileSnapshot, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    profile::generate_student_profile(
        &mut connection,
        &GenerateStudentProfileInput {
            scope: StudentProfileScope {
                class_id: input.class_id,
                student_id: input.student_id,
                range_start: &input.range_start,
                range_end: &input.range_end,
            },
            confirmed_by: "local_teacher",
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn latest_student_profile(
    state: State<'_, AppState>,
    class_id: i64,
    student_id: i64,
) -> Result<Option<StudentProfileSnapshot>, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    profile::latest_student_profile(&connection, class_id, student_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_class_profile(
    state: State<'_, AppState>,
    input: ClassProfileScopeRequest,
) -> Result<ClassProfilePreview, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    class_profile_service::preview_class_profile(
        &connection,
        &ClassProfileScope {
            class_id: input.class_id,
            range_start: &input.range_start,
            range_end: &input.range_end,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn generate_class_profile(
    state: State<'_, AppState>,
    input: GenerateClassProfileRequest,
) -> Result<ClassProfileSnapshot, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    class_profile_service::generate_class_profile(
        &mut connection,
        &GenerateClassProfileInput {
            scope: ClassProfileScope {
                class_id: input.class_id,
                range_start: &input.range_start,
                range_end: &input.range_end,
            },
            expected_source_watermark: &input.expected_source_watermark,
            confirmed_by: "local_teacher",
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn latest_class_profile(
    state: State<'_, AppState>,
    class_id: i64,
) -> Result<Option<ClassProfileSnapshot>, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    class_profile_service::latest_class_profile(&connection, class_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_class_teaching_events(
    state: State<'_, AppState>,
    input: ClassProfileScopeRequest,
) -> Result<Vec<ClassTeachingEvent>, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    teaching_events::list_teaching_events(
        &connection,
        input.class_id,
        &input.range_start,
        &input.range_end,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_class_teaching_event(
    state: State<'_, AppState>,
    input: CreateTeachingEventRequest,
) -> Result<ClassTeachingEvent, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    teaching_events::create_teaching_event(
        &mut connection,
        &CreateTeachingEventInput {
            request_key: &input.request_key,
            class_id: input.class_id,
            event_type: &input.event_type,
            title: &input.title,
            range_start: &input.range_start,
            range_end: &input.range_end,
            note: input.note.as_deref(),
            actor_id: "local_teacher",
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn revise_class_teaching_event(
    state: State<'_, AppState>,
    input: ReviseTeachingEventRequest,
) -> Result<ClassTeachingEvent, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    teaching_events::revise_teaching_event(
        &mut connection,
        &ReviseTeachingEventInput {
            request_key: &input.request_key,
            event_key: &input.event_key,
            expected_revision: input.expected_revision,
            event_type: &input.event_type,
            title: &input.title,
            range_start: &input.range_start,
            range_end: &input.range_end,
            note: input.note.as_deref(),
            actor_id: "local_teacher",
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn void_class_teaching_event(
    state: State<'_, AppState>,
    input: VoidTeachingEventRequest,
) -> Result<ClassTeachingEvent, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    teaching_events::void_teaching_event(
        &mut connection,
        &VoidTeachingEventInput {
            request_key: &input.request_key,
            event_key: &input.event_key,
            expected_revision: input.expected_revision,
            actor_id: "local_teacher",
        },
    )
    .map_err(|error| error.to_string())
}
