import { FixedIntakeOption } from "../../api/exam";
import { FixedIntakeAnswerSourcePanel } from "./FixedIntakeAnswerSourcePanel";
import { FixedIntakeAnswerSheetProgressPanel } from "./FixedIntakeAnswerSheetProgressPanel";
import { FixedIntakeDictationProgressPanel } from "./FixedIntakeDictationProgressPanel";
import { FixedIntakeGroupingPanel } from "./FixedIntakeGroupingPanel";
import { FixedIntakeOrdinaryProgressPanel } from "./FixedIntakeOrdinaryProgressPanel";
import { FixedIntakeQualityPanel } from "./FixedIntakeQualityPanel";
import { FixedIntakeResultSummaryPanel } from "./FixedIntakeResultSummaryPanel";
import { FixedIntakeUploadForm } from "./FixedIntakeUploadForm";
import {
  NEW_RUBRIC_POINT,
  rubricPointMappingsReady,
} from "./fixedIntakeState";
import { useFixedIntakeController } from "./useFixedIntakeController";

const MATERIAL_TYPE_LABEL: Record<string, string> = {
  ordinary_paper: "普通试卷",
  answer_sheet: "答题卡",
  dictation: "默写",
  unknown: "待确认",
};

export function FixedIntakeTab({
  options,
  onOpenReview,
  onDone,
  onError,
}: {
  options: FixedIntakeOption[];
  onOpenReview: (tab: "objective" | "subjective" | "dictation") => void;
  onDone: (message: string) => void;
  onError: (message: string) => void;
}) {
  const { view, actions } = useFixedIntakeController(
    options,
    onDone,
    onError,
  );
  const {
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
    analyzingPageIds,
    confirmingOrdinaryPageIds,
    answerSheetTemplateStatus,
    answerSheetTemplateStatusLoaded,
    answerSheetTemplateRun,
    answerSheetTemplateBusy,
    processingAnswerSheetPageIds,
    dictationTemplateStatus,
    dictationTemplateStatusLoaded,
    dictationTemplateRun,
    dictationTemplateBusy,
    processingDictationPageIds,
    answerSheetEligiblePages,
    answerSheetTemplateTargetPageNo,
    answerSheetTemplateSetReady,
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
  } = view;
  const {
    selectClass,
    selectAssessment,
    changeAnswerText,
    changeExpectedPages,
    clearAnswerSource,
    pickStudentPapers,
    pickAnswer,
    submit,
    analyzeAnswerSource,
    confirmMatchingAnswerSource,
    keepCurrentBoundAnswers,
    adoptAnswerSourceAsNewVersion,
    changeRubricPointMapping,
    confirmGrouping,
    toggleRejectedPage,
    confirmGroupingQuality,
    replaceRejectedPage,
    toggleAbsentStudent,
    changeGroupingStart,
    confirmMaterialType,
    analyzeOrdinaryPages,
    confirmReadyOrdinaryPages,
    pickAndAnalyzeAnswerSheetTemplate,
    confirmAnswerSheetTemplate,
    processAnswerSheetPages,
    pickAndAnalyzeDictationTemplate,
    confirmDictationTemplate,
    processDictationPages,
  } = actions;

  if (!options.length) {
    return (
      <div className="exam-card objective-empty">
        <b>还没有可上传的固定卷作业</b>
        <span className="muted">需先准备一份已确认并带答案/评分点的作业版本；上传入口不会临时拼出不可追溯的题目或答案。</span>
      </div>
    );
  }

  return (
    <div className="intake-layout">
      <FixedIntakeUploadForm
        classOptions={classOptions}
        classId={classId}
        selectClass={selectClass}
        assessmentOptions={assessmentOptions}
        assessmentVersionId={assessmentVersionId}
        selectAssessment={selectAssessment}
        pickStudentPapers={pickStudentPapers}
        studentPaths={studentPaths}
        pageCycle={pageCycle}
        expectedPages={expectedPages}
        changeExpectedPages={changeExpectedPages}
        pickAnswer={pickAnswer}
        answerPath={answerPath}
        answerText={answerText}
        changeAnswerText={changeAnswerText}
        clearAnswerSource={clearAnswerSource}
        busy={busy}
        submit={submit}
      />

      <aside className="exam-card intake-result-card">
        <div className="exam-card-head">
          <b>本批处理状态</b>
          {result && <span className={`tag intake-route ${result.route}`}>{routeLabel}</span>}
        </div>
        {!result ? (
          <div className="empty-state">上传后这里只显示三种结果和一个下一步，不让老师处理技术参数。</div>
        ) : (
          <>
            <div className="intake-import-summary">
              <b>已归档 {result.studentDocumentCount} 份学生卷，共 {result.studentPageCount} 页</b>
              <span>{result.answerDocumentCount ? "答案资料已归档，等待识别或确认" : "沿用作业已确认答案"}</span>
              <span>已按文件名自然顺序整理，并用拍摄/文件时间交叉核对 · 顺序可信度 {Math.round(result.orderConfidence * 100)}%</span>
              <span>
                每人 {result.expectedPagesPerAttempt} 页
                {result.pageCycleSource === "visual_repeating_layout_v1"
                  ? ` · 重复版式识别 ${Math.round(result.pageCycleConfidence * 100)}%`
                  : " · 已按老师填写页数排列"}
              </span>
              <span>资料类型：{MATERIAL_TYPE_LABEL[result.materialType] || result.materialType} · 预计 {result.studentGroupCount} 名学生</span>
            </div>
            {result.answerDocumentCount > 0 && (
              <FixedIntakeAnswerSourcePanel
                result={result}
                answerSourceBusy={answerSourceBusy}
                answerSourceAnalysis={answerSourceAnalysis}
                answerSourceError={answerSourceError}
                rubricPointMappings={rubricPointMappings}
                changeRubricPointMapping={changeRubricPointMapping}
                NEW_RUBRIC_POINT={NEW_RUBRIC_POINT}
                rubricPointMappingsReady={rubricPointMappingsReady}
                confirmMatchingAnswerSource={confirmMatchingAnswerSource}
                adoptAnswerSourceAsNewVersion={adoptAnswerSourceAsNewVersion}
                keepCurrentBoundAnswers={keepCurrentBoundAnswers}
                analyzeAnswerSource={analyzeAnswerSource}
              />
            )}
            <FixedIntakeGroupingPanel
              result={result}
              confirmingType={confirmingType}
              confirmMaterialType={confirmMaterialType}
              groupingStartNo={groupingStartNo}
              changeGroupingStart={changeGroupingStart}
              absentStudentNos={absentStudentNos}
              groupingAbsenceCandidates={groupingAbsenceCandidates}
              toggleAbsentStudent={toggleAbsentStudent}
              confirmingGrouping={confirmingGrouping}
              confirmGrouping={confirmGrouping}
            />
            <FixedIntakeQualityPanel
              result={result}
              rejectedPageIds={rejectedPageIds}
              loadingEvidence={loadingEvidence}
              groupingEvidence={groupingEvidence}
              toggleRejectedPage={toggleRejectedPage}
              confirmingQuality={confirmingQuality}
              confirmGroupingQuality={confirmGroupingQuality}
              retakingPageId={retakingPageId}
              replaceRejectedPage={replaceRejectedPage}
            />
            <FixedIntakeOrdinaryProgressPanel
              result={result}
              analyzingPageIds={analyzingPageIds}
              ordinaryRunValues={ordinaryRunValues}
              ordinaryEligiblePageCount={ordinaryEligiblePageCount}
              ordinaryUnconfirmedReadyCount={ordinaryUnconfirmedReadyCount}
              ordinaryConfirmedCount={ordinaryConfirmedCount}
              ordinaryReviewCount={ordinaryReviewCount}
              ordinaryBlockedCount={ordinaryBlockedCount}
              ordinaryPendingCount={ordinaryPendingCount}
              confirmingOrdinaryPageIds={confirmingOrdinaryPageIds}
              confirmReadyOrdinaryPages={confirmReadyOrdinaryPages}
              analyzeOrdinaryPages={analyzeOrdinaryPages}
              groupingEvidence={groupingEvidence}
            />
            <FixedIntakeAnswerSheetProgressPanel
              result={result}
              answerSheetTemplateStatusLoaded={answerSheetTemplateStatusLoaded}
              answerSheetTemplateSetReady={answerSheetTemplateSetReady}
              answerSheetTemplateStatus={answerSheetTemplateStatus}
              answerSheetTemplateTargetPageNo={answerSheetTemplateTargetPageNo}
              answerSheetTemplateBusy={answerSheetTemplateBusy}
              pickAndAnalyzeAnswerSheetTemplate={pickAndAnalyzeAnswerSheetTemplate}
              answerSheetTemplateRun={answerSheetTemplateRun}
              confirmAnswerSheetTemplate={confirmAnswerSheetTemplate}
              answerSheetEligiblePages={answerSheetEligiblePages}
              answerSheetProcessedValues={answerSheetProcessedValues}
              answerSheetReadyObservationCount={answerSheetReadyObservationCount}
              answerSheetReviewObservationCount={answerSheetReviewObservationCount}
              answerSheetSubjectiveRecognizedCount={answerSheetSubjectiveRecognizedCount}
              answerSheetSubjectiveReviewCount={answerSheetSubjectiveReviewCount}
              answerSheetFailureCount={answerSheetFailureCount}
              answerSheetPendingCount={answerSheetPendingCount}
              processingAnswerSheetPageIds={processingAnswerSheetPageIds}
              processAnswerSheetPages={processAnswerSheetPages}
              groupingEvidence={groupingEvidence}
            />
            <FixedIntakeDictationProgressPanel
              result={result}
              dictationTemplateStatusLoaded={dictationTemplateStatusLoaded}
              dictationTemplateStatus={dictationTemplateStatus}
              dictationTemplateBusy={dictationTemplateBusy}
              pickAndAnalyzeDictationTemplate={pickAndAnalyzeDictationTemplate}
              dictationTemplateRun={dictationTemplateRun}
              confirmDictationTemplate={confirmDictationTemplate}
              answerSheetEligiblePages={answerSheetEligiblePages}
              dictationProcessedValues={dictationProcessedValues}
              dictationExactCount={dictationExactCount}
              dictationReviewCount={dictationReviewCount}
              dictationFailureCount={dictationFailureCount}
              dictationPendingCount={dictationPendingCount}
              processingDictationPageIds={processingDictationPageIds}
              processDictationPages={processDictationPages}
              groupingEvidence={groupingEvidence}
            />
            <FixedIntakeResultSummaryPanel
              result={result}
              answerSheetSubjectiveRegionCount={answerSheetSubjectiveRegionCount}
              onOpenReview={onOpenReview}
            />
          </>
        )}
      </aside>
    </div>
  );
}
