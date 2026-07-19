import { call } from "./client";

export interface KnowledgePoint {
  id: number;
  subject_id: number | null;
  parent_id: number | null;
  code: string | null;
  name: string;
}

export interface Question {
  id: number;
  subject_id: number | null;
  question_no: string | null;
  qtype: "single" | "multi" | "judge" | "fill" | "subjective";
  stem: string;
  image_path: string | null;
  correct_answer: string | null;
  knowledge_point_id: number | null;
  difficulty: number | null;
  analysis: string | null;
  max_score: number;
  enabled: boolean;
}

export interface QuestionOptionInput {
  label: string;
  content: string;
  is_correct: boolean;
  knowledge_point_id: number | null;
  analysis: string | null;
  ord: number;
}

export interface QuestionInput {
  subject_id: number | null;
  question_no: string | null;
  qtype: Question["qtype"];
  stem: string;
  image_path: string | null;
  correct_answer: string | null;
  knowledge_point_id: number | null;
  difficulty: number | null;
  analysis: string | null;
  max_score: number;
  enabled: boolean;
  options: QuestionOptionInput[];
}

export interface AnswerDetail {
  id: number;
  student_id: number;
  student_name: string;
  question_id: number;
  question_no: string | null;
  question_stem: string;
  question_type: string;
  picked: string | null;
  correct_answer: string | null;
  machine_correct: boolean | null;
  human_correct: boolean | null;
  is_correct: boolean | null;
  score: number | null;
  max_score: number;
  knowledge_point_id: number | null;
  knowledge_point_name: string | null;
  status: "pending_review" | "confirmed";
  machine_note: string | null;
  human_note: string | null;
  created_at: string;
  decided_at: string | null;
}

export interface ObjectiveWorkbenchRow {
  assessment_id: number;
  assessment_version_id: number;
  assessment_title: string;
  attempt_id: number;
  attempt_state: "ingesting" | "grading" | "ready_to_publish" | "published";
  active_publication_id: number | null;
  student_id: number;
  student_no: string;
  student_name: string;
  assessment_item_id: number;
  order_index: number;
  max_score: number;
  question_version_id: number;
  question_no: string;
  question_type: "single" | "multiple" | "true_false";
  question_stem: string;
  answer_region_revision_id: number;
  crop_path: string | null;
  suggestion_id: number;
  observation_state: "recognized" | "blank" | "altered" | "low_confidence" | "failed";
  observed_answer_json: string | null;
  confidence: number | null;
  suggestion_outcome: "correct" | "incorrect" | "unscored";
  suggested_score: number | null;
  batch_eligible: boolean;
  exclusion_reason: string | null;
  grade_decision_id: number | null;
  grade_decision_revision: number | null;
  teacher_score: number | null;
  confirmation_level: "teacher_accepted" | "teacher_corrected" | null;
  review_mode: "single" | "strict_batch" | null;
  current_suggestion_confirmed: boolean;
  decided_at: string | null;
}

export interface AcceptedAnswerPromotionResult {
  outcome: "created_new_version" | "already_promoted" | "already_available";
  promotion_id: number | null;
  accepted_text: string;
  adopted_assessment_version_id: number;
  adopted_assessment_revision: number;
  adopted_answer_key_version_id: number;
  current_grade_unchanged: boolean;
  current_publication_unchanged: boolean;
}

export interface SubjectiveKnowledgeOption {
  id: number;
  public_id: string;
  code: string | null;
  title: string;
}

export interface SubjectiveAbilityOption {
  id: number;
  public_id: string;
  code: string;
  title: string;
}

export interface SubjectiveKnowledgeLinkView {
  knowledge_node_id: number;
  knowledge_node_public_id: string;
  knowledge_title: string;
  relation_type: string;
}

export interface SubjectiveAbilityLinkView {
  ability_dimension_id: number;
  ability_dimension_public_id: string;
  ability_title: string;
  evidence_strength: number;
  response_mode: string;
}

export interface SubjectiveLinkSourceView {
  source_type: "answer_slot" | "rubric_point";
  source_public_id: string;
  stable_id: string;
  order_index: number;
  label: string;
  max_score: number;
  knowledge_links: SubjectiveKnowledgeLinkView[];
  ability_links: SubjectiveAbilityLinkView[];
}

export interface SubjectiveLinkEditor {
  source_assessment_item_id: number;
  assessment_id: number;
  base_assessment_version_id: number;
  base_assessment_revision: number;
  base_assessment_item_id: number;
  question_version_id: number;
  question_type: "fill_blank" | "short_answer";
  question_no: string;
  question_stem: string;
  link_set_id: number;
  link_set_revision: number;
  sources: SubjectiveLinkSourceView[];
  knowledge_options: SubjectiveKnowledgeOption[];
  ability_options: SubjectiveAbilityOption[];
}

export interface SubjectiveKnowledgeLinkInput {
  knowledge_node_id: number;
  relation_type: string;
}

export interface SubjectiveAbilityLinkInput {
  ability_dimension_id: number;
  evidence_strength: number;
  response_mode: string;
}

export interface SubjectiveSourceLinkInput {
  source_type: "answer_slot" | "rubric_point";
  source_public_id: string;
  knowledge_links: SubjectiveKnowledgeLinkInput[];
  ability_links: SubjectiveAbilityLinkInput[];
}

export interface SubjectiveLinkEditResult {
  outcome: "created_new_version" | "already_saved" | "already_current";
  edit_id: number | null;
  adopted_assessment_version_id: number;
  adopted_assessment_revision: number;
  adopted_link_set_id: number;
  adopted_link_set_revision: number;
  knowledge_link_count: number;
  ability_link_count: number;
  current_attempts_unchanged: boolean;
  current_publications_unchanged: boolean;
}

export interface ObjectiveAttemptSummary {
  assessment_id: number;
  assessment_version_id: number;
  assessment_title: string;
  attempt_id: number;
  attempt_state: "ingesting" | "grading" | "ready_to_publish" | "published";
  active_publication_id: number | null;
  student_id: number;
  student_no: string;
  student_name: string;
  item_count: number;
  observed_count: number;
  confirmed_count: number;
  teacher_total_score: number;
  max_total_score: number;
  published_total_score: number | null;
  can_publish: boolean;
}

export interface ObjectiveWorkbench {
  rows: ObjectiveWorkbenchRow[];
  attempts: ObjectiveAttemptSummary[];
}

export interface GradeDecision {
  id: number;
  attempt_id: number;
  assessment_item_id: number;
  revision: number;
  teacher_score: number;
  confirmation_level: "teacher_accepted" | "teacher_corrected";
  state: string;
  decided_at: string;
}

export interface ObjectiveReviewBatch {
  id: number;
  requested_count: number;
  confirmed_count: number;
  excluded_count: number;
  items: Array<{
    suggestion_id: number;
    outcome: "confirmed" | "excluded";
    reason_code: string | null;
    grade_decision_id: number | null;
  }>;
}

export interface GradePublication {
  id: number;
  attempt_id: number;
  revision: number;
  state: "published";
  total_score: number;
  published_at: string;
}

export interface FixedIntakeOption {
  classId: number;
  className: string;
  assessmentId: number;
  assessmentVersionId: number;
  assessmentTitle: string;
  revision: number;
  templateVersion: string | null;
  itemCount: number;
}

export interface FixedIntakeRequest {
  assessmentVersionId: number;
  studentPaths: string[];
  answerPath: string | null;
  answerText: string | null;
  expectedPagesPerAttempt: number;
  materialType?: "auto" | "ordinary_paper" | "answer_sheet" | "dictation";
  idempotencyKey: string;
}

export interface FixedIntakeDocumentSummary {
  role: "student_work" | "answer_source";
  format: "jpeg" | "pdf" | "text";
  originalName: string;
  pageCount: number;
}

export interface GroupingRosterStudent {
  studentId: number;
  studentNo: string;
  studentName: string;
}

export interface FixedIntakeResult {
  batchId: number;
  batchPublicId: string;
  documents: FixedIntakeDocumentSummary[];
  studentDocumentCount: number;
  studentPageCount: number;
  answerDocumentCount: number;
  route: "ready_for_batch_confirm" | "review_required" | "blocked";
  targetCount: number;
  readyCount: number;
  reviewCount: number;
  blockedCount: number;
  completedCount: number;
  reasonCodes: string[];
  orderPolicy: string;
  orderConfidence: number;
  orderConflictCodes: string[];
  materialType: "ordinary_paper" | "answer_sheet" | "dictation" | "unknown";
  materialTypeDecision: "suggested" | "teacher_confirmed" | "rejected";
  materialTypeConfidence: number;
  materialTypeNeedsConfirmation: boolean;
  groupingRoute: "preview_ready" | "review_required" | "blocked";
  studentGroupCount: number;
  groupingIssueCodes: string[];
  expectedPagesPerAttempt: number;
  pageCycleSource: string;
  pageCycleConfidence: number;
  pageCycleNeedsTeacherInput: boolean;
  groupingRoster: GroupingRosterStudent[];
  groupingConfirmed: boolean;
  groupingFirstStudentNo: string | null;
  groupingLastStudentNo: string | null;
  qualityReviewCompleted: boolean;
  mappedGroupCount: number;
  rejectedGroupCount: number;
  nextAction: string;
}

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

export interface PageCycleSuggestion {
  expectedPagesPerAttempt: number;
  confidence: number;
  source: string;
  issueCodes: string[];
  needsTeacherInput: boolean;
}

export interface MaterialTypeConfirmationResult {
  materialType: "ordinary_paper" | "answer_sheet" | "dictation";
  materialTypeDecision: "teacher_confirmed";
  materialTypeConfidence: number;
  groupingRoute: "preview_ready" | "review_required" | "blocked";
  studentGroupCount: number;
  groupingIssueCodes: string[];
  nextAction: string;
}

export interface GroupingConfirmationResult {
  groupingRoute: "preview_ready";
  studentGroupCount: number;
  groupingIssueCodes: string[];
  groupingConfirmed: true;
  groupingFirstStudentNo: string;
  groupingLastStudentNo: string;
  nextAction: string;
}

export interface GroupedPageEvidence {
  groupIndex: number;
  studentId: number;
  studentNo: string;
  studentName: string;
  pages: Array<{
    pageId: number;
    replacedPageId: number | null;
    pageNo: number;
    importIndex: number;
    archivedPath: string;
    originalName: string | null;
    pageState: string;
    qualityResult: "pass" | "needs_review" | "reject" | null;
    matchDecision: "suggested" | "teacher_confirmed" | "rejected" | "unmatched" | null;
  }>;
}

export interface GroupingQualityConfirmationResult {
  qualityReviewCompleted: true;
  mappedGroupCount: number;
  rejectedGroupCount: number;
  nextAction: string;
}

export interface GroupingRetakeResult {
  replacementPageId: number;
  activatedStudent: boolean;
  mappedGroupCount: number;
  rejectedGroupCount: number;
  nextAction: string;
}

export interface OrdinaryPaperRunResult {
  ai_run_id: number;
  status: "succeeded" | "failed";
  output: {
    schema_version: number;
    page_id: number;
    expected_page_no: number;
    state: "ready" | "needs_review" | "blocked";
    quality: {
      result: "pass" | "needs_review" | "reject";
      issue_codes: string[];
    };
    alignment: { confidence: number } | null;
    regions: Array<{
      assessment_item_id: number;
      region_index: number;
      mapping_confidence: number;
      mark_cells: Array<{ label: string }>;
    }>;
    printed_questions: Array<{
      assessment_item_id: number;
      stem: string;
      material_text: string | null;
      options: Array<{ label: string; content: string; order_index: number }>;
      extraction_confidence: number;
      privacy: {
        schema_version: number;
        sanitized: boolean;
        student_identity_detected: boolean;
        student_answer_detected: boolean;
        teacher_mark_detected: boolean;
        score_detected: boolean;
      };
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

export interface OrdinaryStructureConfirmationResult {
  confirmation: {
    id: number;
    ai_run_id: number;
    page_id: number;
    alignment_revision_id: number;
    region_revision_ids: number[];
    confirmed_by: string;
    created_at: string;
  };
  alignment: {
    id: number;
    decision: "teacher_confirmed";
  };
  regions: Array<{
    id: number;
    assessment_item_id: number;
    region_index: number;
    decision: "teacher_confirmed";
  }>;
}

export interface OrdinaryQuestionSyncSummary {
  schema_version: number;
  state:
    | "completed"
    | "completed_with_failures"
    | "no_printed_questions"
    | "no_safe_print_layer"
    | "source_processing"
    | "source_failed";
  assessment_version_id: number;
  page_no: number;
  source_page_id: number;
  source_ai_run_id: number;
  printed_question_count: number;
  eligible_count: number;
  enqueued_count: number;
  matched_count: number;
  candidate_created_count: number;
  needs_review_count: number;
  privacy_rejected_count: number;
  low_confidence_skipped_count: number;
  failed_count: number;
  reused_existing_source: boolean;
}

export interface AnswerSheetPageProcessingResult {
  structure: {
    materialization: {
      id: number;
      page_id: number;
      template_revision_id: number;
      alignment_revision_id: number;
      region_revision_ids: number[];
    };
    regions: Array<{
      id: number;
      assessment_item_id: number;
      region_index: number;
      decision: "teacher_confirmed";
    }>;
    routes: Array<{
      answer_region_revision_id: number;
      assessment_item_id: number;
      region_index: number;
      recognition_route: "objective_omr" | "handwriting_ocr";
      question_type: "single" | "multiple" | "true_false" | "fill_blank" | "short_answer";
    }>;
  };
  observations: Array<{
    observation: {
      id: number;
      answer_region_revision_id: number;
      result_state: "recognized" | "blank" | "altered" | "low_confidence" | "failed";
      confidence: number | null;
    };
    suggestion: {
      id: number;
      outcome: "correct" | "incorrect" | "unscored";
      batch_eligible: boolean;
      exclusion_reason: string | null;
    };
  }>;
  subjectiveRegions: Array<{
    answerRegionRevisionId: number;
    assessmentItemId: number;
    regionIndex: number;
    cropArtifactId: number;
    state: "awaiting_handwriting_recognition";
    nextAction: string;
  }>;
  subjectiveTranscriptions: SubjectiveTranscriptionRevision[];
  subjectiveFailures: Array<{
    answerRegionRevisionId: number;
    safeMessage: string;
  }>;
}

export interface SubjectiveTranscriptionRevision {
  id: number;
  public_id: string;
  attempt_id: number;
  assessment_item_id: number;
  answer_region_revision_id: number;
  question_type: "fill_blank" | "short_answer";
  revision: number;
  source_ai_run_id: number | null;
  result_state: "recognized" | "not_written" | "unreadable" | "recognize_failed" | "ambiguous_final";
  raw_ocr_text: string | null;
  normalized_text: string | null;
  teacher_corrected_text: string | null;
  confidence: number | null;
  failure_meta_json: string | null;
  corrected_by: string | null;
  corrected_at: string | null;
  state: "active" | "superseded" | "voided";
  created_at: string;
}

export interface SubjectiveWorkbenchRow {
  assessment_id: number;
  assessment_version_id: number;
  assessment_title: string;
  attempt_id: number;
  attempt_state: "ingesting" | "grading" | "ready_to_publish" | "published";
  active_publication_id: number | null;
  student_id: number;
  student_no: string;
  student_name: string;
  assessment_item_id: number;
  order_index: number;
  question_no: string;
  question_type: "fill_blank" | "short_answer";
  question_stem: string;
  max_score: number;
  answer_region_revision_id: number;
  crop_path: string | null;
  transcription_revision_id: number;
  transcription_revision: number;
  result_state: "recognized" | "not_written" | "unreadable" | "recognize_failed" | "ambiguous_final";
  raw_ocr_text: string | null;
  normalized_text: string | null;
  teacher_corrected_text: string | null;
  confidence: number | null;
  answer_json: string;
  answer_slots_json: string;
  rubric_points_json: string;
  suggestion_id: number;
  short_answer_analysis_id: number | null;
  machine_grade_ai_run_id: number | null;
  suggestion_outcome: "correct" | "incorrect" | "partial" | "unscored";
  suggested_score: number | null;
  suggestion_result_json: string;
  batch_eligible: boolean;
  exclusion_reason: string | null;
  grade_decision_id: number | null;
  grade_decision_revision: number | null;
  teacher_score: number | null;
  confirmation_level: "teacher_accepted" | "teacher_corrected" | null;
  review_mode: "single" | "teacher_corrected" | null;
  current_suggestion_confirmed: boolean;
  decided_at: string | null;
  teacher_components_json: string;
  accepted_answer_promotion_id: number | null;
  accepted_answer_promoted_at: string | null;
}

export interface SubjectiveComponentGradeInput {
  source_type: "answer_slot" | "rubric_point";
  source_public_id: string;
  teacher_score: number;
  evidence_text: string | null;
  teacher_note: string | null;
}

export interface ShortAnswerGradeAnalysis {
  id: number;
  public_id: string;
  transcription_revision_id: number;
  suggestion_id: number;
  attempt_id: number;
  assessment_item_id: number;
  answer_key_version_id: number;
  rubric_version_id: number;
  machine_grade_ai_run_id: number;
  outcome: "correct" | "incorrect" | "partial";
  suggested_score: number;
  result_json: string;
  confidence: number;
  exclusion_reason: string;
  state: "active" | "superseded" | "voided";
  created_at: string;
}

export interface SubjectiveWorkbench {
  rows: SubjectiveWorkbenchRow[];
  attempts: ObjectiveAttemptSummary[];
}

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

export interface DictationTemplateRevision {
  id: number;
  assessment_version_id: number;
  revision: number;
  template_version: string;
  page_no: number;
  source_ai_run_id: number | null;
  state: "active" | "superseded" | "voided";
}

export interface DictationTemplateStatus {
  assessmentVersionId: number;
  pageNo: number;
  activeTemplate: DictationTemplateRevision | null;
}

export interface DictationTemplateRunResult {
  ai_run_id: number;
  status: "succeeded" | "failed";
  output: {
    state: "ready" | "needs_review" | "blocked";
    page_no: number;
    canvas_width: number;
    canvas_height: number;
    regions: Array<{
      assessment_item_id: number;
      region_index: number;
      mapping_confidence: number;
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

export interface DictationTranscriptionResult {
  transcription: {
    id: number;
    answer_region_revision_id: number;
    revision: number;
    result_state: "recognized" | "not_written" | "unreadable" | "recognize_failed" | "ambiguous_final";
    raw_ocr_text: string | null;
    normalized_text: string | null;
    teacher_corrected_text: string | null;
    confidence: number | null;
  };
  observation: {
    id: number;
    result: "exact" | "accepted_variant" | "needs_review" | "not_written" | "unreadable" | "recognize_failed" | "ambiguous_final";
    suggested_score: number | null;
  };
}

export interface DictationPageProcessingResult {
  structure: {
    materialization: {
      id: number;
      page_id: number;
      template_revision_id: number;
      alignment_revision_id: number;
      region_revision_ids: number[];
    };
    regions: Array<{
      id: number;
      assessment_item_id: number;
      region_index: number;
      decision: "teacher_confirmed";
    }>;
  };
  transcriptions: DictationTranscriptionResult[];
}

export interface DictationWorkbenchRow {
  assessment_id: number;
  assessment_version_id: number;
  assessment_title: string;
  attempt_id: number;
  attempt_state: "ingesting" | "grading" | "ready_to_publish" | "published";
  active_publication_id: number | null;
  student_id: number;
  student_no: string;
  student_name: string;
  assessment_item_id: number;
  order_index: number;
  question_no: string;
  question_type: "fill_blank" | "short_answer";
  question_stem: string;
  max_score: number;
  answer_region_revision_id: number;
  crop_path: string | null;
  transcription_revision_id: number;
  transcription_revision: number;
  source_ai_run_id: number | null;
  result_state: "recognized" | "not_written" | "unreadable" | "recognize_failed" | "ambiguous_final";
  raw_ocr_text: string | null;
  normalized_text: string | null;
  teacher_corrected_text: string | null;
  confidence: number | null;
  point_result: string;
  canonical_text: string;
  accepted_variants: string[];
  suggested_score: number | null;
  requires_teacher_review: boolean;
  grade_decision_id: number | null;
  grade_decision_revision: number | null;
  teacher_score: number | null;
  confirmation_level: "teacher_accepted" | "teacher_corrected" | null;
  review_mode: "single" | "strict_batch" | null;
  current_transcription_confirmed: boolean;
  decided_at: string | null;
}

export type DictationAttemptSummary = ObjectiveAttemptSummary;

export interface DictationWorkbench {
  rows: DictationWorkbenchRow[];
  attempts: DictationAttemptSummary[];
}

export interface DictationReviewBatch {
  id: number;
  requested_count: number;
  confirmed_count: number;
  excluded_count: number;
  items: Array<{
    transcription_revision_id: number;
    outcome: "confirmed" | "excluded";
    reason_code: string | null;
    grade_decision_id: number | null;
  }>;
}

export const kpList = () => call<KnowledgePoint[]>("kp_list");
export const kpCreate = (name: string, code: string | null, parent_id: number | null) =>
  call<KnowledgePoint>("kp_create", { subjectId: null, parentId: parent_id, code, name });

export const questionsList = () => call<Question[]>("questions_list");
export const questionCreate = (q: QuestionInput) => call<number>("question_create", { q });

export const examAnswersList = (limit = 50) =>
  call<AnswerDetail[]>("exam_answers_list", { limit });
export const examAnswerSuggest = (student_id: number, question_id: number, picked: string) =>
  call<AnswerDetail>("exam_answer_suggest", { studentId: student_id, questionId: question_id, picked });
export const examAnswerHumanDecide = (
  answer_id: number,
  is_correct: boolean,
  note: string | null,
) => call<AnswerDetail>("exam_answer_human_decide", { answerId: answer_id, isCorrect: is_correct, note });

export const examObjectiveWorkbench = (assessment_version_id: number | null = null, limit = 500) =>
  call<ObjectiveWorkbench>("exam_objective_workbench", {
    assessmentVersionId: assessment_version_id,
    limit,
  });

export const examObjectiveAccept = (suggestion_id: number) =>
  call<GradeDecision>("exam_objective_accept", { suggestionId: suggestion_id });

export const examObjectiveCorrect = (
  suggestion_id: number,
  teacher_score: number,
  teacher_note: string | null,
) => call<GradeDecision>("exam_objective_correct", {
  suggestionId: suggestion_id,
  teacherScore: teacher_score,
  teacherNote: teacher_note,
});

export const examObjectiveStrictBatchAccept = (
  suggestion_ids: number[],
  idempotency_key: string,
  confidence_threshold = 0.95,
) => call<ObjectiveReviewBatch>("exam_objective_strict_batch_accept", {
  suggestionIds: suggestion_ids,
  confidenceThreshold: confidence_threshold,
  idempotencyKey: idempotency_key,
});

export const examObjectivePublishAttempt = (attempt_id: number) =>
  call<GradePublication>("exam_objective_publish_attempt", { attemptId: attempt_id });

export const examObjectiveRecognizeRegion = (
  answer_region_revision_id: number,
  idempotency_key: string,
) => call<unknown>("exam_objective_recognize_region", {
  answerRegionRevisionId: answer_region_revision_id,
  idempotencyKey: idempotency_key,
});

export const examFixedIntakeOptions = () =>
  call<FixedIntakeOption[]>("exam_fixed_intake_options");

export const examFixedIntakeInferPageCycle = (student_paths: string[]) =>
  call<PageCycleSuggestion>("exam_fixed_intake_infer_page_cycle", {
    studentPaths: student_paths,
  });

export const examFixedIntakePrepare = (request: FixedIntakeRequest) =>
  call<FixedIntakeResult>("exam_fixed_intake_prepare", { request });

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

export const examFixedIntakeConfirmMaterialType = (
  batch_id: number,
  material_type: "ordinary_paper" | "answer_sheet" | "dictation",
) => call<MaterialTypeConfirmationResult>("exam_fixed_intake_confirm_material_type", {
  batchId: batch_id,
  materialType: material_type,
});

export const examFixedIntakeConfirmGrouping = (
  batch_id: number,
  first_student_no: string,
  absent_student_nos: string[],
) => call<GroupingConfirmationResult>("exam_fixed_intake_confirm_grouping", {
  batchId: batch_id,
  firstStudentNo: first_student_no,
  absentStudentNos: absent_student_nos,
});

export const examFixedIntakeGroupingEvidence = (batch_id: number) =>
  call<GroupedPageEvidence[]>("exam_fixed_intake_grouping_evidence", { batchId: batch_id });

export const examFixedIntakeConfirmGroupingQuality = (
  batch_id: number,
  rejected_page_ids: number[],
) => call<GroupingQualityConfirmationResult>("exam_fixed_intake_confirm_grouping_quality", {
  batchId: batch_id,
  rejectedPageIds: rejected_page_ids,
});

export const examFixedIntakeReplaceRejectedPage = (
  batch_id: number,
  rejected_page_id: number,
  replacement_path: string,
) => call<GroupingRetakeResult>("exam_fixed_intake_replace_rejected_page", {
  batchId: batch_id,
  rejectedPageId: rejected_page_id,
  replacementPath: replacement_path,
});

export const examOrdinaryPaperAnalyzePage = (
  page_id: number,
  idempotency_key: string,
) => call<OrdinaryPaperRunResult>("exam_ordinary_paper_analyze_page", {
  pageId: page_id,
  idempotencyKey: idempotency_key,
});

export const examOrdinaryPaperConfirmPageStructure = (
  page_id: number,
  ai_run_id: number,
) => call<OrdinaryStructureConfirmationResult>(
  "exam_ordinary_paper_confirm_page_structure",
  { pageId: page_id, aiRunId: ai_run_id },
);

export const examOrdinaryPaperSyncQuestions = (
  page_id: number,
  ai_run_id: number,
) => call<OrdinaryQuestionSyncSummary>(
  "exam_ordinary_paper_sync_questions",
  { pageId: page_id, aiRunId: ai_run_id },
);

export const examAnswerSheetProcessPage = (page_id: number) =>
  call<AnswerSheetPageProcessingResult>("exam_answer_sheet_process_page", {
    pageId: page_id,
  });

export const examAnswerSheetRecognizeSubjectiveRegion = (
  answer_region_revision_id: number,
  idempotency_key: string,
) => call<SubjectiveTranscriptionRevision>("exam_answer_sheet_recognize_subjective_region", {
  answerRegionRevisionId: answer_region_revision_id,
  idempotencyKey: idempotency_key,
});

export const examAnswerSheetCorrectSubjectiveTranscription = (
  answer_region_revision_id: number,
  corrected_text: string,
) => call<SubjectiveTranscriptionRevision>(
  "exam_answer_sheet_correct_subjective_transcription",
  {
    answerRegionRevisionId: answer_region_revision_id,
    correctedText: corrected_text,
  },
);

export const examAnswerSheetGradeShortAnswer = (
  transcription_revision_id: number,
  idempotency_key: string,
) => call<ShortAnswerGradeAnalysis>("exam_answer_sheet_grade_short_answer", {
  transcriptionRevisionId: transcription_revision_id,
  idempotencyKey: idempotency_key,
});

export const examAnswerSheetSubjectiveWorkbench = (
  assessment_version_id: number | null = null,
  limit = 1000,
) => call<SubjectiveWorkbench>("exam_answer_sheet_subjective_workbench", {
  assessmentVersionId: assessment_version_id,
  limit,
});

export const examAnswerSheetSubjectiveAccept = (suggestion_id: number) =>
  call<GradeDecision>("exam_answer_sheet_subjective_accept", {
    suggestionId: suggestion_id,
  });

export const examAnswerSheetSubjectiveCorrect = (
  suggestion_id: number,
  teacher_score: number,
  teacher_note: string,
) => call<GradeDecision>("exam_answer_sheet_subjective_correct", {
  suggestionId: suggestion_id,
  teacherScore: teacher_score,
  teacherNote: teacher_note,
});

export const examAnswerSheetSubjectiveCorrectComponents = (
  suggestion_id: number,
  components: SubjectiveComponentGradeInput[],
  teacher_note: string,
) => call<GradeDecision>("exam_answer_sheet_subjective_correct_components", {
  suggestionId: suggestion_id,
  components,
  teacherNote: teacher_note,
});

export const examAnswerSheetPromoteAcceptedAnswer = (grade_decision_id: number) =>
  call<AcceptedAnswerPromotionResult>("exam_answer_sheet_promote_accepted_answer", {
    gradeDecisionId: grade_decision_id,
  });

export const examSubjectiveLinkEditor = (assessment_item_id: number) =>
  call<SubjectiveLinkEditor>("exam_subjective_link_editor", {
    assessmentItemId: assessment_item_id,
  });

export const examSubjectiveLinkSave = (
  assessment_item_id: number,
  sources: SubjectiveSourceLinkInput[],
) => call<SubjectiveLinkEditResult>("exam_subjective_link_save", {
  assessmentItemId: assessment_item_id,
  sources,
});

export const examAnswerSheetSubjectivePublishAttempt = (attempt_id: number) =>
  call<GradePublication>("exam_answer_sheet_subjective_publish_attempt", {
    attemptId: attempt_id,
  });

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

export const examDictationTemplateStatus = (reference_page_id: number) =>
  call<DictationTemplateStatus>("exam_dictation_template_status", {
    referencePageId: reference_page_id,
  });

export const examDictationAnalyzeTemplate = (
  reference_page_id: number,
  blank_path: string,
  idempotency_key: string,
) => call<DictationTemplateRunResult>("exam_dictation_analyze_template", {
  referencePageId: reference_page_id,
  blankPath: blank_path,
  idempotencyKey: idempotency_key,
});

export const examDictationConfirmTemplate = (reference_page_id: number, ai_run_id: number) =>
  call<{ template: DictationTemplateRevision }>("exam_dictation_confirm_template", {
    referencePageId: reference_page_id,
    aiRunId: ai_run_id,
  });

export const examDictationProcessPage = (page_id: number) =>
  call<DictationPageProcessingResult>("exam_dictation_process_page", { pageId: page_id });

export const examDictationRecognizeRegion = (
  answer_region_revision_id: number,
  idempotency_key: string,
) => call<DictationTranscriptionResult>("exam_dictation_recognize_region", {
  answerRegionRevisionId: answer_region_revision_id,
  idempotencyKey: idempotency_key,
});

export const examDictationWorkbench = (assessment_version_id: number | null = null, limit = 1000) =>
  call<DictationWorkbench>("exam_dictation_workbench", {
    assessmentVersionId: assessment_version_id,
    limit,
  });

export const examDictationAccept = (transcription_revision_id: number) =>
  call<GradeDecision>("exam_dictation_accept", {
    transcriptionRevisionId: transcription_revision_id,
  });

export const examDictationCorrectGrade = (
  transcription_revision_id: number,
  teacher_score: number,
  teacher_note: string,
  teacher_evidence_text: string | null,
) => call<GradeDecision>("exam_dictation_correct_grade", {
  transcriptionRevisionId: transcription_revision_id,
  teacherScore: teacher_score,
  teacherNote: teacher_note,
  teacherEvidenceText: teacher_evidence_text,
});

export const examDictationStrictBatchAccept = (
  transcription_revision_ids: number[],
  idempotency_key: string,
) => call<DictationReviewBatch>("exam_dictation_strict_batch_accept", {
  transcriptionRevisionIds: transcription_revision_ids,
  idempotencyKey: idempotency_key,
});

export const examDictationPublishAttempt = (attempt_id: number) =>
  call<GradePublication>("exam_dictation_publish_attempt", { attemptId: attempt_id });

export const examDictationCorrectTranscription = (
  answer_region_revision_id: number,
  corrected_text: string,
) => call<DictationTranscriptionResult>("exam_dictation_correct_transcription", {
  answerRegionRevisionId: answer_region_revision_id,
  correctedText: corrected_text,
});
