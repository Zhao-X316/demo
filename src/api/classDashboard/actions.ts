import type { ClassProfileStudent } from "./profiles";
import { call } from "../client";

export type ClassActionKind = "reteach" | "practice" | "recitation" | "temporary_group";

export interface ClassActionTarget {
  student: ClassProfileStudent;
  source_status: "needs_support" | "developing" | "stable";
  recommended: boolean;
}

export interface ClassActionCandidate {
  question_version_public_id: string;
  answer_key_version_public_id: string;
  rubric_version_public_id: string;
  link_set_public_id: string;
  title: string;
  question_type: string;
  max_score: number;
  active_assignment_count: number;
  reason: string;
}

export interface ClassActionPreview {
  schema_version: number;
  rule_version: string;
  calculated_at: string;
  class_id: number;
  class_name: string;
  snapshot_public_id: string;
  snapshot_revision: number;
  snapshot_payload_sha256: string;
  snapshot_source_watermark: string;
  node_metric_public_id: string;
  target_type: "knowledge_node" | "ability_dimension";
  target_public_id: string;
  target_title: string;
  action_kind: ClassActionKind;
  suggested_title: string;
  suggested_rationale: string;
  suggested_estimated_minutes: number;
  destination_module: "class_dashboard" | "exam" | "recitation";
  destination_view: string;
  targets: ClassActionTarget[];
  candidates: ClassActionCandidate[];
  can_confirm: boolean;
  blockers: string[];
  warnings: string[];
  boundary_note: string;
}

export interface ClassActionMaterialization {
  public_id: string;
  destination_type: "exam_assessment";
  destination_public_id: string;
  destination_version_public_id: string;
  created_by: string;
  created_at: string;
}

export interface ClassActionDraft {
  public_id: string;
  class_id: number;
  class_name: string;
  snapshot_public_id: string;
  snapshot_revision: number;
  node_metric_public_id: string;
  target_type: "knowledge_node" | "ability_dimension";
  target_public_id: string;
  target_title: string;
  action_kind: ClassActionKind;
  title: string;
  rationale: string;
  estimated_minutes: number;
  destination_module: "class_dashboard" | "exam" | "recitation";
  destination_view: string;
  snapshot_payload_sha256: string;
  payload_sha256: string;
  state: "teacher_confirmed";
  confirmed_by: string;
  confirmed_at: string;
  targets: ClassActionTarget[];
  candidates: ClassActionCandidate[];
  materialization: ClassActionMaterialization | null;
}

export interface PreviewClassActionInput {
  snapshotPublicId: string;
  nodeMetricPublicId: string;
  actionKind: ClassActionKind;
}

export interface ConfirmClassActionInput extends PreviewClassActionInput {
  requestKey: string;
  expectedSnapshotPayloadSha256: string;
  title: string;
  rationale: string;
  estimatedMinutes: number;
  targetStudentIds: number[];
  candidateQuestionVersionPublicIds: string[];
}

export const previewClassAction = (input: PreviewClassActionInput) =>
  call<ClassActionPreview>("preview_class_action", { input });

export const confirmClassAction = (input: ConfirmClassActionInput) =>
  call<ClassActionDraft>("confirm_class_action", { input });

export const listClassActionDrafts = (classId: number, limit = 20) =>
  call<ClassActionDraft[]>("list_class_action_drafts", { classId, limit });

export const materializeClassAction = (requestKey: string, draftPublicId: string) =>
  call<ClassActionDraft>("materialize_class_action", {
    input: { requestKey, draftPublicId },
  });
