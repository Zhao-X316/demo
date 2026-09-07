import type { SubjectiveWorkbenchRow } from "../../api/exam";

export function SubjectiveReviewToolbar({
  assessmentVersions,
  assessmentVersionId,
  onAssessmentVersionChange,
  itemOptions,
  assessmentItemId,
  onAssessmentItemChange,
  rowCount,
  confirmedCount,
  directCount,
  exceptionCount,
}: {
  assessmentVersions: Array<{ id: number; title: string }>;
  assessmentVersionId: number;
  onAssessmentVersionChange: (assessmentVersionId: number) => void;
  itemOptions: SubjectiveWorkbenchRow[];
  assessmentItemId: number;
  onAssessmentItemChange: (assessmentItemId: number) => void;
  rowCount: number;
  confirmedCount: number;
  directCount: number;
  exceptionCount: number;
}) {
  return (
    <section className="objective-toolbar exam-card">
      <label className="field">
        <span className="fl">作业版本</span>
        <select value={assessmentVersionId} onChange={(event) => onAssessmentVersionChange(Number(event.target.value))}>
          {assessmentVersions.map((version) => <option value={version.id} key={version.id}>{version.title} · v{version.id}</option>)}
        </select>
      </label>
      <label className="field">
        <span className="fl">按题终审</span>
        <select value={assessmentItemId} onChange={(event) => onAssessmentItemChange(Number(event.target.value))}>
          {itemOptions.map((row) => (
            <option value={row.assessment_item_id} key={row.assessment_item_id}>
              第{row.question_no}题 · {row.question_type === "fill_blank" ? "填空" : "简答"} · {row.question_stem.slice(0, 28)}
            </option>
          ))}
        </select>
      </label>
      <div className="objective-stats">
        <span><b>{rowCount}</b> 份作答</span>
        <span className="ok-text"><b>{confirmedCount}</b> 已确认</span>
        <span><b>{directCount}</b> 有明确建议</span>
        <span className={exceptionCount ? "bad-text" : ""}><b>{exceptionCount}</b> 需人工记分</span>
      </div>
      <div className="meta objective-batch-note">填空题只做已确认答案的精确匹配；简答题逐点引用学生原文给建议，不按整段相似度直接给分，也不自动确认。</div>
    </section>
  );
}
