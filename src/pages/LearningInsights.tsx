import { useEffect, useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";

import {
  ClassWrongbookDashboard,
  confirmWrongbookReinforcement,
  confirmWrongbookErrorCauses,
  CorrectionAssignment,
  CorrectionAssignmentStatus,
  createWrongbookReportSnapshot,
  createWrongbookSingleCorrection,
  ErrorCauseReview,
  loadClassWrongbookDashboard,
  loadWrongbookSchedulePolicy,
  loadWrongbookStatistics,
  generateStudentProfile,
  loadLatestStudentProfile,
  previewStudentProfile,
  ProfileNodeMetric,
  ProfileNodeStatus,
  ProfileRecitationSummary,
  ProfileTeacherAssessment,
  ProfileTeacherAssessmentValue,
  ProfileWrongbookSummary,
  previewWrongbookReinforcement,
  ReinforcementAssignment,
  ReinforcementAssignmentStatus,
  ReinforcementSuggestion,
  SchedulePolicy,
  updateWrongbookSchedulePolicy,
  WrongbookQuestion,
  WrongbookStatistics,
  WrongbookStatus,
  StudentProfilePreview,
  StudentProfileSnapshot,
  saveProfileTeacherAssessment,
  writeWrongbookReportSnapshot,
} from "../api/learning";
import { Class, Student, classesList, studentsList } from "../api/manage";

interface Props {
  onOpenExam: () => void;
  onOpenStudents: () => void;
}

const STATUS_LABELS: Record<WrongbookStatus, string> = {
  needs_correction: "待订正",
  corrected_once: "已订正一次",
  rechecked_correct: "再次作答正确",
};

const QUESTION_TYPE_LABELS: Record<string, string> = {
  single: "单选",
  multiple: "多选",
  true_false: "判断",
  fill_blank: "填空",
  short_answer: "简答",
};

const CONTEXT_LABELS: Record<string, string> = {
  classwork: "课堂练习",
  in_class: "课堂练习",
  closed_book: "闭卷",
  homework: "家庭作业",
  quiz: "随堂测验",
  exam: "正式考试",
  open_book: "开卷",
  correction: "订正",
  demo: "演示",
};

const CORRECTION_STATUS_LABELS: Record<CorrectionAssignmentStatus, string> = {
  waiting_upload: "订正已建立 · 等待上传",
  in_progress: "订正处理中",
  ready_to_publish: "订正待发布",
  published: "订正已发布",
};

const REINFORCEMENT_STATUS_LABELS: Record<ReinforcementAssignmentStatus, string> = {
  scheduled: "巩固已安排",
  in_progress: "巩固处理中",
  ready_to_publish: "巩固待发布",
  published: "巩固已发布",
};

const PROFILE_STATUS_LABELS: Record<ProfileNodeStatus, string> = {
  unassessed: "未评估",
  insufficient_evidence: "证据不足",
  needs_support: "需要支持",
  developing: "发展中",
  stable: "相对稳定",
};

const PROFILE_CONFIDENCE_LABELS: Record<string, string> = {
  none: "暂无",
  low: "低",
  medium: "中",
  high: "高",
};

const PROFILE_FRESHNESS_LABELS: Record<string, string> = {
  none: "暂无",
  fresh: "近期",
  aging: "较早",
  stale: "久未更新",
};

const PROFILE_TEACHER_ASSESSMENT_LABELS: Record<ProfileTeacherAssessmentValue, string> = {
  not_taught: "尚未教学",
  needs_support: "需要重点支持",
  developing: "发展中",
  stable: "相对稳定",
  observe: "继续观察",
};

function statusClass(status: WrongbookStatus) {
  if (status === "needs_correction") return "tag fail";
  if (status === "corrected_once") return "tag wait";
  return "tag pass";
}

function score(value: number) {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

function formatTime(value: string) {
  return new Date(value).toLocaleString();
}

function shanghaiDate(value = new Date()) {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: "Asia/Shanghai",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(value);
}

function shiftShanghaiDate(date: string, days: number) {
  const value = new Date(`${date}T00:00:00+08:00`);
  value.setUTCDate(value.getUTCDate() + days);
  return shanghaiDate(value);
}

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

function CauseDistributionList({
  title,
  items,
  empty,
}: {
  title: string;
  items: WrongbookStatistics["question_causes"];
  empty: string;
}) {
  return (
    <div className="wrongbook-distribution">
      <div className="wrongbook-distribution-title">
        <b>{title}</b>
        <span>仅使用老师确认错因</span>
      </div>
      {items.length === 0 ? (
        <div className="wrongbook-distribution-empty">{empty}</div>
      ) : (
        <div className="wrongbook-distribution-list">
          {items.slice(0, 8).map((item) => (
            <div key={item.public_id}>
              <div>
                <b title={item.title}>{item.title}</b>
                <span>{item.confirmed_review_count} 条已确认记录</span>
              </div>
              <div>
                {item.causes.map((cause) => (
                  <span key={cause.cause_code}>{cause.cause_label} {cause.count}</span>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function WrongbookReportPanel({
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

function ProfileWrongbookHistory({
  summary,
  frozen,
  onOpenExam,
}: {
  summary: ProfileWrongbookSummary;
  frozen: boolean;
  onOpenExam: () => void;
}) {
  if (summary.fact_count === 0) return null;
  return (
    <section className="profile-wrongbook-history">
      <div className="profile-metric-title">
        <b>错题恢复过程</b>
        <span>{frozen ? "随本版快照冻结" : "生成前只读预览"} · 不用一次订正替代掌握结论</span>
      </div>
      <div className="profile-wrongbook-stats">
        <div><span>当前错题事实</span><b>{summary.fact_count}</b></div>
        <div><span>待订正</span><b>{summary.needs_correction_count}</b></div>
        <div><span>订正 / 复测正确</span>
          <b>{summary.corrected_once_count + summary.rechecked_correct_count}</b></div>
        <div><span>重复出错</span><b>{summary.repeated_error_count}</b></div>
      </div>
      <details>
        <summary>查看恢复事实</summary>
        <div className="profile-wrongbook-list">
          {summary.facts.map((fact) => (
            <div key={fact.question_version_public_id}>
              <div>
                <b>{fact.stem}</b>
                <span>{formatTime(fact.latest_response_at)} · 已发布 {fact.published_response_count} 次</span>
              </div>
              <span className={statusClass(fact.status)}>{STATUS_LABELS[fact.status]}</span>
              {fact.repeated_error && <span className="tag fail">重复出错</span>}
              {fact.knowledge_nodes.length > 0 && (
                <span>{fact.knowledge_nodes.map((node) => node.title).join("、")}</span>
              )}
            </div>
          ))}
        </div>
      </details>
      <div className="profile-inline-note">
        <span>{summary.note}</span>
        <button onClick={onOpenExam}>回到错题与批改查看原始事实</button>
      </div>
    </section>
  );
}

function StudentTrendPanel({ snapshot }: { snapshot: StudentProfileSnapshot }) {
  const trend = snapshot.trend;
  if (trend.comparison_status !== "comparable") {
    return (
      <section className="profile-trend-panel">
        <div className="profile-metric-title">
          <b>个人同口径趋势</b>
          <span>尚无可比较基线</span>
        </div>
        <p>{trend.note}</p>
      </section>
    );
  }
  const deltaLabel = (value: number | null) => {
    if (value == null) return "—";
    if (value > 0) return `+${value}`;
    return String(value);
  };
  return (
    <section className="profile-trend-panel">
      <div className="profile-metric-title">
        <b>个人同口径趋势</b>
        <span>对比第 {trend.previous_revision} 版 · {trend.previous_generated_at
          ? formatTime(trend.previous_generated_at)
          : "时间未知"}</span>
      </div>
      <div className="profile-trend-grid">
        <div><span>知识已评估变化</span><b>{deltaLabel(trend.knowledge_assessed_delta)}</b></div>
        <div><span>能力已评估变化</span><b>{deltaLabel(trend.ability_assessed_delta)}</b></div>
        <div><span>需支持节点变化</span><b>{deltaLabel(trend.needs_support_delta)}</b></div>
        <div><span>相对稳定节点变化</span><b>{deltaLabel(trend.stable_delta)}</b></div>
      </div>
      {trend.changed_nodes.length > 0 && (
        <details>
          <summary>查看 {trend.changed_nodes.length} 个发生变化的节点</summary>
          <div className="profile-trend-changes">
            {trend.changed_nodes.map((change) => (
              <div key={`${change.target_type}:${change.target_public_id}`}>
                <b>{change.target_title}</b>
                <span>
                  {PROFILE_STATUS_LABELS[change.previous_status]} →{" "}
                  {PROFILE_STATUS_LABELS[change.current_status]}
                </span>
                <span>{change.mastery_score_delta == null
                  ? "表现分不可比"
                  : `已测表现变化 ${Math.round(change.mastery_score_delta * 100)} 个百分点`}
                </span>
              </div>
            ))}
          </div>
        </details>
      )}
      <p>{trend.note}</p>
    </section>
  );
}

function TeacherAssessmentEditor({
  snapshotPublicId,
  metric,
  current,
  onSaved,
}: {
  snapshotPublicId: string;
  metric: ProfileNodeMetric;
  current: ProfileTeacherAssessment | null;
  onSaved: (value: ProfileTeacherAssessment) => void;
}) {
  const active = current?.state === "active" ? current : null;
  const [assessment, setAssessment] = useState<ProfileTeacherAssessmentValue | "">(
    active?.assessment ?? "",
  );
  const [note, setNote] = useState(active?.note ?? "");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");

  useEffect(() => {
    const next = current?.state === "active" ? current : null;
    setAssessment(next?.assessment ?? "");
    setNote(next?.note ?? "");
    setSaveError("");
  }, [current]);

  const saveAssessment = async (clear: boolean) => {
    if (!clear && !assessment) return;
    setSaving(true);
    setSaveError("");
    try {
      const saved = await saveProfileTeacherAssessment({
        snapshotPublicId,
        nodeMetricPublicId: metric.public_id,
        expectedRevision: current?.revision ?? 0,
        assessment: clear ? null : assessment || null,
        note: clear ? null : note.trim() || null,
      });
      onSaved(saved);
    } catch (reason) {
      setSaveError(String(reason));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="profile-teacher-assessment">
      <div>
        <b>老师补充判断</b>
        <span>与系统结论并列保存，不改写掌握分或原始证据。</span>
      </div>
      <div className="profile-teacher-assessment-fields">
        <select aria-label={`${metric.target_title}老师补充判断`}
          value={assessment}
          onChange={(event) =>
            setAssessment(event.target.value as ProfileTeacherAssessmentValue | "")}>
          <option value="">请选择</option>
          {Object.entries(PROFILE_TEACHER_ASSESSMENT_LABELS).map(([value, label]) => (
            <option value={value} key={value}>{label}</option>
          ))}
        </select>
        <input aria-label={`${metric.target_title}老师补充说明`}
          value={note} maxLength={500} placeholder="可选：课堂观察或教学背景"
          onChange={(event) => setNote(event.target.value)} />
        <button className="primary" disabled={saving || !assessment}
          onClick={() => saveAssessment(false)}>
          {saving ? "保存中…" : "保存判断"}
        </button>
        {current?.state === "active" && (
          <button disabled={saving} onClick={() => saveAssessment(true)}>清除</button>
        )}
      </div>
      {active?.assessment && (
        <span className="profile-teacher-assessment-current">
          当前：{PROFILE_TEACHER_ASSESSMENT_LABELS[active.assessment]}
          {active.note ? ` · ${active.note}` : ""}
          {" "}· 第 {active.revision} 版
        </span>
      )}
      {saveError && <span className="error">{saveError}</span>}
    </div>
  );
}

function ProfileMetricList({
  title,
  metrics,
  onOpenExam,
  snapshotPublicId,
  teacherAssessments,
  onAssessmentSaved,
}: {
  title: string;
  metrics: ProfileNodeMetric[];
  onOpenExam: () => void;
  snapshotPublicId: string;
  teacherAssessments: ProfileTeacherAssessment[];
  onAssessmentSaved: (value: ProfileTeacherAssessment) => void;
}) {
  return (
    <section className="profile-metric-section">
      <div className="profile-metric-title">
        <b>{title}</b>
        <span>{metrics.length} 个范围节点 · 未评估不等于薄弱</span>
      </div>
      <div className="profile-metric-list">
        {metrics.map((metric) => {
          const eligible = !["unassessed", "insufficient_evidence"].includes(metric.status);
          return (
            <details className={`profile-metric ${metric.status}`} key={metric.public_id}>
              <summary>
                <div>
                  <b>{metric.target_title}</b>
                  <span>{metric.explanation}</span>
                </div>
                <span className={`profile-status ${metric.status}`}>
                  {PROFILE_STATUS_LABELS[metric.status]}
                </span>
                <div className="profile-metric-numbers">
                  <span>{eligible && metric.mastery_score != null
                    ? `已测表现 ${Math.round(metric.mastery_score * 100)}%`
                    : PROFILE_STATUS_LABELS[metric.status]}</span>
                  <span>可信度 {PROFILE_CONFIDENCE_LABELS[metric.confidence_level]}</span>
                  <span>{metric.evidence_count} 条证据</span>
                </div>
              </summary>
              <div className="profile-metric-detail">
                <div className="profile-metric-facts">
                  <span>独立组 <b>{metric.independent_group_count}</b></span>
                  <span>跨日期 <b>{metric.distinct_date_count}</b></span>
                  <span>不同来源 <b>{metric.distinct_source_count}</b></span>
                  <span>新鲜度 <b>{PROFILE_FRESHNESS_LABELS[metric.freshness]}</b></span>
                  <span>最近证据 <b>{metric.last_evidence_at ? formatTime(metric.last_evidence_at) : "暂无"}</b></span>
                </div>
                {metric.evidence.length === 0 ? (
                  <div className="profile-no-evidence">当前范围没有可追溯的老师确认逐点证据。</div>
                ) : (
                  <div className="profile-evidence-list">
                    {metric.evidence.map((evidence) => (
                      <div key={evidence.public_id}>
                        <div>
                          <b>{CONTEXT_LABELS[evidence.assessment_context] ?? evidence.assessment_context}</b>
                          <span>{formatTime(evidence.occurred_at)}</span>
                        </div>
                        <span>表现 {Math.round(evidence.value * 100)}%</span>
                        <span>质量 {Math.round(evidence.evidence_quality * 100)}%</span>
                        <span>{evidence.source_type}</span>
                      </div>
                    ))}
                  </div>
                )}
                {metric.evidence.some((evidence) =>
                  ["grading", "correction"].includes(evidence.source_module)) && (
                  <button onClick={onOpenExam}>回到题目批改查看原始证据</button>
                )}
                <TeacherAssessmentEditor
                  snapshotPublicId={snapshotPublicId}
                  metric={metric}
                  current={teacherAssessments.find(
                    (item) => item.node_metric_public_id === metric.public_id,
                  ) ?? null}
                  onSaved={onAssessmentSaved}
                />
              </div>
            </details>
          );
        })}
      </div>
    </section>
  );
}

const RECITATION_EVIDENCE_LABELS: Record<string, string> = {
  recitation_overall: "总体终审",
  recitation_fluency: "表达流畅度",
  recitation_retention: "跨日保持",
};

function RecitationHistory({
  summary,
  frozen,
}: {
  summary: ProfileRecitationSummary;
  frozen: boolean;
}) {
  const total = summary.overall_count + summary.fluency_count + summary.retention_count;
  if (total === 0) return null;
  const valueLabel = (sourceType: string, value: number) =>
    sourceType === "recitation_overall"
      ? (value >= 0.5 ? "通过" : "未通过")
      : `${Math.round(value * 100)}%`;
  return (
    <section className="profile-recitation-history">
      <div className="profile-metric-title">
        <b>背诵内容记录</b>
        <span>
          {frozen ? "随本版快照冻结" : "生成前只读预览"} · 不扩散为具体知识点或高阶能力
        </span>
      </div>
      <div className="profile-recitation-stats">
        <div><span>总体终审</span><b>{summary.overall_count}</b></div>
        <div><span>流畅度</span><b>{summary.fluency_count}</b></div>
        <div><span>跨日保持</span><b>{summary.retention_count}</b></div>
        <div><span>最近记录</span><b>{summary.latest_at ? formatTime(summary.latest_at) : "暂无"}</b></div>
      </div>
      <details>
        <summary>查看 {total} 条内容级证据</summary>
        <div className="profile-evidence-list">
          {summary.evidence.map((evidence) => (
            <div key={evidence.public_id}>
              <div>
                <b>{RECITATION_EVIDENCE_LABELS[evidence.source_type] ?? evidence.source_type}</b>
                <span>{formatTime(evidence.occurred_at)}</span>
              </div>
              <span>{valueLabel(evidence.source_type, evidence.value)}</span>
              <span>质量 {Math.round(evidence.evidence_quality * 100)}%</span>
              <span>{CONTEXT_LABELS[evidence.assessment_context] ?? evidence.assessment_context}</span>
            </div>
          ))}
        </div>
      </details>
    </section>
  );
}

function StudentProfilePanel({
  classId,
  students,
  refreshToken: externalRefreshToken,
  onOpenExam,
}: {
  classId: number;
  students: Student[];
  refreshToken: number;
  onOpenExam: () => void;
}) {
  const today = shanghaiDate();
  const [studentId, setStudentId] = useState<number | null>(students[0]?.id ?? null);
  const [rangeStart, setRangeStart] = useState(shiftShanghaiDate(today, -89));
  const [rangeEnd, setRangeEnd] = useState(today);
  const [preview, setPreview] = useState<StudentProfilePreview | null>(null);
  const [snapshot, setSnapshot] = useState<StudentProfileSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [profileError, setProfileError] = useState("");
  const [generatedRefreshToken, setGeneratedRefreshToken] = useState(0);

  useEffect(() => {
    setStudentId((current) =>
      current != null && students.some((student) => student.id === current)
        ? current
        : students[0]?.id ?? null);
  }, [students]);

  useEffect(() => {
    if (studentId == null || !rangeStart || !rangeEnd || rangeStart > rangeEnd) {
      setPreview(null);
      setSnapshot(null);
      if (rangeStart && rangeEnd && rangeStart > rangeEnd) {
        setProfileError("开始日期不能晚于结束日期");
      }
      return;
    }
    let current = true;
    setLoading(true);
    setProfileError("");
    Promise.all([
      previewStudentProfile({ classId, studentId, rangeStart, rangeEnd }),
      loadLatestStudentProfile(classId, studentId),
    ])
      .then(([nextPreview, latest]) => {
        if (!current) return;
        setPreview(nextPreview);
        setSnapshot(latest);
      })
      .catch((reason) => {
        if (!current) return;
        setPreview(null);
        setProfileError(String(reason));
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [classId, externalRefreshToken, generatedRefreshToken, rangeEnd, rangeStart, studentId]);

  const generate = async () => {
    if (studentId == null || !preview?.can_generate) return;
    setGenerating(true);
    setProfileError("");
    try {
      const generated = await generateStudentProfile({
        classId,
        studentId,
        rangeStart,
        rangeEnd,
      });
      setSnapshot(generated);
      setGeneratedRefreshToken((value) => value + 1);
    } catch (reason) {
      setProfileError(String(reason));
    } finally {
      setGenerating(false);
    }
  };

  const updateTeacherAssessment = (saved: ProfileTeacherAssessment) => {
    setSnapshot((current) => {
      if (!current || current.public_id !== saved.snapshot_public_id) return current;
      const next = current.teacher_assessments.filter(
        (item) => item.node_metric_public_id !== saved.node_metric_public_id,
      );
      next.push(saved);
      return { ...current, teacher_assessments: next };
    });
  };

  if (students.length === 0) {
    return <div className="empty-state">当前班级没有启用学生，无法生成个人掌握快照。</div>;
  }

  return (
    <div className="student-profile-panel">
      <div className="profile-toolbar">
        <div>
          <b>个人学习掌握快照</b>
          <span>老师按需生成；只使用已发布、老师确认且链接明确的逐点证据。</span>
        </div>
        <div>
          <label>
            <span>学生</span>
            <select aria-label="掌握快照学生" value={studentId ?? ""}
              onChange={(event) => setStudentId(Number(event.target.value))}>
              {students.map((student) => (
                <option value={student.id} key={student.id}>
                  {student.student_no}号 {student.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>从</span>
            <input type="date" value={rangeStart}
              onChange={(event) => setRangeStart(event.target.value)} />
          </label>
          <label>
            <span>到</span>
            <input type="date" value={rangeEnd}
              onChange={(event) => setRangeEnd(event.target.value)} />
          </label>
        </div>
      </div>

      <div className="learning-safety">
        <b>计算边界</b>
        同一道题同一天只算一个独立组；订正、巩固复测和开卷已降权。错题恢复事实与老师补充判断不会覆盖系统证据结论。
      </div>
      {profileError && <div className="error">{profileError}</div>}
      {loading && !preview && <div className="loading">正在核对可用学习证据…</div>}

      {preview && (
        <section className="profile-preview">
          <div className="profile-preview-head">
            <div>
              <b>生成前预览</b>
              <span>{preview.student.student_no}号 {preview.student.name} · {preview.range_start} 至 {preview.range_end}</span>
            </div>
            <button className="primary" disabled={!preview.can_generate || generating}
              onClick={generate}>
              {generating ? "生成中…" : "确认生成快照"}
            </button>
          </div>
          <div className="profile-preview-stats">
            <div><span>正式逐点证据</span><b>{preview.counts.mapped_formal_evidence}</b></div>
            <div><span>知识覆盖</span><b>{preview.counts.knowledge_node_assessed} / {preview.counts.knowledge_node_total}</b></div>
            <div><span>背诵内容记录</span><b>
              {preview.recitation_summary.overall_count
                + preview.recitation_summary.fluency_count
                + preview.recitation_summary.retention_count}
            </b></div>
            <div><span>知识达门槛</span><b>{preview.counts.knowledge_node_eligible}</b></div>
          </div>
          {preview.blocker && <div className="profile-blocker">{preview.blocker}</div>}
          <div className="profile-preview-notes">
            <span>{preview.scope_note}</span>
            <span>{preview.evidence_note}</span>
            {(preview.counts.machine_only_excluded
              + preview.counts.unmapped_formal_excluded
              + preview.counts.unsupported_contract_excluded) > 0 && (
              <span>
                未纳入：纯机器 {preview.counts.machine_only_excluded}、
                未映射逐点 {preview.counts.unmapped_formal_excluded}、
                不支持的来源契约 {preview.counts.unsupported_contract_excluded}。
              </span>
            )}
            {preview.counts.teacher_overall_excluded > 0 && (
              <span>
                另有 {preview.counts.teacher_overall_excluded} 条仅总体确认记录只进入背诵内容历史，
                不参与知识点或能力计算。
              </span>
            )}
          </div>
          <RecitationHistory summary={preview.recitation_summary} frozen={false} />
          <ProfileWrongbookHistory summary={preview.wrongbook_summary} frozen={false}
            onOpenExam={onOpenExam} />
        </section>
      )}

      {snapshot && (
        <section className="profile-snapshot">
          <div className="profile-snapshot-head">
            <div>
              <b>第 {snapshot.revision} 版掌握快照</b>
              <span>
                {snapshot.range_start} 至 {snapshot.range_end} · 数据截至 {formatTime(snapshot.evidence_cutoff_at)}
                {" "}· 规则第 {snapshot.policy.revision} 版
              </span>
            </div>
            <span className={snapshot.is_stale ? "tag fail" : "tag pass"}>
              {snapshot.is_stale ? "有新证据，建议重生成" : "当前快照"}
            </span>
          </div>
          {snapshot.stale_reason && <div className="profile-stale">{snapshot.stale_reason}</div>}
          <div className="profile-snapshot-summary">
            <div>
              <span>知识已评估</span>
              <b>{snapshot.knowledge_node_assessed} / {snapshot.knowledge_node_total}</b>
            </div>
            <div>
              <span>知识达门槛</span>
              <b>{snapshot.knowledge_node_eligible}</b>
            </div>
            <div>
              <span>能力已评估</span>
              <b>{snapshot.ability_node_assessed} / {snapshot.ability_node_total}</b>
            </div>
            <div>
              <span>证据总数</span>
              <b>{snapshot.evidence_count}</b>
            </div>
          </div>
          <StudentTrendPanel snapshot={snapshot} />
          <RecitationHistory summary={snapshot.recitation_summary} frozen />
          <ProfileWrongbookHistory summary={snapshot.wrongbook_summary} frozen
            onOpenExam={onOpenExam} />
          <ProfileMetricList title="知识掌握" metrics={snapshot.knowledge_metrics}
            onOpenExam={onOpenExam} snapshotPublicId={snapshot.public_id}
            teacherAssessments={snapshot.teacher_assessments}
            onAssessmentSaved={updateTeacherAssessment} />
          <ProfileMetricList title="学科能力" metrics={snapshot.ability_metrics}
            onOpenExam={onOpenExam} snapshotPublicId={snapshot.public_id}
            teacherAssessments={snapshot.teacher_assessments}
            onAssessmentSaved={updateTeacherAssessment} />
          <div className="profile-footnote">
            本快照不修改成绩、任务、错题或上游证据；历史版本保留，不生成学生排名。
          </div>
        </section>
      )}
    </div>
  );
}

interface CauseEditorProps {
  classId: number;
  item: WrongbookQuestion;
  onSaved: (review: ErrorCauseReview) => void;
}

function CauseEditor({ classId, item, onSaved }: CauseEditorProps) {
  const [open, setOpen] = useState(false);
  const [selected, setSelected] = useState<string[]>(item.cause_review?.cause_codes ?? []);
  const [note, setNote] = useState(item.cause_review?.teacher_note ?? "");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");

  useEffect(() => {
    setSelected(item.cause_review?.cause_codes ?? []);
    setNote(item.cause_review?.teacher_note ?? "");
  }, [item.cause_review]);

  const toggleCause = (code: string) => {
    setSelected((current) => {
      if (current.includes(code)) return current.filter((value) => value !== code);
      if (current.length >= 3) return current;
      return [...current, code];
    });
  };

  const save = async () => {
    setSaving(true);
    setSaveError("");
    try {
      const review = await confirmWrongbookErrorCauses({
        classId,
        studentId: item.student_id,
        questionVersionPublicId: item.question_version_id,
        gradeDecisionPublicId: item.latest_error_grade_decision_public_id,
        publicationPublicId: item.latest_error_publication_public_id,
        causeCodes: selected,
        teacherNote: note.trim() || null,
      });
      onSaved(review);
      setOpen(false);
    } catch (reason) {
      setSaveError(String(reason));
    } finally {
      setSaving(false);
    }
  };

  const labelFor = (code: string) =>
    item.cause_options.find((option) => option.code === code)?.label ?? code;

  return (
    <div className="wrongbook-causes">
      {item.cause_review && (
        <div className="wrongbook-cause-summary">
          <b>老师确认错因</b>
          {item.cause_review.cause_codes.map((code) => (
            <span key={code}>{labelFor(code)}</span>
          ))}
          {item.cause_review.teacher_note && <em>{item.cause_review.teacher_note}</em>}
        </div>
      )}
      {!open ? (
        <button className="wrongbook-cause-open" onClick={() => setOpen(true)}>
          {item.cause_review ? "修改错因" : "确认错因"}
        </button>
      ) : (
        <div className="wrongbook-cause-editor">
          <div className="wrongbook-cause-title">
            <b>选择主要错因</b>
            <span>最多 3 个；只有保存后才进入正式统计</span>
          </div>
          <div className="wrongbook-cause-options">
            {item.cause_options.map((option) => {
              const checked = selected.includes(option.code);
              return (
                <label
                  className={checked ? "selected" : ""}
                  title={option.description}
                  key={option.code}
                >
                  <input
                    type="checkbox"
                    checked={checked}
                    disabled={!checked && selected.length >= 3}
                    onChange={() => toggleCause(option.code)}
                  />
                  <span>{option.label}</span>
                </label>
              );
            })}
          </div>
          <textarea
            aria-label="错因备注"
            value={note}
            maxLength={500}
            onChange={(event) => setNote(event.target.value)}
            placeholder={selected.includes("other") ? "选择“其他”时请补充说明" : "补充说明（可选）"}
          />
          {saveError && <div className="error">{saveError}</div>}
          <div className="wrongbook-cause-actions">
            <button
              onClick={() => {
                setOpen(false);
                setSelected(item.cause_review?.cause_codes ?? []);
                setNote(item.cause_review?.teacher_note ?? "");
                setSaveError("");
              }}
            >
              取消
            </button>
            <button className="primary" disabled={selected.length === 0 || saving} onClick={save}>
              {saving ? "保存中…" : "保存错因"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function CorrectionAction({
  classId,
  item,
  onCreated,
  onOpenExam,
}: {
  classId: number;
  item: WrongbookQuestion;
  onCreated: (assignment: CorrectionAssignment) => void;
  onOpenExam: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState("");

  const create = async () => {
    setCreating(true);
    setCreateError("");
    try {
      const assignment = await createWrongbookSingleCorrection({
        classId,
        studentId: item.student_id,
        questionVersionPublicId: item.question_version_id,
        sourceGradeDecisionPublicId: item.latest_error_grade_decision_public_id,
        sourcePublicationPublicId: item.latest_error_publication_public_id,
      });
      onCreated(assignment);
      setConfirming(false);
    } catch (reason) {
      setCreateError(String(reason));
    } finally {
      setCreating(false);
    }
  };

  if (item.correction_assignment) {
    return (
      <div className="wrongbook-correction-summary">
        <div>
          <span>{CORRECTION_STATUS_LABELS[item.correction_assignment.status]}</span>
          <b title={item.correction_assignment.assessment_title}>
            {item.correction_assignment.assessment_title}
          </b>
        </div>
        <button onClick={onOpenExam}>
          {item.correction_assignment.status === "published" ? "查看批改" : "去上传批改"}
        </button>
      </div>
    );
  }

  if (item.status !== "needs_correction") return null;

  return (
    <div className="wrongbook-correction">
      {!confirming ? (
        <button className="primary" onClick={() => setConfirming(true)}>建立订正</button>
      ) : (
        <div className="wrongbook-correction-confirm">
          <b>为 {item.student_no} 号 {item.student_name} 建立这道订正？</b>
          <span>只建立这一题并等待照片上传；不会修改原成绩、原错因或掌握结论。</span>
          {createError && <div className="error">{createError}</div>}
          <div>
            <button
              disabled={creating}
              onClick={() => {
                setConfirming(false);
                setCreateError("");
              }}
            >
              取消
            </button>
            <button className="primary" disabled={creating} onClick={create}>
              {creating ? "建立中…" : "确认建立"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function ReinforcementAction({
  classId,
  item,
  onCreated,
  onOpenExam,
}: {
  classId: number;
  item: WrongbookQuestion;
  onCreated: (assignment: ReinforcementAssignment) => void;
  onOpenExam: () => void;
}) {
  const [preview, setPreview] = useState<ReinforcementSuggestion | null>(null);
  const [loadingPreview, setLoadingPreview] = useState(false);
  const [creating, setCreating] = useState(false);
  const [actionError, setActionError] = useState("");

  if (item.reinforcement_assignment) {
    const assignment = item.reinforcement_assignment;
    return (
      <div className="wrongbook-reinforcement-summary">
        <div>
          <span>{REINFORCEMENT_STATUS_LABELS[assignment.status]} · {assignment.due_date}</span>
          <b>{assignment.priority === "high" ? "重复出错，已按高优先级安排" : "跨日期再次作答"}</b>
        </div>
        <button onClick={onOpenExam}>
          {assignment.status === "published" ? "查看批改" : "去上传批改"}
        </button>
      </div>
    );
  }
  if (item.status !== "corrected_once") return null;

  const scope = {
    classId,
    studentId: item.student_id,
    questionVersionPublicId: item.question_version_id,
    sourceGradeDecisionPublicId: item.latest_error_grade_decision_public_id,
    sourcePublicationPublicId: item.latest_error_publication_public_id,
  };
  const loadPreview = async () => {
    setLoadingPreview(true);
    setActionError("");
    try {
      setPreview(await previewWrongbookReinforcement(scope));
    } catch (reason) {
      setActionError(String(reason));
    } finally {
      setLoadingPreview(false);
    }
  };
  const confirm = async () => {
    if (!preview) return;
    setCreating(true);
    setActionError("");
    try {
      const assignment = await confirmWrongbookReinforcement({
        ...scope,
        expectedPolicyPublicId: preview.policy_public_id,
        expectedDueDate: preview.suggested_due_date,
        previewedAsOfDate: preview.previewed_as_of_date,
      });
      onCreated(assignment);
      setPreview(null);
    } catch (reason) {
      setActionError(String(reason));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="wrongbook-reinforcement">
      {!preview ? (
        <>
          <button className="primary" disabled={loadingPreview} onClick={loadPreview}>
            {loadingPreview ? "计算中…" : "安排巩固"}
          </button>
          {actionError && <div className="error">{actionError}</div>}
        </>
      ) : (
        <div className="wrongbook-reinforcement-confirm">
          <div>
            <b>建议 {preview.suggested_due_date} 再做一次</b>
            <span>{preview.reason}</span>
            <small>
              当天已有 {preview.existing_task_count} / {preview.daily_limit_per_student} 项；
              规则第 {preview.policy_revision} 版
            </small>
          </div>
          {actionError && <div className="error">{actionError}</div>}
          <div className="wrongbook-reinforcement-actions">
            <button
              disabled={creating}
              onClick={() => {
                setPreview(null);
                setActionError("");
              }}
            >
              取消
            </button>
            <button className="primary" disabled={creating} onClick={confirm}>
              {creating ? "安排中…" : "确认安排"}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function SchedulePolicyPanel({
  policy,
  onSaved,
}: {
  policy: SchedulePolicy;
  onSaved: (policy: SchedulePolicy) => void;
}) {
  const [delay, setDelay] = useState(policy.default_delay_days);
  const [dailyLimit, setDailyLimit] = useState(policy.daily_limit_per_student);
  const [skipWeekend, setSkipWeekend] = useState(policy.weekend_policy === "next_workday");
  const [skipHoliday, setSkipHoliday] = useState(policy.holiday_policy === "next_workday");
  const [holidays, setHolidays] = useState(policy.holidays);
  const [newHolidayDate, setNewHolidayDate] = useState("");
  const [newHolidayLabel, setNewHolidayLabel] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState("");

  useEffect(() => {
    setDelay(policy.default_delay_days);
    setDailyLimit(policy.daily_limit_per_student);
    setSkipWeekend(policy.weekend_policy === "next_workday");
    setSkipHoliday(policy.holiday_policy === "next_workday");
    setHolidays(policy.holidays);
  }, [policy]);

  const addHoliday = () => {
    if (!newHolidayDate || !newHolidayLabel.trim()) return;
    setHolidays((current) => [
      ...current.filter((item) => item.calendar_date !== newHolidayDate),
      { calendar_date: newHolidayDate, label: newHolidayLabel.trim() },
    ].sort((left, right) => left.calendar_date.localeCompare(right.calendar_date)));
    setNewHolidayDate("");
    setNewHolidayLabel("");
  };
  const save = async () => {
    setSaving(true);
    setSaveError("");
    try {
      const saved = await updateWrongbookSchedulePolicy({
        defaultDelayDays: delay,
        dailyLimitPerStudent: dailyLimit,
        weekendPolicy: skipWeekend ? "next_workday" : "allow",
        holidayPolicy: skipHoliday ? "next_workday" : "allow",
        maxShiftDays: policy.max_shift_days,
        holidays,
      });
      onSaved(saved);
    } catch (reason) {
      setSaveError(String(reason));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="schedule-policy-panel">
      <div className="schedule-policy-title">
        <div>
          <b>巩固规则</b>
          <span>默认自动计算日期，但只有老师确认后才会建立任务。</span>
        </div>
        <span>当前第 {policy.revision} 版</span>
      </div>
      <div className="schedule-policy-main">
        <label>
          <span>订正后间隔</span>
          <div><input type="number" min={1} max={60} value={delay}
            onChange={(event) => setDelay(Number(event.target.value))} /> 天</div>
        </label>
        <label>
          <span>每名学生每天最多</span>
          <div><input type="number" min={1} max={20} value={dailyLimit}
            onChange={(event) => setDailyLimit(Number(event.target.value))} /> 项</div>
        </label>
        <label className="schedule-policy-check">
          <input type="checkbox" checked={skipWeekend}
            onChange={(event) => setSkipWeekend(event.target.checked)} />
          周末顺延
        </label>
        <label className="schedule-policy-check">
          <input type="checkbox" checked={skipHoliday}
            onChange={(event) => setSkipHoliday(event.target.checked)} />
          登记假期顺延
        </label>
      </div>
      <details className="schedule-holidays">
        <summary>校历假期（{holidays.length} 天）</summary>
        <div className="schedule-holiday-add">
          <input aria-label="假期日期" type="date" value={newHolidayDate}
            onChange={(event) => setNewHolidayDate(event.target.value)} />
          <input aria-label="假期名称" value={newHolidayLabel} maxLength={40}
            placeholder="如：校运动会" onChange={(event) => setNewHolidayLabel(event.target.value)} />
          <button disabled={!newHolidayDate || !newHolidayLabel.trim()} onClick={addHoliday}>
            添加
          </button>
        </div>
        <div className="schedule-holiday-list">
          {holidays.map((holiday) => (
            <span key={holiday.calendar_date}>
              {holiday.calendar_date} · {holiday.label}
              <button aria-label={`删除 ${holiday.label}`}
                onClick={() => setHolidays((current) =>
                  current.filter((item) => item.calendar_date !== holiday.calendar_date))}>×</button>
            </span>
          ))}
          {holidays.length === 0 && <em>暂未登记假期</em>}
        </div>
      </details>
      {saveError && <div className="error">{saveError}</div>}
      <div className="schedule-policy-actions">
        <span>修改后生成新版本，已经安排的任务日期不会被静默改动。</span>
        <button className="primary" disabled={saving} onClick={save}>
          {saving ? "保存中…" : "保存规则"}
        </button>
      </div>
    </section>
  );
}

function ItemCard({
  classId,
  item,
  onCauseSaved,
  onCorrectionCreated,
  onReinforcementCreated,
  onOpenExam,
}: {
  classId: number;
  item: WrongbookQuestion;
  onCauseSaved: (review: ErrorCauseReview) => void;
  onCorrectionCreated: (assignment: CorrectionAssignment) => void;
  onReinforcementCreated: (assignment: ReinforcementAssignment) => void;
  onOpenExam: () => void;
}) {
  return (
    <article className={`wrongbook-item ${item.repeated_error ? "repeated" : ""}`}>
      <div className="wrongbook-student">
        <span className="avt q">{item.student_no}</span>
        <div>
          <b>{item.student_name}</b>
          <span>{item.student_no} 号 · {item.latest_assessment_title}</span>
        </div>
        <span className={statusClass(item.status)}>{STATUS_LABELS[item.status]}</span>
      </div>
      <div className="wrongbook-question">
        <div className="wrongbook-question-head">
          <span className="cno">{QUESTION_TYPE_LABELS[item.question_type] ?? item.question_type}</span>
          <b>{item.stem}</b>
        </div>
        <div className="wrongbook-facts">
          <span>最近得分 <b>{score(item.latest_score)} / {score(item.latest_max_score)}</b></span>
          <span>{CONTEXT_LABELS[item.latest_assessment_context] ?? item.latest_assessment_context}</span>
          <span>已发布作答 {item.published_response_count} 次</span>
          <span>非满分 {item.error_response_count} 次</span>
          {item.repeated_error && <span className="bad-text">重复出错</span>}
        </div>
        <div className="wrongbook-labels">
          {item.knowledge_nodes.map((node) => (
            <span className="wrongbook-label knowledge" key={node.public_id}>知识 · {node.title}</span>
          ))}
          {item.ability_dimensions.map((node) => (
            <span className="wrongbook-label ability" key={node.public_id}>能力 · {node.title}</span>
          ))}
          {item.knowledge_nodes.length === 0 && item.ability_dimensions.length === 0 && (
            <span className="wrongbook-unlinked">尚未绑定已确认的知识点或能力，不影响错题事实</span>
          )}
        </div>
        <CauseEditor classId={classId} item={item} onSaved={onCauseSaved} />
        <CorrectionAction
          classId={classId}
          item={item}
          onCreated={onCorrectionCreated}
          onOpenExam={onOpenExam}
        />
        <ReinforcementAction
          classId={classId}
          item={item}
          onCreated={onReinforcementCreated}
          onOpenExam={onOpenExam}
        />
      </div>
      <div className="wrongbook-time">
        <span>最近错误</span>
        <b>{formatTime(item.last_error_at)}</b>
        <span>最近发布</span>
        <b>{formatTime(item.latest_response_at)}</b>
      </div>
    </article>
  );
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
