import { call } from "./client";

export interface KnowledgePoint {
  id: number;
  subject_id: number | null;
  parent_id: number | null;
  code: string | null;
  name: string;
}

export interface Question {
  id: number;
  subject_id: number | null;
  question_no: string | null;
  qtype: "single" | "multi" | "judge" | "fill" | "subjective";
  stem: string;
  image_path: string | null;
  correct_answer: string | null;
  knowledge_point_id: number | null;
  difficulty: number | null;
  analysis: string | null;
  max_score: number;
  enabled: boolean;
}

export interface QuestionOptionInput {
  label: string;
  content: string;
  is_correct: boolean;
  knowledge_point_id: number | null;
  analysis: string | null;
  ord: number;
}

export interface QuestionInput {
  subject_id: number | null;
  question_no: string | null;
  qtype: Question["qtype"];
  stem: string;
  image_path: string | null;
  correct_answer: string | null;
  knowledge_point_id: number | null;
  difficulty: number | null;
  analysis: string | null;
  max_score: number;
  enabled: boolean;
  options: QuestionOptionInput[];
}

export interface AnswerDetail {
  id: number;
  student_id: number;
  student_name: string;
  question_id: number;
  question_no: string | null;
  question_stem: string;
  question_type: string;
  picked: string | null;
  correct_answer: string | null;
  machine_correct: boolean | null;
  human_correct: boolean | null;
  is_correct: boolean | null;
  score: number | null;
  max_score: number;
  knowledge_point_id: number | null;
  knowledge_point_name: string | null;
  status: "pending_review" | "confirmed";
  machine_note: string | null;
  human_note: string | null;
  created_at: string;
  decided_at: string | null;
}

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

export interface FixedIntakeOption {
  classId: number;
  className: string;
  assessmentId: number;
  assessmentVersionId: number;
  assessmentTitle: string;
  revision: number;
  templateVersion: string | null;
  itemCount: number;
}

export interface FixedIntakeRequest {
  assessmentVersionId: number;
  studentPaths: string[];
  answerPath: string | null;
  answerText: string | null;
  expectedPagesPerAttempt: number;
  materialType?: "auto" | "ordinary_paper" | "answer_sheet" | "dictation";
  idempotencyKey: string;
}

export interface FixedIntakeDocumentSummary {
  role: "student_work" | "answer_source";
  format: "jpeg" | "pdf" | "text";
  originalName: string;
  pageCount: number;
}

export interface GroupingRosterStudent {
  studentId: number;
  studentNo: string;
  studentName: string;
}

export interface FixedIntakeResult {
  batchId: number;
  batchPublicId: string;
  documents: FixedIntakeDocumentSummary[];
  studentDocumentCount: number;
  studentPageCount: number;
  answerDocumentCount: number;
  route: "ready_for_batch_confirm" | "review_required" | "blocked";
  targetCount: number;
  readyCount: number;
  reviewCount: number;
  blockedCount: number;
  completedCount: number;
  reasonCodes: string[];
  orderPolicy: string;
  orderConfidence: number;
  orderConflictCodes: string[];
  materialType: "ordinary_paper" | "answer_sheet" | "dictation" | "unknown";
  materialTypeDecision: "suggested" | "teacher_confirmed" | "rejected";
  materialTypeConfidence: number;
  materialTypeNeedsConfirmation: boolean;
  groupingRoute: "preview_ready" | "review_required" | "blocked";
  studentGroupCount: number;
  groupingIssueCodes: string[];
  expectedPagesPerAttempt: number;
  pageCycleSource: string;
  pageCycleConfidence: number;
  pageCycleNeedsTeacherInput: boolean;
  groupingRoster: GroupingRosterStudent[];
  groupingConfirmed: boolean;
  groupingFirstStudentNo: string | null;
  groupingLastStudentNo: string | null;
  nextAction: string;
}

export interface PageCycleSuggestion {
  expectedPagesPerAttempt: number;
  confidence: number;
  source: string;
  issueCodes: string[];
  needsTeacherInput: boolean;
}

export interface MaterialTypeConfirmationResult {
  materialType: "ordinary_paper" | "answer_sheet" | "dictation";
  materialTypeDecision: "teacher_confirmed";
  materialTypeConfidence: number;
  groupingRoute: "preview_ready" | "review_required" | "blocked";
  studentGroupCount: number;
  groupingIssueCodes: string[];
  nextAction: string;
}

export interface GroupingConfirmationResult {
  groupingRoute: "preview_ready";
  studentGroupCount: number;
  groupingIssueCodes: string[];
  groupingConfirmed: true;
  groupingFirstStudentNo: string;
  groupingLastStudentNo: string;
  nextAction: string;
}

export const kpList = () => call<KnowledgePoint[]>("kp_list");
export const kpCreate = (name: string, code: string | null, parent_id: number | null) =>
  call<KnowledgePoint>("kp_create", { subjectId: null, parentId: parent_id, code, name });

export const questionsList = () => call<Question[]>("questions_list");
export const questionCreate = (q: QuestionInput) => call<number>("question_create", { q });

export const examAnswersList = (limit = 50) =>
  call<AnswerDetail[]>("exam_answers_list", { limit });
export const examAnswerSuggest = (student_id: number, question_id: number, picked: string) =>
  call<AnswerDetail>("exam_answer_suggest", { studentId: student_id, questionId: question_id, picked });
export const examAnswerHumanDecide = (
  answer_id: number,
  is_correct: boolean,
  note: string | null,
) => call<AnswerDetail>("exam_answer_human_decide", { answerId: answer_id, isCorrect: is_correct, note });

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

export const examFixedIntakeOptions = () =>
  call<FixedIntakeOption[]>("exam_fixed_intake_options");

export const examFixedIntakeInferPageCycle = (student_paths: string[]) =>
  call<PageCycleSuggestion>("exam_fixed_intake_infer_page_cycle", {
    studentPaths: student_paths,
  });

export const examFixedIntakePrepare = (request: FixedIntakeRequest) =>
  call<FixedIntakeResult>("exam_fixed_intake_prepare", { request });

export const examFixedIntakeConfirmMaterialType = (
  batch_id: number,
  material_type: "ordinary_paper" | "answer_sheet" | "dictation",
) => call<MaterialTypeConfirmationResult>("exam_fixed_intake_confirm_material_type", {
  batchId: batch_id,
  materialType: material_type,
});

export const examFixedIntakeConfirmGrouping = (
  batch_id: number,
  first_student_no: string,
  absent_student_nos: string[],
) => call<GroupingConfirmationResult>("exam_fixed_intake_confirm_grouping", {
  batchId: batch_id,
  firstStudentNo: first_student_no,
  absentStudentNos: absent_student_nos,
});
