import { call } from "../client";

export interface AnswerSheetTemplateRevision {
  id: number;
  public_id: string;
  assessment_version_id: number;
  revision: number;
  template_version: string;
  page_no: number;
  blank_artifact_id: number;
  source_ai_run_id: number | null;
  confirmed_by: string;
  state: "active" | "superseded" | "voided";
  created_at: string;
}

export interface AnswerSheetTemplateStatus {
  assessmentVersionId: number;
  pageNo: number;
  activeTemplate: AnswerSheetTemplateRevision | null;
  templateSet: {
    assessment_version_id: number;
    template_version: string;
    ready: boolean;
    template_set_hash: string | null;
    pages: Array<{
      page_no: number;
      expected_item_count: number;
      objective_item_count: number;
      subjective_item_count: number;
      active_template_revision_id: number | null;
      ready: boolean;
      issue_codes: string[];
    }>;
    issue_codes: string[];
  };
}

export interface AnswerSheetTemplateRunResult {
  ai_run_id: number;
  status: "succeeded" | "failed";
  output: {
    state: "ready" | "needs_review" | "blocked";
    page_no: number;
    canvas_width: number;
    canvas_height: number;
    alignment_mode: "printed_anchors" | "page_contour";
    anchors: Array<{ key: string }>;
    items: Array<{
      assessment_item_id: number;
      region_index: number;
      cells: Array<{ label: string }>;
    }>;
    subjective_regions: Array<{
      assessment_item_id: number;
      region_index: number;
      question_type: "fill_blank" | "short_answer";
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

export const examAnswerSheetTemplateStatus = (reference_page_id: number) =>
  call<AnswerSheetTemplateStatus>("exam_answer_sheet_template_status", {
    referencePageId: reference_page_id,
  });

export const examAnswerSheetAnalyzeTemplate = (
  reference_page_id: number,
  blank_path: string,
  idempotency_key: string,
) => call<AnswerSheetTemplateRunResult>("exam_answer_sheet_analyze_template", {
  referencePageId: reference_page_id,
  blankPath: blank_path,
  idempotencyKey: idempotency_key,
});

export const examAnswerSheetConfirmTemplate = (reference_page_id: number, ai_run_id: number) =>
  call<AnswerSheetTemplateRevision>("exam_answer_sheet_confirm_template", {
    referencePageId: reference_page_id,
    aiRunId: ai_run_id,
  });
