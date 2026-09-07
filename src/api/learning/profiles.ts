import type { WrongbookStatus, CorrectionAssignmentStatus, ReinforcementAssignmentStatus, NamedReference } from "./wrongbook";
import { call } from "../client";

export interface ProfileStudent {
  id: number;
  class_id: number;
  student_no: string;
  name: string;
}

export interface ProfilePolicy {
  public_id: string;
  revision: number;
  min_independent_groups: number;
  min_distinct_dates: number;
  min_distinct_sources: number;
  needs_support_below: number;
  stable_at_or_above: number;
  freshness_days: number;
}

export interface ProfilePreviewCounts {
  mapped_formal_evidence: number;
  knowledge_node_total: number;
  knowledge_node_assessed: number;
  knowledge_node_eligible: number;
  ability_node_total: number;
  ability_node_assessed: number;
  ability_node_eligible: number;
  machine_only_excluded: number;
  teacher_overall_excluded: number;
  unmapped_formal_excluded: number;
  unsupported_contract_excluded: number;
  out_of_scope_excluded: number;
  referenced_knowledge_map_count: number;
}

export interface ProfileRecitationEvidenceView {
  public_id: string;
  source_type: string;
  source_ref_type: string;
  source_ref_id: string;
  decision_ref_id: string | null;
  decision_revision: number | null;
  evidence_kind: string;
  value: number;
  evidence_quality: number;
  assessment_context: string;
  occurred_at: string;
}

export interface ProfileRecitationSummary {
  overall_count: number;
  fluency_count: number;
  retention_count: number;
  latest_overall_value: number | null;
  latest_fluency_value: number | null;
  latest_retention_value: number | null;
  latest_at: string | null;
  evidence: ProfileRecitationEvidenceView[];
}

export interface ProfileWrongbookFactView {
  question_version_public_id: string;
  question_type: string;
  stem: string;
  status: WrongbookStatus;
  first_error_at: string;
  last_error_at: string;
  latest_response_at: string;
  published_response_count: number;
  error_response_count: number;
  repeated_error: boolean;
  correction_status: CorrectionAssignmentStatus | null;
  reinforcement_status: ReinforcementAssignmentStatus | null;
  knowledge_nodes: NamedReference[];
  ability_dimensions: NamedReference[];
}

export interface ProfileWrongbookSummary {
  fact_count: number;
  needs_correction_count: number;
  corrected_once_count: number;
  rechecked_correct_count: number;
  repeated_error_count: number;
  latest_response_at: string | null;
  note: string;
  facts: ProfileWrongbookFactView[];
}

export interface ProfileScopeOption {
  selector_kind: "auto_evidence_maps" | "knowledge_map" | "curriculum_node";
  selector_public_id: string | null;
  selector_key: string;
  label: string;
  detail: string;
  node_type: string | null;
  knowledge_map_public_id: string | null;
  textbook_edition_public_id: string | null;
  knowledge_node_count: number;
}

export interface ProfileScopeSelectionView {
  selector_kind: "auto_evidence_maps" | "knowledge_map" | "curriculum_node";
  selector_public_id: string | null;
  selector_key: string;
  title: string;
  path: string;
  node_type: string | null;
  knowledge_map_public_id: string | null;
  knowledge_map_version: string | null;
  textbook_edition_public_id: string | null;
  textbook_title: string | null;
  knowledge_node_count: number;
}

export interface StudentProfilePreview {
  schema_version: number;
  rule_version: string;
  calculated_at: string;
  student: ProfileStudent;
  range_start: string;
  range_end: string;
  scope_selection: ProfileScopeSelectionView;
  policy: ProfilePolicy;
  counts: ProfilePreviewCounts;
  recitation_summary: ProfileRecitationSummary;
  wrongbook_summary: ProfileWrongbookSummary;
  source_watermark: string;
  can_generate: boolean;
  blocker: string | null;
  scope_note: string;
  evidence_note: string;
}

export interface ProfileEvidenceView {
  public_id: string;
  source_module: string;
  source_type: string;
  source_ref_type: string;
  source_ref_id: string;
  decision_ref_type: string | null;
  decision_ref_id: string | null;
  decision_revision: number | null;
  evidence_kind: string;
  value: number;
  evidence_quality: number;
  assessment_context: string;
  confirmation_level: string;
  occurred_at: string;
  independence_group_key: string;
  effective_weight: number;
}

export type ProfileNodeStatus =
  | "unassessed"
  | "insufficient_evidence"
  | "needs_support"
  | "developing"
  | "stable";

export interface ProfileNodeMetric {
  public_id: string;
  target_type: "knowledge_node" | "ability_dimension";
  target_public_id: string;
  target_title: string;
  mastery_score: number | null;
  status: ProfileNodeStatus;
  confidence_level: "none" | "low" | "medium" | "high";
  freshness: "none" | "fresh" | "aging" | "stale";
  evidence_count: number;
  independent_group_count: number;
  distinct_date_count: number;
  distinct_source_count: number;
  last_evidence_at: string | null;
  source_breakdown: Record<string, number>;
  explanation: string;
  evidence: ProfileEvidenceView[];
}

export interface ProfileNodeTrendChange {
  target_type: "knowledge_node" | "ability_dimension";
  target_public_id: string;
  target_title: string;
  previous_status: ProfileNodeStatus;
  current_status: ProfileNodeStatus;
  previous_mastery_score: number | null;
  current_mastery_score: number | null;
  mastery_score_delta: number | null;
}

export interface StudentProfileTrend {
  comparison_status: "no_comparable_baseline" | "comparable";
  comparison_kind: "same_scope_refresh" | null;
  previous_snapshot_public_id: string | null;
  previous_revision: number | null;
  previous_generated_at: string | null;
  knowledge_assessed_before: number | null;
  knowledge_assessed_current: number;
  knowledge_assessed_delta: number | null;
  ability_assessed_before: number | null;
  ability_assessed_current: number;
  ability_assessed_delta: number | null;
  needs_support_before: number | null;
  needs_support_current: number;
  needs_support_delta: number | null;
  stable_before: number | null;
  stable_current: number;
  stable_delta: number | null;
  changed_nodes: ProfileNodeTrendChange[];
  note: string;
}

export type ProfileTeacherAssessmentValue =
  | "not_taught"
  | "needs_support"
  | "developing"
  | "stable"
  | "observe";

export interface ProfileTeacherAssessment {
  public_id: string;
  snapshot_public_id: string;
  node_metric_public_id: string;
  target_type: "knowledge_node" | "ability_dimension";
  target_public_id: string;
  target_title: string;
  revision: number;
  assessment: ProfileTeacherAssessmentValue | null;
  note: string | null;
  state: "active" | "voided";
  supersedes_public_id: string | null;
  created_by: string;
  created_at: string;
}

export interface StudentProfileSnapshot {
  public_id: string;
  revision: number;
  student: ProfileStudent;
  range_start: string;
  range_end: string;
  scope_kind: string;
  scope_selection: ProfileScopeSelectionView;
  evidence_cutoff_at: string;
  policy: ProfilePolicy;
  source_watermark: string;
  evidence_count: number;
  knowledge_node_total: number;
  knowledge_node_assessed: number;
  knowledge_node_eligible: number;
  ability_node_total: number;
  ability_node_assessed: number;
  ability_node_eligible: number;
  state: string;
  payload_sha256: string;
  generated_by: string;
  generated_at: string;
  confirmed_by: string;
  confirmed_at: string;
  is_stale: boolean;
  stale_reason: string | null;
  recitation_summary: ProfileRecitationSummary;
  wrongbook_summary: ProfileWrongbookSummary;
  trend: StudentProfileTrend;
  teacher_assessments: ProfileTeacherAssessment[];
  knowledge_metrics: ProfileNodeMetric[];
  ability_metrics: ProfileNodeMetric[];
}

export interface StudentProfileScopeInput {
  classId: number;
  studentId: number;
  rangeStart: string;
  rangeEnd: string;
  scopeSelectorKind?: ProfileScopeOption["selector_kind"];
  scopeSelectorPublicId?: string | null;
}

export const loadProfileScopeOptions = () =>
  call<ProfileScopeOption[]>("list_profile_scope_options");

export const previewStudentProfile = (input: StudentProfileScopeInput) =>
  call<StudentProfilePreview>("preview_student_profile", { input });

export const generateStudentProfile = (input: StudentProfileScopeInput) =>
  call<StudentProfileSnapshot>("generate_student_profile", { input });

export const loadLatestStudentProfile = (classId: number, studentId: number) =>
  call<StudentProfileSnapshot | null>("latest_student_profile", { classId, studentId });

export interface SaveProfileTeacherAssessmentInput {
  snapshotPublicId: string;
  nodeMetricPublicId: string;
  expectedRevision: number;
  assessment: ProfileTeacherAssessmentValue | null;
  note?: string | null;
}

export const saveProfileTeacherAssessment = (
  input: SaveProfileTeacherAssessmentInput,
) => call<ProfileTeacherAssessment>("save_profile_teacher_assessment", { input });
