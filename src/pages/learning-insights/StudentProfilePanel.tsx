import {
  ProfileNodeStatus,
  ProfileTeacherAssessmentValue,
  StudentProfileSnapshot,
  ProfileWrongbookSummary,
  ProfileNodeMetric,
  ProfileTeacherAssessment,
  saveProfileTeacherAssessment,
  ProfileRecitationSummary,
  ProfileScopeOption,
  StudentProfilePreview,
  loadProfileScopeOptions,
  previewStudentProfile,
  loadLatestStudentProfile,
  generateStudentProfile,
  createStudentProfileReportSnapshot,
  writeStudentProfileReportSnapshot,
} from "../../api/learning";
import {
  formatTime,
  statusClass,
  STATUS_LABELS,
  CONTEXT_LABELS,
  shanghaiDate,
  shiftShanghaiDate,
  newRequestKey,
} from "./shared";
import { useState, useEffect, useMemo } from "react";
import { Student } from "../../api/manage";
import { save } from "@tauri-apps/plugin-dialog";

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

function profileReportFileName(snapshot: StudentProfileSnapshot) {
  return `${snapshot.student.student_no}号${snapshot.student.name}_个人学习报告_${snapshot.range_start}至${snapshot.range_end}.html`
    .replace(/[<>:"/\\|?*]/g, "_");
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

export function StudentProfilePanel({
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
  const [scopeOptions, setScopeOptions] = useState<ProfileScopeOption[]>([]);
  const [scopeSelectorKey, setScopeSelectorKey] = useState("auto_evidence_maps");
  const [preview, setPreview] = useState<StudentProfilePreview | null>(null);
  const [snapshot, setSnapshot] = useState<StudentProfileSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [reporting, setReporting] = useState(false);
  const [profileError, setProfileError] = useState("");
  const [reportSuccess, setReportSuccess] = useState("");
  const [generatedRefreshToken, setGeneratedRefreshToken] = useState(0);
  const selectedScope = useMemo(
    () => scopeOptions.find((item) => item.selector_key === scopeSelectorKey)
      ?? scopeOptions[0]
      ?? null,
    [scopeOptions, scopeSelectorKey],
  );

  useEffect(() => {
    let current = true;
    loadProfileScopeOptions()
      .then((items) => {
        if (!current) return;
        setScopeOptions(items);
        setScopeSelectorKey((selected) =>
          items.some((item) => item.selector_key === selected)
            ? selected
            : items[0]?.selector_key ?? "auto_evidence_maps");
      })
      .catch((reason) => {
        if (current) setProfileError(String(reason));
      });
    return () => {
      current = false;
    };
  }, []);

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
    setReportSuccess("");
    Promise.all([
      previewStudentProfile({
        classId,
        studentId,
        rangeStart,
        rangeEnd,
        scopeSelectorKind: selectedScope?.selector_kind ?? "auto_evidence_maps",
        scopeSelectorPublicId: selectedScope?.selector_public_id ?? null,
      }),
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
  }, [
    classId,
    externalRefreshToken,
    generatedRefreshToken,
    rangeEnd,
    rangeStart,
    selectedScope,
    studentId,
  ]);

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
        scopeSelectorKind: selectedScope?.selector_kind ?? "auto_evidence_maps",
        scopeSelectorPublicId: selectedScope?.selector_public_id ?? null,
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

  const exportProfileReport = async () => {
    if (!snapshot || snapshot.is_stale) return;
    setReporting(true);
    setProfileError("");
    setReportSuccess("");
    try {
      const outputPath = await save({
        defaultPath: profileReportFileName(snapshot),
        filters: [{ name: "可打印网页", extensions: ["html"] }],
      });
      if (!outputPath) return;
      const report = await createStudentProfileReportSnapshot(
        newRequestKey("student-profile-report"),
        snapshot.public_id,
        snapshot.payload_sha256,
      );
      const written = await writeStudentProfileReportSnapshot(report.public_id, outputPath);
      setReportSuccess(`已导出 ${written.file_name}。这是教师内部文件，不会自动发送。`);
    } catch (reason) {
      setProfileError(String(reason));
    } finally {
      setReporting(false);
    }
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
            <span>教材范围</span>
            <select
              aria-label="掌握快照教材范围"
              value={scopeSelectorKey}
              onChange={(event) => {
                setScopeSelectorKey(event.target.value);
                setPreview(null);
              }}
            >
              {scopeOptions.map((option) => (
                <option value={option.selector_key} key={option.selector_key}>
                  {option.node_type === "textbook" ? "本册 · " : ""}
                  {option.label}
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
              <span>
                {preview.student.student_no}号 {preview.student.name} ·
                {preview.range_start} 至 {preview.range_end} · {preview.scope_selection.path}
              </span>
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
                不支持的来源契约 {preview.counts.unsupported_contract_excluded}、
                教材范围外 {preview.counts.out_of_scope_excluded}。
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
                {snapshot.range_start} 至 {snapshot.range_end} · {snapshot.scope_selection.path}
                · 数据截至 {formatTime(snapshot.evidence_cutoff_at)}
                {" "}· 规则第 {snapshot.policy.revision} 版
              </span>
            </div>
            <div className="profile-snapshot-actions">
              <span className={snapshot.is_stale ? "tag fail" : "tag pass"}>
                {snapshot.is_stale ? "有新证据，建议重生成" : "当前快照"}
              </span>
              <button
                className="primary"
                disabled={snapshot.is_stale || reporting}
                onClick={exportProfileReport}
                title={snapshot.is_stale ? "请先重新生成当前掌握快照" : "生成教师内部可打印报告"}
              >
                {reporting ? "导出中…" : "导出个人学习报告"}
              </button>
            </div>
          </div>
          {snapshot.stale_reason && <div className="profile-stale">{snapshot.stale_reason}</div>}
          {reportSuccess && <div className="success">{reportSuccess}</div>}
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
            个人报告只供教师内部使用，由老师核对后决定是否线下提供。
          </div>
        </section>
      )}
    </div>
  );
}
