import type { SubjectiveTranscriptionRevision } from "./subjective";
import { call } from "../client";

export interface FixedIntakeOption {
  classId: number;
  className: string;
  assessmentId: number;
  assessmentVersionId: number;
  assessmentTitle: string;
  revision: number;
  templateVersion: string | null;
  itemCount: number;
  isDefault: boolean;
  defaultSelectionPublicId: string | null;
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
  qualityReviewCompleted: boolean;
  mappedGroupCount: number;
  rejectedGroupCount: number;
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

export interface GroupedPageEvidence {
  groupIndex: number;
  studentId: number;
  studentNo: string;
  studentName: string;
  pages: Array<{
    pageId: number;
    replacedPageId: number | null;
    pageNo: number;
    importIndex: number;
    archivedPath: string;
    originalName: string | null;
    pageState: string;
    qualityResult: "pass" | "needs_review" | "reject" | null;
    matchDecision: "suggested" | "teacher_confirmed" | "rejected" | "unmatched" | null;
  }>;
}

export interface GroupingQualityConfirmationResult {
  qualityReviewCompleted: true;
  mappedGroupCount: number;
  rejectedGroupCount: number;
  nextAction: string;
}

export interface GroupingRetakeResult {
  replacementPageId: number;
  activatedStudent: boolean;
  mappedGroupCount: number;
  rejectedGroupCount: number;
  nextAction: string;
}

export interface OrdinaryPaperRunResult {
  ai_run_id: number;
  status: "succeeded" | "failed";
  output: {
    schema_version: number;
    page_id: number;
    expected_page_no: number;
    state: "ready" | "needs_review" | "blocked";
    quality: {
      result: "pass" | "needs_review" | "reject";
      issue_codes: string[];
    };
    alignment: { confidence: number } | null;
    regions: Array<{
      assessment_item_id: number;
      region_index: number;
      mapping_confidence: number;
      mark_cells: Array<{ label: string }>;
    }>;
    printed_questions: Array<{
      assessment_item_id: number;
      stem: string;
      material_text: string | null;
      options: Array<{ label: string; content: string; order_index: number }>;
      extraction_confidence: number;
      privacy: {
        schema_version: number;
        sanitized: boolean;
        student_identity_detected: boolean;
        student_answer_detected: boolean;
        teacher_mark_detected: boolean;
        score_detected: boolean;
      };
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

export interface OrdinaryStructureConfirmationResult {
  confirmation: {
    id: number;
    ai_run_id: number;
    page_id: number;
    alignment_revision_id: number;
    region_revision_ids: number[];
    confirmed_by: string;
    created_at: string;
  };
  alignment: {
    id: number;
    decision: "teacher_confirmed";
  };
  regions: Array<{
    id: number;
    assessment_item_id: number;
    region_index: number;
    decision: "teacher_confirmed";
  }>;
}

export interface OrdinaryQuestionSyncSummary {
  schema_version: number;
  state:
    | "completed"
    | "completed_with_failures"
    | "no_printed_questions"
    | "no_safe_print_layer"
    | "source_processing"
    | "source_failed";
  assessment_version_id: number;
  page_no: number;
  source_page_id: number;
  source_ai_run_id: number;
  printed_question_count: number;
  eligible_count: number;
  enqueued_count: number;
  matched_count: number;
  candidate_created_count: number;
  needs_review_count: number;
  privacy_rejected_count: number;
  low_confidence_skipped_count: number;
  failed_count: number;
  reused_existing_source: boolean;
}

export interface AnswerSheetPageProcessingResult {
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
    routes: Array<{
      answer_region_revision_id: number;
      assessment_item_id: number;
      region_index: number;
      recognition_route: "objective_omr" | "handwriting_ocr";
      question_type: "single" | "multiple" | "true_false" | "fill_blank" | "short_answer";
    }>;
  };
  observations: Array<{
    observation: {
      id: number;
      answer_region_revision_id: number;
      result_state: "recognized" | "blank" | "altered" | "low_confidence" | "failed";
      confidence: number | null;
    };
    suggestion: {
      id: number;
      outcome: "correct" | "incorrect" | "unscored";
      batch_eligible: boolean;
      exclusion_reason: string | null;
    };
  }>;
  subjectiveRegions: Array<{
    answerRegionRevisionId: number;
    assessmentItemId: number;
    regionIndex: number;
    cropArtifactId: number;
    state: "awaiting_handwriting_recognition";
    nextAction: string;
  }>;
  subjectiveTranscriptions: SubjectiveTranscriptionRevision[];
  subjectiveFailures: Array<{
    answerRegionRevisionId: number;
    safeMessage: string;
  }>;
}

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

export const examFixedIntakeGroupingEvidence = (batch_id: number) =>
  call<GroupedPageEvidence[]>("exam_fixed_intake_grouping_evidence", { batchId: batch_id });

export const examFixedIntakeConfirmGroupingQuality = (
  batch_id: number,
  rejected_page_ids: number[],
) => call<GroupingQualityConfirmationResult>("exam_fixed_intake_confirm_grouping_quality", {
  batchId: batch_id,
  rejectedPageIds: rejected_page_ids,
});

export const examFixedIntakeReplaceRejectedPage = (
  batch_id: number,
  rejected_page_id: number,
  replacement_path: string,
) => call<GroupingRetakeResult>("exam_fixed_intake_replace_rejected_page", {
  batchId: batch_id,
  rejectedPageId: rejected_page_id,
  replacementPath: replacement_path,
});

export const examOrdinaryPaperAnalyzePage = (
  page_id: number,
  idempotency_key: string,
) => call<OrdinaryPaperRunResult>("exam_ordinary_paper_analyze_page", {
  pageId: page_id,
  idempotencyKey: idempotency_key,
});

export const examOrdinaryPaperConfirmPageStructure = (
  page_id: number,
  ai_run_id: number,
) => call<OrdinaryStructureConfirmationResult>(
  "exam_ordinary_paper_confirm_page_structure",
  { pageId: page_id, aiRunId: ai_run_id },
);

export const examOrdinaryPaperSyncQuestions = (
  page_id: number,
  ai_run_id: number,
) => call<OrdinaryQuestionSyncSummary>(
  "exam_ordinary_paper_sync_questions",
  { pageId: page_id, aiRunId: ai_run_id },
);

export const examAnswerSheetProcessPage = (page_id: number) =>
  call<AnswerSheetPageProcessingResult>("exam_answer_sheet_process_page", {
    pageId: page_id,
  });
