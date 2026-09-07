import { shanghaiDate, shiftShanghaiDate, formatTime, CauseDistributionList } from "./shared";
import { useState, useEffect } from "react";
import { WrongbookStatistics, loadWrongbookStatistics, createWrongbookReportSnapshot, writeWrongbookReportSnapshot } from "../../api/learning";
import { save } from "@tauri-apps/plugin-dialog";

function reportFileName(
  className: string,
  student: { no: string; name: string } | null,
  rangeStart: string,
  rangeEnd: string,
) {
  const title = student
    ? `${className}_${student.no}号${student.name}_学习事实`
    : `${className}_错题事实汇总`;
  return `${title}_${rangeStart}_至_${rangeEnd}.csv`.replace(/[<>:"/\\|?*]/g, "_");
}

export function WrongbookReportPanel({
  classId,
  className,
  selectedStudent,
  refreshToken,
}: {
  classId: number;
  className: string;
  selectedStudent: { id: number; no: string; name: string } | null;
  refreshToken: number;
}) {
  const today = shanghaiDate();
  const [rangeStart, setRangeStart] = useState(shiftShanghaiDate(today, -29));
  const [rangeEnd, setRangeEnd] = useState(today);
  const [statistics, setStatistics] = useState<WrongbookStatistics | null>(null);
  const [loading, setLoading] = useState(false);
  const [reportError, setReportError] = useState("");
  const [exporting, setExporting] = useState(false);
  const [exportedFile, setExportedFile] = useState("");

  useEffect(() => {
    let current = true;
    if (!rangeStart || !rangeEnd || rangeStart > rangeEnd) {
      setStatistics(null);
      setReportError("开始日期不能晚于结束日期");
      return () => {
        current = false;
      };
    }
    setLoading(true);
    setReportError("");
    setExportedFile("");
    loadWrongbookStatistics({
      classId,
      studentId: selectedStudent?.id ?? null,
      rangeStart,
      rangeEnd,
    })
      .then((value) => {
        if (current) setStatistics(value);
      })
      .catch((reason) => {
        if (current) {
          setStatistics(null);
          setReportError(String(reason));
        }
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [classId, rangeEnd, rangeStart, refreshToken, selectedStudent?.id]);

  const exportReport = async () => {
    if (!statistics) return;
    setExporting(true);
    setReportError("");
    setExportedFile("");
    try {
      const outputPath = await save({
        defaultPath: reportFileName(className, selectedStudent, rangeStart, rangeEnd),
        filters: [{ name: "CSV 表格", extensions: ["csv"] }],
      });
      if (!outputPath) return;
      const snapshot = await createWrongbookReportSnapshot({
        reportKind: selectedStudent ? "student_parent" : "class_summary",
        classId,
        studentId: selectedStudent?.id ?? null,
        rangeStart,
        rangeEnd,
      });
      const written = await writeWrongbookReportSnapshot(snapshot.public_id, outputPath);
      setExportedFile(written.file_name);
    } catch (reason) {
      setReportError(String(reason));
    } finally {
      setExporting(false);
    }
  };

  return (
    <section className="wrongbook-report-panel">
      <div className="wrongbook-report-head">
        <div>
          <b>{selectedStudent
            ? `${selectedStudent.no}号 ${selectedStudent.name} · 学习事实报告`
            : "班级错题统计与导出"}</b>
          <span>
            {selectedStudent
              ? "只包含这名学生，不带入同班其他学生信息。"
              : "按学号展示事实，不生成名次或掌握总分。"}
          </span>
        </div>
        <div className="wrongbook-report-range">
          <label>
            <span>开始</span>
            <input aria-label="统计开始日期" type="date" value={rangeStart}
              onChange={(event) => setRangeStart(event.target.value)} />
          </label>
          <label>
            <span>结束</span>
            <input aria-label="统计结束日期" type="date" value={rangeEnd}
              onChange={(event) => setRangeEnd(event.target.value)} />
          </label>
          <button className="primary" disabled={!statistics || loading || exporting} onClick={exportReport}>
            {exporting ? "正在导出…" : selectedStudent ? "导出家长沟通表" : "导出班级表格"}
          </button>
        </div>
      </div>
      {reportError && <div className="error">{reportError}</div>}
      {exportedFile && <div className="success">已导出 {exportedFile}</div>}
      {loading && !statistics && <div className="loading">正在按当前口径统计…</div>}
      {statistics && (
        <>
          <div className="wrongbook-report-summary">
            <div><span>当前事实</span><b>{statistics.summary.fact_count}</b></div>
            <div><span>关联发布证据</span><b>{statistics.summary.evidence_count}</b></div>
            <div><span>老师确认错因</span><b>{statistics.summary.confirmed_cause_review_count}</b></div>
            <div><span>重复出错</span><b>{statistics.summary.repeated_error_count}</b></div>
          </div>

          {!selectedStudent && statistics.students.length > 0 && (
            <div className="wrongbook-student-facts">
              <div className="wrongbook-student-facts-head">
                <b>学生事实</b>
                <span>自然学号顺序，不按数量高低排序</span>
              </div>
              <div className="wrongbook-student-facts-list">
                {statistics.students.map((student) => (
                  <div key={student.student_id}>
                    <b>{student.student_no}号 {student.student_name}</b>
                    <span>当前 {student.fact_count} 题</span>
                    <span>待订正 {student.needs_correction_count}</span>
                    <span>订正/复测正确 {student.corrected_once_count + student.rechecked_correct_count}</span>
                    <span>重复 {student.repeated_error_count}</span>
                    <span>最近验证 {student.latest_verification_at
                      ? formatTime(student.latest_verification_at)
                      : "暂无"}</span>
                  </div>
                ))}
              </div>
            </div>
          )}

          <div className="wrongbook-distribution-grid">
            <CauseDistributionList
              title="题目错因分布"
              items={statistics.question_causes}
              empty="选定范围内还没有老师确认的题目错因。"
            />
            <CauseDistributionList
              title="知识点错因分布"
              items={statistics.knowledge_causes}
              empty="尚无同时具备老师确认错因与已确认知识链接的记录。"
            />
          </div>
          <div className="wrongbook-report-rule">
            <span>{statistics.meta.activity_filter_rule}</span>
            <span>{statistics.meta.evidence_count_rule}</span>
            <span>统计口径 {statistics.meta.rule_version} · 生成 {formatTime(statistics.meta.calculated_at)}</span>
          </div>
        </>
      )}
    </section>
  );
}
