import type { K1QuestionType, BlueprintCandidate } from "./blueprints";
import { call } from "../client";

export interface BlueprintPaperEditorItem {
  source_slot_order_index: number;
  order_index: number;
  question_version_public_id: string;
  question_type: K1QuestionType;
  stem: string;
  material_text: string | null;
  score: number;
  page_break_before: boolean;
}

export interface BlueprintPaperEditor {
  schema_version: number;
  rule_version: string;
  assembly_public_id: string;
  title: string;
  class_name: string;
  knowledge_map_title: string;
  curriculum_node_title: string | null;
  current_edition_public_id: string | null;
  current_revision: number;
  source_assessment_version_public_id: string;
  required_knowledge_node_public_ids: string[];
  items: BlueprintPaperEditorItem[];
  candidates: BlueprintCandidate[];
  boundary_note: string;
}

export interface BlueprintPaperItemInput {
  sourceSlotOrderIndex: number;
  questionVersionPublicId: string;
  pageBreakBefore: boolean;
}

export interface ConfirmBlueprintPaperInput {
  requestKey: string;
  assemblyPublicId: string;
  expectedSourceAssessmentVersionPublicId: string;
  title: string;
  items: BlueprintPaperItemInput[];
}

export interface BlueprintPaperEditionItem {
  source_slot_order_index: number;
  order_index: number;
  question_version_public_id: string;
  question_type: K1QuestionType;
  stem: string;
  material_text: string | null;
  score: number;
  page_break_before: boolean;
}

export interface BlueprintPaperEdition {
  public_id: string;
  assembly_public_id: string;
  revision: number;
  title: string;
  assessment_public_id: string;
  source_assessment_version_public_id: string;
  assessment_version_public_id: string;
  supersedes_edition_public_id: string | null;
  item_set_hash: string;
  question_html_sha256: string;
  answer_html_sha256: string;
  suggested_question_file_name: string;
  suggested_answer_file_name: string;
  state: "confirmed";
  confirmed_by: string;
  confirmed_at: string;
  items: BlueprintPaperEditionItem[];
}

export interface WrittenBlueprintPaper {
  edition_public_id: string;
  export_kind: "question" | "answer";
  file_name: string;
  byte_size: number;
  sha256: string;
}

export const loadBlueprintPaperEditor = (assemblyPublicId: string) =>
  call<BlueprintPaperEditor>("k1_blueprint_paper_editor", { assemblyPublicId });

export const confirmBlueprintPaper = (input: ConfirmBlueprintPaperInput) =>
  call<BlueprintPaperEdition>("k1_blueprint_paper_confirm", { input });

export const writeBlueprintPaper = (
  editionPublicId: string,
  exportKind: "question" | "answer",
  outputPath: string,
) => call<WrittenBlueprintPaper>("k1_blueprint_paper_write", {
  editionPublicId,
  exportKind,
  outputPath,
});
