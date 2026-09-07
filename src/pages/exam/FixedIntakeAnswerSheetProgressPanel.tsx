import type {
  AnswerSheetPageProcessingResult,
  AnswerSheetTemplateRunResult,
  AnswerSheetTemplateStatus,
  FixedIntakeResult,
  GroupedPageEvidence,
} from "../../api/exam";

interface FixedIntakeAnswerSheetProgressPanelProps {
  result: FixedIntakeResult;
  answerSheetTemplateStatusLoaded: boolean;
  answerSheetTemplateSetReady: boolean;
  answerSheetTemplateStatus: AnswerSheetTemplateStatus | null;
  answerSheetTemplateTargetPageNo: number;
  answerSheetTemplateBusy: boolean;
  pickAndAnalyzeAnswerSheetTemplate: () => Promise<void>;
  answerSheetTemplateRun: AnswerSheetTemplateRunResult | null;
  confirmAnswerSheetTemplate: () => Promise<void>;
  answerSheetEligiblePages: GroupedPageEvidence["pages"];
  answerSheetProcessedValues: AnswerSheetPageProcessingResult[];
  answerSheetReadyObservationCount: number;
  answerSheetReviewObservationCount: number;
  answerSheetSubjectiveRecognizedCount: number;
  answerSheetSubjectiveReviewCount: number;
  answerSheetFailureCount: number;
  answerSheetPendingCount: number;
  processingAnswerSheetPageIds: number[];
  processAnswerSheetPages: (
    evidence?: GroupedPageEvidence[],
    retryFailed?: boolean,
  ) => Promise<void>;
  groupingEvidence: GroupedPageEvidence[];
}

export function FixedIntakeAnswerSheetProgressPanel({
  result,
  answerSheetTemplateStatusLoaded,
  answerSheetTemplateSetReady,
  answerSheetTemplateStatus,
  answerSheetTemplateTargetPageNo,
  answerSheetTemplateBusy,
  pickAndAnalyzeAnswerSheetTemplate,
  answerSheetTemplateRun,
  confirmAnswerSheetTemplate,
  answerSheetEligiblePages,
  answerSheetProcessedValues,
  answerSheetReadyObservationCount,
  answerSheetReviewObservationCount,
  answerSheetSubjectiveRecognizedCount,
  answerSheetSubjectiveReviewCount,
  answerSheetFailureCount,
  answerSheetPendingCount,
  processingAnswerSheetPageIds,
  processAnswerSheetPages,
  groupingEvidence,
}: FixedIntakeAnswerSheetProgressPanelProps) {
  return (
            <>
            {result.qualityReviewCompleted && result.materialType === "answer_sheet" && (
              <div className="intake-analysis-card">
                <div className="intake-quality-head">
                  <div>
                    <b>答题卡自动识别</b>
                    <span>按页确认整套空白答题卡；有定位点就按定位点校正，没有定位点就识别纸张边缘，客观格本机识别，主观区单独送手写识别。</span>
                  </div>
                  <strong>
                    {!answerSheetTemplateStatusLoaded
                      ? "正在检查模板…"
                      : answerSheetTemplateSetReady
                        ? `整套 ${answerSheetTemplateStatus?.templateSet.pages.length ?? 0} 页已确认`
                        : `还差第 ${answerSheetTemplateTargetPageNo || 1} 页空白卡`}
                  </strong>
                </div>
                {!answerSheetTemplateSetReady ? (
                  <div className="intake-analysis-actions vertical">
                    <span className="muted">
                      请上传第 {answerSheetTemplateTargetPageNo || 1} 页空白卡。系统只建立题号、涂点格和主观作答区，不把它当学生作答。
                    </span>
                    <button disabled={answerSheetTemplateBusy || !answerSheetTemplateStatusLoaded} onClick={() => void pickAndAnalyzeAnswerSheetTemplate()}>
                      {answerSheetTemplateBusy ? "正在识别空白卡…" : `选择第 ${answerSheetTemplateTargetPageNo || 1} 页空白卡`}
                    </button>
                    {answerSheetTemplateRun?.status === "failed" && (
                      <div className="intake-analysis-issues">
                        <span>{answerSheetTemplateRun.failure?.safe_message}</span>
                      </div>
                    )}
                    {answerSheetTemplateRun?.output && (
                      <>
                        <div className="intake-analysis-summary">
                          <span>
                            {answerSheetTemplateRun.output.alignment_mode === "page_contour"
                              ? "纸张边缘定位"
                              : `印刷定位点 ${answerSheetTemplateRun.output.anchors.length}/4`}
                          </span>
                          <span>客观格 {answerSheetTemplateRun.output.items.length}</span>
                          <span>主观区 {answerSheetTemplateRun.output.subjective_regions.length}</span>
                          <span>可信度 {Math.round(answerSheetTemplateRun.output.confidence * 100)}%</span>
                          <span className={answerSheetTemplateRun.output.state === "ready" ? "ready" : "review"}>
                            {answerSheetTemplateRun.output.state === "ready" ? "可以确认" : "需要换图或复核"}
                          </span>
                        </div>
                        {answerSheetTemplateRun.output.issue_codes.length > 0 && (
                          <div className="intake-analysis-issues">
                            {answerSheetTemplateRun.output.issue_codes.slice(0, 4).map((code) => <span key={code}>{code}</span>)}
                          </div>
                        )}
                        {answerSheetTemplateRun.output.state === "ready" && (
                          <button disabled={answerSheetTemplateBusy} onClick={() => void confirmAnswerSheetTemplate()}>
                            {answerSheetTemplateBusy
                              ? "正在确认…"
                              : `确认第 ${answerSheetTemplateTargetPageNo || 1} 页${(answerSheetTemplateStatus?.templateSet.pages.filter((page) => !page.ready).length ?? 0) <= 1 ? `，开始处理 ${answerSheetEligiblePages.length} 张学生卡` : "，继续下一页"}`}
                          </button>
                        )}
                      </>
                    )}
                  </div>
                ) : (
                  <>
                    <div className="intake-analysis-summary">
                      <span>已处理 {answerSheetProcessedValues.length}/{answerSheetEligiblePages.length} 页</span>
                      <span className="ready">清晰题区 {answerSheetReadyObservationCount}</span>
                      <span className="review">需老师看 {answerSheetReviewObservationCount}</span>
                      <span className="ready">主观区已转写 {answerSheetSubjectiveRecognizedCount}</span>
                      <span className="review">主观区需老师看 {answerSheetSubjectiveReviewCount}</span>
                      <span className="blocked">失败页 {answerSheetFailureCount}</span>
                      <span>待处理 {answerSheetPendingCount}</span>
                    </div>
                    <div className="intake-analysis-actions">
                      {answerSheetPendingCount > 0 && (
                        <button disabled={processingAnswerSheetPageIds.length > 0} onClick={() => void processAnswerSheetPages()}>
                          {processingAnswerSheetPageIds.length ? `正在处理 ${processingAnswerSheetPageIds.length} 页…` : `继续识别 ${answerSheetPendingCount} 页`}
                        </button>
                      )}
                      {answerSheetFailureCount > 0 && (
                        <button className="secondary" disabled={processingAnswerSheetPageIds.length > 0} onClick={() => void processAnswerSheetPages(groupingEvidence, true)}>
                          重试 {answerSheetFailureCount} 张失败卡
                        </button>
                      )}
                    </div>
                  </>
                )}
              </div>
            )}
            </>
  );
}
