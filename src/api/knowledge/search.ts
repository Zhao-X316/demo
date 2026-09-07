import type { K1QuestionType } from "./blueprints";
import { call } from "../client";

export interface QuestionSearchInput {
  query: string;
  ownerScope: "all" | "personal" | "official";
  questionType: K1QuestionType | null;
  minimumQuality: "C0" | "L0" | "L1" | "L2" | "L3" | "L4";
  state: "active" | "candidate" | "draft" | "review_pending" | "published" | "all";
  knowledgeMapPublicId: string | null;
  curriculumNodePublicId: string | null;
  knowledgeNodePublicId: string | null;
  duplicateOnly: boolean;
  limit: number;
  offset: number;
}

export interface QuestionSearchOption {
  label: string;
  content: string;
}

export interface QuestionSearchKnowledge {
  public_id: string;
  title: string;
  relation_type: string;
}

export interface QuestionSearchAbility {
  public_id: string;
  title: string;
  evidence_strength: number;
  response_mode: string;
}

export interface DuplicateCandidate {
  question_version_public_id: string;
  stem: string;
  match_kind: "exact" | "similar";
  similarity: number;
  decision: "independent" | "same_family" | null;
  decision_note: string | null;
  decision_revision: number | null;
}

export interface QuestionSearchItem {
  question_public_id: string;
  question_version_public_id: string;
  revision: number;
  owner_scope: "personal" | "official";
  owner_label: string;
  question_type: K1QuestionType;
  stem: string;
  material_text: string | null;
  max_score: number;
  quality_level: "C0" | "L0" | "L1" | "L2" | "L3" | "L4";
  state: string;
  options: QuestionSearchOption[];
  knowledge_nodes: QuestionSearchKnowledge[];
  ability_dimensions: QuestionSearchAbility[];
  assessment_usage_count: number;
  duplicate_candidates: DuplicateCandidate[];
}

export interface QuestionSearchResponse {
  schema_version: number;
  rule_version: string;
  calculated_at: string;
  total: number;
  limit: number;
  offset: number;
  items: QuestionSearchItem[];
  boundary_note: string;
}

export interface ReviewDuplicateInput {
  requestKey: string;
  leftQuestionVersionPublicId: string;
  rightQuestionVersionPublicId: string;
  decision: "independent" | "same_family";
  note: string | null;
}

export interface DuplicateReviewDecision {
  public_id: string;
  left_question_version_public_id: string;
  right_question_version_public_id: string;
  revision: number;
  match_kind: "exact" | "similar";
  similarity: number;
  decision: "independent" | "same_family";
  note: string | null;
  decided_by: string;
  decided_at: string;
  state: "active" | "superseded";
  identity_changed: false;
}

export const searchQuestions = (input: QuestionSearchInput) =>
  call<QuestionSearchResponse>("k1_question_search", { input });

export interface SemanticSearchCandidate {
  questionVersionPublicId: string;
  revision: number;
  ownerScope: "personal" | "official";
  ownerLabel: string;
  questionType: K1QuestionType;
  stem: string;
  materialText: string | null;
  maxScore: number;
  qualityLevel: "C0" | "L0" | "L1" | "L2" | "L3" | "L4";
  options: QuestionSearchOption[];
  knowledgeTitles: string[];
  abilityTitles: string[];
}

export interface SemanticQuestionSearchItem {
  candidate: SemanticSearchCandidate;
  score: number;
  reason: string;
}

export interface SemanticRunFailure {
  schema_version: number;
  code: string;
  safe_message: string;
  retryable: boolean;
}

export interface SemanticQuestionSearchResponse {
  aiRunId: number;
  status: "succeeded" | "failed";
  state: "ready" | "needs_review" | "blocked" | "failed";
  confidence: number | null;
  issueCodes: string[];
  items: SemanticQuestionSearchItem[];
  failure: SemanticRunFailure | null;
  catalogSnapshotHash: string;
  boundaryNote: string;
}

export interface SemanticQuestionSearchInput {
  requestKey: string;
  search: QuestionSearchInput;
}

export const semanticSearchQuestions = (input: SemanticQuestionSearchInput) =>
  call<SemanticQuestionSearchResponse>("k1_question_semantic_search", { input });

export const reviewDuplicate = (input: ReviewDuplicateInput) =>
  call<DuplicateReviewDecision>("k1_duplicate_review", { input });
