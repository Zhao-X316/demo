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
