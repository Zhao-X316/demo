import type { K1QuestionType } from "./blueprints";
import { call } from "../client";

export interface LinkSuggestionSource {
  sourceType: "question" | "option" | "answer_slot" | "rubric_point";
  sourcePublicId: string;
  label: string;
  detail: string;
  orderIndex: number;
  requiredForL3: boolean;
  requiredKnowledgeRelation:
    | "direct_assessment"
    | "rubric_basis"
    | null;
}

export interface LinkKnowledgeCandidate {
  publicId: string;
  code: string | null;
  title: string;
  curriculumTitle: string | null;
}

export interface LinkAbilityCandidate {
  publicId: string;
  code: string;
  title: string;
  description: string | null;
}

export interface SuggestedKnowledgeLink {
  knowledgeNodePublicId: string;
  relationType: string;
  confidence: number;
  reason: string;
}

export interface SuggestedAbilityLink {
  abilityDimensionPublicId: string;
  evidenceStrength: number;
  responseMode: string;
  confidence: number;
  reason: string;
}

export interface SourceLinkSuggestion {
  sourceType: LinkSuggestionSource["sourceType"];
  sourcePublicId: string;
  knowledgeLinks: SuggestedKnowledgeLink[];
  abilityLinks: SuggestedAbilityLink[];
}

export interface LinkSuggestionOutput {
  schemaVersion: number;
  state: "ready" | "needs_review" | "blocked";
  confidence: number;
  issueCodes: string[];
  sourceSuggestions: SourceLinkSuggestion[];
}

export interface LinkSuggestionInput {
  schemaVersion: number;
  inputVersion: string;
  questionVersionPublicId: string;
  questionContentHash: string;
  questionType: K1QuestionType;
  stem: string;
  materialText: string | null;
  knowledgeMapPublicId: string;
  knowledgeMapRevision: number;
  textbookTitle: string;
  sources: LinkSuggestionSource[];
  knowledgeCandidates: LinkKnowledgeCandidate[];
  abilityCandidates: LinkAbilityCandidate[];
}

export interface LinkSuggestionDraftView {
  publicId: string;
  aiRunPublicId: string;
  questionVersionPublicId: string;
  knowledgeMapPublicId: string;
  suggestion: LinkSuggestionOutput;
  contentHash: string;
  createdAt: string;
}

export interface LinkReviewInboxItem {
  questionVersionPublicId: string;
  questionContentHash: string;
  questionType: K1QuestionType;
  stem: string;
  materialText: string | null;
  maxScore: number;
  qualityLevel: "L2";
  sources: LinkSuggestionSource[];
  latestSuggestion: LinkSuggestionDraftView | null;
  createdAt: string;
}

export interface LinkReviewMapOption {
  publicId: string;
  title: string;
  revision: number;
  subjectTitle: string;
  knowledgeCount: number;
  abilityCount: number;
}

export interface LinkReviewCatalog {
  maps: LinkReviewMapOption[];
}

export interface ConfirmedSourceLinksInput {
  sourceType: LinkSuggestionSource["sourceType"];
  sourcePublicId: string;
  knowledgeLinks: Array<{
    knowledgeNodePublicId: string;
    relationType: string;
  }>;
  abilityLinks: Array<{
    abilityDimensionPublicId: string;
    evidenceStrength: number;
    responseMode: string;
  }>;
}

export interface ConfirmLinkReviewInput {
  requestKey: string;
  questionVersionPublicId: string;
  expectedQuestionContentHash: string;
  knowledgeMapPublicId: string;
  suggestionDraftPublicId: string | null;
  expectedSuggestionContentHash: string | null;
  sources: ConfirmedSourceLinksInput[];
  reviewedBy: "local_teacher";
  note: string | null;
}

export interface LinkReviewResult {
  publicId: string;
  questionVersionPublicId: string;
  knowledgeMapPublicId: string;
  suggestionDraftPublicId: string | null;
  resultLinkSetPublicId: string;
  resultQuality: "L3";
  knowledgeLinkCount: number;
  abilityLinkCount: number;
  reviewedBy: string;
  note: string | null;
  reviewedAt: string;
  createsAssessment: false;
  createsGrade: false;
  createsLearningEvidence: false;
}

export interface LinkSuggestionAnalysisResult {
  aiRunId: number;
  status: "succeeded" | "failed";
  suggestion: LinkSuggestionDraftView | null;
  output: LinkSuggestionOutput | null;
  failure: {
    schema_version: number;
    code: string;
    safe_message: string;
    retryable: boolean;
  } | null;
}

export const loadLinkReviewCatalog = () =>
  call<LinkReviewCatalog>("k1_link_review_catalog");

export const loadLinkReviewInbox = (limit = 100) =>
  call<LinkReviewInboxItem[]>("k1_link_review_inbox", { limit });

export const loadLinkReviewEditor = (
  questionVersionPublicId: string,
  knowledgeMapPublicId: string,
) => call<LinkSuggestionInput>("k1_link_review_editor", {
  questionVersionPublicId,
  knowledgeMapPublicId,
});

export const suggestKnowledgeLinks = (
  questionVersionPublicId: string,
  knowledgeMapPublicId: string,
  requestKey: string,
) => call<LinkSuggestionAnalysisResult>("k1_link_suggest", {
  input: { questionVersionPublicId, knowledgeMapPublicId, requestKey },
});

export const confirmKnowledgeLinks = (input: ConfirmLinkReviewInput) =>
  call<LinkReviewResult>("k1_link_confirm", { input });
