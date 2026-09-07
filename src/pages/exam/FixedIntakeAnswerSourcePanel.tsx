import type {
  AnswerSourceAnalysisResult,
  AnswerSourceReviewSummary,
  FixedIntakeResult,
} from "../../api/exam";
import { answerJsonLabel, shortAnswerRubricPoints } from "./examPure";

interface FixedIntakeAnswerSourcePanelProps {
  result: FixedIntakeResult;
  answerSourceBusy: boolean;
  answerSourceAnalysis: AnswerSourceAnalysisResult | null;
  answerSourceError: string;
  rubricPointMappings: Record<string, string>;
  changeRubricPointMapping: (
    assessmentItemId: number,
    orderIndex: number,
    stableId: string,
  ) => void;
  NEW_RUBRIC_POINT: string;
  rubricPointMappingsReady: (
    review: AnswerSourceReviewSummary,
    mappings: Record<string, string>,
  ) => boolean;
  confirmMatchingAnswerSource: () => Promise<void>;
  adoptAnswerSourceAsNewVersion: () => Promise<void>;
  keepCurrentBoundAnswers: () => Promise<void>;
  analyzeAnswerSource: (batchId: number, retry?: boolean) => Promise<void>;
}

export function FixedIntakeAnswerSourcePanel({
  result,
  answerSourceBusy,
  answerSourceAnalysis,
  answerSourceError,
  rubricPointMappings,
  changeRubricPointMapping,
  NEW_RUBRIC_POINT,
  rubricPointMappingsReady,
  confirmMatchingAnswerSource,
  adoptAnswerSourceAsNewVersion,
  keepCurrentBoundAnswers,
  analyzeAnswerSource,
}: FixedIntakeAnswerSourcePanelProps) {
  return (
              <div className="intake-analysis-card">
                <div className="intake-quality-head">
                  <div>
                    <b>答案资料核对</b>
                    <span>系统只按题整理上传答案，并与这份作业已确认的答案比较；不会静默替换。</span>
                  </div>
                  <strong>
                    {answerSourceBusy
                      ? "正在整理"
                      : answerSourceAnalysis?.review?.route === "confirmed"
                        ? "已确认一致"
                      : answerSourceAnalysis?.review?.route === "kept_bound"
                          ? "已沿用当前答案"
                          : answerSourceAnalysis?.review?.route === "adopted_new_version"
                            ? "已另存新版本"
                          : "等待核对"}
                  </strong>
                </div>
                {answerSourceBusy && (
                  <div className="empty-state compact">正在按题号整理答案，并保留页码或图片区域来源…</div>
                )}
                {!answerSourceBusy && answerSourceError && (
                  <div className="intake-analysis-issues">
                    <span>{answerSourceError}</span>
                  </div>
                )}
                {!answerSourceBusy && answerSourceAnalysis?.run.status === "failed" && (
                  <div className="intake-analysis-issues">
                    <span>{answerSourceAnalysis.run.failure?.safe_message || "答案资料暂时无法整理。"}</span>
                  </div>
                )}
                {answerSourceAnalysis?.review && (
                  <>
                    <div className="intake-analysis-summary">
                      <span className="ready">一致 {answerSourceAnalysis.review.matchedCount}</span>
                      <span className="review">冲突 {answerSourceAnalysis.review.conflictCount}</span>
                      <span className="blocked">缺题 {answerSourceAnalysis.review.missingCount}</span>
                    </div>
                    {answerSourceAnalysis.review.route === "blocked" && (
                      <div className="intake-analysis-issues">
                        {answerSourceAnalysis.review.items
                          .filter((item) => item.matchState !== "matched")
                          .slice(0, 5)
                          .map((item) => {
                            const proposedPoints = item.questionType === "short_answer"
                              ? shortAnswerRubricPoints(item.candidateAnswerJson)
                              : [];
                            return (
                              <div key={item.assessmentItemId} className="intake-answer-review-item">
                                <span>
                                  第 {item.questionNo} 题 · 当前：{answerJsonLabel(item.boundAnswerJson)} · 上传：{answerJsonLabel(item.candidateAnswerJson)}
                                </span>
                                {item.questionType === "short_answer" && (
                                  <div className="muted">
                                    <b>上传评分点</b>
                                    {proposedPoints.map((point) => (
                                      <span key={`candidate-${point.orderIndex}`}>
                                        {point.orderIndex + 1}. {point.canonicalText}（{point.maxScore} 分）
                                      </span>
                                    ))}
                                    <b>当前评分点与已确认链接</b>
                                    {item.boundRubricPoints.map((point) => (
                                      <span key={point.stableId}>
                                        {point.orderIndex + 1}. {point.canonicalText}（{point.maxScore} 分）
                                        {point.confirmedKnowledgeTitles.length > 0 && ` · 知识：${point.confirmedKnowledgeTitles.join("、")}`}
                                        {point.confirmedAbilityTitles.length > 0 && ` · 能力：${point.confirmedAbilityTitles.join("、")}`}
                                      </span>
                                    ))}
                                    <b>确认评分点对应</b>
                                    <span>结构未变时已按顺序预填；如有增删或重排，只需改下面的对应关系。</span>
                                    {proposedPoints.map((point) => {
                                      const mappingKey = `${item.assessmentItemId}:${point.orderIndex}`;
                                      return (
                                        <label className="rubric-mapping-row" key={`mapping-${mappingKey}`}>
                                          <span>{point.orderIndex + 1}. {point.canonicalText}</span>
                                          <select
                                            value={rubricPointMappings[mappingKey] || ""}
                                            onChange={(event) => changeRubricPointMapping(
                                              item.assessmentItemId,
                                              point.orderIndex,
                                              event.target.value,
                                            )}
                                          >
                                            <option value="">请选择对应关系</option>
                                            {item.boundRubricPoints.map((current) => (
                                              <option value={current.stableId} key={current.stableId}>
                                                沿用旧点 {current.orderIndex + 1}：{current.canonicalText}
                                              </option>
                                            ))}
                                            <option value={NEW_RUBRIC_POINT}>这是新增评分点</option>
                                          </select>
                                        </label>
                                      );
                                    })}
                                    <span>未被选择的旧评分点会在新版本中退役；新增点不会猜测继承知识/能力链接，补链前不产生对应图谱证据。</span>
                                  </div>
                                )}
                              </div>
                            );
                          })}
                        {answerSourceAnalysis.review.conflictCount + answerSourceAnalysis.review.missingCount > 5 && (
                          <span>另有 {answerSourceAnalysis.review.conflictCount + answerSourceAnalysis.review.missingCount - 5} 题需要处理。</span>
                        )}
                      </div>
                    )}
                    {answerSourceAnalysis.review.route === "adopted_new_version" && answerSourceAnalysis.review.adoption && (
                      <div className="muted">
                        已把 {answerSourceAnalysis.review.adoption.changedItemCount} 道冲突题保存为作业第 {answerSourceAnalysis.review.adoption.adoptedAssessmentRevision} 版；本批学生照片仍按原答案批改，不会被重写。
                        {answerSourceAnalysis.review.adoption.changedRubricCount > 0 && (
                          <> 其中 {answerSourceAnalysis.review.adoption.changedRubricCount} 道简答题同步建立新评分点，并沿用 {answerSourceAnalysis.review.adoption.carriedKnowledgeLinkCount} 条知识链接、{answerSourceAnalysis.review.adoption.carriedAbilityLinkCount} 条能力链接；新增 {answerSourceAnalysis.review.adoption.newRubricPointCount} 点、退役 {answerSourceAnalysis.review.adoption.retiredRubricPointCount} 点。{answerSourceAnalysis.review.adoption.unlinkedNewRubricPointCount > 0 && ` 新增的 ${answerSourceAnalysis.review.adoption.unlinkedNewRubricPointCount} 个评分点待补知识/能力链接，补链前不进入图谱。`}</>
                        )}
                      </div>
                    )}
                    {answerSourceAnalysis.review.route === "blocked" && (
                      <div className="muted">
                        当前纵切不会用冲突答案改写既有作业；如暂不采纳上传资料，可一次沿用当前已确认答案继续。
                      </div>
                    )}
                  </>
                )}
                <div className="intake-analysis-actions">
                  {!answerSourceBusy && answerSourceAnalysis?.review?.route === "ready_to_confirm" && (
                    <button onClick={() => void confirmMatchingAnswerSource()}>
                      确认这些答案与当前作业一致
                    </button>
                  )}
                  {!answerSourceBusy && answerSourceAnalysis?.review?.route === "blocked" && (
                    answerSourceAnalysis.review.sourceState === "ready"
                    && answerSourceAnalysis.review.conflictCount > 0
                    && answerSourceAnalysis.review.missingCount === 0 && (
                      <button
                        disabled={!rubricPointMappingsReady(answerSourceAnalysis.review, rubricPointMappings)}
                        onClick={() => void adoptAnswerSourceAsNewVersion()}
                      >
                        采用答案与评分点，另存新版本
                      </button>
                    )
                  )}
                  {!answerSourceBusy && answerSourceAnalysis?.review?.route === "blocked" && (
                    <button className="secondary" onClick={() => void keepCurrentBoundAnswers()}>
                      沿用当前作业答案继续
                    </button>
                  )}
                  {!answerSourceBusy && (answerSourceError || answerSourceAnalysis?.run.failure?.retryable) && (
                    <button className="secondary" onClick={() => void analyzeAnswerSource(result.batchId, true)}>
                      重试整理答案
                    </button>
                  )}
                </div>
              </div>
  );
}
