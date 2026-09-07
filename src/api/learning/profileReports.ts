import { call } from "../client";

export interface StudentProfileReportSnapshot {
  public_id: string;
  snapshot_public_id: string;
  student_id: number;
  class_id: number;
  report_kind: "student_learning_summary";
  purpose: "teacher_internal_feedback";
  actor_role: "local_teacher";
  schema_version: number;
  rule_version: string;
  source_snapshot_payload_sha256: string;
  teacher_assessment_watermark: string;
  payload_sha256: string;
  html_sha256: string;
  suggested_file_name: string;
  generated_by: string;
  generated_at: string;
}

export interface WrittenStudentProfileReport {
  snapshot_public_id: string;
  file_name: string;
  byte_size: number;
  sha256: string;
}

export const createStudentProfileReportSnapshot = (
  requestKey: string,
  snapshotPublicId: string,
  expectedSnapshotPayloadSha256: string,
) => call<StudentProfileReportSnapshot>("create_student_profile_report_snapshot", {
  input: { requestKey, snapshotPublicId, expectedSnapshotPayloadSha256 },
});

export const writeStudentProfileReportSnapshot = (
  reportPublicId: string,
  outputPath: string,
) => call<WrittenStudentProfileReport>("write_student_profile_report_snapshot", {
  reportPublicId,
  outputPath,
});
