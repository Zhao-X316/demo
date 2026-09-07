import type { DashboardClass } from "./operations";
import type { ProfileScopeSelectionView, ProfileScopeOption } from "../learning";
import { call } from "../client";

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
  scope_selection: ProfileScopeSelectionView;
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

export interface ClassProfileStudentStatusCounts {
  total_student_count: number;
  data_unavailable_count: number;
  evidence_insufficient_count: number;
  needs_support_count: number;
  developing_count: number;
  stable_count: number;
}

export interface ClassProfileStudentStatus {
  student: ClassProfileStudent;
  status:
    | "data_unavailable"
    | "evidence_insufficient"
    | "needs_support"
    | "developing"
    | "stable";
  eligible_node_count: number;
  needs_support_node_count: number;
  developing_node_count: number;
  stable_node_count: number;
  reason_node_titles: string[];
  explanation: string;
}

export interface ClassProfileTrend {
  comparison_status: string;
  comparison_kind: string | null;
  previous_snapshot_public_id: string | null;
  previous_revision: number | null;
  previous_generated_at: string | null;
  snapshot_student_count_before: number | null;
  snapshot_student_count_current: number;
  snapshot_student_count_delta: number | null;
  eligible_student_count_before: number | null;
  eligible_student_count_current: number;
  eligible_student_count_delta: number | null;
  knowledge_common_support_before: number | null;
  knowledge_common_support_current: number;
  knowledge_common_support_delta: number | null;
  ability_common_support_before: number | null;
  ability_common_support_current: number;
  ability_common_support_delta: number | null;
  previous_status_counts: ClassProfileStudentStatusCounts | null;
  current_status_counts: ClassProfileStudentStatusCounts;
  note: string;
}

export interface ClassProfileSnapshot {
  public_id: string;
  revision: number;
  class: DashboardClass;
  range_start: string;
  range_end: string;
  scope_kind: string;
  scope_selection: ProfileScopeSelectionView;
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
  student_status_counts: ClassProfileStudentStatusCounts;
  student_statuses: ClassProfileStudentStatus[];
  trend: ClassProfileTrend;
}

export interface ClassProfileExportSnapshot {
  public_id: string;
  snapshot_public_id: string;
  class_id: number;
  report_kind: "deidentified_class_summary";
  purpose: "internal_teaching";
  actor_role: "local_teacher";
  min_group_size: number;
  schema_version: number;
  rule_version: string;
  source_snapshot_payload_sha256: string;
  payload_sha256: string;
  csv_sha256: string;
  suggested_file_name: string;
  generated_by: string;
  generated_at: string;
}

export interface WrittenClassProfileExport {
  snapshot_public_id: string;
  file_name: string;
  byte_size: number;
  sha256: string;
}

export interface ClassProfileScopeInput {
  classId: number;
  rangeStart: string;
  rangeEnd: string;
  scopeSelectorKind?: ProfileScopeOption["selector_kind"];
  scopeSelectorPublicId?: string | null;
}

export const previewClassProfile = (input: ClassProfileScopeInput) =>
  call<ClassProfilePreview>("preview_class_profile", { input });

export const generateClassProfile = (
  input: ClassProfileScopeInput & { expectedSourceWatermark: string },
) => call<ClassProfileSnapshot>("generate_class_profile", { input });

export const loadLatestClassProfile = (classId: number) =>
  call<ClassProfileSnapshot | null>("latest_class_profile", { classId });

export const createClassProfileExportSnapshot = (
  requestKey: string,
  snapshotPublicId: string,
  expectedSnapshotPayloadSha256: string,
) => call<ClassProfileExportSnapshot>("create_class_profile_export_snapshot", {
  input: { requestKey, snapshotPublicId, expectedSnapshotPayloadSha256 },
});

export const writeClassProfileExportSnapshot = (
  exportPublicId: string,
  outputPath: string,
) => call<WrittenClassProfileExport>("write_class_profile_export_snapshot", {
  exportPublicId,
  outputPath,
});
