import type { IntakeResume } from "../../api/workspace";
const LABEL: Record<string, string> = {
  ordinary_paper_structure: "普通卷版面",
  answer_source_structure: "答案资料",
  answer_sheet_template: "答题卡模板",
  dictation_template: "默写模板",
};
const STATUS: Record<string, string> = {
  succeeded: "处理记录已保存",
  failed: "处理失败",
  processing: "上次处理中，需检查",
  pending: "等待处理",
};
export default function ResumeProgress({
  resume,
  loading,
  onContinue,
  onReview,
}: {
  resume: IntakeResume;
  loading: boolean;
  onContinue: () => void;
  onReview: () => void;
}) {
  return (
    <div className="intake-analysis-card">
      <b>已恢复保存的处理记录</b>
      <p>
        已建立题区的页面 {resume.processedPageIds.length}{" "}
        页。题区建立、识别成功与老师确认分别记录；已有评分请进入核对查看。
      </p>
      {(resume.processingHistory ?? [])
        .filter((row) => row.status === "failed" || row.status === "processing")
        .map((row) => (
          <p key={row.id} className="error">
            {LABEL[row.stage] ?? "材料处理"}：{STATUS[row.status]}。
            {row.message ?? "请检查本页材料并继续处理，查看当前结果。"}
          </p>
        ))}
      <div className="intake-analysis-actions">
        <button onClick={onReview}>查看已有核对结果</button>
        <button disabled={loading} onClick={onContinue}>
          {loading ? "正在继续处理…" : "继续材料处理"}
        </button>
      </div>
      <p className="muted">
        继续处理会复用已保存的识别结果，并接着处理未完成的部分；评分确认和发布仍需单独操作。
      </p>
      <details>
        <summary>查看处理记录</summary>
        {(resume.processingHistory ?? []).length ? (
          (resume.processingHistory ?? []).map((row) => (
            <p key={row.id}>
              {LABEL[row.stage] ?? "材料处理"} ·{" "}
              {STATUS[row.status] ?? row.status} · {row.updatedAt}
            </p>
          ))
        ) : (
          <p>还没有保存的识别运行记录。</p>
        )}
      </details>
    </div>
  );
}
