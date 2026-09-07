import type { FixedIntakeOption } from "../../api/exam.ts";
import {
  fixedIntakeMaterialScopeKey,
  type FixedIntakeState,
} from "./fixedIntakeRootState.ts";

export function fixedIntakeClassOptions(options: FixedIntakeOption[]) {
  const unique = new Map<number, string>();
  options.forEach((option) => unique.set(option.classId, option.className));
  return Array.from(unique, ([id, name]) => ({ id, name }));
}

export function fixedIntakeAssessmentOptions(
  options: FixedIntakeOption[],
  classId: number,
) {
  return options.filter((option) => option.classId === classId);
}

export function createFixedIntakeViewModel(
  options: FixedIntakeOption[],
  state: FixedIntakeState,
) {
  const classOptions = fixedIntakeClassOptions(options);
  const assessmentOptions = fixedIntakeAssessmentOptions(options, state.context.classId);
  const {
    analysis: answerSourceAnalysis,
    busy: answerSourceBusy,
    error: answerSourceError,
    rubricPointMappings,
  } = state.answerSource;
  const { classId, assessmentVersionId } = state.context;
  const { studentPaths, answerPath, answerText, expectedPages, pageCycle } = state.draft;
  const { requestKey, busy, result, confirmingType } = state.batch;
  const {
    startNo: groupingStartNo,
    absentStudentNos,
    confirming: confirmingGrouping,
    evidence: groupingEvidence,
    loadingEvidence,
  } = state.grouping;
  const {
    rejectedPageIds,
    confirming: confirmingQuality,
    retakingPageId,
  } = state.quality;
  const {
    runs: ordinaryPaperRuns,
    analyzingPageIds,
    confirmations: ordinaryConfirmations,
    confirmingPageIds: confirmingOrdinaryPageIds,
  } = state.ordinary;
  const {
    templateStatus: answerSheetTemplateStatus,
    templateStatusLoaded: answerSheetTemplateStatusLoaded,
    templateRun: answerSheetTemplateRun,
    templateBusy: answerSheetTemplateBusy,
    pageResults: answerSheetPageResults,
    pageFailures: answerSheetPageFailures,
    processingPageIds: processingAnswerSheetPageIds,
  } = state.answerSheet;
  const {
    templateStatus: dictationTemplateStatus,
    templateStatusLoaded: dictationTemplateStatusLoaded,
    templateRun: dictationTemplateRun,
    templateBusy: dictationTemplateBusy,
    pageResults: dictationPageResults,
    pageFailures: dictationPageFailures,
    processingPageIds: processingDictationPageIds,
  } = state.dictation;

  const answerSheetEligiblePages = groupingEvidence.flatMap((group) => group.pages).filter((page) =>
    page.qualityResult === "pass" && page.matchDecision === "teacher_confirmed"
  );
  const answerSheetReferencePageId = answerSheetEligiblePages[0]?.pageId ?? 0;
  const answerSheetTemplateTargetPageNo = answerSheetTemplateStatus?.templateSet.pages
    .find((page) => !page.ready)?.page_no
    ?? answerSheetEligiblePages[0]?.pageNo
    ?? 0;
  const answerSheetTemplateTargetPageId = answerSheetEligiblePages
    .find((page) => page.pageNo === answerSheetTemplateTargetPageNo)?.pageId
    ?? answerSheetReferencePageId;
  const answerSheetTemplateSetReady = answerSheetTemplateStatus?.templateSet.ready === true;
  const answerSheetTemplateScopeKey = fixedIntakeMaterialScopeKey(
    state,
    "answer_sheet",
  ) ?? "";
  const dictationReferencePageId = answerSheetEligiblePages[0]?.pageId ?? 0;
  const dictationTemplateScopeKey = fixedIntakeMaterialScopeKey(
    state,
    "dictation",
  ) ?? "";

  const routeLabel = result?.route === "ready_for_batch_confirm"
    ? "可批量确认"
    : result?.route === "review_required"
      ? "需老师复核"
      : "暂时受阻";
  const groupingStartIndex = result?.groupingRoster.findIndex(
    (student) => student.studentNo === groupingStartNo,
  ) ?? -1;
  const groupingAbsenceCandidates = groupingStartIndex >= 0
    ? result?.groupingRoster.slice(groupingStartIndex + 1) ?? []
    : [];
  const ordinaryEligiblePageCount = answerSheetEligiblePages.length;
  const ordinaryRunValues = Object.values(ordinaryPaperRuns);
  const ordinaryUnconfirmedReadyCount = ordinaryRunValues.filter((run) =>
    run.output?.state === "ready" && !ordinaryConfirmations[run.output.page_id]
  ).length;
  const ordinaryConfirmedCount = Object.keys(ordinaryConfirmations).length;
  const ordinaryReviewCount = ordinaryRunValues.filter((run) =>
    run.output?.state === "needs_review"
  ).length;
  const ordinaryBlockedCount = ordinaryRunValues.filter((run) =>
    run.status === "failed" || run.output?.state === "blocked"
  ).length;
  const ordinaryPendingCount = Math.max(
    0,
    ordinaryEligiblePageCount - ordinaryRunValues.length,
  );
  const answerSheetProcessedValues = Object.values(answerSheetPageResults);
  const answerSheetObservations = answerSheetProcessedValues.flatMap(
    (value) => value.observations,
  );
  const answerSheetReadyObservationCount = answerSheetObservations.filter((value) =>
    value.observation.result_state === "recognized" && value.suggestion.batch_eligible
  ).length;
  const answerSheetReviewObservationCount = answerSheetObservations.length
    - answerSheetReadyObservationCount;
  const answerSheetSubjectiveRegionCount = answerSheetProcessedValues.reduce(
    (total, value) => total + value.subjectiveRegions.length,
    0,
  );
  const answerSheetSubjectiveTranscriptions = answerSheetProcessedValues.flatMap(
    (value) => value.subjectiveTranscriptions,
  );
  const answerSheetSubjectiveRecognizedCount = answerSheetSubjectiveTranscriptions.filter(
    (value) => value.result_state === "recognized",
  ).length;
  const answerSheetSubjectiveReviewCount = answerSheetSubjectiveRegionCount
    - answerSheetSubjectiveRecognizedCount;
  const answerSheetFailureCount = Object.keys(answerSheetPageFailures).length;
  const answerSheetPendingCount = Math.max(
    0,
    answerSheetEligiblePages.length - answerSheetProcessedValues.length - answerSheetFailureCount,
  );
  const dictationProcessedValues = Object.values(dictationPageResults);
  const dictationTranscriptions = dictationProcessedValues.flatMap(
    (value) => value.transcriptions,
  );
  const dictationExactCount = dictationTranscriptions.filter((value) =>
    value.observation.result === "exact"
    || value.observation.result === "accepted_variant"
  ).length;
  const dictationReviewCount = dictationTranscriptions.length - dictationExactCount;
  const dictationFailureCount = Object.keys(dictationPageFailures).length;
  const dictationPendingCount = Math.max(
    0,
    answerSheetEligiblePages.length - dictationProcessedValues.length - dictationFailureCount,
  );

  return {
    classOptions,
    assessmentOptions,
    answerSourceAnalysis,
    answerSourceBusy,
    answerSourceError,
    rubricPointMappings,
    classId,
    assessmentVersionId,
    studentPaths,
    answerPath,
    answerText,
    expectedPages,
    pageCycle,
    requestKey,
    busy,
    result,
    confirmingType,
    groupingStartNo,
    absentStudentNos,
    confirmingGrouping,
    groupingEvidence,
    loadingEvidence,
    rejectedPageIds,
    confirmingQuality,
    retakingPageId,
    ordinaryPaperRuns,
    analyzingPageIds,
    ordinaryConfirmations,
    confirmingOrdinaryPageIds,
    answerSheetTemplateStatus,
    answerSheetTemplateStatusLoaded,
    answerSheetTemplateRun,
    answerSheetTemplateBusy,
    answerSheetPageResults,
    answerSheetPageFailures,
    processingAnswerSheetPageIds,
    dictationTemplateStatus,
    dictationTemplateStatusLoaded,
    dictationTemplateRun,
    dictationTemplateBusy,
    dictationPageResults,
    dictationPageFailures,
    processingDictationPageIds,
    answerSheetEligiblePages,
    answerSheetReferencePageId,
    answerSheetTemplateTargetPageNo,
    answerSheetTemplateTargetPageId,
    answerSheetTemplateSetReady,
    answerSheetTemplateScopeKey,
    dictationReferencePageId,
    dictationTemplateScopeKey,
    routeLabel,
    groupingAbsenceCandidates,
    ordinaryEligiblePageCount,
    ordinaryRunValues,
    ordinaryUnconfirmedReadyCount,
    ordinaryConfirmedCount,
    ordinaryReviewCount,
    ordinaryBlockedCount,
    ordinaryPendingCount,
    answerSheetProcessedValues,
    answerSheetReadyObservationCount,
    answerSheetReviewObservationCount,
    answerSheetSubjectiveRegionCount,
    answerSheetSubjectiveRecognizedCount,
    answerSheetSubjectiveReviewCount,
    answerSheetFailureCount,
    answerSheetPendingCount,
    dictationProcessedValues,
    dictationExactCount,
    dictationReviewCount,
    dictationFailureCount,
    dictationPendingCount,
  };
}

export type FixedIntakeViewModel = ReturnType<typeof createFixedIntakeViewModel>;
