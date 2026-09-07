import { useState, useEffect, useMemo } from "react";
import { Class, Student, classesList, studentsList } from "../api/manage";
import {
  ClassWrongbookDashboard,
  WrongbookStatus,
  SchedulePolicy,
  loadWrongbookSchedulePolicy,
  loadClassWrongbookDashboard,
} from "../api/learning";
import { SchedulePolicyPanel } from "./learning-insights/SchedulePolicyPanel";
import { WrongbookReportPanel } from "./learning-insights/WrongbookReportPanel";
import { StudentProfilePanel } from "./learning-insights/StudentProfilePanel";
import { formatTime } from "./learning-insights/shared";
import { ItemCard } from "./learning-insights/WrongbookItemCard";

interface Props {
  onOpenExam: () => void;
  onOpenStudents: () => void;
}

export default function LearningInsights({ onOpenExam, onOpenStudents }: Props) {
  const [classes, setClasses] = useState<Class[]>([]);
  const [allStudents, setAllStudents] = useState<Student[]>([]);
  const [classId, setClassId] = useState<number | null>(null);
  const [dashboard, setDashboard] = useState<ClassWrongbookDashboard | null>(null);
  const [studentFilter, setStudentFilter] = useState("all");
  const [statusFilter, setStatusFilter] = useState<WrongbookStatus | "all">("all");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [refreshTick, setRefreshTick] = useState(0);
  const [policy, setPolicy] = useState<SchedulePolicy | null>(null);
  const [policyOpen, setPolicyOpen] = useState(false);
  const [policyError, setPolicyError] = useState("");
  const [reportOpen, setReportOpen] = useState(false);
  const [reportRefreshTick, setReportRefreshTick] = useState(0);
  const [activeTab, setActiveTab] = useState<"wrongbook" | "profile">("wrongbook");

  useEffect(() => {
    Promise.all([classesList(), studentsList()])
      .then(([items, studentItems]) => {
        setClasses(items);
        setAllStudents(studentItems);
        setClassId((current) => current ?? items[0]?.id ?? null);
        if (items.length === 0) setLoading(false);
      })
      .catch((reason) => {
        setError(String(reason));
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    loadWrongbookSchedulePolicy()
      .then(setPolicy)
      .catch((reason) => setPolicyError(String(reason)));
  }, []);

  useEffect(() => {
    if (classId === null) {
      setDashboard(null);
      return;
    }
    let current = true;
    setLoading(true);
    setError("");
    setStudentFilter("all");
    loadClassWrongbookDashboard(classId)
      .then((value) => {
        if (current) setDashboard(value);
      })
      .catch((reason) => {
        if (current) {
          setDashboard(null);
          setError(String(reason));
        }
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [classId, refreshTick]);

  const students = useMemo(() => {
    const seen = new Map<number, { id: number; no: string; name: string }>();
    dashboard?.items.forEach((item) => {
      seen.set(item.student_id, {
        id: item.student_id,
        no: item.student_no,
        name: item.student_name,
      });
    });
    return [...seen.values()];
  }, [dashboard]);

  const visibleItems = useMemo(
    () =>
      (dashboard?.items ?? []).filter(
        (item) =>
          (studentFilter === "all" || item.student_id === Number(studentFilter))
          && (statusFilter === "all" || item.status === statusFilter),
      ),
    [dashboard, statusFilter, studentFilter],
  );
  const selectedStudent = useMemo(
    () => studentFilter === "all"
      ? null
      : students.find((student) => student.id === Number(studentFilter)) ?? null,
    [studentFilter, students],
  );
  const classStudents = useMemo(
    () => allStudents
      .filter((student) => student.enabled && student.class_id === classId)
      .sort((left, right) =>
        left.student_no.localeCompare(right.student_no, "zh-CN", { numeric: true })),
    [allStudents, classId],
  );

  if (classes.length === 0 && !loading) {
    return (
      <div className="page learning-insights">
        <div className="page-head"><h1>错题与掌握</h1></div>
        <div className="sub">先建立班级并加入学生，才能按明确范围汇总错题事实。</div>
        {error && <div className="error">{error}</div>}
        <div className="empty-state">
          暂无班级<br />
          <button className="primary" onClick={onOpenStudents}>去建立班级</button>
        </div>
      </div>
    );
  }

  return (
    <div className="page learning-insights">
      <div className="page-head learning-head">
        <div>
          <h1>错题与掌握</h1>
          <div className="sub">错题处理与学习掌握共用一个入口，但事实、证据和掌握结论保持分层。</div>
        </div>
        <div className="learning-scope">
          <label>
            <span>班级</span>
            <select value={classId ?? ""} onChange={(event) => setClassId(Number(event.target.value))}>
              {classes.map((item) => <option value={item.id} key={item.id}>{item.name}</option>)}
            </select>
          </label>
          {activeTab === "wrongbook" && <button disabled={!policy} onClick={() => setPolicyOpen((current) => !current)}>
            {policyOpen ? "收起规则" : "巩固规则"}
          </button>}
          {activeTab === "wrongbook" && <button onClick={() => setReportOpen((current) => !current)}>
            {reportOpen ? "收起统计" : "统计与导出"}
          </button>}
          <button onClick={() => setRefreshTick((value) => value + 1)}>刷新</button>
        </div>
      </div>

      {policyError && <div className="error">{policyError}</div>}
      {activeTab === "wrongbook" && policyOpen && policy && <SchedulePolicyPanel policy={policy} onSaved={setPolicy} />}
      {activeTab === "wrongbook" && reportOpen && dashboard && (
        <WrongbookReportPanel
          classId={dashboard.class.id}
          className={dashboard.class.name}
          selectedStudent={selectedStudent}
          refreshToken={reportRefreshTick}
        />
      )}

      <div className="tabs learning-tabs">
        <button className={activeTab === "wrongbook" ? "tab active" : "tab"}
          onClick={() => setActiveTab("wrongbook")}>错题事实</button>
        <button className={activeTab === "profile" ? "tab active" : "tab"}
          onClick={() => setActiveTab("profile")}>
          个人掌握快照
        </button>
      </div>

      {activeTab === "profile" && classId != null && (
        <StudentProfilePanel classId={classId} students={classStudents}
          refreshToken={refreshTick} onOpenExam={onOpenExam} />
      )}

      {activeTab === "wrongbook" && (
        <>
      <div className="learning-safety">
        <b>边界</b>
        这里只显示当前有效发布快照中的老师评分。订正一次 ≠ 已掌握；没有知识点标签 ≠ 没有错题。
      </div>

      {error && <div className="error">{error}</div>}
      {loading && !dashboard && <div className="loading">正在读取已发布错题事实…</div>}
      {dashboard && (
        <>
          <div className="learning-context">
            <b>{dashboard.class.name}</b>
            <span>{dashboard.class.enabled_student_count} 名启用学生</span>
            {dashboard.class.term && <span>{dashboard.class.term}</span>}
            {dashboard.class.textbook && <span>{dashboard.class.textbook}</span>}
            <span className="spacer" />
            <span>发布水位 {dashboard.meta.exam_watermark
              ? formatTime(dashboard.meta.exam_watermark)
              : "暂无"}</span>
          </div>

          <div className="learning-stats">
            <div className="learning-stat bad">
              <span>待订正</span>
              <b>{dashboard.summary.needs_correction_count}</b>
            </div>
            <div className="learning-stat warn">
              <span>已订正一次</span>
              <b>{dashboard.summary.corrected_once_count}</b>
            </div>
            <div className="learning-stat ok">
              <span>再次作答正确</span>
              <b>{dashboard.summary.rechecked_correct_count}</b>
            </div>
            <div className="learning-stat">
              <span>重复出错</span>
              <b>{dashboard.summary.repeated_error_count}</b>
            </div>
          </div>

          <section className="wrongbook-panel">
            <div className="wrongbook-toolbar">
              <div>
                <b>错题事实</b>
                <span>{dashboard.summary.affected_student_count} 名学生 · {dashboard.summary.wrong_question_count} 道题</span>
              </div>
              <div className="wrongbook-filters">
                <select aria-label="筛选学生" value={studentFilter} onChange={(event) => setStudentFilter(event.target.value)}>
                  <option value="all">全部学生</option>
                  {students.map((student) => (
                    <option value={student.id} key={student.id}>{student.no}号 {student.name}</option>
                  ))}
                </select>
                <select
                  aria-label="筛选订正状态"
                  value={statusFilter}
                  onChange={(event) => setStatusFilter(event.target.value as WrongbookStatus | "all")}
                >
                  <option value="all">全部状态</option>
                  <option value="needs_correction">待订正</option>
                  <option value="corrected_once">已订正一次</option>
                  <option value="rechecked_correct">再次作答正确</option>
                </select>
                <button className="primary" onClick={onOpenExam}>去题目批改</button>
              </div>
            </div>

            {visibleItems.length === 0 ? (
              <div className="empty-state">
                {dashboard.items.length === 0
                  ? "当前没有符合口径的已发布错题。"
                  : "当前筛选条件下没有错题。"}
              </div>
            ) : (
              <div className="wrongbook-list">
                {visibleItems.map((item) => (
                  <ItemCard
                    classId={dashboard.class.id}
                    item={item}
                    key={`${item.student_id}-${item.question_version_id}`}
                    onCauseSaved={(review) => {
                      setDashboard((current) => current
                        ? {
                          ...current,
                          items: current.items.map((candidate) =>
                            candidate.student_id === item.student_id
                            && candidate.question_version_id === item.question_version_id
                              ? { ...candidate, cause_review: review }
                              : candidate),
                        }
                        : current);
                      setReportRefreshTick((value) => value + 1);
                    }}
                    onCorrectionCreated={(assignment) => {
                      setDashboard((current) => current
                        ? {
                          ...current,
                          items: current.items.map((candidate) =>
                            candidate.student_id === item.student_id
                            && candidate.question_version_id === item.question_version_id
                              ? { ...candidate, correction_assignment: assignment }
                              : candidate),
                        }
                        : current);
                      setReportRefreshTick((value) => value + 1);
                    }}
                    onReinforcementCreated={(assignment) => {
                      setDashboard((current) => current
                        ? {
                          ...current,
                          items: current.items.map((candidate) =>
                            candidate.student_id === item.student_id
                            && candidate.question_version_id === item.question_version_id
                              ? { ...candidate, reinforcement_assignment: assignment }
                              : candidate),
                        }
                        : current);
                      setReportRefreshTick((value) => value + 1);
                    }}
                    onOpenExam={onOpenExam}
                  />
                ))}
              </div>
            )}
          </section>

          <div className="learning-rule-note">
            {dashboard.summary.denominator_note}
            <span>本页按学号展示，不生成学生排名，也不读取旧 mastery 聚合。</span>
          </div>
        </>
      )}
        </>
      )}
    </div>
  );
}
