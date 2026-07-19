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

export interface QuestionPerformanceContext {
  assessmentContext: string;
  publishedResponseCount: number;
  averageScoreRate: number | null;
  fullCreditRate: number | null;
}

export interface QuestionPerformanceItem {
  questionVersionPublicId: string;
  revision: number;
  ownerScope: string;
  questionType: K1QuestionType;
  stem: string;
  maxScore: number;
  qualityLevel: string;
  state: string;
  assessmentUsageCount: number;
  publishedResponseCount: number;
  fullCreditCount: number;
  partialCreditCount: number;
  zeroScoreCount: number;
  averageScoreRate: number | null;
  fullCreditRate: number | null;
  firstAttemptCount: number;
  correctionAttemptCount: number;
  latestPublishedAt: string | null;
  contextBreakdown: QuestionPerformanceContext[];
  hasVersionUpdateImpact: boolean;
}

export interface QuestionPerformanceCatalog {
  schemaVersion: number;
  ruleVersion: string;
  calculatedAt: string;
  items: QuestionPerformanceItem[];
  boundaryNote: string;
}

export interface QuestionImpactTargetVersion {
  answerKeyVersionPublicId: string;
  answerKeyRevision: number;
  rubricVersionPublicId: string;
  rubricRevision: number;
  linkSetPublicId: string;
  linkSetRevision: number;
}

export interface QuestionImpactRow {
  assessmentPublicId: string;
  assessmentTitle: string;
  className: string;
  assessmentVersionPublicId: string;
  assessmentItemPublicId: string;
  sourceAnswerKeyVersionPublicId: string;
  sourceRubricVersionPublicId: string;
  sourceLinkSetPublicId: string;
  answerChanged: boolean;
  rubricChanged: boolean;
  linkChanged: boolean;
  unpublishedAttemptCount: number;
  publishedAttemptCount: number;
  activeLearningEvidenceCount: number;
  profileSnapshotCount: number;
}

export interface QuestionVersionImpactPreview {
  schemaVersion: number;
  ruleVersion: string;
  calculatedAt: string;
  previewHash: string;
  questionVersionPublicId: string;
  questionType: K1QuestionType;
  stem: string;
  target: QuestionImpactTargetVersion;
  affectedAssessmentCount: number;
  affectedAssessmentVersionCount: number;
  affectedItemCount: number;
  unpublishedAttemptCount: number;
  publishedAttemptCount: number;
  activeLearningEvidenceCount: number;
  profileSnapshotCount: number;
  rows: QuestionImpactRow[];
  boundaryNote: string;
}

export type QuestionImpactAction =
  | "future_only"
  | "recalculate_unpublished"
  | "review_published";

export interface ConfirmQuestionImpactPlanInput {
  requestKey: string;
  questionVersionPublicId: string;
  expectedPreviewHash: string;
  action: QuestionImpactAction;
  plannedBy: "local_teacher";
}

export interface QuestionImpactPlan {
  publicId: string;
  questionVersionPublicId: string;
  expectedPreviewHash: string;
  action: QuestionImpactAction;
  taskCount: number;
  plannedBy: string;
  plannedAt: string;
  changesAssessmentBinding: false;
  changesGrade: false;
  changesPublication: false;
  changesLearningEvidence: false;
}

export interface QuestionImpactReviewCase {
  publicId: string;
  impactTaskPublicId: string;
  planPublicId: string;
  caseKind: "unpublished_recalculation" | "published_review";
  assessmentTitle: string;
  className: string;
  studentNo: string;
  studentName: string;
  questionVersionPublicId: string;
  questionType: K1QuestionType;
  questionStem: string;
  questionNo: number;
  maxScore: number;
  attemptPublicId: string;
  attemptState: string;
  publicationPublicId: string | null;
  sourceGradeDecisionPublicId: string | null;
  sourceGradeDecisionRevision: number | null;
  sourceTeacherScore: number | null;
  sourcePointResultsJson: string | null;
  sourceSnapshotHash: string;
  studentResponseState: string | null;
  studentResponseText: string | null;
  cropPath: string | null;
  targetAnswerJson: string;
  targetComponents: QuestionImpactTargetComponent[];
  targetAnswerKeyVersionPublicId: string;
  targetAnswerKeyRevision: number;
  targetRubricVersionPublicId: string;
  targetRubricRevision: number;
  targetLinkSetPublicId: string;
  targetLinkSetRevision: number;
  preparedBy: string;
  preparedAt: string;
  state: "open" | "grade_confirmed" | "republished";
  resolutionPublicId: string | null;
  resolvedGradeDecisionPublicId: string | null;
  resolvedTeacherScore: number | null;
  resolvedAt: string | null;
  activePublicationPublicId: string | null;
  nextStepNote: string;
  changesAssessmentBinding: false;
  changesGrade: boolean;
  changesPublication: boolean;
  changesLearningEvidence: boolean;
}

export interface QuestionImpactTargetComponent {
  sourceType: "answer_slot" | "rubric_point";
  sourcePublicId: string;
  stableId: string;
  orderIndex: number;
  label: string;
  maxScore: number;
}

export interface QuestionImpactReviewCaseCatalog {
  schemaVersion: number;
  ruleVersion: string;
  planPublicId: string;
  cases: QuestionImpactReviewCase[];
  boundaryNote: string;
}

export interface PrepareQuestionImpactReviewCasesInput {
  planPublicId: string;
  expectedTaskCount: number;
  preparedBy: "local_teacher";
}

export interface PrepareQuestionImpactReviewCasesResult {
  planPublicId: string;
  taskCount: number;
  createdCount: number;
  existingCount: number;
  cases: QuestionImpactReviewCase[];
  changesAssessmentBinding: false;
  changesGrade: false;
  changesPublication: false;
  changesLearningEvidence: false;
}

export interface ResolveQuestionImpactComponentInput {
  sourcePublicId: string;
  teacherScore: number;
  evidenceText: string | null;
  teacherNote: string | null;
}

export interface ResolveQuestionImpactReviewCaseInput {
  requestKey: string;
  casePublicId: string;
  expectedSourceSnapshotHash: string;
  teacherScore: number | null;
  components: ResolveQuestionImpactComponentInput[];
  teacherNote: string;
  resolvedBy: "local_teacher";
}

export interface ResolveQuestionImpactReviewCaseResult {
  casePublicId: string;
  resolutionPublicId: string;
  gradeDecision: {
    public_id: string;
    revision: number;
    teacher_score: number;
    point_results_json: string;
    state: string;
  };
  oldPublicationUnchanged: boolean;
  learningEvidenceUnchanged: boolean;
  requiresExplicitPublication: boolean;
}

export interface PublishQuestionImpactReviewCaseInput {
  casePublicId: string;
  expectedGradeDecisionPublicId: string;
  publishedBy: "local_teacher";
}

export interface PublishQuestionImpactReviewCaseResult {
  casePublicId: string;
  publication: {
    public_id: string;
    revision: number;
    state: string;
    total_score: number;
  };
  priorPublicationSuperseded: boolean;
  learningEvidenceSwitched: boolean;
}

export const loadQuestionPerformance = (limit = 200) =>
  call<QuestionPerformanceCatalog>("k1_question_performance", { limit });

export const previewQuestionImpact = (questionVersionPublicId: string) =>
  call<QuestionVersionImpactPreview>("k1_question_impact_preview", {
    questionVersionPublicId,
  });

export const confirmQuestionImpact = (input: ConfirmQuestionImpactPlanInput) =>
  call<QuestionImpactPlan>("k1_question_impact_confirm", { input });

export const loadQuestionImpactCases = (planPublicId: string) =>
  call<QuestionImpactReviewCaseCatalog>("k1_question_impact_cases", {
    planPublicId,
  });

export const prepareQuestionImpactCases = (
  input: PrepareQuestionImpactReviewCasesInput,
) =>
  call<PrepareQuestionImpactReviewCasesResult>(
    "k1_question_impact_prepare",
    { input },
  );

export const resolveQuestionImpactCase = (
  input: ResolveQuestionImpactReviewCaseInput,
) =>
  call<ResolveQuestionImpactReviewCaseResult>(
    "k1_question_impact_resolve",
    { input },
  );

export const publishQuestionImpactCase = (
  input: PublishQuestionImpactReviewCaseInput,
) =>
  call<PublishQuestionImpactReviewCaseResult>(
    "k1_question_impact_publish",
    { input },
  );
