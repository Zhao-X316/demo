//! M3 错题事实 / M6 掌握分析教师入口命令。

use module_exam::service::assessment::{
    self as assessment_service, NewMultiItemTargetedAssessment, NewTargetedAssessmentItem,
};
use module_profile::action_drafts::{
    self, ClassActionDraft, ClassActionPreview, ConfirmClassActionInput, PreviewClassActionInput,
    RecordPracticeMaterializationInput, CLASS_ACTION_PRACTICE_TEMPLATE_VERSION,
};
use module_profile::class_exports::{
    self, ClassProfileExportSnapshot, CreateClassProfileExportInput, WrittenClassProfileExport,
    LOCAL_TEACHER_ACTOR_ID,
};
use module_profile::class_profile::{
    self as class_profile_service, ClassProfilePreview, ClassProfileScope, ClassProfileSnapshot,
    GenerateScopedClassProfileInput,
};
use module_profile::class_teaching_inputs::{
    self, ClassTeachingInputDraft, ClassTeachingInputPreview, ConfirmClassTeachingInput,
    PreviewClassTeachingInput,
};
use module_profile::profile::{
    self, GenerateScopedStudentProfileInput, ProfileScopeOption, ProfileScopeSelectionInput,
    StudentProfilePreview, StudentProfileScope, StudentProfileSnapshot,
};
use module_profile::student_reports::{
    self, CreateStudentProfileReportInput, StudentProfileReportSnapshot,
    WrittenStudentProfileReport,
};
use module_profile::teacher_assessments::{
    self, ProfileTeacherAssessment, SaveProfileTeacherAssessmentInput,
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
    scope_selector_kind: Option<String>,
    scope_selector_public_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProfileTeacherAssessmentRequest {
    snapshot_public_id: String,
    node_metric_public_id: String,
    expected_revision: i64,
    assessment: Option<String>,
    note: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStudentProfileReportRequest {
    request_key: String,
    snapshot_public_id: String,
    expected_snapshot_payload_sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassProfileScopeRequest {
    class_id: i64,
    range_start: String,
    range_end: String,
    scope_selector_kind: Option<String>,
    scope_selector_public_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateClassProfileRequest {
    class_id: i64,
    range_start: String,
    range_end: String,
    scope_selector_kind: Option<String>,
    scope_selector_public_id: Option<String>,
    expected_source_watermark: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateClassProfileExportRequest {
    request_key: String,
    snapshot_public_id: String,
    expected_snapshot_payload_sha256: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewClassTeachingInputRequest {
    snapshot_public_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmClassTeachingInputRequest {
    request_key: String,
    snapshot_public_id: String,
    expected_snapshot_payload_sha256: String,
    title: String,
    teaching_note: String,
    estimated_minutes: i64,
    selected_node_metric_public_ids: Vec<String>,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewClassActionRequest {
    snapshot_public_id: String,
    node_metric_public_id: String,
    action_kind: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmClassActionRequest {
    request_key: String,
    snapshot_public_id: String,
    node_metric_public_id: String,
    action_kind: String,
    expected_snapshot_payload_sha256: String,
    title: String,
    rationale: String,
    estimated_minutes: i64,
    target_student_ids: Vec<i64>,
    candidate_question_version_public_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterializeClassActionRequest {
    request_key: String,
    draft_public_id: String,
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
pub fn list_profile_scope_options(
    state: State<'_, AppState>,
) -> Result<Vec<ProfileScopeOption>, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    profile::list_profile_scope_options(&connection).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_student_profile(
    state: State<'_, AppState>,
    input: StudentProfileScopeRequest,
) -> Result<StudentProfilePreview, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let selector_kind = input
        .scope_selector_kind
        .as_deref()
        .unwrap_or("auto_evidence_maps");
    profile::preview_scoped_student_profile(
        &connection,
        &StudentProfileScope {
            class_id: input.class_id,
            student_id: input.student_id,
            range_start: &input.range_start,
            range_end: &input.range_end,
        },
        &ProfileScopeSelectionInput {
            selector_kind,
            selector_public_id: input.scope_selector_public_id.as_deref(),
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
    let selector_kind = input
        .scope_selector_kind
        .as_deref()
        .unwrap_or("auto_evidence_maps");
    profile::generate_scoped_student_profile(
        &mut connection,
        &GenerateScopedStudentProfileInput {
            scope: StudentProfileScope {
                class_id: input.class_id,
                student_id: input.student_id,
                range_start: &input.range_start,
                range_end: &input.range_end,
            },
            selection: ProfileScopeSelectionInput {
                selector_kind,
                selector_public_id: input.scope_selector_public_id.as_deref(),
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
pub fn save_profile_teacher_assessment(
    state: State<'_, AppState>,
    input: SaveProfileTeacherAssessmentRequest,
) -> Result<ProfileTeacherAssessment, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    teacher_assessments::save_profile_teacher_assessment(
        &mut connection,
        &SaveProfileTeacherAssessmentInput {
            snapshot_public_id: &input.snapshot_public_id,
            node_metric_public_id: &input.node_metric_public_id,
            expected_revision: input.expected_revision,
            assessment: input.assessment.as_deref(),
            note: input.note.as_deref(),
            actor_id: "local_teacher",
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_student_profile_report_snapshot(
    state: State<'_, AppState>,
    input: CreateStudentProfileReportRequest,
) -> Result<StudentProfileReportSnapshot, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    student_reports::create_report_snapshot(
        &mut connection,
        &CreateStudentProfileReportInput {
            request_key: &input.request_key,
            snapshot_public_id: &input.snapshot_public_id,
            expected_snapshot_payload_sha256: &input.expected_snapshot_payload_sha256,
            report_kind: "student_learning_summary",
            purpose: "teacher_internal_feedback",
            actor_role: "local_teacher",
            actor_id: LOCAL_TEACHER_ACTOR_ID,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn write_student_profile_report_snapshot(
    state: State<'_, AppState>,
    report_public_id: String,
    output_path: String,
) -> Result<WrittenStudentProfileReport, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    student_reports::write_report_snapshot_html(&connection, &report_public_id, &output_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_class_profile(
    state: State<'_, AppState>,
    input: ClassProfileScopeRequest,
) -> Result<ClassProfilePreview, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    let selector_kind = input
        .scope_selector_kind
        .as_deref()
        .unwrap_or("auto_evidence_maps");
    class_profile_service::preview_scoped_class_profile(
        &connection,
        &ClassProfileScope {
            class_id: input.class_id,
            range_start: &input.range_start,
            range_end: &input.range_end,
        },
        &ProfileScopeSelectionInput {
            selector_kind,
            selector_public_id: input.scope_selector_public_id.as_deref(),
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
    let selector_kind = input
        .scope_selector_kind
        .as_deref()
        .unwrap_or("auto_evidence_maps");
    class_profile_service::generate_scoped_class_profile(
        &mut connection,
        &GenerateScopedClassProfileInput {
            scope: ClassProfileScope {
                class_id: input.class_id,
                range_start: &input.range_start,
                range_end: &input.range_end,
            },
            selection: ProfileScopeSelectionInput {
                selector_kind,
                selector_public_id: input.scope_selector_public_id.as_deref(),
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
pub fn create_class_profile_export_snapshot(
    state: State<'_, AppState>,
    input: CreateClassProfileExportRequest,
) -> Result<ClassProfileExportSnapshot, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    class_exports::create_export_snapshot(
        &mut connection,
        &CreateClassProfileExportInput {
            request_key: &input.request_key,
            snapshot_public_id: &input.snapshot_public_id,
            expected_snapshot_payload_sha256: &input.expected_snapshot_payload_sha256,
            report_kind: "deidentified_class_summary",
            purpose: "internal_teaching",
            actor_role: "local_teacher",
            actor_id: LOCAL_TEACHER_ACTOR_ID,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn write_class_profile_export_snapshot(
    state: State<'_, AppState>,
    export_public_id: String,
    output_path: String,
) -> Result<WrittenClassProfileExport, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    class_exports::write_export_snapshot_csv(&connection, &export_public_id, &output_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_class_teaching_input(
    state: State<'_, AppState>,
    input: PreviewClassTeachingInputRequest,
) -> Result<ClassTeachingInputPreview, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    class_teaching_inputs::preview_class_teaching_input(
        &connection,
        &PreviewClassTeachingInput {
            snapshot_public_id: &input.snapshot_public_id,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn confirm_class_teaching_input(
    state: State<'_, AppState>,
    input: ConfirmClassTeachingInputRequest,
) -> Result<ClassTeachingInputDraft, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    class_teaching_inputs::confirm_class_teaching_input(
        &mut connection,
        &ConfirmClassTeachingInput {
            request_key: &input.request_key,
            snapshot_public_id: &input.snapshot_public_id,
            expected_snapshot_payload_sha256: &input.expected_snapshot_payload_sha256,
            title: &input.title,
            teaching_note: &input.teaching_note,
            estimated_minutes: input.estimated_minutes,
            selected_node_metric_public_ids: &input.selected_node_metric_public_ids,
            confirmed_by: LOCAL_TEACHER_ACTOR_ID,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_class_teaching_inputs(
    state: State<'_, AppState>,
    class_id: i64,
    limit: Option<i64>,
) -> Result<Vec<ClassTeachingInputDraft>, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    class_teaching_inputs::list_class_teaching_inputs(&connection, class_id, limit.unwrap_or(20))
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

#[tauri::command]
pub fn preview_class_action(
    state: State<'_, AppState>,
    input: PreviewClassActionRequest,
) -> Result<ClassActionPreview, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    action_drafts::preview_class_action(
        &connection,
        &PreviewClassActionInput {
            snapshot_public_id: &input.snapshot_public_id,
            node_metric_public_id: &input.node_metric_public_id,
            action_kind: &input.action_kind,
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn confirm_class_action(
    state: State<'_, AppState>,
    input: ConfirmClassActionRequest,
) -> Result<ClassActionDraft, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    action_drafts::confirm_class_action(
        &mut connection,
        &ConfirmClassActionInput {
            request_key: &input.request_key,
            snapshot_public_id: &input.snapshot_public_id,
            node_metric_public_id: &input.node_metric_public_id,
            action_kind: &input.action_kind,
            expected_snapshot_payload_sha256: &input.expected_snapshot_payload_sha256,
            title: &input.title,
            rationale: &input.rationale,
            estimated_minutes: input.estimated_minutes,
            target_student_ids: &input.target_student_ids,
            candidate_question_version_public_ids: &input.candidate_question_version_public_ids,
            confirmed_by: "local_teacher",
        },
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_class_action_drafts(
    state: State<'_, AppState>,
    class_id: i64,
    limit: Option<i64>,
) -> Result<Vec<ClassActionDraft>, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    action_drafts::list_class_action_drafts(&connection, class_id, limit.unwrap_or(20))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn materialize_class_action(
    state: State<'_, AppState>,
    input: MaterializeClassActionRequest,
) -> Result<ClassActionDraft, String> {
    let mut connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    if let Some(existing) =
        action_drafts::get_class_action_draft(&connection, &input.draft_public_id)
            .map_err(|error| error.to_string())?
    {
        if existing.materialization.is_some() {
            return Ok(existing);
        }
    }
    let plan = action_drafts::prepare_practice_materialization(&connection, &input.draft_public_id)
        .map_err(|error| error.to_string())?;
    struct ResolvedItem {
        question_version_id: i64,
        answer_key_version_id: i64,
        rubric_version_id: i64,
        link_set_id: i64,
        score: f64,
        presentation_snapshot_json: String,
    }
    let mut resolved = Vec::with_capacity(plan.candidates.len());
    for candidate in &plan.candidates {
        let item = connection
            .query_row(
                "SELECT question.id,answer.id,rubric.id,links.id,question.max_score,
                        question.public_id,question.stem,question.question_type
                 FROM k1_question_versions question
                 JOIN k1_answer_key_versions answer
                   ON answer.public_id=?2 AND answer.question_version_id=question.id
                 JOIN k1_rubric_versions rubric
                   ON rubric.public_id=?3 AND rubric.question_version_id=question.id
                 JOIN k1_link_sets links
                   ON links.public_id=?4 AND links.question_version_id=question.id
                 WHERE question.public_id=?1
                   AND question.state='published'
                   AND question.quality_level IN ('L2','L3','L4')
                   AND answer.state='confirmed'
                   AND rubric.state='confirmed'
                   AND links.state='confirmed'",
                (
                    &candidate.question_version_public_id,
                    &candidate.answer_key_version_public_id,
                    &candidate.rubric_version_public_id,
                    &candidate.link_set_public_id,
                ),
                |row| {
                    let question_public_id: String = row.get(5)?;
                    let stem: String = row.get(6)?;
                    let question_type: String = row.get(7)?;
                    Ok(ResolvedItem {
                        question_version_id: row.get(0)?,
                        answer_key_version_id: row.get(1)?,
                        rubric_version_id: row.get(2)?,
                        link_set_id: row.get(3)?,
                        score: row.get(4)?,
                        presentation_snapshot_json: serde_json::json!({
                            "schema_version": 1,
                            "source": "m6.1_class_action",
                            "question_version_public_id": question_public_id,
                            "stem": stem,
                            "question_type": question_type
                        })
                        .to_string(),
                    })
                },
            )
            .map_err(|error| error.to_string())?;
        resolved.push(item);
    }
    let borrowed_items: Vec<_> = resolved
        .iter()
        .map(|item| NewTargetedAssessmentItem {
            question_version_id: item.question_version_id,
            answer_key_version_id: item.answer_key_version_id,
            rubric_version_id: item.rubric_version_id,
            link_set_id: item.link_set_id,
            score: item.score,
            option_order_json: None,
            presentation_snapshot_json: &item.presentation_snapshot_json,
        })
        .collect();
    let tx = connection
        .transaction()
        .map_err(|error| error.to_string())?;
    let assessment = assessment_service::create_confirmed_multi_item_targeted_in_transaction(
        &tx,
        &NewMultiItemTargetedAssessment {
            title: &plan.title,
            class_id: plan.class_id,
            target_student_ids: &plan.target_student_ids,
            assessment_context: "homework",
            evidence_policy: "include_low_weight",
            template_version: CLASS_ACTION_PRACTICE_TEMPLATE_VERSION,
            created_by: "local_teacher",
            items: &borrowed_items,
        },
    )
    .map_err(|error| error.to_string())?;
    action_drafts::record_practice_materialization_in_transaction(
        &tx,
        &RecordPracticeMaterializationInput {
            request_key: &input.request_key,
            draft_id: plan.draft_id,
            draft_public_id: &plan.draft_public_id,
            destination_public_id: &assessment.assessment_public_id,
            destination_version_public_id: &assessment.assessment_version_public_id,
            created_by: "local_teacher",
        },
    )
    .map_err(|error| error.to_string())?;
    tx.commit().map_err(|error| error.to_string())?;
    action_drafts::get_class_action_draft(&connection, &input.draft_public_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "教学行动草稿不存在".to_string())
}
