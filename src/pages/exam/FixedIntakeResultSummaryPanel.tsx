import type { FixedIntakeResult } from "../../api/exam";

const INTAKE_REASON_LABEL: Record<string, string> = {
  BATCH_EMPTY: "没有可处理页面",
  IMPORT_SEQUENCE_GAP: "页面顺序不连续",
  SOURCE_DOCUMENT_UNREGISTERED: "原始资料未完整登记",
  PAGE_QUALITY_REVIEW_REQUIRED: "需要检查页面清晰度",
  PAGE_IDENTITY_UNCONFIRMED: "等待确认学生和页码",
  PAGE_NUMBER_CONFLICT: "页码存在冲突",
  PAGE_SEQUENCE_INVALID: "同一学生页序异常",
  STUDENT_PAGES_NONCONTIGUOUS: "同一学生页面未连续排列",
  GROUP_FIRST_PAGE_INVALID: "学生卷未从第 1 页开始",
  GROUP_PAGE_COUNT_MISMATCH: "学生卷页数与设置不一致",
  ANSWER_AUTHORITY_MISSING: "等待确认标准答案",
  ANSWER_CANDIDATE_INVALID: "答案资料需要修正",
  ANSWER_CANDIDATE_DRIFT: "答案版本发生变化",
  ANSWER_SAME_LEVEL_CONFLICT: "答案资料存在分歧",
  ANSWER_SOURCE_STRUCTURE_PENDING: "答案资料等待自动整理",
  ANSWER_SOURCE_STRUCTURE_FAILED: "答案资料整理失败",
  ANSWER_SOURCE_CONFIRMATION_REQUIRED: "答案资料与当前答案一致，等待一次确认",
  ANSWER_SOURCE_CONFLICT_OR_MISSING: "答案资料存在冲突或缺题",
  ANSWER_REGION_MISSING: "等待定位答题区域",
  ANSWER_REGION_AMBIGUOUS: "答题区域不唯一",
  OBJECTIVE_RESULT_REVIEW_REQUIRED: "识别结果需要老师复核",
  ANSWER_VERSION_DRIFT: "识别使用的答案版本已变化",
  REVIEW_INPUT_PRESENT: "本批存在需复核资料",
  DUPLICATE_PAGE_ARTIFACT: "检测到重复页面",
  STUDENT_GROUP_EXCEEDS_ROSTER: "照片组数超过班级人数",
  STUDENT_RANGE_OR_ABSENCE_CONFIRMATION_REQUIRED: "需确认本批学号范围或缺交学生",
  MATERIAL_TYPE_CONFIRMATION_REQUIRED: "需确认普通试卷、答题卡或默写",
  PAGE_TYPE_CYCLE_UNVERIFIED: "多页卷需要确认页面周期",
  PAGE_TYPE_CYCLE_MISMATCH: "页面周期中疑似缺页或错序",
  ORDER_EVIDENCE_CONFLICT: "文件名顺序与拍摄时间存在分歧",
  CAPTURE_TIME_ORDER_CONFLICT: "拍摄时间与文件名顺序冲突",
  FILE_TIME_ORDER_CONFLICT: "文件时间与文件名顺序冲突",
  FILENAME_NATURAL_TIE: "存在无法单靠文件名区分的照片",
};

interface FixedIntakeResultSummaryPanelProps {
  result: FixedIntakeResult;
  answerSheetSubjectiveRegionCount: number;
  onOpenReview: (tab: "objective" | "subjective" | "dictation") => void;
}

export function FixedIntakeResultSummaryPanel({
  result,
  answerSheetSubjectiveRegionCount,
  onOpenReview,
}: FixedIntakeResultSummaryPanelProps) {
  return (
            <>
            <div className="intake-route-grid">
              <div className={result.route === "ready_for_batch_confirm" ? "active ready" : ""}>
                <span>可批量确认</span>
                <b>{result.targetCount ? result.readyCount : "—"}</b>
              </div>
              <div className={result.route === "review_required" ? "active review" : ""}>
                <span>需复核</span>
                <b>{result.targetCount ? result.reviewCount : "—"}</b>
              </div>
              <div className={result.route === "blocked" ? "active blocked" : ""}>
                <span>受阻</span>
                <b>{result.targetCount ? result.blockedCount : "—"}</b>
              </div>
            </div>
            {result.reasonCodes.length > 0 && (
              <div className="intake-reasons">
                {result.reasonCodes.slice(0, 3).map((code) => (
                  <span key={code}>{INTAKE_REASON_LABEL[code] || code}</span>
                ))}
              </div>
            )}
            {(result.orderConflictCodes.length > 0 || result.groupingIssueCodes.length > 0) && (
              <div className="intake-reasons">
                {[...result.orderConflictCodes, ...result.groupingIssueCodes]
                  .filter((code, index, all) => all.indexOf(code) === index)
                  .slice(0, 4)
                  .map((code) => <span key={code}>{INTAKE_REASON_LABEL[code] || code}</span>)}
              </div>
            )}
            <div className="intake-next">
              <span>下一步</span>
              <b>{result.nextAction}</b>
              <button
                disabled={result.route === "blocked" || result.groupingRoute === "blocked" || result.materialTypeNeedsConfirmation || !result.groupingConfirmed || !result.qualityReviewCompleted}
                onClick={() => onOpenReview(
                  result.materialType === "dictation"
                    ? "dictation"
                    : result.materialType === "answer_sheet" && answerSheetSubjectiveRegionCount > 0
                      ? "subjective"
                      : "objective",
                )}
              >进入批改终审</button>
            </div>
            </>
  );
}
