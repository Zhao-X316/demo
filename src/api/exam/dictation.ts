import type { ObjectiveAttemptSummary, GradeDecision, GradePublication } from "./objective";
import { call } from "../client";

export interface DictationTemplateRevision {
  id: number;
  assessment_version_id: number;
  revision: number;
  template_version: string;
  page_no: number;
  source_ai_run_id: number | null;
  state: "active" | "superseded" | "voided";
}

export interface DictationTemplateStatus {
  assessmentVersionId: number;
  pageNo: number;
  activeTemplate: DictationTemplateRevision | null;
}

export interface DictationTemplateRunResult {
  ai_run_id: number;
  status: "succeeded" | "failed";
  output: {
    state: "ready" | "needs_review" | "blocked";
    page_no: number;
    canvas_width: number;
    canvas_height: number;
    regions: Array<{
      assessment_item_id: number;
      region_index: number;
      mapping_confidence: number;
    }>;
    confidence: number;
    issue_codes: string[];
  } | null;
  failure: {
    code: string;
    safe_message: string;
    retryable: boolean;
  } | null;
}

export interface DictationTranscriptionResult {
  transcription: {
    id: number;
    answer_region_revision_id: number;
    revision: number;
    result_state: "recognized" | "not_written" | "unreadable" | "recognize_failed" | "ambiguous_final";
    raw_ocr_text: string | null;
    normalized_text: string | null;
    teacher_corrected_text: string | null;
    confidence: number | null;
  };
  observation: {
    id: number;
    result: "exact" | "accepted_variant" | "needs_review" | "not_written" | "unreadable" | "recognize_failed" | "ambiguous_final";
    suggested_score: number | null;
  };
}

export interface DictationPageProcessingResult {
  structure: {
    materialization: {
      id: number;
      page_id: number;
      template_revision_id: number;
      alignment_revision_id: number;
      region_revision_ids: number[];
    };
    regions: Array<{
      id: number;
      assessment_item_id: number;
      region_index: number;
      decision: "teacher_confirmed";
    }>;
  };
  transcriptions: DictationTranscriptionResult[];
}

export interface DictationWorkbenchRow {
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
  source_ai_run_id: number | null;
  result_state: "recognized" | "not_written" | "unreadable" | "recognize_failed" | "ambiguous_final";
  raw_ocr_text: string | null;
  normalized_text: string | null;
  teacher_corrected_text: string | null;
  confidence: number | null;
  point_result: string;
  canonical_text: string;
  accepted_variants: string[];
  suggested_score: number | null;
  requires_teacher_review: boolean;
  grade_decision_id: number | null;
  grade_decision_revision: number | null;
  teacher_score: number | null;
  confirmation_level: "teacher_accepted" | "teacher_corrected" | null;
  review_mode: "single" | "strict_batch" | null;
  current_transcription_confirmed: boolean;
  decided_at: string | null;
}

export type DictationAttemptSummary = ObjectiveAttemptSummary;

export interface DictationWorkbench {
  rows: DictationWorkbenchRow[];
  attempts: DictationAttemptSummary[];
}

export interface DictationReviewBatch {
  id: number;
  requested_count: number;
  confirmed_count: number;
  excluded_count: number;
  items: Array<{
    transcription_revision_id: number;
    outcome: "confirmed" | "excluded";
    reason_code: string | null;
    grade_decision_id: number | null;
  }>;
}

export const examDictationTemplateStatus = (reference_page_id: number) =>
  call<DictationTemplateStatus>("exam_dictation_template_status", {
    referencePageId: reference_page_id,
  });

export const examDictationAnalyzeTemplate = (
  reference_page_id: number,
  blank_path: string,
  idempotency_key: string,
) => call<DictationTemplateRunResult>("exam_dictation_analyze_template", {
  referencePageId: reference_page_id,
  blankPath: blank_path,
  idempotencyKey: idempotency_key,
});

export const examDictationConfirmTemplate = (reference_page_id: number, ai_run_id: number) =>
  call<{ template: DictationTemplateRevision }>("exam_dictation_confirm_template", {
    referencePageId: reference_page_id,
    aiRunId: ai_run_id,
  });

export const examDictationProcessPage = (page_id: number) =>
  call<DictationPageProcessingResult>("exam_dictation_process_page", { pageId: page_id });

export const examDictationRecognizeRegion = (
  answer_region_revision_id: number,
  idempotency_key: string,
) => call<DictationTranscriptionResult>("exam_dictation_recognize_region", {
  answerRegionRevisionId: answer_region_revision_id,
  idempotencyKey: idempotency_key,
});

export const examDictationWorkbench = (assessment_version_id: number | null = null, limit = 1000) =>
  call<DictationWorkbench>("exam_dictation_workbench", {
    assessmentVersionId: assessment_version_id,
    limit,
  });

export const examDictationAccept = (transcription_revision_id: number) =>
  call<GradeDecision>("exam_dictation_accept", {
    transcriptionRevisionId: transcription_revision_id,
  });

export const examDictationCorrectGrade = (
  transcription_revision_id: number,
  teacher_score: number,
  teacher_note: string,
  teacher_evidence_text: string | null,
) => call<GradeDecision>("exam_dictation_correct_grade", {
  transcriptionRevisionId: transcription_revision_id,
  teacherScore: teacher_score,
  teacherNote: teacher_note,
  teacherEvidenceText: teacher_evidence_text,
});

export const examDictationStrictBatchAccept = (
  transcription_revision_ids: number[],
  idempotency_key: string,
) => call<DictationReviewBatch>("exam_dictation_strict_batch_accept", {
  transcriptionRevisionIds: transcription_revision_ids,
  idempotencyKey: idempotency_key,
});

export const examDictationPublishAttempt = (attempt_id: number) =>
  call<GradePublication>("exam_dictation_publish_attempt", { attemptId: attempt_id });

export const examDictationCorrectTranscription = (
  answer_region_revision_id: number,
  corrected_text: string,
) => call<DictationTranscriptionResult>("exam_dictation_correct_transcription", {
  answerRegionRevisionId: answer_region_revision_id,
  correctedText: corrected_text,
});
