import type {
  AnswerSheetPageProcessingResult,
  AnswerSheetTemplateRunResult,
  AnswerSheetTemplateStatus,
  AnswerSourceAnalysisResult,
  AnswerSourceReviewSummary,
  DictationPageProcessingResult,
  DictationTemplateRunResult,
  DictationTemplateStatus,
  FixedIntakeResult,
  GroupedPageEvidence,
  GroupingConfirmationResult,
  GroupingQualityConfirmationResult,
  GroupingRetakeResult,
  MaterialTypeConfirmationResult,
  OrdinaryPaperRunResult,
  OrdinaryStructureConfirmationResult,
  PageCycleSuggestion,
} from "../../api/exam";
import { shortAnswerRubricPoints } from "./examPure.ts";

export const NEW_RUBRIC_POINT = "__new_rubric_point__";

export interface FixedIntakeAnswerSourceState {
  analysis: AnswerSourceAnalysisResult | null;
  busy: boolean;
  error: string;
  rubricPointMappings: Record<string, string>;
}

export type FixedIntakeAnswerSourceAction =
  | { type: "ANSWER_SOURCE_RESET" }
  | { type: "ANSWER_SOURCE_STARTED" }
  | { type: "ANSWER_SOURCE_SUCCEEDED"; analysis: AnswerSourceAnalysisResult }
  | { type: "ANSWER_SOURCE_FAILED"; error: string }
  | { type: "ANSWER_SOURCE_FINISHED" }
  | { type: "ANSWER_SOURCE_RESOLUTION_STARTED" }
  | { type: "ANSWER_SOURCE_RESOLUTION_SUCCEEDED"; review: AnswerSourceReviewSummary }
  | {
    type: "RUBRIC_MAPPING_CHANGED";
    assessmentItemId: number;
    orderIndex: number;
    stableId: string;
  };

export function createInitialAnswerSourceState(): FixedIntakeAnswerSourceState {
  return {
    analysis: null,
    busy: false,
    error: "",
    rubricPointMappings: {},
  };
}

export function rubricPointMappingKey(assessmentItemId: number, orderIndex: number) {
  return `${assessmentItemId}:${orderIndex}`;
}

export function initialRubricPointMappings(review: AnswerSourceReviewSummary | null) {
  const result: Record<string, string> = {};
  review?.items
    .filter((item) => item.questionType === "short_answer" && item.matchState === "conflict")
    .forEach((item) => {
      const candidates = shortAnswerRubricPoints(item.candidateAnswerJson);
      const sameShape = candidates.length === item.boundRubricPoints.length
        && candidates.every((candidate) => item.boundRubricPoints.some(
          (current) => current.orderIndex === candidate.orderIndex,
        ));
      candidates.forEach((candidate) => {
        const current = item.boundRubricPoints.find(
          (point) => point.orderIndex === candidate.orderIndex,
        );
        result[rubricPointMappingKey(item.assessmentItemId, candidate.orderIndex)] = sameShape && current
          ? current.stableId
          : "";
      });
    });
  return result;
}

export function rubricPointMappingsReady(
  review: AnswerSourceReviewSummary,
  mappings: Record<string, string>,
) {
  for (const item of review.items) {
    if (item.questionType !== "short_answer" || item.matchState !== "conflict") continue;
    const reused = new Set<string>();
    for (const point of shortAnswerRubricPoints(item.candidateAnswerJson)) {
      const selected = mappings[rubricPointMappingKey(item.assessmentItemId, point.orderIndex)] || "";
      if (!selected) return false;
      if (selected === NEW_RUBRIC_POINT) continue;
      if (reused.has(selected)) return false;
      reused.add(selected);
    }
  }
  return true;
}

export function fixedIntakeAnswerSourceReducer(
  state: FixedIntakeAnswerSourceState,
  action: FixedIntakeAnswerSourceAction,
): FixedIntakeAnswerSourceState {
  switch (action.type) {
    case "ANSWER_SOURCE_RESET":
      return createInitialAnswerSourceState();
    case "ANSWER_SOURCE_STARTED":
      return {
        ...state,
        busy: true,
        error: "",
      };
    case "ANSWER_SOURCE_SUCCEEDED":
      return {
        ...state,
        analysis: action.analysis,
        rubricPointMappings: initialRubricPointMappings(action.analysis.review),
      };
    case "ANSWER_SOURCE_FAILED":
      return {
        ...state,
        error: action.error,
      };
    case "ANSWER_SOURCE_FINISHED":
      return {
        ...state,
        busy: false,
      };
    case "ANSWER_SOURCE_RESOLUTION_STARTED":
      return {
        ...state,
        busy: true,
      };
    case "ANSWER_SOURCE_RESOLUTION_SUCCEEDED":
      if (!state.analysis) return state;
      return {
        ...state,
        analysis: {
          ...state.analysis,
          review: action.review,
        },
      };
    case "RUBRIC_MAPPING_CHANGED":
      return {
        ...state,
        rubricPointMappings: {
          ...state.rubricPointMappings,
          [rubricPointMappingKey(action.assessmentItemId, action.orderIndex)]: action.stableId,
        },
      };
  }
}

export interface FixedIntakeBatchSlice {
  requestKey: string;
  busy: boolean;
  result: FixedIntakeResult | null;
  confirmingType: boolean;
}

export interface FixedIntakeContextSlice {
  classId: number;
  assessmentVersionId: number;
}

export interface FixedIntakeDraftSlice {
  studentPaths: string[];
  answerPath: string | null;
  answerText: string;
  expectedPages: string;
  pageCycle: PageCycleSuggestion | null;
}

export interface FixedIntakeGroupingSlice {
  startNo: string;
  absentStudentNos: string[];
  confirming: boolean;
  evidence: GroupedPageEvidence[];
  loadingEvidence: boolean;
}

export interface FixedIntakeQualitySlice {
  rejectedPageIds: number[];
  confirming: boolean;
  retakingPageId: number | null;
}

export interface FixedIntakeOrdinarySlice {
  runs: Record<number, OrdinaryPaperRunResult>;
  analyzingPageIds: number[];
  confirmations: Record<number, OrdinaryStructureConfirmationResult>;
  confirmingPageIds: number[];
}

export interface FixedIntakeAnswerSheetSlice {
  templateStatus: AnswerSheetTemplateStatus | null;
  templateStatusLoaded: boolean;
  templateRun: AnswerSheetTemplateRunResult | null;
  templateBusy: boolean;
  pageResults: Record<number, AnswerSheetPageProcessingResult>;
  pageFailures: Record<number, string>;
  processingPageIds: number[];
}

export interface FixedIntakeDictationSlice {
  templateStatus: DictationTemplateStatus | null;
  templateStatusLoaded: boolean;
  templateRun: DictationTemplateRunResult | null;
  templateBusy: boolean;
  pageResults: Record<number, DictationPageProcessingResult>;
  pageFailures: Record<number, string>;
  processingPageIds: number[];
}

export interface FixedIntakeBatchWorkflowState {
  context: FixedIntakeContextSlice;
  draft: FixedIntakeDraftSlice;
  batch: FixedIntakeBatchSlice;
  grouping: FixedIntakeGroupingSlice;
  quality: FixedIntakeQualitySlice;
  ordinary: FixedIntakeOrdinarySlice;
  answerSheet: FixedIntakeAnswerSheetSlice;
  dictation: FixedIntakeDictationSlice;
}

export type FixedIntakeBatchWorkflowAction =
  | { type: "FIXED_INTAKE_SESSION_RESET" }
  | { type: "CONTEXT_CLASS_CHANGED"; classId: number }
  | { type: "CONTEXT_ASSESSMENT_CHANGED"; assessmentVersionId: number }
  | { type: "DRAFT_STUDENT_PATHS_CHANGED"; studentPaths: string[] }
  | { type: "DRAFT_ANSWER_PATH_CHANGED"; answerPath: string | null }
  | { type: "DRAFT_ANSWER_TEXT_CHANGED"; answerText: string }
  | { type: "DRAFT_EXPECTED_PAGES_CHANGED"; expectedPages: string }
  | { type: "DRAFT_PAGE_CYCLE_RESOLVED"; pageCycle: PageCycleSuggestion | null }
  | { type: "PREPARE_INPUT_INVALIDATED" }
  | { type: "PREPARE_STARTED"; requestKey: string }
  | { type: "PREPARE_SUCCEEDED"; result: FixedIntakeResult }
  | { type: "PREPARE_FINISHED" }
  | { type: "ANSWER_SOURCE_REASON_REPLACED"; reasonCode: string | null }
  | { type: "MATERIAL_CONFIRMATION_STARTED" }
  | { type: "MATERIAL_CONFIRMATION_SUCCEEDED"; confirmation: MaterialTypeConfirmationResult }
  | { type: "MATERIAL_CONFIRMATION_FINISHED" }
  | { type: "GROUPING_START_CHANGED"; studentNo: string }
  | { type: "ABSENT_STUDENT_TOGGLED"; studentNo: string }
  | { type: "GROUPING_CONFIRMATION_STARTED" }
  | { type: "GROUPING_CONFIRMATION_SUCCEEDED"; confirmation: GroupingConfirmationResult }
  | { type: "GROUPING_CONFIRMATION_FINISHED" }
  | { type: "GROUPING_EVIDENCE_STARTED" }
  | { type: "GROUPING_EVIDENCE_SUCCEEDED"; evidence: GroupedPageEvidence[] }
  | { type: "GROUPING_EVIDENCE_FINISHED" }
  | { type: "QUALITY_REJECTION_TOGGLED"; pageId: number }
  | { type: "QUALITY_CONFIRMATION_STARTED" }
  | { type: "QUALITY_CONFIRMATION_SUCCEEDED"; confirmation: GroupingQualityConfirmationResult }
  | { type: "QUALITY_CONFIRMATION_FINISHED" }
  | { type: "RETAKE_STARTED"; pageId: number }
  | { type: "RETAKE_SUCCEEDED"; replacement: GroupingRetakeResult }
  | { type: "RETAKE_FINISHED" }
  | { type: "ORDINARY_PARTIAL_RESET" }
  | { type: "ORDINARY_FULL_RESET" }
  | { type: "ORDINARY_ANALYSIS_STARTED"; pageIds: number[] }
  | { type: "ORDINARY_ANALYSIS_SUCCEEDED"; pageId: number; run: OrdinaryPaperRunResult }
  | { type: "ORDINARY_ANALYSIS_FINISHED"; pageId: number }
  | { type: "ORDINARY_CONFIRMATION_STARTED"; pageIds: number[] }
  | {
    type: "ORDINARY_CONFIRMATION_SUCCEEDED";
    pageId: number;
    confirmation: OrdinaryStructureConfirmationResult;
  }
  | { type: "ORDINARY_CONFIRMATION_FINISHED"; pageId: number }
  | { type: "ANSWER_SHEET_RESET" }
  | { type: "ANSWER_SHEET_STATUS_LOAD_STARTED" }
  | { type: "ANSWER_SHEET_STATUS_LOAD_SUCCEEDED"; status: AnswerSheetTemplateStatus }
  | { type: "ANSWER_SHEET_STATUS_LOAD_FAILED" }
  | { type: "ANSWER_SHEET_TEMPLATE_ANALYSIS_STARTED" }
  | { type: "ANSWER_SHEET_TEMPLATE_ANALYSIS_SUCCEEDED"; run: AnswerSheetTemplateRunResult }
  | { type: "ANSWER_SHEET_TEMPLATE_CONFIRMATION_STARTED" }
  | { type: "ANSWER_SHEET_TEMPLATE_CONFIRMATION_SUCCEEDED"; status: AnswerSheetTemplateStatus }
  | { type: "ANSWER_SHEET_TEMPLATE_OPERATION_FINISHED" }
  | { type: "ANSWER_SHEET_PAGES_STARTED"; pageIds: number[] }
  | {
    type: "ANSWER_SHEET_PAGE_SUCCEEDED";
    pageId: number;
    result: AnswerSheetPageProcessingResult;
  }
  | { type: "ANSWER_SHEET_PAGE_FAILED"; pageId: number; error: string }
  | { type: "ANSWER_SHEET_PAGE_FINISHED"; pageId: number }
  | { type: "DICTATION_RESET" }
  | { type: "DICTATION_STATUS_LOAD_STARTED" }
  | { type: "DICTATION_STATUS_LOAD_SUCCEEDED"; status: DictationTemplateStatus }
  | { type: "DICTATION_STATUS_LOAD_FAILED" }
  | { type: "DICTATION_TEMPLATE_ANALYSIS_STARTED" }
  | { type: "DICTATION_TEMPLATE_ANALYSIS_SUCCEEDED"; run: DictationTemplateRunResult }
  | { type: "DICTATION_TEMPLATE_CONFIRMATION_STARTED" }
  | { type: "DICTATION_TEMPLATE_CONFIRMATION_SUCCEEDED"; status: DictationTemplateStatus }
  | { type: "DICTATION_TEMPLATE_OPERATION_FINISHED" }
  | { type: "DICTATION_PAGES_STARTED"; pageIds: number[] }
  | {
    type: "DICTATION_PAGE_SUCCEEDED";
    pageId: number;
    result: DictationPageProcessingResult;
  }
  | { type: "DICTATION_PAGE_FAILED"; pageId: number; error: string }
  | { type: "DICTATION_PAGE_FINISHED"; pageId: number };

const ANSWER_SOURCE_REASON_CODES = new Set([
  "ANSWER_SOURCE_STRUCTURE_PENDING",
  "ANSWER_SOURCE_STRUCTURE_FAILED",
  "ANSWER_SOURCE_CONFIRMATION_REQUIRED",
  "ANSWER_SOURCE_CONFLICT_OR_MISSING",
]);

export function createInitialFixedIntakeBatchWorkflowState(
  initialContext: FixedIntakeContextSlice = { classId: 0, assessmentVersionId: 0 },
): FixedIntakeBatchWorkflowState {
  return {
    context: {
      classId: initialContext.classId,
      assessmentVersionId: initialContext.assessmentVersionId,
    },
    draft: {
      studentPaths: [],
      answerPath: null,
      answerText: "",
      expectedPages: "1",
      pageCycle: null,
    },
    batch: {
      requestKey: "",
      busy: false,
      result: null,
      confirmingType: false,
    },
    grouping: {
      startNo: "",
      absentStudentNos: [],
      confirming: false,
      evidence: [],
      loadingEvidence: false,
    },
    quality: {
      rejectedPageIds: [],
      confirming: false,
      retakingPageId: null,
    },
    ordinary: {
      runs: {},
      analyzingPageIds: [],
      confirmations: {},
      confirmingPageIds: [],
    },
    answerSheet: {
      templateStatus: null,
      templateStatusLoaded: false,
      templateRun: null,
      templateBusy: false,
      pageResults: {},
      pageFailures: {},
      processingPageIds: [],
    },
    dictation: {
      templateStatus: null,
      templateStatusLoaded: false,
      templateRun: null,
      templateBusy: false,
      pageResults: {},
      pageFailures: {},
      processingPageIds: [],
    },
  };
}

export function fixedIntakeBatchWorkflowReducer(
  state: FixedIntakeBatchWorkflowState,
  action: FixedIntakeBatchWorkflowAction,
): FixedIntakeBatchWorkflowState {
  switch (action.type) {
    case "CONTEXT_CLASS_CHANGED":
      if (state.context.classId === action.classId) return state;
      return {
        ...state,
        context: {
          ...state.context,
          classId: action.classId,
        },
      };
    case "CONTEXT_ASSESSMENT_CHANGED":
      if (state.context.assessmentVersionId === action.assessmentVersionId) return state;
      return {
        ...state,
        context: {
          ...state.context,
          assessmentVersionId: action.assessmentVersionId,
        },
      };
    case "DRAFT_STUDENT_PATHS_CHANGED":
      if (state.draft.studentPaths === action.studentPaths) return state;
      return {
        ...state,
        draft: {
          ...state.draft,
          studentPaths: action.studentPaths,
        },
      };
    case "DRAFT_ANSWER_PATH_CHANGED":
      if (state.draft.answerPath === action.answerPath) return state;
      return {
        ...state,
        draft: {
          ...state.draft,
          answerPath: action.answerPath,
        },
      };
    case "DRAFT_ANSWER_TEXT_CHANGED":
      if (state.draft.answerText === action.answerText) return state;
      return {
        ...state,
        draft: {
          ...state.draft,
          answerText: action.answerText,
        },
      };
    case "DRAFT_EXPECTED_PAGES_CHANGED":
      if (state.draft.expectedPages === action.expectedPages) return state;
      return {
        ...state,
        draft: {
          ...state.draft,
          expectedPages: action.expectedPages,
        },
      };
    case "DRAFT_PAGE_CYCLE_RESOLVED":
      if (state.draft.pageCycle === action.pageCycle) return state;
      return {
        ...state,
        draft: {
          ...state.draft,
          pageCycle: action.pageCycle,
        },
      };
    case "FIXED_INTAKE_SESSION_RESET":
      return {
        context: state.context,
        draft: state.draft,
        batch: {
          ...state.batch,
          requestKey: "",
          result: null,
        },
        grouping: {
          ...state.grouping,
          startNo: "",
          absentStudentNos: [],
          evidence: [],
        },
        quality: {
          ...state.quality,
          rejectedPageIds: [],
        },
        ordinary: state.ordinary,
        answerSheet: state.answerSheet,
        dictation: state.dictation,
      };
    case "PREPARE_INPUT_INVALIDATED":
      return {
        ...state,
        batch: {
          ...state.batch,
          requestKey: "",
          result: null,
        },
      };
    case "PREPARE_STARTED":
      return {
        ...state,
        batch: {
          ...state.batch,
          requestKey: action.requestKey,
          busy: true,
        },
      };
    case "PREPARE_SUCCEEDED":
      return {
        ...state,
        batch: {
          ...state.batch,
          requestKey: "",
          result: action.result,
        },
        grouping: {
          ...state.grouping,
          startNo: action.result.groupingFirstStudentNo
            || action.result.groupingRoster[0]?.studentNo
            || "",
          absentStudentNos: [],
        },
      };
    case "PREPARE_FINISHED":
      return {
        ...state,
        batch: {
          ...state.batch,
          busy: false,
        },
      };
    case "ANSWER_SOURCE_REASON_REPLACED": {
      if (!state.batch.result) return state;
      const reasonCodes = state.batch.result.reasonCodes.filter(
        (code) => !ANSWER_SOURCE_REASON_CODES.has(code),
      );
      if (action.reasonCode) reasonCodes.push(action.reasonCode);
      return {
        ...state,
        batch: {
          ...state.batch,
          result: {
            ...state.batch.result,
            reasonCodes,
          },
        },
      };
    }
    case "MATERIAL_CONFIRMATION_STARTED":
      return {
        ...state,
        batch: {
          ...state.batch,
          confirmingType: true,
        },
      };
    case "MATERIAL_CONFIRMATION_SUCCEEDED":
      if (!state.batch.result) return state;
      return {
        ...state,
        batch: {
          ...state.batch,
          result: {
            ...state.batch.result,
            ...action.confirmation,
            materialTypeNeedsConfirmation: false,
          },
        },
      };
    case "MATERIAL_CONFIRMATION_FINISHED":
      return {
        ...state,
        batch: {
          ...state.batch,
          confirmingType: false,
        },
      };
    case "GROUPING_START_CHANGED":
      return {
        ...state,
        grouping: {
          ...state.grouping,
          startNo: action.studentNo,
          absentStudentNos: [],
        },
      };
    case "ABSENT_STUDENT_TOGGLED":
      return {
        ...state,
        grouping: {
          ...state.grouping,
          absentStudentNos: state.grouping.absentStudentNos.includes(action.studentNo)
            ? state.grouping.absentStudentNos.filter((value) => value !== action.studentNo)
            : [...state.grouping.absentStudentNos, action.studentNo],
        },
      };
    case "GROUPING_CONFIRMATION_STARTED":
      return {
        ...state,
        grouping: {
          ...state.grouping,
          confirming: true,
        },
      };
    case "GROUPING_CONFIRMATION_SUCCEEDED":
      if (!state.batch.result) return state;
      return {
        context: state.context,
        draft: state.draft,
        batch: {
          ...state.batch,
          result: {
            ...state.batch.result,
            ...action.confirmation,
          },
        },
        grouping: {
          ...state.grouping,
          evidence: [],
        },
        quality: {
          ...state.quality,
          rejectedPageIds: [],
        },
        ordinary: state.ordinary,
        answerSheet: state.answerSheet,
        dictation: state.dictation,
      };
    case "GROUPING_CONFIRMATION_FINISHED":
      return {
        ...state,
        grouping: {
          ...state.grouping,
          confirming: false,
        },
      };
    case "GROUPING_EVIDENCE_STARTED":
      return {
        ...state,
        grouping: {
          ...state.grouping,
          loadingEvidence: true,
        },
      };
    case "GROUPING_EVIDENCE_SUCCEEDED":
      return {
        ...state,
        grouping: {
          ...state.grouping,
          evidence: action.evidence,
        },
      };
    case "GROUPING_EVIDENCE_FINISHED":
      return {
        ...state,
        grouping: {
          ...state.grouping,
          loadingEvidence: false,
        },
      };
    case "QUALITY_REJECTION_TOGGLED":
      if (state.batch.result?.qualityReviewCompleted) return state;
      return {
        ...state,
        quality: {
          ...state.quality,
          rejectedPageIds: state.quality.rejectedPageIds.includes(action.pageId)
            ? state.quality.rejectedPageIds.filter((value) => value !== action.pageId)
            : [...state.quality.rejectedPageIds, action.pageId],
        },
      };
    case "QUALITY_CONFIRMATION_STARTED":
      return {
        ...state,
        quality: {
          ...state.quality,
          confirming: true,
        },
      };
    case "QUALITY_CONFIRMATION_SUCCEEDED":
      if (!state.batch.result) return state;
      return {
        ...state,
        batch: {
          ...state.batch,
          result: {
            ...state.batch.result,
            ...action.confirmation,
          },
        },
      };
    case "QUALITY_CONFIRMATION_FINISHED":
      return {
        ...state,
        quality: {
          ...state.quality,
          confirming: false,
        },
      };
    case "RETAKE_STARTED":
      return {
        ...state,
        quality: {
          ...state.quality,
          retakingPageId: action.pageId,
        },
      };
    case "RETAKE_SUCCEEDED":
      if (!state.batch.result) return state;
      return {
        ...state,
        batch: {
          ...state.batch,
          result: {
            ...state.batch.result,
            mappedGroupCount: action.replacement.mappedGroupCount,
            rejectedGroupCount: action.replacement.rejectedGroupCount,
            nextAction: action.replacement.nextAction,
          },
        },
      };
    case "RETAKE_FINISHED":
      return {
        ...state,
        quality: {
          ...state.quality,
          retakingPageId: null,
        },
      };
    case "ORDINARY_PARTIAL_RESET":
      if (
        Object.keys(state.ordinary.runs).length === 0
        && state.ordinary.analyzingPageIds.length === 0
      ) return state;
      return {
        ...state,
        ordinary: {
          ...state.ordinary,
          runs: {},
          analyzingPageIds: [],
        },
      };
    case "ORDINARY_FULL_RESET":
      if (
        Object.keys(state.ordinary.runs).length === 0
        && state.ordinary.analyzingPageIds.length === 0
        && Object.keys(state.ordinary.confirmations).length === 0
        && state.ordinary.confirmingPageIds.length === 0
      ) return state;
      return {
        ...state,
        ordinary: {
          runs: {},
          analyzingPageIds: [],
          confirmations: {},
          confirmingPageIds: [],
        },
      };
    case "ORDINARY_ANALYSIS_STARTED": {
      const analyzingPageIds = [...state.ordinary.analyzingPageIds];
      for (const pageId of action.pageIds) {
        if (!analyzingPageIds.includes(pageId)) analyzingPageIds.push(pageId);
      }
      if (analyzingPageIds.length === state.ordinary.analyzingPageIds.length) return state;
      return {
        ...state,
        ordinary: {
          ...state.ordinary,
          analyzingPageIds,
        },
      };
    }
    case "ORDINARY_ANALYSIS_SUCCEEDED":
      if (state.ordinary.runs[action.pageId] === action.run) return state;
      return {
        ...state,
        ordinary: {
          ...state.ordinary,
          runs: {
            ...state.ordinary.runs,
            [action.pageId]: action.run,
          },
        },
      };
    case "ORDINARY_ANALYSIS_FINISHED":
      if (!state.ordinary.analyzingPageIds.includes(action.pageId)) return state;
      return {
        ...state,
        ordinary: {
          ...state.ordinary,
          analyzingPageIds: state.ordinary.analyzingPageIds.filter(
            (pageId) => pageId !== action.pageId,
          ),
        },
      };
    case "ORDINARY_CONFIRMATION_STARTED":
      if (
        action.pageIds.length === state.ordinary.confirmingPageIds.length
        && action.pageIds.every((pageId, index) => pageId === state.ordinary.confirmingPageIds[index])
      ) return state;
      return {
        ...state,
        ordinary: {
          ...state.ordinary,
          confirmingPageIds: action.pageIds,
        },
      };
    case "ORDINARY_CONFIRMATION_SUCCEEDED":
      if (state.ordinary.confirmations[action.pageId] === action.confirmation) return state;
      return {
        ...state,
        ordinary: {
          ...state.ordinary,
          confirmations: {
            ...state.ordinary.confirmations,
            [action.pageId]: action.confirmation,
          },
        },
      };
    case "ORDINARY_CONFIRMATION_FINISHED":
      if (!state.ordinary.confirmingPageIds.includes(action.pageId)) return state;
      return {
        ...state,
        ordinary: {
          ...state.ordinary,
          confirmingPageIds: state.ordinary.confirmingPageIds.filter(
            (pageId) => pageId !== action.pageId,
          ),
        },
      };
    case "ANSWER_SHEET_RESET":
      if (
        state.answerSheet.templateStatus === null
        && state.answerSheet.templateStatusLoaded === false
        && state.answerSheet.templateRun === null
        && Object.keys(state.answerSheet.pageResults).length === 0
        && Object.keys(state.answerSheet.pageFailures).length === 0
        && state.answerSheet.processingPageIds.length === 0
      ) return state;
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          templateStatus: null,
          templateStatusLoaded: false,
          templateRun: null,
          pageResults: {},
          pageFailures: {},
          processingPageIds: [],
        },
      };
    case "ANSWER_SHEET_STATUS_LOAD_STARTED":
      if (!state.answerSheet.templateStatusLoaded) return state;
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          templateStatusLoaded: false,
        },
      };
    case "ANSWER_SHEET_STATUS_LOAD_SUCCEEDED":
      if (
        state.answerSheet.templateStatus === action.status
        && state.answerSheet.templateStatusLoaded
      ) return state;
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          templateStatus: action.status,
          templateStatusLoaded: true,
        },
      };
    case "ANSWER_SHEET_STATUS_LOAD_FAILED":
      if (state.answerSheet.templateStatusLoaded) return state;
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          templateStatusLoaded: true,
        },
      };
    case "ANSWER_SHEET_TEMPLATE_ANALYSIS_STARTED":
      if (state.answerSheet.templateBusy && state.answerSheet.templateRun === null) return state;
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          templateBusy: true,
          templateRun: null,
        },
      };
    case "ANSWER_SHEET_TEMPLATE_ANALYSIS_SUCCEEDED":
      if (state.answerSheet.templateRun === action.run) return state;
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          templateRun: action.run,
        },
      };
    case "ANSWER_SHEET_TEMPLATE_CONFIRMATION_STARTED":
      if (state.answerSheet.templateBusy) return state;
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          templateBusy: true,
        },
      };
    case "ANSWER_SHEET_TEMPLATE_CONFIRMATION_SUCCEEDED":
      if (
        state.answerSheet.templateStatus === action.status
        && state.answerSheet.templateRun === null
      ) return state;
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          templateStatus: action.status,
          templateRun: null,
        },
      };
    case "ANSWER_SHEET_TEMPLATE_OPERATION_FINISHED":
      if (!state.answerSheet.templateBusy) return state;
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          templateBusy: false,
        },
      };
    case "ANSWER_SHEET_PAGES_STARTED": {
      const processingPageIds = [...state.answerSheet.processingPageIds];
      for (const pageId of action.pageIds) {
        if (!processingPageIds.includes(pageId)) processingPageIds.push(pageId);
      }
      if (processingPageIds.length === state.answerSheet.processingPageIds.length) return state;
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          processingPageIds,
        },
      };
    }
    case "ANSWER_SHEET_PAGE_SUCCEEDED": {
      const hadFailure = Object.prototype.hasOwnProperty.call(
        state.answerSheet.pageFailures,
        action.pageId,
      );
      if (state.answerSheet.pageResults[action.pageId] === action.result && !hadFailure) return state;
      const pageFailures = { ...state.answerSheet.pageFailures };
      delete pageFailures[action.pageId];
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          pageResults: {
            ...state.answerSheet.pageResults,
            [action.pageId]: action.result,
          },
          pageFailures,
        },
      };
    }
    case "ANSWER_SHEET_PAGE_FAILED":
      if (state.answerSheet.pageFailures[action.pageId] === action.error) return state;
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          pageFailures: {
            ...state.answerSheet.pageFailures,
            [action.pageId]: action.error,
          },
        },
      };
    case "ANSWER_SHEET_PAGE_FINISHED":
      if (!state.answerSheet.processingPageIds.includes(action.pageId)) return state;
      return {
        ...state,
        answerSheet: {
          ...state.answerSheet,
          processingPageIds: state.answerSheet.processingPageIds.filter(
            (pageId) => pageId !== action.pageId,
          ),
        },
      };
    case "DICTATION_RESET":
      if (
        state.dictation.templateStatus === null
        && state.dictation.templateStatusLoaded === false
        && state.dictation.templateRun === null
        && Object.keys(state.dictation.pageResults).length === 0
        && Object.keys(state.dictation.pageFailures).length === 0
        && state.dictation.processingPageIds.length === 0
      ) return state;
      return {
        ...state,
        dictation: {
          ...state.dictation,
          templateStatus: null,
          templateStatusLoaded: false,
          templateRun: null,
          pageResults: {},
          pageFailures: {},
          processingPageIds: [],
        },
      };
    case "DICTATION_STATUS_LOAD_STARTED":
      if (!state.dictation.templateStatusLoaded) return state;
      return {
        ...state,
        dictation: {
          ...state.dictation,
          templateStatusLoaded: false,
        },
      };
    case "DICTATION_STATUS_LOAD_SUCCEEDED":
      if (
        state.dictation.templateStatus === action.status
        && state.dictation.templateStatusLoaded
      ) return state;
      return {
        ...state,
        dictation: {
          ...state.dictation,
          templateStatus: action.status,
          templateStatusLoaded: true,
        },
      };
    case "DICTATION_STATUS_LOAD_FAILED":
      if (state.dictation.templateStatusLoaded) return state;
      return {
        ...state,
        dictation: {
          ...state.dictation,
          templateStatusLoaded: true,
        },
      };
    case "DICTATION_TEMPLATE_ANALYSIS_STARTED":
      if (state.dictation.templateBusy) return state;
      return {
        ...state,
        dictation: {
          ...state.dictation,
          templateBusy: true,
        },
      };
    case "DICTATION_TEMPLATE_ANALYSIS_SUCCEEDED":
      if (state.dictation.templateRun === action.run) return state;
      return {
        ...state,
        dictation: {
          ...state.dictation,
          templateRun: action.run,
        },
      };
    case "DICTATION_TEMPLATE_CONFIRMATION_STARTED":
      if (state.dictation.templateBusy) return state;
      return {
        ...state,
        dictation: {
          ...state.dictation,
          templateBusy: true,
        },
      };
    case "DICTATION_TEMPLATE_CONFIRMATION_SUCCEEDED":
      if (state.dictation.templateStatus === action.status) return state;
      return {
        ...state,
        dictation: {
          ...state.dictation,
          templateStatus: action.status,
        },
      };
    case "DICTATION_TEMPLATE_OPERATION_FINISHED":
      if (!state.dictation.templateBusy) return state;
      return {
        ...state,
        dictation: {
          ...state.dictation,
          templateBusy: false,
        },
      };
    case "DICTATION_PAGES_STARTED": {
      const processingPageIds = [...state.dictation.processingPageIds];
      for (const pageId of action.pageIds) {
        if (!processingPageIds.includes(pageId)) processingPageIds.push(pageId);
      }
      if (processingPageIds.length === state.dictation.processingPageIds.length) return state;
      return {
        ...state,
        dictation: {
          ...state.dictation,
          processingPageIds,
        },
      };
    }
    case "DICTATION_PAGE_SUCCEEDED": {
      const hadFailure = Object.prototype.hasOwnProperty.call(
        state.dictation.pageFailures,
        action.pageId,
      );
      if (state.dictation.pageResults[action.pageId] === action.result && !hadFailure) return state;
      const pageFailures = { ...state.dictation.pageFailures };
      delete pageFailures[action.pageId];
      return {
        ...state,
        dictation: {
          ...state.dictation,
          pageResults: {
            ...state.dictation.pageResults,
            [action.pageId]: action.result,
          },
          pageFailures,
        },
      };
    }
    case "DICTATION_PAGE_FAILED":
      if (state.dictation.pageFailures[action.pageId] === action.error) return state;
      return {
        ...state,
        dictation: {
          ...state.dictation,
          pageFailures: {
            ...state.dictation.pageFailures,
            [action.pageId]: action.error,
          },
        },
      };
    case "DICTATION_PAGE_FINISHED":
      if (!state.dictation.processingPageIds.includes(action.pageId)) return state;
      return {
        ...state,
        dictation: {
          ...state.dictation,
          processingPageIds: state.dictation.processingPageIds.filter(
            (pageId) => pageId !== action.pageId,
          ),
        },
      };
  }
}
