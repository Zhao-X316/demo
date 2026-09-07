import type { K1QuestionType } from "./blueprints";
import { call } from "../client";

export interface CandidateReviewOption {
  label: string;
  content: string;
  order_index: number;
}

export interface CandidateReviewItem {
  candidate_public_id: string;
  source_type: "blank_paper" | "source_document" | "student_paper" | "workbook";
  privacy_status: "reusable_asset" | "text_only";
  question_type: K1QuestionType;
  stem: string;
  material_text: string | null;
  max_score: number;
  options: CandidateReviewOption[];
  content_hash: string;
  quality_issues: string[];
  source_question_version_public_id: string;
  source_quality_level: "C0";
  created_at: string;
  review_action: "promote_l1" | "discard" | null;
  result_question_version_public_id: string | null;
  reviewed_at: string | null;
}

export interface CandidateReviewInbox {
  schema_version: number;
  rule_version: string;
  calculated_at: string;
  pending_count: number;
  items: CandidateReviewItem[];
  boundary_note: string;
}

export interface PromoteCandidateInput {
  requestKey: string;
  candidatePublicId: string;
  expectedContentHash: string;
  questionType: K1QuestionType;
  stem: string;
  materialText: string | null;
  maxScore: number;
  options: Array<{
    label: string;
    content: string;
    orderIndex: number;
  }>;
  answerText: string;
  note: string | null;
  reviewedBy: "local_teacher";
}

export interface DiscardCandidateInput {
  requestKey: string;
  candidatePublicId: string;
  expectedContentHash: string;
  note: string | null;
  reviewedBy: "local_teacher";
}

export interface CandidateReviewDecision {
  public_id: string;
  candidate_public_id: string;
  action: "promote_l1" | "discard";
  source_question_version_public_id: string;
  result_question_version_public_id: string | null;
  result_answer_key_version_public_id: string | null;
  result_quality_level: "L1" | null;
  reviewed_by: string;
  reviewed_at: string;
  current_assessment_rebound: false;
  boundary_note: string;
}

export const loadCandidateReviewInbox = (includeReviewed = false, limit = 100) =>
  call<CandidateReviewInbox>("k1_candidate_review_list", { includeReviewed, limit });

export const promoteCandidateToL1 = (input: PromoteCandidateInput) =>
  call<CandidateReviewDecision>("k1_candidate_promote_l1", { input });

export const discardCandidate = (input: DiscardCandidateInput) =>
  call<CandidateReviewDecision>("k1_candidate_discard", { input });
