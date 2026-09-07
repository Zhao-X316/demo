//! Serialized intake inputs and outcomes shared with the desktop UI.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedIntakeOption {
    pub class_id: i64,
    pub class_name: String,
    pub assessment_id: i64,
    pub assessment_version_id: i64,
    pub assessment_title: String,
    pub revision: i64,
    pub template_version: Option<String>,
    pub item_count: i64,
    pub is_default: bool,
    pub default_selection_public_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedIntakeRequest {
    pub assessment_version_id: i64,
    pub student_paths: Vec<String>,
    pub answer_path: Option<String>,
    pub answer_text: Option<String>,
    pub expected_pages_per_attempt: i64,
    #[serde(default)]
    pub material_type: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedIntakeDocumentSummary {
    pub role: String,
    pub format: String,
    pub original_name: String,
    pub page_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupingRosterStudent {
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedIntakeResult {
    pub batch_id: i64,
    pub batch_public_id: String,
    pub documents: Vec<FixedIntakeDocumentSummary>,
    pub student_document_count: i64,
    pub student_page_count: i64,
    pub answer_document_count: i64,
    pub route: String,
    pub target_count: i64,
    pub ready_count: i64,
    pub review_count: i64,
    pub blocked_count: i64,
    pub completed_count: i64,
    pub reason_codes: Vec<String>,
    pub order_policy: String,
    pub order_confidence: f64,
    pub order_conflict_codes: Vec<String>,
    pub material_type: String,
    pub material_type_decision: String,
    pub material_type_confidence: f64,
    pub material_type_needs_confirmation: bool,
    pub grouping_route: String,
    pub student_group_count: i64,
    pub grouping_issue_codes: Vec<String>,
    pub expected_pages_per_attempt: i64,
    pub page_cycle_source: String,
    pub page_cycle_confidence: f64,
    pub page_cycle_needs_teacher_input: bool,
    pub grouping_roster: Vec<GroupingRosterStudent>,
    pub grouping_confirmed: bool,
    pub grouping_first_student_no: Option<String>,
    pub grouping_last_student_no: Option<String>,
    pub quality_review_completed: bool,
    pub mapped_group_count: i64,
    pub rejected_group_count: i64,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageCycleSuggestion {
    pub expected_pages_per_attempt: i64,
    pub confidence: f64,
    pub source: String,
    pub issue_codes: Vec<String>,
    pub needs_teacher_input: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialTypeConfirmationResult {
    pub material_type: String,
    pub material_type_decision: String,
    pub material_type_confidence: f64,
    pub grouping_route: String,
    pub student_group_count: i64,
    pub grouping_issue_codes: Vec<String>,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupingConfirmationResult {
    pub grouping_route: String,
    pub student_group_count: i64,
    pub grouping_issue_codes: Vec<String>,
    pub grouping_confirmed: bool,
    pub grouping_first_student_no: String,
    pub grouping_last_student_no: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupingQualityConfirmationResult {
    pub quality_review_completed: bool,
    pub mapped_group_count: i64,
    pub rejected_group_count: i64,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupingRetakeResult {
    pub replacement_page_id: i64,
    pub activated_student: bool,
    pub mapped_group_count: i64,
    pub rejected_group_count: i64,
    pub next_action: String,
}
