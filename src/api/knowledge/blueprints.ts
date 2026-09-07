import { call } from "../client";

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
