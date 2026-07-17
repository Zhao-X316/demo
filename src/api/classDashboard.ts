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

export const loadClassOperationsDashboard = (classId: number, asOfDate?: string) =>
  call<ClassOperationsDashboard>("class_operations_dashboard", {
    classId,
    asOfDate: asOfDate || null,
  });
