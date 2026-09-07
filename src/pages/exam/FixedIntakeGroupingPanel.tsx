import type { FixedIntakeResult, GroupingRosterStudent } from "../../api/exam";

type FixedIntakeMaterialType = "ordinary_paper" | "answer_sheet" | "dictation";

interface FixedIntakeGroupingPanelProps {
  result: FixedIntakeResult;
  confirmingType: boolean;
  confirmMaterialType: (materialType: FixedIntakeMaterialType) => Promise<void>;
  groupingStartNo: string;
  changeGroupingStart: (studentNo: string) => void;
  absentStudentNos: string[];
  groupingAbsenceCandidates: GroupingRosterStudent[];
  toggleAbsentStudent: (studentNo: string) => void;
  confirmingGrouping: boolean;
  confirmGrouping: () => Promise<void>;
}

export function FixedIntakeGroupingPanel({
  result,
  confirmingType,
  confirmMaterialType,
  groupingStartNo,
  changeGroupingStart,
  absentStudentNos,
  groupingAbsenceCandidates,
  toggleAbsentStudent,
  confirmingGrouping,
  confirmGrouping,
}: FixedIntakeGroupingPanelProps) {
  return (
            <>
            {result.materialTypeNeedsConfirmation && (
              <div className="intake-material-confirm">
                <b>只确认一次，这批是什么？</b>
                <span>系统无法仅凭文件名可靠区分，不会直接进入错误识别路线。</span>
                <div>
                  <button disabled={confirmingType} onClick={() => confirmMaterialType("ordinary_paper")}>普通试卷</button>
                  <button disabled={confirmingType} onClick={() => confirmMaterialType("answer_sheet")}>答题卡</button>
                  <button disabled={confirmingType} onClick={() => confirmMaterialType("dictation")}>默写</button>
                </div>
              </div>
            )}
            {!result.materialTypeNeedsConfirmation
              && result.groupingRoute !== "blocked"
              && !result.groupingConfirmed && (
              <div className="intake-grouping-confirm">
                <b>确认照片从哪位学生开始</b>
                <span>系统会按学号升序连续对应 {result.studentGroupCount} 名学生；只需标出中间缺交的人。</span>
                <label className="field">
                  <span className="fl">第一份是谁</span>
                  <select
                    value={groupingStartNo}
                    onChange={(event) => changeGroupingStart(event.target.value)}
                  >
                    {result.groupingRoster.map((student) => (
                      <option key={student.studentId} value={student.studentNo}>
                        {student.studentNo}号 · {student.studentName}
                      </option>
                    ))}
                  </select>
                </label>
                <details>
                  <summary>{absentStudentNos.length ? `已标记 ${absentStudentNos.length} 人缺交` : "有人缺交？点这里勾选"}</summary>
                  <div className="intake-absence-list">
                    {groupingAbsenceCandidates.map((student) => (
                      <label key={student.studentId}>
                        <input
                          type="checkbox"
                          checked={absentStudentNos.includes(student.studentNo)}
                          onChange={() => toggleAbsentStudent(student.studentNo)}
                        />
                        <span>{student.studentNo}号 · {student.studentName}</span>
                      </label>
                    ))}
                  </div>
                </details>
                <button disabled={confirmingGrouping || !groupingStartNo} onClick={confirmGrouping}>
                  {confirmingGrouping ? "正在确认对应关系…" : `确认这 ${result.studentGroupCount} 份学生顺序`}
                </button>
              </div>
            )}
            </>
  );
}
