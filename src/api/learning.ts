import { call } from "./client";

export interface WrongbookMeta {
  schema_version: number;
  rule_version: string;
  calculated_at: string;
  exam_watermark: string | null;
}

export interface WrongbookClass {
  id: number;
  name: string;
  term: string | null;
  textbook: string | null;
  enabled_student_count: number;
}

export interface NamedReference {
  public_id: string;
  title: string;
}

export interface ErrorCauseOption {
  code: string;
  label: string;
  description: string;
}

export interface ErrorCauseReview {
  public_id: string;
  revision: number;
  grade_decision_public_id: string;
  publication_public_id: string;
  cause_codes: string[];
  teacher_note: string | null;
  confirmed_by: string;
  confirmed_at: string;
}

export type CorrectionAssignmentStatus =
  | "waiting_upload"
  | "in_progress"
  | "ready_to_publish"
  | "published";

export interface CorrectionAssignment {
  public_id: string;
  class_id: number;
  student_id: number;
  student_no: string;
  student_name: string;
  question_version_public_id: string;
  source_grade_decision_public_id: string;
  source_publication_public_id: string;
  assessment_public_id: string;
  assessment_version_public_id: string;
  assessment_title: string;
  status: CorrectionAssignmentStatus;
  latest_attempt_public_id: string | null;
  created_by: string;
  created_at: string;
}

export interface ScheduleHoliday {
  calendar_date: string;
  label: string;
}

export interface SchedulePolicy {
  id: number;
  public_id: string;
  policy_key: string;
  revision: number;
  timezone: string;
  default_delay_days: number;
  daily_limit_per_student: number;
  weekend_policy: "allow" | "next_workday";
  holiday_policy: "allow" | "next_workday";
  max_shift_days: number;
  state: string;
  created_by: string;
  created_at: string;
  holidays: ScheduleHoliday[];
}

export interface UpdateSchedulePolicyInput {
  defaultDelayDays: number;
  dailyLimitPerStudent: number;
  weekendPolicy: "allow" | "next_workday";
  holidayPolicy: "allow" | "next_workday";
  maxShiftDays: number;
  holidays: ScheduleHoliday[];
}

export interface ReinforcementSuggestion {
  class_id: number;
  student_id: number;
  student_no: string;
  student_name: string;
  question_version_public_id: string;
  source_grade_decision_public_id: string;
  source_publication_public_id: string;
  strategy: "same_question_recheck";
  priority: "normal" | "high";
  reason: string;
  policy_public_id: string;
  policy_revision: number;
  previewed_as_of_date: string;
  corrected_on: string;
  earliest_due_date: string;
  suggested_due_date: string;
  shifted_days: number;
  existing_task_count: number;
  daily_limit_per_student: number;
}

export type ReinforcementAssignmentStatus =
  | "scheduled"
  | "in_progress"
  | "ready_to_publish"
  | "published";

export interface ReinforcementAssignment {
  public_id: string;
  class_id: number;
  student_id: number;
  student_no: string;
  student_name: string;
  question_version_public_id: string;
  source_grade_decision_public_id: string;
  source_publication_public_id: string;
  strategy: "same_question_recheck";
  priority: "normal" | "high";
  policy_public_id: string;
  policy_revision: number;
  due_date: string;
  assessment_public_id: string;
  assessment_version_public_id: string;
  assessment_title: string;
  task_id: number;
  status: ReinforcementAssignmentStatus;
  latest_attempt_public_id: string | null;
  created_by: string;
  created_at: string;
}

export type WrongbookStatus = "needs_correction" | "corrected_once" | "rechecked_correct";

export interface WrongbookQuestion {
  student_id: number;
  student_no: string;
  student_name: string;
  question_version_id: string;
  question_type: string;
  stem: string;
  status: WrongbookStatus;
  first_error_at: string;
  last_error_at: string;
  latest_response_at: string;
  latest_score: number;
  latest_max_score: number;
  latest_score_ratio: number;
  published_response_count: number;
  error_response_count: number;
  repeated_error: boolean;
  latest_assessment_title: string;
  latest_assessment_context: string;
  latest_error_grade_decision_public_id: string;
  latest_error_publication_public_id: string;
  cause_options: ErrorCauseOption[];
  cause_review: ErrorCauseReview | null;
  correction_assignment: CorrectionAssignment | null;
  reinforcement_assignment: ReinforcementAssignment | null;
  knowledge_nodes: NamedReference[];
  ability_dimensions: NamedReference[];
}

export interface WrongbookSummary {
  affected_student_count: number;
  wrong_question_count: number;
  needs_correction_count: number;
  corrected_once_count: number;
  rechecked_correct_count: number;
  repeated_error_count: number;
  denominator_note: string;
}

export interface ClassWrongbookDashboard {
  meta: WrongbookMeta;
  class: WrongbookClass;
  summary: WrongbookSummary;
  items: WrongbookQuestion[];
}

export const loadClassWrongbookDashboard = (classId: number) =>
  call<ClassWrongbookDashboard>("class_wrongbook_dashboard", { classId });

export interface ConfirmWrongbookErrorCausesInput {
  classId: number;
  studentId: number;
  questionVersionPublicId: string;
  gradeDecisionPublicId: string;
  publicationPublicId: string;
  causeCodes: string[];
  teacherNote?: string | null;
}

export const confirmWrongbookErrorCauses = (input: ConfirmWrongbookErrorCausesInput) =>
  call<ErrorCauseReview>("confirm_wrongbook_error_causes", { input });

export interface CreateWrongbookCorrectionInput {
  classId: number;
  studentId: number;
  questionVersionPublicId: string;
  sourceGradeDecisionPublicId: string;
  sourcePublicationPublicId: string;
}

export const createWrongbookSingleCorrection = (input: CreateWrongbookCorrectionInput) =>
  call<CorrectionAssignment>("create_wrongbook_single_correction", { input });

export const loadWrongbookSchedulePolicy = () =>
  call<SchedulePolicy>("wrongbook_schedule_policy");

export const updateWrongbookSchedulePolicy = (input: UpdateSchedulePolicyInput) =>
  call<SchedulePolicy>("update_wrongbook_schedule_policy", { input });

export interface WrongbookReinforcementScopeInput {
  classId: number;
  studentId: number;
  questionVersionPublicId: string;
  sourceGradeDecisionPublicId: string;
  sourcePublicationPublicId: string;
}

export const previewWrongbookReinforcement = (input: WrongbookReinforcementScopeInput) =>
  call<ReinforcementSuggestion>("preview_wrongbook_reinforcement", { input });

export interface ConfirmWrongbookReinforcementInput
  extends WrongbookReinforcementScopeInput {
  expectedPolicyPublicId: string;
  expectedDueDate: string;
  previewedAsOfDate: string;
}

export const confirmWrongbookReinforcement = (
  input: ConfirmWrongbookReinforcementInput,
) => call<ReinforcementAssignment>("confirm_wrongbook_reinforcement", { input });

export interface StudentReference {
  id: number;
  student_no: string;
  name: string;
}

export interface WrongbookStatisticsMeta {
  schema_version: number;
  rule_version: string;
  calculated_at: string;
  range_start: string;
  range_end: string;
  exam_watermark: string | null;
  activity_filter_rule: string;
  evidence_count_rule: string;
}

export interface WrongbookStatisticsSummary {
  student_count: number;
  fact_count: number;
  evidence_count: number;
  needs_correction_count: number;
  corrected_once_count: number;
  rechecked_correct_count: number;
  repeated_error_count: number;
  confirmed_cause_review_count: number;
  confirmed_cause_item_count: number;
  unlinked_fact_count: number;
}

export interface StudentWrongbookFacts {
  student_id: number;
  student_no: string;
  student_name: string;
  fact_count: number;
  evidence_count: number;
  needs_correction_count: number;
  corrected_once_count: number;
  rechecked_correct_count: number;
  repeated_error_count: number;
  confirmed_cause_review_count: number;
  latest_response_at: string | null;
  latest_verification_at: string | null;
}

export interface CauseCount {
  cause_code: string;
  cause_label: string;
  count: number;
}

export interface ConfirmedCauseDistribution {
  public_id: string;
  title: string;
  confirmed_review_count: number;
  causes: CauseCount[];
}

export interface WrongbookStatistics {
  meta: WrongbookStatisticsMeta;
  class: WrongbookClass;
  selected_student: StudentReference | null;
  summary: WrongbookStatisticsSummary;
  students: StudentWrongbookFacts[];
  question_causes: ConfirmedCauseDistribution[];
  knowledge_causes: ConfirmedCauseDistribution[];
}

export interface WrongbookStatisticsInput {
  classId: number;
  studentId?: number | null;
  rangeStart: string;
  rangeEnd: string;
}

export const loadWrongbookStatistics = (input: WrongbookStatisticsInput) =>
  call<WrongbookStatistics>("wrongbook_statistics", { input });

export type WrongbookReportKind = "class_summary" | "student_parent";

export interface WrongbookReportSnapshot {
  public_id: string;
  report_kind: WrongbookReportKind;
  class_id: number;
  student_id: number | null;
  range_start: string;
  range_end: string;
  schema_version: number;
  rule_version: string;
  source_exam_watermark: string | null;
  evidence_count: number;
  fact_count: number;
  confirmed_cause_review_count: number;
  payload_sha256: string;
  csv_sha256: string;
  suggested_file_name: string;
  generated_by: string;
  generated_at: string;
}

export interface WrittenWrongbookReport {
  snapshot_public_id: string;
  file_name: string;
  byte_size: number;
  sha256: string;
}

export interface CreateWrongbookReportInput extends WrongbookStatisticsInput {
  reportKind: WrongbookReportKind;
}

export const createWrongbookReportSnapshot = (input: CreateWrongbookReportInput) =>
  call<WrongbookReportSnapshot>("create_wrongbook_report_snapshot", { input });

export const writeWrongbookReportSnapshot = (
  snapshotPublicId: string,
  outputPath: string,
) => call<WrittenWrongbookReport>("write_wrongbook_report_snapshot", {
  snapshotPublicId,
  outputPath,
});

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

export interface StudentProfilePreview {
  schema_version: number;
  rule_version: string;
  calculated_at: string;
  student: ProfileStudent;
  range_start: string;
  range_end: string;
  policy: ProfilePolicy;
  counts: ProfilePreviewCounts;
  recitation_summary: ProfileRecitationSummary;
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

export interface StudentProfileSnapshot {
  public_id: string;
  revision: number;
  student: ProfileStudent;
  range_start: string;
  range_end: string;
  scope_kind: string;
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
  knowledge_metrics: ProfileNodeMetric[];
  ability_metrics: ProfileNodeMetric[];
}

export interface StudentProfileScopeInput {
  classId: number;
  studentId: number;
  rangeStart: string;
  rangeEnd: string;
}

export const previewStudentProfile = (input: StudentProfileScopeInput) =>
  call<StudentProfilePreview>("preview_student_profile", { input });

export const generateStudentProfile = (input: StudentProfileScopeInput) =>
  call<StudentProfileSnapshot>("generate_student_profile", { input });

export const loadLatestStudentProfile = (classId: number, studentId: number) =>
  call<StudentProfileSnapshot | null>("latest_student_profile", { classId, studentId });
