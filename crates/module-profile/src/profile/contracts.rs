//! Public inputs and snapshot contracts; persistence and evidence rules live in profile.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct StudentProfileScope<'a> {
    pub class_id: i64,
    pub student_id: i64,
    pub range_start: &'a str,
    pub range_end: &'a str,
}

#[derive(Debug, Clone)]
pub struct ProfileScopeSelectionInput<'a> {
    pub selector_kind: &'a str,
    pub selector_public_id: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct GenerateStudentProfileInput<'a> {
    pub scope: StudentProfileScope<'a>,
    pub confirmed_by: &'a str,
}

#[derive(Debug, Clone)]
pub struct GenerateScopedStudentProfileInput<'a> {
    pub scope: StudentProfileScope<'a>,
    pub selection: ProfileScopeSelectionInput<'a>,
    pub confirmed_by: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileScopeOption {
    pub selector_kind: String,
    pub selector_public_id: Option<String>,
    pub selector_key: String,
    pub label: String,
    pub detail: String,
    pub node_type: Option<String>,
    pub knowledge_map_public_id: Option<String>,
    pub textbook_edition_public_id: Option<String>,
    pub knowledge_node_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileScopeSelectionView {
    pub selector_kind: String,
    pub selector_public_id: Option<String>,
    pub selector_key: String,
    pub title: String,
    pub path: String,
    pub node_type: Option<String>,
    pub knowledge_map_public_id: Option<String>,
    pub knowledge_map_version: Option<String>,
    pub textbook_edition_public_id: Option<String>,
    pub textbook_title: Option<String>,
    pub knowledge_node_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileStudent {
    pub id: i64,
    pub class_id: i64,
    pub student_no: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfilePolicy {
    pub public_id: String,
    pub revision: i64,
    pub min_independent_groups: i64,
    pub min_distinct_dates: i64,
    pub min_distinct_sources: i64,
    pub needs_support_below: f64,
    pub stable_at_or_above: f64,
    pub freshness_days: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfilePreviewCounts {
    pub mapped_formal_evidence: i64,
    pub knowledge_node_total: i64,
    pub knowledge_node_assessed: i64,
    pub knowledge_node_eligible: i64,
    pub ability_node_total: i64,
    pub ability_node_assessed: i64,
    pub ability_node_eligible: i64,
    pub machine_only_excluded: i64,
    pub teacher_overall_excluded: i64,
    pub unmapped_formal_excluded: i64,
    pub unsupported_contract_excluded: i64,
    pub out_of_scope_excluded: i64,
    pub referenced_knowledge_map_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudentProfilePreview {
    pub schema_version: i64,
    pub rule_version: String,
    pub calculated_at: String,
    pub student: ProfileStudent,
    pub range_start: String,
    pub range_end: String,
    pub scope_selection: ProfileScopeSelectionView,
    pub policy: ProfilePolicy,
    pub counts: ProfilePreviewCounts,
    pub recitation_summary: ProfileRecitationSummary,
    pub wrongbook_summary: ProfileWrongbookSummary,
    pub source_watermark: String,
    pub can_generate: bool,
    pub blocker: Option<String>,
    pub scope_note: String,
    pub evidence_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileEvidenceView {
    pub public_id: String,
    pub source_module: String,
    pub source_type: String,
    pub source_ref_type: String,
    pub source_ref_id: String,
    pub decision_ref_type: Option<String>,
    pub decision_ref_id: Option<String>,
    pub decision_revision: Option<i64>,
    pub evidence_kind: String,
    pub value: f64,
    pub evidence_quality: f64,
    pub assessment_context: String,
    pub confirmation_level: String,
    pub occurred_at: String,
    pub independence_group_key: String,
    pub effective_weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileRecitationEvidenceView {
    pub public_id: String,
    pub source_type: String,
    pub source_ref_type: String,
    pub source_ref_id: String,
    pub decision_ref_id: Option<String>,
    pub decision_revision: Option<i64>,
    pub evidence_kind: String,
    pub value: f64,
    pub evidence_quality: f64,
    pub assessment_context: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileRecitationSummary {
    pub overall_count: i64,
    pub fluency_count: i64,
    pub retention_count: i64,
    pub latest_overall_value: Option<f64>,
    pub latest_fluency_value: Option<f64>,
    pub latest_retention_value: Option<f64>,
    pub latest_at: Option<String>,
    pub evidence: Vec<ProfileRecitationEvidenceView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileNamedReference {
    pub public_id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileWrongbookFactView {
    pub question_version_public_id: String,
    pub question_type: String,
    pub stem: String,
    pub status: String,
    pub first_error_at: String,
    pub last_error_at: String,
    pub latest_response_at: String,
    pub published_response_count: i64,
    pub error_response_count: i64,
    pub repeated_error: bool,
    pub correction_status: Option<String>,
    pub reinforcement_status: Option<String>,
    pub knowledge_nodes: Vec<ProfileNamedReference>,
    pub ability_dimensions: Vec<ProfileNamedReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileWrongbookSummary {
    pub fact_count: i64,
    pub needs_correction_count: i64,
    pub corrected_once_count: i64,
    pub rechecked_correct_count: i64,
    pub repeated_error_count: i64,
    pub latest_response_at: Option<String>,
    pub note: String,
    pub facts: Vec<ProfileWrongbookFactView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileNodeMetric {
    pub public_id: String,
    pub target_type: String,
    pub target_public_id: String,
    pub target_title: String,
    pub mastery_score: Option<f64>,
    pub status: String,
    pub confidence_level: String,
    pub freshness: String,
    pub evidence_count: i64,
    pub independent_group_count: i64,
    pub distinct_date_count: i64,
    pub distinct_source_count: i64,
    pub last_evidence_at: Option<String>,
    pub source_breakdown: BTreeMap<String, i64>,
    pub explanation: String,
    pub evidence: Vec<ProfileEvidenceView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileNodeTrendChange {
    pub target_type: String,
    pub target_public_id: String,
    pub target_title: String,
    pub previous_status: String,
    pub current_status: String,
    pub previous_mastery_score: Option<f64>,
    pub current_mastery_score: Option<f64>,
    pub mastery_score_delta: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudentProfileTrend {
    pub comparison_status: String,
    pub comparison_kind: Option<String>,
    pub previous_snapshot_public_id: Option<String>,
    pub previous_revision: Option<i64>,
    pub previous_generated_at: Option<String>,
    pub knowledge_assessed_before: Option<i64>,
    pub knowledge_assessed_current: i64,
    pub knowledge_assessed_delta: Option<i64>,
    pub ability_assessed_before: Option<i64>,
    pub ability_assessed_current: i64,
    pub ability_assessed_delta: Option<i64>,
    pub needs_support_before: Option<i64>,
    pub needs_support_current: i64,
    pub needs_support_delta: Option<i64>,
    pub stable_before: Option<i64>,
    pub stable_current: i64,
    pub stable_delta: Option<i64>,
    pub changed_nodes: Vec<ProfileNodeTrendChange>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StudentProfileSnapshot {
    pub public_id: String,
    pub revision: i64,
    pub student: ProfileStudent,
    pub range_start: String,
    pub range_end: String,
    pub scope_kind: String,
    pub scope_selection: ProfileScopeSelectionView,
    pub evidence_cutoff_at: String,
    pub policy: ProfilePolicy,
    pub source_watermark: String,
    pub evidence_count: i64,
    pub knowledge_node_total: i64,
    pub knowledge_node_assessed: i64,
    pub knowledge_node_eligible: i64,
    pub ability_node_total: i64,
    pub ability_node_assessed: i64,
    pub ability_node_eligible: i64,
    pub state: String,
    pub payload_sha256: String,
    pub generated_by: String,
    pub generated_at: String,
    pub confirmed_by: String,
    pub confirmed_at: String,
    pub is_stale: bool,
    pub stale_reason: Option<String>,
    pub recitation_summary: ProfileRecitationSummary,
    pub wrongbook_summary: ProfileWrongbookSummary,
    pub trend: StudentProfileTrend,
    pub teacher_assessments: Vec<crate::teacher_assessments::ProfileTeacherAssessment>,
    pub knowledge_metrics: Vec<ProfileNodeMetric>,
    pub ability_metrics: Vec<ProfileNodeMetric>,
}
