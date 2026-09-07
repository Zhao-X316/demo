import type { WrongbookClass } from "./wrongbook";
import { call } from "../client";

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
