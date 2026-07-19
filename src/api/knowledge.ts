import { call } from "./client";

export type K1QuestionType =
  | "single"
  | "multiple"
  | "true_false"
  | "fill_blank"
  | "short_answer";

export interface BlueprintClassOption {
  id: number;
  name: string;
  term: string | null;
}

export interface BlueprintMapOption {
  public_id: string;
  title: string;
  revision: number;
}

export interface BlueprintCurriculumOption {
  public_id: string;
  knowledge_map_public_id: string;
  parent_public_id: string | null;
  node_type: "unit" | "lesson" | "topic";
  title: string;
  order_index: number;
}

export interface BlueprintKnowledgeOption {
  public_id: string;
  knowledge_map_public_id: string;
  curriculum_node_public_id: string | null;
  title: string;
  order_index: number;
}

export interface BlueprintOptions {
  classes: BlueprintClassOption[];
  knowledge_maps: BlueprintMapOption[];
  curriculum_nodes: BlueprintCurriculumOption[];
  knowledge_nodes: BlueprintKnowledgeOption[];
  question_types: K1QuestionType[];
}

export interface BlueprintQuestionTypeTarget {
  question_type: K1QuestionType;
  count: number;
}

export interface BlueprintPreviewInput {
  classId: number;
  knowledgeMapPublicId: string;
  curriculumNodePublicId: string | null;
  totalScore: number;
  questionTypeTargets: Array<{
    questionType: K1QuestionType;
    count: number;
  }>;
  requiredKnowledgeNodePublicIds: string[];
}

export interface BlueprintCandidateKnowledge {
  public_id: string;
  title: string;
  relation_type: "direct_assessment" | "rubric_basis";
}

export interface BlueprintCandidateAbility {
  public_id: string;
  title: string;
  evidence_strength: number;
  response_mode: string;
}

export interface BlueprintCandidate {
  question_version_public_id: string;
  question_type: K1QuestionType;
  stem: string;
  material_text: string | null;
  score: number;
  quality_level: "L3" | "L4";
  knowledge_nodes: BlueprintCandidateKnowledge[];
  ability_dimensions: BlueprintCandidateAbility[];
  explanation: string;
}

export interface BlueprintSelectionSummary {
  selected_count: number;
  selected_score: number;
  selected_type_counts: BlueprintQuestionTypeTarget[];
  covered_required_knowledge_node_public_ids: string[];
}

export interface BlueprintPreview {
  schema_version: number;
  rule_version: string;
  calculated_at: string;
  class_id: number;
  class_name: string;
  knowledge_map_public_id: string;
  knowledge_map_title: string;
  curriculum_node_public_id: string | null;
  curriculum_node_title: string | null;
  total_score: number;
  question_type_targets: BlueprintQuestionTypeTarget[];
  required_knowledge_node_public_ids: string[];
  preview_hash: string;
  candidates: BlueprintCandidate[];
  recommended_question_version_public_ids: string[];
  recommended_summary: BlueprintSelectionSummary;
  can_confirm: boolean;
  blockers: string[];
  warnings: string[];
  boundary_note: string;
}

export interface BlueprintAssemblyItem {
  order_index: number;
  question_version_public_id: string;
  question_type: K1QuestionType;
  stem: string;
  score: number;
  explanation: string;
}

export interface BlueprintAssembly {
  public_id: string;
  class_id: number;
  class_name: string;
  knowledge_map_public_id: string;
  knowledge_map_title: string;
  curriculum_node_public_id: string | null;
  curriculum_node_title: string | null;
  title: string;
  total_score: number;
  question_type_targets: BlueprintQuestionTypeTarget[];
  required_knowledge_node_public_ids: string[];
  preview_hash: string;
  selected_set_hash: string;
  assessment_public_id: string;
  assessment_version_public_id: string;
  state: "confirmed";
  confirmed_by: string;
  confirmed_at: string;
  items: BlueprintAssemblyItem[];
}

export interface ConfirmBlueprintInput {
  requestKey: string;
  title: string;
  preview: BlueprintPreviewInput;
  expectedPreviewHash: string;
  selectedQuestionVersionPublicIds: string[];
}

export const loadBlueprintOptions = () =>
  call<BlueprintOptions>("k1_blueprint_options");

export const previewBlueprint = (input: BlueprintPreviewInput) =>
  call<BlueprintPreview>("k1_blueprint_preview", { input });

export const confirmBlueprint = (input: ConfirmBlueprintInput) =>
  call<BlueprintAssembly>("k1_blueprint_confirm", { input });

export const listBlueprintAssemblies = (classId: number, limit = 20) =>
  call<BlueprintAssembly[]>("k1_blueprint_list", { classId, limit });

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

export const reviewDuplicate = (input: ReviewDuplicateInput) =>
  call<DuplicateReviewDecision>("k1_duplicate_review", { input });
