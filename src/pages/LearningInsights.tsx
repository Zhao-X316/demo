import { useEffect, useMemo, useState } from "react";

import {
  ClassWrongbookDashboard,
  confirmWrongbookReinforcement,
  confirmWrongbookErrorCauses,
  CorrectionAssignment,
  CorrectionAssignmentStatus,
  createWrongbookSingleCorrection,
  ErrorCauseReview,
  loadClassWrongbookDashboard,
  loadWrongbookSchedulePolicy,
  previewWrongbookReinforcement,
  ReinforcementAssignment,
  ReinforcementAssignmentStatus,
  ReinforcementSuggestion,
  SchedulePolicy,
  updateWrongbookSchedulePolicy,
  WrongbookQuestion,
  WrongbookStatus,
} from "../api/learning";
import { Class, classesList } from "../api/manage";

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

  useEffect(() => {
    classesList()
      .then((items) => {
        setClasses(items);
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
          <div className="sub">同一入口先呈现可核对的错题事实；掌握分析在证据规则验证后接入。</div>
        </div>
        <div className="learning-scope">
          <label>
            <span>班级</span>
            <select value={classId ?? ""} onChange={(event) => setClassId(Number(event.target.value))}>
              {classes.map((item) => <option value={item.id} key={item.id}>{item.name}</option>)}
            </select>
          </label>
          <button disabled={!policy} onClick={() => setPolicyOpen((current) => !current)}>
            {policyOpen ? "收起规则" : "巩固规则"}
          </button>
          <button onClick={() => setRefreshTick((value) => value + 1)}>刷新</button>
        </div>
      </div>

      {policyError && <div className="error">{policyError}</div>}
      {policyOpen && policy && <SchedulePolicyPanel policy={policy} onSaved={setPolicy} />}

      <div className="tabs learning-tabs">
        <button className="tab active">错题事实</button>
        <button className="tab" disabled title="需完成跨日期证据与权重验证">
          掌握分析 · 待验证
        </button>
      </div>

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
    </div>
  );
}
