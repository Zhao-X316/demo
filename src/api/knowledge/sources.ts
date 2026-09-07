import type { K1QuestionType } from "./blueprints";
import { call } from "../client";

export interface SourceDocument {
  publicId: string;
  ownerScope: "personal";
  ownerId: string;
  sourceArtifactId: number;
  sourceArtifactPublicId: string;
  sourceType: "blank_paper" | "source_document";
  sourceFormat: "jpeg" | "pdf" | "text" | "docx" | "xlsx";
  sourceHash: string;
  pageCount: number;
  extractionVersion: string;
  createdBy: string;
  createdAt: string;
}

export interface SourceQuestionOption {
  label: string;
  content: string;
  order_index: number;
}

export interface SourceDraft {
  publicId: string;
  sourceDocumentPublicId: string;
  extractionRunPublicId: string;
  orderIndex: number;
  questionNo: string | null;
  questionType: K1QuestionType;
  stem: string;
  materialText: string | null;
  maxScore: number;
  options: SourceQuestionOption[];
  sourceAnchorJson: string;
  confidence: number;
  contentHash: string;
  reviewAction: "accept" | "discard" | null;
  resultKind: "exact_reused" | "draft_created" | "discarded" | null;
  resultQuestionVersionPublicId: string | null;
}

export interface SourceExtractionRecord {
  publicId: string;
  sourceDocumentPublicId: string;
  aiRunPublicId: string;
  extractionState: "ready" | "needs_review" | "blocked";
  confidence: number;
  issueCodes: string[];
  drafts: SourceDraft[];
  createdAt: string;
}

export interface SourceInboxItem {
  document: SourceDocument;
  latestExtraction: SourceExtractionRecord | null;
  latestAiRunPublicId: string | null;
  latestAiStatus: "pending" | "processing" | "succeeded" | "failed" | "voided" | null;
  latestAiErrorMetaJson: string | null;
  totalDrafts: number;
  pendingDrafts: number;
}

export interface SourceRunFailure {
  schemaVersion: number;
  code: string;
  safeMessage: string;
  retryable: boolean;
}

export interface SourceImportAnalysisResult {
  document: SourceDocument;
  aiRunId: number;
  status: "succeeded" | "failed";
  extraction: SourceExtractionRecord | null;
  failure: SourceRunFailure | null;
}

export interface AcceptSourceDraftInput {
  requestKey: string;
  draftPublicId: string;
  expectedContentHash: string;
  corrected: {
    questionType: K1QuestionType;
    stem: string;
    materialText: string | null;
    maxScore: number;
    options: Array<{
      label: string;
      content: string;
      orderIndex: number;
    }>;
  } | null;
  note: string | null;
}

export interface SourceDraftReview {
  publicId: string;
  sourceDraftPublicId: string;
  action: "accept" | "discard";
  resultKind: "exact_reused" | "draft_created" | "discarded";
  resultQuestionVersionPublicId: string | null;
  reviewedBy: string;
  note: string | null;
  reviewedAt: string;
}

export const importAndAnalyzeSource = (
  path: string,
  sourceType: "blank_paper" | "source_document",
  requestKey: string,
) => call<SourceImportAnalysisResult>("k1_source_import_analyze", {
  input: { path, sourceType, requestKey },
});

export const loadSourceInbox = (limit = 50) =>
  call<SourceInboxItem[]>("k1_source_inbox", { limit });

export const acceptSourceDraft = (input: AcceptSourceDraftInput) =>
  call<SourceDraftReview>("k1_source_accept", { input });

export const discardSourceDraft = (
  draftPublicId: string,
  expectedContentHash: string,
  note: string | null,
  requestKey: string,
) => call<SourceDraftReview>("k1_source_discard", {
  input: { requestKey, draftPublicId, expectedContentHash, note },
});
