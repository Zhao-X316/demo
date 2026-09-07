import type { K1QuestionType } from "./blueprints";
import { call } from "../client";

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
  assessmentVersionRevision: number;
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
  isCurrentDefault: boolean;
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

export interface UpgradeAssessmentDefaultInput {
  requestKey: string;
  planPublicId: string;
  sourceAssessmentVersionPublicId: string;
  expectedCurrentDefaultVersionPublicId: string;
  upgradedBy: "local_teacher";
}

export interface AssessmentDefaultUpgrade {
  selectionPublicId: string;
  planPublicId: string;
  assessmentPublicId: string;
  assessmentTitle: string;
  sourceAssessmentVersionPublicId: string;
  sourceRevision: number;
  defaultAssessmentVersionPublicId: string;
  defaultRevision: number;
  upgradedItemCount: number;
  selectedBy: string;
  selectedAt: string;
  defaultForFutureIntake: true;
  changesHistoricalAttempts: false;
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

export const upgradeAssessmentDefaultFromImpact = (
  input: UpgradeAssessmentDefaultInput,
) =>
  call<AssessmentDefaultUpgrade>(
    "k1_question_impact_upgrade_assessment",
    { input },
  );
