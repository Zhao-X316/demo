import type {
  FixedIntakeResult,
  GroupedPageEvidence,
  OrdinaryPaperRunResult,
} from "../../api/exam";

interface FixedIntakeOrdinaryProgressPanelProps {
  result: FixedIntakeResult;
  analyzingPageIds: number[];
  ordinaryRunValues: OrdinaryPaperRunResult[];
  ordinaryEligiblePageCount: number;
  ordinaryUnconfirmedReadyCount: number;
  ordinaryConfirmedCount: number;
  ordinaryReviewCount: number;
  ordinaryBlockedCount: number;
  ordinaryPendingCount: number;
  confirmingOrdinaryPageIds: number[];
  confirmReadyOrdinaryPages: () => Promise<void>;
  analyzeOrdinaryPages: (
    evidence?: GroupedPageEvidence[],
    retryFailed?: boolean,
  ) => Promise<void>;
  groupingEvidence: GroupedPageEvidence[];
}

export function FixedIntakeOrdinaryProgressPanel({
  result,
  analyzingPageIds,
  ordinaryRunValues,
  ordinaryEligiblePageCount,
  ordinaryUnconfirmedReadyCount,
  ordinaryConfirmedCount,
  ordinaryReviewCount,
  ordinaryBlockedCount,
  ordinaryPendingCount,
  confirmingOrdinaryPageIds,
  confirmReadyOrdinaryPages,
  analyzeOrdinaryPages,
  groupingEvidence,
}: FixedIntakeOrdinaryProgressPanelProps) {
  return (
            <>
            {result.qualityReviewCompleted && result.materialType === "ordinary_paper" && (
              <div className="intake-analysis-card">
                <div className="intake-quality-head">
                  <div>
                    <b>普通试卷正在自动识别</b>
                    <span>逐页检查版面、配准和题区；有分歧的页面留给老师，不会在这里自动计分。</span>
                  </div>
                  <strong>{analyzingPageIds.length ? `正在处理 ${analyzingPageIds.length} 页` : `已处理 ${ordinaryRunValues.length}/${ordinaryEligiblePageCount} 页`}</strong>
                </div>
                <div className="intake-analysis-summary">
                  <span className="ready">可确认 {ordinaryUnconfirmedReadyCount}</span>
                  <span>已确认 {ordinaryConfirmedCount}</span>
                  <span className="review">需复核 {ordinaryReviewCount}</span>
                  <span className="blocked">受阻 {ordinaryBlockedCount}</span>
                  <span>待处理 {ordinaryPendingCount}</span>
                </div>
                {ordinaryRunValues.some((run) => run.failure) && (
                  <div className="intake-analysis-issues">
                    {ordinaryRunValues.filter((run) => run.failure).slice(0, 3).map((run) => (
                      <span key={run.ai_run_id}>{run.failure?.safe_message}</span>
                    ))}
                  </div>
                )}
                <div className="intake-analysis-actions">
                  {ordinaryUnconfirmedReadyCount > 0 && (
                    <button
                      disabled={analyzingPageIds.length > 0 || confirmingOrdinaryPageIds.length > 0}
                      onClick={() => void confirmReadyOrdinaryPages()}
                    >
                      {confirmingOrdinaryPageIds.length
                        ? `正在确认并识别 ${confirmingOrdinaryPageIds.length} 页…`
                        : `确认 ${ordinaryUnconfirmedReadyCount} 页并开始批改`}
                    </button>
                  )}
                  {ordinaryPendingCount > 0 && (
                    <button disabled={analyzingPageIds.length > 0} onClick={() => void analyzeOrdinaryPages()}>
                      {analyzingPageIds.length ? "正在自动识别…" : `继续识别 ${ordinaryPendingCount} 页`}
                    </button>
                  )}
                  {ordinaryRunValues.some((run) => run.status === "failed" && run.failure?.retryable) && (
                    <button className="secondary" disabled={analyzingPageIds.length > 0} onClick={() => void analyzeOrdinaryPages(groupingEvidence, true)}>
                      重试可恢复页面
                    </button>
                  )}
                </div>
              </div>
            )}
            </>
  );
}
