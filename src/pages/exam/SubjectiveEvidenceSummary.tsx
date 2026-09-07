import { convertFileSrc } from "@tauri-apps/api/core";
import type { SubjectiveWorkbenchRow } from "../../api/exam";
import { answerJsonLabel } from "./examPure";

type RubricPoint = {
  orderIndex: number;
  stableId: string;
  canonicalText: string;
  maxScore: number;
};

type PointResult = {
  stableId: string;
  status: string;
  suggestedScore: number;
  evidenceSnippets: string[];
  reason: string;
};

const SHORT_ANSWER_POINT_STATUS: Record<string, string> = {
  covered: "已覆盖",
  partial: "部分覆盖",
  missing: "未覆盖",
  contradicted: "存在矛盾",
  uncertain: "无法确定",
};

function subjectiveStateLabel(row: SubjectiveWorkbenchRow) {
  if (row.result_state === "not_written") return "未作答";
  if (row.result_state === "unreadable") return "无法辨认";
  if (row.result_state === "recognize_failed") return "识别失败";
  if (row.result_state === "ambiguous_final") return "涂改结果不明确";
  if (row.question_type === "short_answer" && row.short_answer_analysis_id != null) {
    if (row.suggestion_outcome === "correct") return "逐点评分建议：全部覆盖";
    if (row.suggestion_outcome === "partial") return "逐点评分建议：部分覆盖";
    if (row.suggestion_outcome === "incorrect") return "逐点评分建议：未覆盖";
  }
  if (row.suggestion_outcome === "correct") return "与已确认答案一致";
  if (row.suggestion_outcome === "incorrect") return "与已确认答案不一致";
  if (row.question_type === "short_answer") return "等待按评分点终审";
  return "无法自动计分";
}

export function SubjectiveEvidenceSummary({
  row,
  rubricPoints,
  pointResults,
  promotedRubricEvidence,
}: {
  row: SubjectiveWorkbenchRow;
  rubricPoints: RubricPoint[];
  pointResults: PointResult[];
  promotedRubricEvidence: unknown[];
}) {
  return (
    <>
              <div className="objective-student">
                <b>{row.student_name}</b>
                <span>{row.student_no}号 · 第{row.question_no}题</span>
                <span>{row.question_type === "fill_blank" ? "填空" : "简答"} · 满分 {row.max_score}</span>
                <span className={row.current_suggestion_confirmed ? "tag pass" : row.suggested_score == null ? "tag fail" : "tag wait"}>
                  {row.current_suggestion_confirmed ? "已终审" : row.suggested_score == null ? "需老师处理" : "有评分建议"}
                </span>
              </div>
              <div className="objective-evidence">
                {row.crop_path
                  ? <img className="objective-crop" src={convertFileSrc(row.crop_path)} alt={`${row.student_name} 第${row.question_no}题作答`} />
                  : <div className="objective-crop missing">裁剪图不可用</div>}
                <div className="objective-facts">
                  <span>机器状态 <b>{subjectiveStateLabel(row)}</b></span>
                  <span>机器原文 <b>{row.raw_ocr_text || "—"}</b></span>
                  {row.teacher_corrected_text && <span>老师校正 <b>{row.teacher_corrected_text}</b></span>}
                  <span>标准答案 <b>{answerJsonLabel(row.answer_json)}</b></span>
                  <span>置信度 <b>{row.confidence == null ? "—" : `${Math.round(row.confidence * 100)}%`}</b></span>
                  <span>建议得分 <b>{row.suggested_score == null ? "—" : `${row.suggested_score} / ${row.max_score}`}</b></span>
                  {row.current_suggestion_confirmed && <span>老师终审 <b>{row.teacher_score} 分 · {row.confirmation_level === "teacher_corrected" ? "人工修正" : "接受建议"}</b></span>}
                  {row.accepted_answer_promotion_id != null && <span>答案库 <b>已加入未来可接受写法</b></span>}
                  {promotedRubricEvidence.length > 0 && <span>评分规则 <b>已沉淀 {promotedRubricEvidence.length} 条老师确认表述</b></span>}
                </div>
              </div>
              {row.question_type === "short_answer" && (
                pointResults.length > 0 ? (
                  <div className="short-answer-analysis">
                    {pointResults.map((result) => {
                      const rubric = rubricPoints.find((point) => point.stableId === result.stableId);
                      return (
                        <div className={`short-answer-point ${result.status}`} key={result.stableId}>
                          <div>
                            <b>{rubric?.canonicalText || result.stableId}</b>
                            <span>{SHORT_ANSWER_POINT_STATUS[result.status] || result.status} · {result.suggestedScore} / {rubric?.maxScore ?? "—"} 分</span>
                          </div>
                          <p>{result.reason || "等待老师结合原图核对"}</p>
                          {result.evidenceSnippets.length > 0
                            ? result.evidenceSnippets.map((snippet) => <q key={snippet}>{snippet}</q>)
                            : <em>学生答案中未定位到可引用片段</em>}
                        </div>
                      );
                    })}
                  </div>
                ) : (
                  <div className="intake-reasons short-answer-rubric">
                    {rubricPoints.length > 0
                      ? rubricPoints.map((point) => <span key={`${point.orderIndex}-${point.canonicalText}`}>{point.canonicalText}（{point.maxScore}分）</span>)
                      : <span>评分点尚未完整，必须老师人工核对</span>}
                  </div>
                )
              )}
    </>
  );
}
