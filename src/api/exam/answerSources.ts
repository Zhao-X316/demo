import { call } from "../client";

export interface AnswerSourceReviewItem {
  assessmentItemId: number;
  orderIndex: number;
  questionNo: string;
  questionType: string;
  questionStem: string;
  boundAnswerKeyVersionId: number;
  boundAnswerJson: string;
  boundRubricVersionId: number;
  boundLinkSetId: number;
  boundRubricPoints: Array<{
    stableId: string;
    orderIndex: number;
    canonicalText: string;
    maxScore: number;
    confirmedKnowledgeTitles: string[];
    confirmedAbilityTitles: string[];
  }>;
  candidateId: number | null;
  candidateAnswerJson: string | null;
  sourceAnchorJson: string | null;
  matchState: "matched" | "conflict" | "missing";
}

export interface AnswerSourceReviewSummary {
  ingestBatchId: number;
  sourceAiRunId: number;
  sourceState: "ready" | "needs_review" | "blocked";
  route: "ready_to_confirm" | "blocked" | "confirmed" | "kept_bound" | "adopted_new_version";
  matchedCount: number;
  conflictCount: number;
  missingCount: number;
  resolution: "confirmed_matches" | "kept_bound" | null;
  adoption: {
    sourceAssessmentVersionId: number;
    adoptedAssessmentVersionId: number;
    adoptedAssessmentVersionPublicId: string;
    adoptedAssessmentRevision: number;
    changedItemCount: number;
    changedRubricCount: number;
    carriedKnowledgeLinkCount: number;
    carriedAbilityLinkCount: number;
    newRubricPointCount: number;
    retiredRubricPointCount: number;
    unlinkedNewRubricPointCount: number;
    droppedKnowledgeLinkCount: number;
    droppedAbilityLinkCount: number;
    currentBatchUnchanged: boolean;
  } | null;
  items: AnswerSourceReviewItem[];
}

export interface RubricPointMappingInput {
  assessmentItemId: number;
  candidateOrderIndex: number;
  action: "reuse_existing" | "new_point";
  previousStableId: string | null;
}

export interface AnswerSourceAnalysisResult {
  run: {
    ai_run_id: number;
    status: "succeeded" | "failed";
    output: {
      state: "ready" | "needs_review" | "blocked";
      confidence: number;
      issue_codes: string[];
    } | null;
    failure: {
      safe_message: string;
      retryable: boolean;
    } | null;
  };
  review: AnswerSourceReviewSummary | null;
}

export const examAnswerSourceAnalyze = (batch_id: number, idempotency_key: string) =>
  call<AnswerSourceAnalysisResult>("exam_answer_source_analyze", {
    batchId: batch_id,
    idempotencyKey: idempotency_key,
  });

export const examAnswerSourceConfirmMatches = (batch_id: number, source_ai_run_id: number) =>
  call<AnswerSourceReviewSummary>("exam_answer_source_confirm_matches", {
    batchId: batch_id,
    sourceAiRunId: source_ai_run_id,
  });

export const examAnswerSourceKeepBound = (batch_id: number, source_ai_run_id: number) =>
  call<AnswerSourceReviewSummary>("exam_answer_source_keep_bound", {
    batchId: batch_id,
    sourceAiRunId: source_ai_run_id,
  });

export const examAnswerSourceAdoptNewVersion = (
  batch_id: number,
  source_ai_run_id: number,
  rubric_mappings: RubricPointMappingInput[] = [],
) =>
  call<AnswerSourceReviewSummary>("exam_answer_source_adopt_new_version", {
    batchId: batch_id,
    sourceAiRunId: source_ai_run_id,
    rubricMappings: rubric_mappings,
  });
