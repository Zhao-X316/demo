import { call } from "../client";

export interface ClassTeachingInputItem {
  node_metric_public_id: string;
  target_type: "knowledge_node" | "ability_dimension";
  target_public_id: string;
  target_title: string;
  confidence_level: "low" | "medium" | "high";
  total_student_count: number;
  eligible_student_count: number;
  needs_support_count: number;
  needs_support_ratio: number;
  explanation: string;
}

export interface ClassTeachingInputPreview {
  schema_version: number;
  rule_version: string;
  calculated_at: string;
  class_id: number;
  class_name: string;
  snapshot_public_id: string;
  snapshot_revision: number;
  snapshot_payload_sha256: string;
  range_start: string;
  range_end: string;
  suggested_title: string;
  suggested_teaching_note: string;
  suggested_estimated_minutes: number;
  items: ClassTeachingInputItem[];
  can_confirm: boolean;
  blockers: string[];
  warnings: string[];
  denominator_note: string;
  boundary_note: string;
}

export interface ClassTeachingInputDraft {
  public_id: string;
  class_id: number;
  class_name: string;
  snapshot_public_id: string;
  snapshot_revision: number;
  title: string;
  teaching_note: string;
  estimated_minutes: number;
  source_snapshot_payload_sha256: string;
  schema_version: number;
  rule_version: string;
  payload_sha256: string;
  state: "teacher_confirmed";
  confirmed_by: string;
  confirmed_at: string;
  items: ClassTeachingInputItem[];
}

export interface ConfirmClassTeachingInputRequest {
  requestKey: string;
  snapshotPublicId: string;
  expectedSnapshotPayloadSha256: string;
  title: string;
  teachingNote: string;
  estimatedMinutes: number;
  selectedNodeMetricPublicIds: string[];
}

export const previewClassTeachingInput = (snapshotPublicId: string) =>
  call<ClassTeachingInputPreview>("preview_class_teaching_input", {
    input: { snapshotPublicId },
  });

export const confirmClassTeachingInput = (input: ConfirmClassTeachingInputRequest) =>
  call<ClassTeachingInputDraft>("confirm_class_teaching_input", { input });

export const listClassTeachingInputs = (classId: number, limit = 20) =>
  call<ClassTeachingInputDraft[]>("list_class_teaching_inputs", { classId, limit });
