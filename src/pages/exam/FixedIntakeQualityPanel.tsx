import { convertFileSrc } from "@tauri-apps/api/core";
import type { FixedIntakeResult, GroupedPageEvidence } from "../../api/exam";

interface FixedIntakeQualityPanelProps {
  result: FixedIntakeResult;
  rejectedPageIds: number[];
  loadingEvidence: boolean;
  groupingEvidence: GroupedPageEvidence[];
  toggleRejectedPage: (pageId: number) => void;
  confirmingQuality: boolean;
  confirmGroupingQuality: () => Promise<void>;
  retakingPageId: number | null;
  replaceRejectedPage: (pageId: number) => Promise<void>;
}

export function FixedIntakeQualityPanel({
  result,
  rejectedPageIds,
  loadingEvidence,
  groupingEvidence,
  toggleRejectedPage,
  confirmingQuality,
  confirmGroupingQuality,
  retakingPageId,
  replaceRejectedPage,
}: FixedIntakeQualityPanelProps) {
  return (
            <>
            {result.groupingConfirmed && (
              <div className="intake-grouping-confirm confirmed">
                <b>照片与学生顺序已确认</b>
                <span>{result.groupingFirstStudentNo}号至 {result.groupingLastStudentNo}号，共 {result.studentGroupCount} 名；后续页面质量异常只影响对应页组。</span>
              </div>
            )}
            {result.groupingConfirmed && !result.qualityReviewCompleted && (
              <div className="intake-quality-review">
                <div className="intake-quality-head">
                  <div>
                    <b>看一眼照片是否清楚</b>
                    <span>默认全部合格；模糊、反光或缺边的页面点一下标记“需重拍”。</span>
                  </div>
                  <strong>{rejectedPageIds.length ? `${rejectedPageIds.length} 页需重拍` : "全部清楚"}</strong>
                </div>
                {loadingEvidence ? (
                  <div className="empty-state compact">正在生成按学生排列的照片联系表…</div>
                ) : (
                  <div className="intake-contact-sheet">
                    {groupingEvidence.map((group) => (
                      <div className="intake-contact-group" key={group.groupIndex}>
                        <div className="intake-contact-student">
                          <b>{group.studentNo}号</b>
                          <span>{group.studentName}</span>
                        </div>
                        <div className="intake-contact-pages">
                          {group.pages.map((page) => {
                            const rejected = rejectedPageIds.includes(page.pageId);
                            return (
                              <button
                                type="button"
                                key={page.pageId}
                                className={`intake-page-thumb ${rejected ? "rejected" : ""}`}
                                onClick={() => toggleRejectedPage(page.pageId)}
                              >
                                <img src={convertFileSrc(page.archivedPath)} alt={`${group.studentName} 第 ${page.pageNo} 页`} />
                                <span>第 {page.pageNo} 页 · {rejected ? "需重拍" : "清楚"}</span>
                              </button>
                            );
                          })}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
                <button
                  disabled={confirmingQuality || loadingEvidence || !groupingEvidence.length}
                  onClick={confirmGroupingQuality}
                >
                  {confirmingQuality
                    ? "正在原子建立页面归属…"
                    : rejectedPageIds.length
                      ? `确认清楚页面，扣住 ${rejectedPageIds.length} 页重拍`
                      : "照片都清楚，确认并建立归属"}
                </button>
              </div>
            )}
            {result.qualityReviewCompleted && (
              <div className="intake-quality-complete">
                <b>页面质量与正式归属已确认</b>
                <span>已进入后续识别：{result.mappedGroupCount} 名；需重拍：{result.rejectedGroupCount} 名。未生成分数或发布。</span>
                {result.rejectedGroupCount > 0 && (
                  <div className="intake-retake-list">
                    {groupingEvidence.flatMap((group) => group.pages
                      .filter((page) => page.qualityResult === "reject")
                      .map((page) => (
                        <button
                          type="button"
                          key={page.pageId}
                          disabled={retakingPageId !== null}
                          onClick={() => replaceRejectedPage(page.pageId)}
                        >
                          {retakingPageId === page.pageId
                            ? "正在归档并替换…"
                            : `${group.studentNo}号 ${group.studentName} · 第${page.pageNo}页重拍`}
                        </button>
                      )),
                    )}
                  </div>
                )}
              </div>
            )}
            </>
  );
}
