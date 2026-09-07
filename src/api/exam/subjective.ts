import type { ObjectiveAttemptSummary, GradeDecision, GradePublication } from "./objective";
import { call } from "../client";

export interface AcceptedAnswerPromotionResult {
  outcome: "created_new_version" | "already_promoted" | "already_available";
  promotion_id: number | null;
  accepted_text: string;
  adopted_assessment_version_id: number;
  adopted_assessment_revision: number;
  adopted_answer_key_version_id: number;
  current_grade_unchanged: boolean;
  current_publication_unchanged: boolean;
}

export interface RubricEvidencePromotionResult {
  outcome: "created_new_version" | "already_promoted" | "already_available";
  promotion_id: number | null;
  rubric_point_stable_id: string;
  evidence_text: string;
  adopted_assessment_version_id: number;
  adopted_assessment_revision: number;
  adopted_rubric_version_id: number;
  adopted_link_set_id: number;
  carried_knowledge_link_count: number;
  carried_ability_link_count: number;
  current_grade_unchanged: boolean;
  current_publication_unchanged: boolean;
}

export interface SubjectiveKnowledgeOption {
  id: number;
  public_id: string;
  code: string | null;
  title: string;
}

export interface SubjectiveAbilityOption {
  id: number;
  public_id: string;
  code: string;
  title: string;
}

export interface SubjectiveKnowledgeLinkView {
  knowledge_node_id: number;
  knowledge_node_public_id: string;
  knowledge_title: string;
  relation_type: string;
}

export interface SubjectiveAbilityLinkView {
  ability_dimension_id: number;
  ability_dimension_public_id: string;
  ability_title: string;
  evidence_strength: number;
  response_mode: string;
}

export interface SubjectiveLinkSourceView {
  source_type: "answer_slot" | "rubric_point";
  source_public_id: string;
  stable_id: string;
  order_index: number;
  label: string;
  max_score: number;
  knowledge_links: SubjectiveKnowledgeLinkView[];
  ability_links: SubjectiveAbilityLinkView[];
}

export interface SubjectiveLinkEditor {
  source_assessment_item_id: number;
  assessment_id: number;
  base_assessment_version_id: number;
  base_assessment_revision: number;
  base_assessment_item_id: number;
  question_version_id: number;
  question_type: "fill_blank" | "short_answer";
  question_no: string;
  question_stem: string;
  link_set_id: number;
  link_set_revision: number;
  sources: SubjectiveLinkSourceView[];
  knowledge_options: SubjectiveKnowledgeOption[];
  ability_options: SubjectiveAbilityOption[];
}

export interface SubjectiveKnowledgeLinkInput {
  knowledge_node_id: number;
  relation_type: string;
}

export interface SubjectiveAbilityLinkInput {
  ability_dimension_id: number;
  evidence_strength: number;
  response_mode: string;
}

export interface SubjectiveSourceLinkInput {
  source_type: "answer_slot" | "rubric_point";
  source_public_id: string;
  knowledge_links: SubjectiveKnowledgeLinkInput[];
  ability_links: SubjectiveAbilityLinkInput[];
}

export interface SubjectiveLinkEditResult {
  outcome: "created_new_version" | "already_saved" | "already_current";
  edit_id: number | null;
  adopted_assessment_version_id: number;
  adopted_assessment_revision: number;
  adopted_link_set_id: number;
  adopted_link_set_revision: number;
  knowledge_link_count: number;
  ability_link_count: number;
  current_attempts_unchanged: boolean;
  current_publications_unchanged: boolean;
}

export interface SubjectiveTranscriptionRevision {
  id: number;
  public_id: string;
  attempt_id: number;
  assessment_item_id: number;
  answer_region_revision_id: number;
  question_type: "fill_blank" | "short_answer";
  revision: number;
  source_ai_run_id: number | null;
  result_state: "recognized" | "not_written" | "unreadable" | "recognize_failed" | "ambiguous_final";
  raw_ocr_text: string | null;
  normalized_text: string | null;
  teacher_corrected_text: string | null;
  confidence: number | null;
  failure_meta_json: string | null;
  corrected_by: string | null;
  corrected_at: string | null;
  state: "active" | "superseded" | "voided";
  created_at: string;
}

export interface SubjectiveWorkbenchRow {
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
  question_no: string;
  question_type: "fill_blank" | "short_answer";
  question_stem: string;
  max_score: number;
  answer_region_revision_id: number;
  crop_path: string | null;
  transcription_revision_id: number;
  transcription_revision: number;
  result_state: "recognized" | "not_written" | "unreadable" | "recognize_failed" | "ambiguous_final";
  raw_ocr_text: string | null;
  normalized_text: string | null;
  teacher_corrected_text: string | null;
  confidence: number | null;
  answer_json: string;
  answer_slots_json: string;
  rubric_points_json: string;
  suggestion_id: number;
  short_answer_analysis_id: number | null;
  machine_grade_ai_run_id: number | null;
  suggestion_outcome: "correct" | "incorrect" | "partial" | "unscored";
  suggested_score: number | null;
  suggestion_result_json: string;
  batch_eligible: boolean;
  exclusion_reason: string | null;
  grade_decision_id: number | null;
  grade_decision_revision: number | null;
  teacher_score: number | null;
  confirmation_level: "teacher_accepted" | "teacher_corrected" | null;
  review_mode: "single" | "teacher_corrected" | null;
  current_suggestion_confirmed: boolean;
  decided_at: string | null;
  teacher_components_json: string;
  accepted_answer_promotion_id: number | null;
  accepted_answer_promoted_at: string | null;
  rubric_evidence_promotions_json: string;
}

export interface SubjectiveComponentGradeInput {
  source_type: "answer_slot" | "rubric_point";
  source_public_id: string;
  teacher_score: number;
  evidence_text: string | null;
  teacher_note: string | null;
}

export interface ShortAnswerGradeAnalysis {
  id: number;
  public_id: string;
  transcription_revision_id: number;
  suggestion_id: number;
  attempt_id: number;
  assessment_item_id: number;
  answer_key_version_id: number;
  rubric_version_id: number;
  machine_grade_ai_run_id: number;
  outcome: "correct" | "incorrect" | "partial";
  suggested_score: number;
  result_json: string;
  confidence: number;
  exclusion_reason: string;
  state: "active" | "superseded" | "voided";
  created_at: string;
}

export interface SubjectiveWorkbench {
  rows: SubjectiveWorkbenchRow[];
  attempts: ObjectiveAttemptSummary[];
}

export const examAnswerSheetRecognizeSubjectiveRegion = (
  answer_region_revision_id: number,
  idempotency_key: string,
) => call<SubjectiveTranscriptionRevision>("exam_answer_sheet_recognize_subjective_region", {
  answerRegionRevisionId: answer_region_revision_id,
  idempotencyKey: idempotency_key,
});

export const examAnswerSheetCorrectSubjectiveTranscription = (
  answer_region_revision_id: number,
  corrected_text: string,
) => call<SubjectiveTranscriptionRevision>(
  "exam_answer_sheet_correct_subjective_transcription",
  {
    answerRegionRevisionId: answer_region_revision_id,
    correctedText: corrected_text,
  },
);

export const examAnswerSheetGradeShortAnswer = (
  transcription_revision_id: number,
  idempotency_key: string,
) => call<ShortAnswerGradeAnalysis>("exam_answer_sheet_grade_short_answer", {
  transcriptionRevisionId: transcription_revision_id,
  idempotencyKey: idempotency_key,
});

export const examAnswerSheetSubjectiveWorkbench = (
  assessment_version_id: number | null = null,
  limit = 1000,
) => call<SubjectiveWorkbench>("exam_answer_sheet_subjective_workbench", {
  assessmentVersionId: assessment_version_id,
  limit,
});

export const examAnswerSheetSubjectiveAccept = (suggestion_id: number) =>
  call<GradeDecision>("exam_answer_sheet_subjective_accept", {
    suggestionId: suggestion_id,
  });

export const examAnswerSheetSubjectiveCorrect = (
  suggestion_id: number,
  teacher_score: number,
  teacher_note: string,
) => call<GradeDecision>("exam_answer_sheet_subjective_correct", {
  suggestionId: suggestion_id,
  teacherScore: teacher_score,
  teacherNote: teacher_note,
});

export const examAnswerSheetSubjectiveCorrectComponents = (
  suggestion_id: number,
  components: SubjectiveComponentGradeInput[],
  teacher_note: string,
) => call<GradeDecision>("exam_answer_sheet_subjective_correct_components", {
  suggestionId: suggestion_id,
  components,
  teacherNote: teacher_note,
});

export const examAnswerSheetPromoteAcceptedAnswer = (grade_decision_id: number) =>
  call<AcceptedAnswerPromotionResult>("exam_answer_sheet_promote_accepted_answer", {
    gradeDecisionId: grade_decision_id,
  });

export const examAnswerSheetPromoteRubricEvidence = (
  grade_decision_id: number,
  source_public_id: string,
) => call<RubricEvidencePromotionResult>("exam_answer_sheet_promote_rubric_evidence", {
  gradeDecisionId: grade_decision_id,
  sourcePublicId: source_public_id,
});

export const examSubjectiveLinkEditor = (assessment_item_id: number) =>
  call<SubjectiveLinkEditor>("exam_subjective_link_editor", {
    assessmentItemId: assessment_item_id,
  });

export const examSubjectiveLinkSave = (
  assessment_item_id: number,
  sources: SubjectiveSourceLinkInput[],
) => call<SubjectiveLinkEditResult>("exam_subjective_link_save", {
  assessmentItemId: assessment_item_id,
  sources,
});

export const examAnswerSheetSubjectivePublishAttempt = (attempt_id: number) =>
  call<GradePublication>("exam_answer_sheet_subjective_publish_attempt", {
    attemptId: attempt_id,
  });
