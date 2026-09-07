import { call } from "../client";

export interface ObjectiveWorkbenchRow {
  assessment_id: number;
  assessment_version_id: number;
  assessment_title: string;
  attempt_id: number;
  attempt_state: "ingesting" | "grading" | "ready_to_publish" | "published";
  active_publication_id: number | null;
  student_id: number;
  student_no: string;
  student_name: string;
  assessment_item_id: number;
  order_index: number;
  max_score: number;
  question_version_id: number;
  question_no: string;
  question_type: "single" | "multiple" | "true_false";
  question_stem: string;
  answer_region_revision_id: number;
  crop_path: string | null;
  suggestion_id: number;
  observation_state: "recognized" | "blank" | "altered" | "low_confidence" | "failed";
  observed_answer_json: string | null;
  confidence: number | null;
  suggestion_outcome: "correct" | "incorrect" | "unscored";
  suggested_score: number | null;
  batch_eligible: boolean;
  exclusion_reason: string | null;
  grade_decision_id: number | null;
  grade_decision_revision: number | null;
  teacher_score: number | null;
  confirmation_level: "teacher_accepted" | "teacher_corrected" | null;
  review_mode: "single" | "strict_batch" | null;
  current_suggestion_confirmed: boolean;
  decided_at: string | null;
}

export interface ObjectiveAttemptSummary {
  assessment_id: number;
  assessment_version_id: number;
  assessment_title: string;
  attempt_id: number;
  attempt_state: "ingesting" | "grading" | "ready_to_publish" | "published";
  active_publication_id: number | null;
  student_id: number;
  student_no: string;
  student_name: string;
  item_count: number;
  observed_count: number;
  confirmed_count: number;
  teacher_total_score: number;
  max_total_score: number;
  published_total_score: number | null;
  can_publish: boolean;
}

export interface ObjectiveWorkbench {
  rows: ObjectiveWorkbenchRow[];
  attempts: ObjectiveAttemptSummary[];
}

export interface GradeDecision {
  id: number;
  attempt_id: number;
  assessment_item_id: number;
  revision: number;
  teacher_score: number;
  confirmation_level: "teacher_accepted" | "teacher_corrected";
  state: string;
  decided_at: string;
}

export interface ObjectiveReviewBatch {
  id: number;
  requested_count: number;
  confirmed_count: number;
  excluded_count: number;
  items: Array<{
    suggestion_id: number;
    outcome: "confirmed" | "excluded";
    reason_code: string | null;
    grade_decision_id: number | null;
  }>;
}

export interface GradePublication {
  id: number;
  attempt_id: number;
  revision: number;
  state: "published";
  total_score: number;
  published_at: string;
}

export const examObjectiveWorkbench = (assessment_version_id: number | null = null, limit = 500) =>
  call<ObjectiveWorkbench>("exam_objective_workbench", {
    assessmentVersionId: assessment_version_id,
    limit,
  });

export const examObjectiveAccept = (suggestion_id: number) =>
  call<GradeDecision>("exam_objective_accept", { suggestionId: suggestion_id });

export const examObjectiveCorrect = (
  suggestion_id: number,
  teacher_score: number,
  teacher_note: string | null,
) => call<GradeDecision>("exam_objective_correct", {
  suggestionId: suggestion_id,
  teacherScore: teacher_score,
  teacherNote: teacher_note,
});

export const examObjectiveStrictBatchAccept = (
  suggestion_ids: number[],
  idempotency_key: string,
  confidence_threshold = 0.95,
) => call<ObjectiveReviewBatch>("exam_objective_strict_batch_accept", {
  suggestionIds: suggestion_ids,
  confidenceThreshold: confidence_threshold,
  idempotencyKey: idempotency_key,
});

export const examObjectivePublishAttempt = (attempt_id: number) =>
  call<GradePublication>("exam_objective_publish_attempt", { attemptId: attempt_id });

export const examObjectiveRecognizeRegion = (
  answer_region_revision_id: number,
  idempotency_key: string,
) => call<unknown>("exam_objective_recognize_region", {
  answerRegionRevisionId: answer_region_revision_id,
  idempotencyKey: idempotency_key,
});
