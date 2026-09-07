import type {
  DictationPageProcessingResult,
  DictationTemplateRunResult,
  DictationTemplateStatus,
  FixedIntakeResult,
  GroupedPageEvidence,
} from "../../api/exam";

interface FixedIntakeDictationProgressPanelProps {
  result: FixedIntakeResult;
  dictationTemplateStatusLoaded: boolean;
  dictationTemplateStatus: DictationTemplateStatus | null;
  dictationTemplateBusy: boolean;
  pickAndAnalyzeDictationTemplate: () => Promise<void>;
  dictationTemplateRun: DictationTemplateRunResult | null;
  confirmDictationTemplate: () => Promise<void>;
  answerSheetEligiblePages: GroupedPageEvidence["pages"];
  dictationProcessedValues: DictationPageProcessingResult[];
  dictationExactCount: number;
  dictationReviewCount: number;
  dictationFailureCount: number;
  dictationPendingCount: number;
  processingDictationPageIds: number[];
  processDictationPages: (
    evidence?: GroupedPageEvidence[],
    retryFailed?: boolean,
  ) => Promise<void>;
  groupingEvidence: GroupedPageEvidence[];
}

export function FixedIntakeDictationProgressPanel({
  result,
  dictationTemplateStatusLoaded,
  dictationTemplateStatus,
  dictationTemplateBusy,
  pickAndAnalyzeDictationTemplate,
  dictationTemplateRun,
  confirmDictationTemplate,
  answerSheetEligiblePages,
  dictationProcessedValues,
  dictationExactCount,
  dictationReviewCount,
  dictationFailureCount,
  dictationPendingCount,
  processingDictationPageIds,
  processDictationPages,
  groupingEvidence,
}: FixedIntakeDictationProgressPanelProps) {
  return (
            <>
            {result.qualityReviewCompleted && result.materialType === "dictation" && (
              <div className="intake-analysis-card">
                <div className="intake-quality-head">
                  <div>
                    <b>默写自动识别</b>
                    <span>第一次确认一张同版空白页；系统逐格读取学生原文，不用标准答案反向改字。</span>
                  </div>
                  <strong>
                    {!dictationTemplateStatusLoaded
                      ? "正在检查模板…"
                      : dictationTemplateStatus?.activeTemplate
                        ? `模板第 ${dictationTemplateStatus.activeTemplate.revision} 版`
                        : "还差空白页"}
                  </strong>
                </div>
                {!dictationTemplateStatus?.activeTemplate ? (
                  <div className="intake-analysis-actions vertical">
                    <span className="muted">请上传这次默写的未作答空白页。系统只定位每题书写框，答案沿用作业中已确认的版本。</span>
                    <button disabled={dictationTemplateBusy || !dictationTemplateStatusLoaded} onClick={() => void pickAndAnalyzeDictationTemplate()}>
                      {dictationTemplateBusy ? "正在识别空白页…" : "选择一张默写空白页"}
                    </button>
                    {dictationTemplateRun?.status === "failed" && (
                      <div className="intake-analysis-issues">
                        <span>{dictationTemplateRun.failure?.safe_message}</span>
                      </div>
                    )}
                    {dictationTemplateRun?.output && (
                      <>
                        <div className="intake-analysis-summary">
                          <span>匹配题区 {dictationTemplateRun.output.regions.length}</span>
                          <span>可信度 {Math.round(dictationTemplateRun.output.confidence * 100)}%</span>
                          <span className={dictationTemplateRun.output.state === "ready" ? "ready" : "review"}>
                            {dictationTemplateRun.output.state === "ready" ? "可以确认" : "需要换图或复核"}
                          </span>
                        </div>
                        {dictationTemplateRun.output.issue_codes.length > 0 && (
                          <div className="intake-analysis-issues">
                            {dictationTemplateRun.output.issue_codes.slice(0, 4).map((code) => <span key={code}>{code}</span>)}
                          </div>
                        )}
                        {dictationTemplateRun.output.state === "ready" && (
                          <button disabled={dictationTemplateBusy} onClick={() => void confirmDictationTemplate()}>
                            {dictationTemplateBusy ? "正在确认并处理全班…" : `确认这张空白页，识别 ${answerSheetEligiblePages.length} 份默写`}
                          </button>
                        )}
                      </>
                    )}
                  </div>
                ) : (
                  <>
                    <div className="intake-analysis-summary">
                      <span>已处理 {dictationProcessedValues.length}/{answerSheetEligiblePages.length} 页</span>
                      <span className="ready">精确命中 {dictationExactCount}</span>
                      <span className="review">需老师看 {dictationReviewCount}</span>
                      <span className="blocked">失败页 {dictationFailureCount}</span>
                      <span>待处理 {dictationPendingCount}</span>
                    </div>
                    <div className="intake-analysis-actions">
                      {dictationPendingCount > 0 && (
                        <button disabled={processingDictationPageIds.length > 0} onClick={() => void processDictationPages()}>
                          {processingDictationPageIds.length ? `正在处理 ${processingDictationPageIds.length} 页…` : `继续识别 ${dictationPendingCount} 页`}
                        </button>
                      )}
                      {dictationFailureCount > 0 && (
                        <button className="secondary" disabled={processingDictationPageIds.length > 0} onClick={() => void processDictationPages(groupingEvidence, true)}>
                          重试 {dictationFailureCount} 张失败默写
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
