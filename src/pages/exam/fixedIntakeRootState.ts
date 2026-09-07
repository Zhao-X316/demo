import {
  createInitialAnswerSourceState,
  createInitialFixedIntakeBatchWorkflowState,
  fixedIntakeAnswerSourceReducer,
  fixedIntakeBatchWorkflowReducer,
  type FixedIntakeAnswerSourceAction,
  type FixedIntakeAnswerSourceState,
  type FixedIntakeBatchWorkflowAction,
  type FixedIntakeBatchWorkflowState,
  type FixedIntakeContextSlice,
  type FixedIntakeDraftSlice,
} from "./fixedIntakeState.ts";
import {
  isFixedIntakeCompletionCurrent,
  type FixedIntakeCompletionIdentity,
} from "./fixedIntakeLifecycle.ts";

export interface FixedIntakeControlSlice {
  scopeRevision: number;
  draftRevision: number;
  activeOperations: string[];
}

export interface FixedIntakeState extends FixedIntakeBatchWorkflowState {
  answerSource: FixedIntakeAnswerSourceState;
  control: FixedIntakeControlSlice;
}

export type FixedIntakeScopeMutation =
  | { type: "class_selected"; classId: number }
  | { type: "assessment_selected"; assessmentVersionId: number }
  | { type: "student_paths_selected"; studentPaths: string[] }
  | { type: "expected_pages_changed"; expectedPages: string }
  | { type: "answer_file_selected"; answerPath: string }
  | { type: "answer_text_changed"; answerText: string }
  | { type: "answer_cleared" };

type FixedIntakeBaseAction = FixedIntakeAnswerSourceAction | FixedIntakeBatchWorkflowAction;

type FixedIntakePageCycle = NonNullable<Extract<
  FixedIntakeBatchWorkflowAction,
  { type: "DRAFT_PAGE_CYCLE_RESOLVED" }
>["pageCycle"]>;

type FixedIntakeCompletionActionType =
  | "DRAFT_PAGE_CYCLE_RESOLVED"
  | "PREPARE_SUCCEEDED"
  | "PREPARE_FINISHED"
  | "ANSWER_SOURCE_SUCCEEDED"
  | "ANSWER_SOURCE_FAILED"
  | "ANSWER_SOURCE_FINISHED"
  | "ANSWER_SOURCE_RESOLUTION_SUCCEEDED"
  | "ANSWER_SOURCE_REASON_REPLACED"
  | "MATERIAL_CONFIRMATION_SUCCEEDED"
  | "MATERIAL_CONFIRMATION_FINISHED"
  | "GROUPING_CONFIRMATION_SUCCEEDED"
  | "GROUPING_CONFIRMATION_FINISHED"
  | "GROUPING_EVIDENCE_SUCCEEDED"
  | "GROUPING_EVIDENCE_FINISHED"
  | "QUALITY_CONFIRMATION_SUCCEEDED"
  | "QUALITY_CONFIRMATION_FINISHED"
  | "RETAKE_SUCCEEDED"
  | "RETAKE_FINISHED"
  | "ORDINARY_ANALYSIS_SUCCEEDED"
  | "ORDINARY_ANALYSIS_FINISHED"
  | "ORDINARY_CONFIRMATION_SUCCEEDED"
  | "ORDINARY_CONFIRMATION_FINISHED"
  | "ANSWER_SHEET_STATUS_LOAD_SUCCEEDED"
  | "ANSWER_SHEET_STATUS_LOAD_FAILED"
  | "ANSWER_SHEET_TEMPLATE_ANALYSIS_SUCCEEDED"
  | "ANSWER_SHEET_TEMPLATE_CONFIRMATION_SUCCEEDED"
  | "ANSWER_SHEET_TEMPLATE_OPERATION_FINISHED"
  | "ANSWER_SHEET_PAGE_SUCCEEDED"
  | "ANSWER_SHEET_PAGE_FAILED"
  | "ANSWER_SHEET_PAGE_FINISHED"
  | "DICTATION_STATUS_LOAD_SUCCEEDED"
  | "DICTATION_STATUS_LOAD_FAILED"
  | "DICTATION_TEMPLATE_ANALYSIS_SUCCEEDED"
  | "DICTATION_TEMPLATE_CONFIRMATION_SUCCEEDED"
  | "DICTATION_TEMPLATE_OPERATION_FINISHED"
  | "DICTATION_PAGE_SUCCEEDED"
  | "DICTATION_PAGE_FAILED"
  | "DICTATION_PAGE_FINISHED";

export type FixedIntakeCompletionAction = Extract<
  FixedIntakeBaseAction,
  { type: FixedIntakeCompletionActionType }
>;

export type FixedIntakeAction =
  | FixedIntakeBaseAction
  | {
    type: "FIXED_INTAKE_SCOPE_INVALIDATED";
    mutation: FixedIntakeScopeMutation;
  }
  | {
    type: "FIXED_INTAKE_ACTIVE_OPERATIONS_CHANGED";
    activeOperations: string[];
  }
  | {
    type: "FIXED_INTAKE_COMPLETION_RECEIVED";
    identity: FixedIntakeCompletionIdentity;
    completion: FixedIntakeCompletionAction;
  }
  | {
    type: "FIXED_INTAKE_PAGE_CYCLE_RECEIVED";
    identity: FixedIntakeCompletionIdentity;
    pageCycle: FixedIntakePageCycle;
  };

export function createInitialFixedIntakeState(
  initialContext: FixedIntakeContextSlice = { classId: 0, assessmentVersionId: 0 },
): FixedIntakeState {
  return {
    ...createInitialFixedIntakeBatchWorkflowState(initialContext),
    answerSource: createInitialAnswerSourceState(),
    control: {
      scopeRevision: 0,
      draftRevision: 0,
      activeOperations: [],
    },
  };
}

function isFixedIntakeAnswerSourceAction(
  action: FixedIntakeBaseAction,
): action is FixedIntakeAnswerSourceAction {
  switch (action.type) {
    case "ANSWER_SOURCE_RESET":
    case "ANSWER_SOURCE_STARTED":
    case "ANSWER_SOURCE_SUCCEEDED":
    case "ANSWER_SOURCE_FAILED":
    case "ANSWER_SOURCE_FINISHED":
    case "ANSWER_SOURCE_RESOLUTION_STARTED":
    case "ANSWER_SOURCE_RESOLUTION_SUCCEEDED":
    case "RUBRIC_MAPPING_CHANGED":
      return true;
    default:
      return false;
  }
}

function reduceFixedIntakeBaseAction(
  state: FixedIntakeState,
  action: FixedIntakeBaseAction,
): FixedIntakeState {
  if (isFixedIntakeAnswerSourceAction(action)) {
    const answerSource = fixedIntakeAnswerSourceReducer(state.answerSource, action);
    if (answerSource === state.answerSource) return state;
    return {
      ...state,
      answerSource,
    };
  }

  const workflow = fixedIntakeBatchWorkflowReducer(state, action);
  if (workflow === state) return state;
  return {
    ...workflow,
    answerSource: state.answerSource,
    control: state.control,
  };
}

function applyFixedIntakeScopeMutation(
  state: FixedIntakeState,
  mutation: FixedIntakeScopeMutation,
): { context: FixedIntakeContextSlice; draft: FixedIntakeDraftSlice; draftChanged: boolean } | null {
  switch (mutation.type) {
    case "class_selected":
      if (state.context.classId === mutation.classId) return null;
      return {
        context: { ...state.context, classId: mutation.classId },
        draft: state.draft,
        draftChanged: false,
      };
    case "assessment_selected":
      if (state.context.assessmentVersionId === mutation.assessmentVersionId) return null;
      return {
        context: { ...state.context, assessmentVersionId: mutation.assessmentVersionId },
        draft: state.draft,
        draftChanged: false,
      };
    case "student_paths_selected":
      return {
        context: state.context,
        draft: {
          ...state.draft,
          studentPaths: [...mutation.studentPaths],
          pageCycle: null,
        },
        draftChanged: true,
      };
    case "expected_pages_changed":
      if (state.draft.expectedPages === mutation.expectedPages) return null;
      return {
        context: state.context,
        draft: { ...state.draft, expectedPages: mutation.expectedPages },
        draftChanged: true,
      };
    case "answer_file_selected":
      if (state.draft.answerPath === mutation.answerPath && state.draft.answerText === "") return null;
      return {
        context: state.context,
        draft: {
          ...state.draft,
          answerPath: mutation.answerPath,
          answerText: "",
        },
        draftChanged: true,
      };
    case "answer_text_changed":
      if (state.draft.answerPath === null && state.draft.answerText === mutation.answerText) return null;
      return {
        context: state.context,
        draft: {
          ...state.draft,
          answerPath: null,
          answerText: mutation.answerText,
        },
        draftChanged: true,
      };
    case "answer_cleared":
      if (state.draft.answerPath === null && state.draft.answerText === "") return null;
      return {
        context: state.context,
        draft: {
          ...state.draft,
          answerPath: null,
          answerText: "",
        },
        draftChanged: true,
      };
  }
}

function invalidateFixedIntakeScope(
  state: FixedIntakeState,
  mutation: FixedIntakeScopeMutation,
): FixedIntakeState {
  const changed = applyFixedIntakeScopeMutation(state, mutation);
  if (!changed) return state;

  return {
    ...createInitialFixedIntakeBatchWorkflowState(changed.context),
    context: changed.context,
    draft: changed.draft,
    answerSource: createInitialAnswerSourceState(),
    control: {
      scopeRevision: state.control.scopeRevision + 1,
      draftRevision: state.control.draftRevision + (changed.draftChanged ? 1 : 0),
      activeOperations: state.control.activeOperations,
    },
  };
}

export function fixedIntakeCompletionIdentity(
  state: FixedIntakeState,
): FixedIntakeCompletionIdentity {
  return {
    scopeRevision: state.control.scopeRevision,
    draftRevision: state.control.draftRevision,
    ...(state.batch.result ? { batchId: state.batch.result.batchId } : {}),
  };
}

type FixedIntakeTemplateMaterial = "answer_sheet" | "dictation";

function fixedIntakeEligiblePageIds(state: FixedIntakeState): number[] {
  return state.grouping.evidence
    .flatMap((group) => group.pages)
    .filter(
      (page) => page.qualityResult === "pass"
        && page.matchDecision === "teacher_confirmed",
    )
    .map((page) => page.pageId);
}

function fixedIntakeRejectedPageIds(state: FixedIntakeState): number[] {
  return state.grouping.evidence
    .flatMap((group) => group.pages)
    .filter(
      (page) => page.qualityResult === "reject"
        && page.matchDecision === "rejected",
    )
    .map((page) => page.pageId);
}

export function fixedIntakeMaterialScopeKey(
  state: FixedIntakeState,
  materialType: FixedIntakeTemplateMaterial,
): string | undefined {
  const result = state.batch.result;
  if (
    !result
    || !result.groupingConfirmed
    || !result.qualityReviewCompleted
    || result.materialType !== materialType
  ) {
    return undefined;
  }

  const referencePageId = fixedIntakeEligiblePageIds(state)[0];
  return referencePageId
    ? `${materialType}:${result.batchId}:${referencePageId}`
    : undefined;
}

export function isFixedIntakeStateCompletionCurrent(
  state: FixedIntakeState,
  expected: FixedIntakeCompletionIdentity,
): boolean {
  const current = fixedIntakeCompletionIdentity(state);
  if (expected.scopeKey !== undefined) {
    const materialType = state.batch.result?.materialType;
    current.scopeKey = materialType === "answer_sheet" || materialType === "dictation"
      ? fixedIntakeMaterialScopeKey(state, materialType)
      : undefined;
  }
  if (expected.pageId !== undefined) {
    current.pageId = fixedIntakeEligiblePageIds(state).includes(expected.pageId)
      ? expected.pageId
      : undefined;
  }
  if (expected.rejectedPageId !== undefined) {
    current.rejectedPageId = fixedIntakeRejectedPageIds(state).includes(expected.rejectedPageId)
      ? expected.rejectedPageId
      : undefined;
  }
  if (expected.ordinaryRunId !== undefined) {
    current.ordinaryRunId = expected.pageId === undefined
      ? undefined
      : state.ordinary.runs[expected.pageId]?.ai_run_id;
  }
  if (expected.answerSheetTemplateRunId !== undefined) {
    current.answerSheetTemplateRunId = state.answerSheet.templateRun?.ai_run_id;
  }
  if (expected.dictationTemplateRunId !== undefined) {
    current.dictationTemplateRunId = state.dictation.templateRun?.ai_run_id;
  }
  if (expected.runId !== undefined) {
    current.runId = state.answerSource.analysis?.review?.sourceAiRunId;
  }
  return isFixedIntakeCompletionCurrent(expected, current);
}

export function fixedIntakeReducer(
  state: FixedIntakeState,
  action: FixedIntakeAction,
): FixedIntakeState {
  switch (action.type) {
    case "FIXED_INTAKE_SCOPE_INVALIDATED":
      return invalidateFixedIntakeScope(state, action.mutation);
    case "FIXED_INTAKE_ACTIVE_OPERATIONS_CHANGED": {
      const same = action.activeOperations.length === state.control.activeOperations.length
        && action.activeOperations.every(
          (key, index) => key === state.control.activeOperations[index],
        );
      if (same) return state;
      return {
        ...state,
        control: {
          ...state.control,
          activeOperations: [...action.activeOperations],
        },
      };
    }
    case "FIXED_INTAKE_COMPLETION_RECEIVED":
      if (!isFixedIntakeStateCompletionCurrent(state, action.identity)) return state;
      return reduceFixedIntakeBaseAction(state, action.completion);
    case "FIXED_INTAKE_PAGE_CYCLE_RECEIVED": {
      if (!isFixedIntakeStateCompletionCurrent(state, action.identity)) return state;
      const withPageCycle = reduceFixedIntakeBaseAction(state, {
        type: "DRAFT_PAGE_CYCLE_RESOLVED",
        pageCycle: action.pageCycle,
      });
      return reduceFixedIntakeBaseAction(withPageCycle, {
        type: "DRAFT_EXPECTED_PAGES_CHANGED",
        expectedPages: String(action.pageCycle.expectedPagesPerAttempt),
      });
    }
    default:
      return reduceFixedIntakeBaseAction(state, action);
  }
}
