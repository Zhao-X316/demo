import type { SubjectiveWorkbenchRow } from "../../api/exam";

type ComponentSpec = {
  sourcePublicId: string;
  stableId: string;
  canonicalText: string;
  maxScore: number;
};

type PointResult = {
  stableId: string;
  suggestedScore: number;
  evidenceSnippets: string[];
};

export function SubjectiveComponentEditor({
  row,
  componentSpecs,
  pointResults,
  componentScores,
  changeComponentScore,
  componentEvidence,
  changeComponentEvidence,
  componentNotes,
  changeComponentNote,
  manualNotes,
  changeManualNote,
  busy,
  correctComponents,
}: {
  row: SubjectiveWorkbenchRow;
  componentSpecs: ComponentSpec[];
  pointResults: PointResult[];
  componentScores: Record<string, string>;
  changeComponentScore: (key: string, value: string) => void;
  componentEvidence: Record<string, string>;
  changeComponentEvidence: (key: string, value: string) => void;
  componentNotes: Record<string, string>;
  changeComponentNote: (key: string, value: string) => void;
  manualNotes: Record<number, string>;
  changeManualNote: (suggestionId: number, value: string) => void;
  busy: boolean;
  correctComponents: (row: SubjectiveWorkbenchRow) => Promise<void>;
}) {
  return (
    <>
                {!row.current_suggestion_confirmed && componentSpecs.length > 0 && (
                  <details open={row.question_type === "short_answer" || componentSpecs.length > 1}>
                    <summary>{row.question_type === "fill_blank" ? "按空格逐项确认" : "按评分点逐项确认"}</summary>
                    <div className="short-answer-analysis component-editor">
                      {componentSpecs.map((component) => {
                        const key = `${row.suggestion_id}:${component.sourcePublicId}`;
                        const machinePoint = pointResults.find((point) => point.stableId === component.stableId);
                        const defaultScore = row.question_type === "short_answer"
                          ? machinePoint?.suggestedScore
                          : componentSpecs.length === 1
                            ? row.suggested_score
                            : undefined;
                        const defaultEvidence = row.question_type === "short_answer"
                          ? machinePoint?.evidenceSnippets[0] ?? ""
                          : componentSpecs.length === 1
                            ? row.teacher_corrected_text ?? row.normalized_text ?? row.raw_ocr_text ?? ""
                            : "";
                        return (
                          <div className="short-answer-point" key={component.sourcePublicId}>
                            <div>
                              <b>{component.canonicalText}</b>
                              <span>满分 {component.maxScore} 分</span>
                            </div>
                            <label className="field">
                              <span className="fl">本项得分</span>
                              <input type="number" min="0" max={component.maxScore} step="0.5"
                                value={componentScores[key] ?? String(defaultScore ?? "")}
                                onChange={(event) => changeComponentScore(key, event.target.value)} />
                            </label>
                            <label className="field">
                              <span className="fl">学生作答证据（给分时必填）</span>
                              <input type="text" placeholder="按原图填写本槽答案或引用学生答案原文"
                                value={componentEvidence[key] ?? defaultEvidence}
                                onChange={(event) => changeComponentEvidence(key, event.target.value)} />
                            </label>
                            <label className="field">
                              <span className="fl">本项备注（可选）</span>
                              <input type="text" placeholder="如：表述不完整，给一半分"
                                value={componentNotes[key] ?? ""}
                                onChange={(event) => changeComponentNote(key, event.target.value)} />
                            </label>
                          </div>
                        );
                      })}
                    </div>
                    <label className="field">
                      <span className="fl">本题整体判定依据（必填）</span>
                      <input type="text" placeholder="如：第1空正确，第2空年份错误"
                        value={manualNotes[row.suggestion_id] ?? ""}
                        onChange={(event) => changeManualNote(row.suggestion_id, event.target.value)} />
                    </label>
                    <button className="primary" disabled={busy} onClick={() => void correctComponents(row)}>
                      保存逐项结论并自动汇总
                    </button>
                  </details>
                )}
    </>
  );
}
