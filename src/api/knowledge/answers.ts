import type { K1QuestionType } from "./blueprints";
import type { SourceRunFailure } from "./sources";
import { call } from "../client";

export interface AnswerTargetSet {
  questionSourceDocumentPublicId: string;
  sourceType: "blank_paper" | "source_document";
  sourceFormat: "jpeg" | "pdf" | "text" | "docx" | "xlsx";
  createdAt: string;
  targetCount: number;
  completedCount: number;
  pendingCount: number;
}

export interface FillAnswerSlot {
  order_index: number;
  canonical_answers: string[];
  accepted_variants?: string[];
  max_score: number;
}

export interface ShortAnswerRubricPoint {
  order_index: number;
  canonical_text: string;
  max_score: number;
  allowed_paraphrases?: string[];
  required_concepts?: string[];
}

export type K1AnswerPayload =
  | { schema_version: 1; correct_labels: string[] }
  | { schema_version: 1; correct: boolean }
  | { schema_version: 1; slots: FillAnswerSlot[] }
  | {
    schema_version: 1;
    reference_answer: string;
    rubric_points?: ShortAnswerRubricPoint[];
  };

export interface AnswerSourceDocument {
  publicId: string;
  ownerScope: "personal";
  ownerId: string;
  questionSourceDocumentPublicId: string;
  sourceArtifactId: number;
  sourceArtifactPublicId: string;
  sourceFormat: "jpeg" | "pdf" | "text" | "docx" | "xlsx";
  sourceHash: string;
  pageCount: number;
  extractionVersion: string;
  createdBy: string;
  createdAt: string;
}

export interface AnswerMatchDraft {
  publicId: string;
  answerSourceDocumentPublicId: string;
  extractionRunPublicId: string;
  questionVersionPublicId: string;
  orderIndex: number;
  questionNo: string;
  questionType: K1QuestionType;
  stem: string;
  maxScore: number;
  qualityLevel: "L0" | "L1";
  candidateState: "ready" | "needs_review" | "missing";
  answerJson: K1AnswerPayload | null;
  sourceAnchor: Record<string, unknown> | null;
  confidence: number;
  contentHash: string;
  reviewed: boolean;
  resultAnswerKeyVersionPublicId: string | null;
  resultRubricVersionPublicId: string | null;
  resultQuality: "L1" | "L2" | null;
}

export interface AnswerExtractionRecord {
  publicId: string;
  answerSourceDocumentPublicId: string;
  aiRunPublicId: string;
  extractionState: "ready" | "needs_review" | "blocked";
  confidence: number;
  issueCodes: string[];
  targetCount: number;
  matchedCount: number;
  drafts: AnswerMatchDraft[];
  createdAt: string;
}

export interface AnswerSourceInboxItem {
  document: AnswerSourceDocument;
  latestExtraction: AnswerExtractionRecord | null;
  latestAiRunPublicId: string | null;
  latestAiStatus: "pending" | "processing" | "succeeded" | "failed" | "voided" | null;
  latestAiErrorMetaJson: string | null;
  targetCount: number;
  pendingMatches: number;
}

export interface KnowledgeAnswerAnalysisResult {
  document: AnswerSourceDocument;
  aiRunId: number;
  status: "succeeded" | "failed";
  extraction: AnswerExtractionRecord | null;
  failure: SourceRunFailure | null;
}

export interface AnswerMatchReview {
  publicId: string;
  matchDraftPublicId: string;
  resultAnswerKeyVersionPublicId: string;
  resultRubricVersionPublicId: string | null;
  resultQuality: "L1" | "L2";
  reviewedBy: string;
  note: string | null;
  reviewedAt: string;
}

export const loadAnswerTargets = (limit = 50) =>
  call<AnswerTargetSet[]>("k1_answer_targets", { limit });

export const importAndAnalyzeAnswers = (
  path: string,
  questionSourceDocumentPublicId: string,
  requestKey: string,
) => call<KnowledgeAnswerAnalysisResult>("k1_answer_import_analyze", {
  input: { path, questionSourceDocumentPublicId, requestKey },
});

export const loadAnswerInbox = (limit = 50) =>
  call<AnswerSourceInboxItem[]>("k1_answer_inbox", { limit });

export const confirmAnswerMatch = (
  matchDraftPublicId: string,
  expectedContentHash: string,
  correctedAnswerJson: K1AnswerPayload,
  note: string | null,
  requestKey: string,
) => call<AnswerMatchReview>("k1_answer_confirm", {
  input: {
    requestKey,
    matchDraftPublicId,
    expectedContentHash,
    correctedAnswerJson,
    note,
  },
});
