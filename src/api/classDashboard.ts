import { call } from "./client";

export type AppModule = "recitation" | "exam";
export type DashboardTargetView = "today" | "desk" | "exam";

export interface DashboardSourceMeta {
  schema_version: number;
  rule_version: string;
  calculated_at: string;
  as_of_date: string;
  recitation_watermark: string | null;
  exam_watermark: string | null;
}

export interface DashboardClass {
  id: number;
  name: string;
  term: string | null;
  textbook: string | null;
  enabled_student_count: number;
}

export interface RecitationOperations {
  expected_student_count: number;
  completed_student_count: number;
  expected_task_count: number;
  confirmed_task_count: number;
  submitted_task_count: number;
  not_submitted_student_count: number;
  pending_teacher_review_count: number;
  overdue_pending_review_count: number;
  recognition_failure_count: number;
  recognition_processing_count: number;
  denominator_note: string;
}

export interface ExamOperations {
  active_assessment_count: number;
  expected_submission_count: number;
  submitted_submission_count: number;
  missing_submission_count: number;
  ingesting_attempt_count: number;
  grading_attempt_count: number;
  ready_to_publish_attempt_count: number;
  published_submission_count: number;
  open_pipeline_issue_count: number;
  denominator_note: string;
}

export interface StudentOperationsRow {
  student_id: number;
  student_no: string;
  student_name: string;
  recitation_status: string;
  recitation_due_task_count: number;
  recitation_confirmed_task_count: number;
  exam_status: string;
  exam_expected_submission_count: number;
  exam_submitted_submission_count: number;
  exam_published_submission_count: number;
}

export interface DashboardAction {
  kind: string;
  title: string;
  detail: string;
  count: number;
  severity: "blocking" | "review" | "info";
  target_module: AppModule;
  target_view: DashboardTargetView;
}

export interface ClassOperationsDashboard {
  meta: DashboardSourceMeta;
  class: DashboardClass;
  recitation: RecitationOperations;
  exam: ExamOperations;
  students: StudentOperationsRow[];
  actions: DashboardAction[];
}

export interface ClassProfilePolicy {
  public_id: string;
  revision: number;
  min_eligible_students: number;
  min_eligible_ratio: number;
  common_support_ratio_at_or_above: number;
}

export interface ClassProfilePreviewCounts {
  total_student_count: number;
  snapshot_student_count: number;
  eligible_student_count: number;
  missing_snapshot_count: number;
  scope_mismatch_count: number;
  stale_snapshot_count: number;
  knowledge_node_total: number;
  knowledge_node_sample_sufficient: number;
  ability_node_total: number;
  ability_node_sample_sufficient: number;
}

export interface ClassProfilePreview {
  schema_version: number;
  rule_version: string;
  calculated_at: string;
  class: DashboardClass;
  range_start: string;
  range_end: string;
  policy: ClassProfilePolicy;
  counts: ClassProfilePreviewCounts;
  source_watermark: string;
  can_generate: boolean;
  blocker: string | null;
  denominator_note: string;
  scope_note: string;
}

export interface ClassProfileStudent {
  id: number;
  class_id: number;
  student_no: string;
  name: string;
}

export interface ClassProfileStudentInput {
  student: ClassProfileStudent;
  student_snapshot_public_id: string | null;
  inclusion_status: string;
  detail: string;
}

export interface ClassProfileCell {
  student: ClassProfileStudent;
  student_snapshot_public_id: string | null;
  student_metric_public_id: string | null;
  status: string;
  mastery_score: number | null;
  confidence_level: string;
  last_evidence_at: string | null;
}

export interface ClassProfileNodeMetric {
  public_id: string;
  target_type: "knowledge_node" | "ability_dimension";
  target_public_id: string;
  target_title: string;
  average_mastery_score: number | null;
  class_status: string;
  confidence_level: string;
  total_student_count: number;
  snapshot_student_count: number;
  assessed_student_count: number;
  eligible_student_count: number;
  needs_support_count: number;
  developing_count: number;
  stable_count: number;
  insufficient_evidence_count: number;
  unassessed_count: number;
  missing_snapshot_count: number;
  scope_mismatch_count: number;
  stale_snapshot_count: number;
  eligible_ratio: number;
  needs_support_ratio: number | null;
  sample_sufficient: boolean;
  last_evidence_at: string | null;
  source_breakdown: Record<string, number>;
  explanation: string;
  cells: ClassProfileCell[];
}

export interface ClassProfileSnapshot {
  public_id: string;
  revision: number;
  class: DashboardClass;
  range_start: string;
  range_end: string;
  scope_kind: string;
  evidence_cutoff_at: string;
  policy: ClassProfilePolicy;
  source_watermark: string;
  total_student_count: number;
  snapshot_student_count: number;
  eligible_student_count: number;
  knowledge_node_total: number;
  knowledge_node_sample_sufficient: number;
  ability_node_total: number;
  ability_node_sample_sufficient: number;
  state: string;
  payload_sha256: string;
  generated_by: string;
  generated_at: string;
  confirmed_by: string;
  confirmed_at: string;
  is_stale: boolean;
  stale_reason: string | null;
  inputs: ClassProfileStudentInput[];
  knowledge_metrics: ClassProfileNodeMetric[];
  ability_metrics: ClassProfileNodeMetric[];
}

export interface ClassProfileScopeInput {
  classId: number;
  rangeStart: string;
  rangeEnd: string;
}

export const loadClassOperationsDashboard = (classId: number, asOfDate?: string) =>
  call<ClassOperationsDashboard>("class_operations_dashboard", {
    classId,
    asOfDate: asOfDate || null,
  });

export const previewClassProfile = (input: ClassProfileScopeInput) =>
  call<ClassProfilePreview>("preview_class_profile", { input });

export const generateClassProfile = (
  input: ClassProfileScopeInput & { expectedSourceWatermark: string },
) => call<ClassProfileSnapshot>("generate_class_profile", { input });

export const loadLatestClassProfile = (classId: number) =>
  call<ClassProfileSnapshot | null>("latest_class_profile", { classId });
