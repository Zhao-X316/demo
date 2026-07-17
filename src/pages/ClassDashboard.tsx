import { useEffect, useMemo, useState } from "react";
import {
  AppModule,
  ClassOperationsDashboard,
  DashboardTargetView,
  loadClassOperationsDashboard,
} from "../api/classDashboard";
import { Class, classesList } from "../api/manage";

interface Props {
  onNavigate: (module: AppModule, view: DashboardTargetView | "students") => void;
}

const RECITATION_STATUS: Record<string, string> = {
  not_scheduled: "今日未布置",
  not_submitted: "未交",
  submitted: "已交待处理",
  partial: "部分完成",
  recognition_failed: "识别失败",
  pending_review: "待老师终审",
  completed: "已完成",
};

const EXAM_STATUS: Record<string, string> = {
  not_assigned: "暂无进行中作业",
  not_submitted: "未上传",
  ingesting: "图片处理中",
  grading: "批改中",
  ready_to_publish: "待发布",
  partial: "部分完成",
  published: "已发布",
};

function localDate() {
  const now = new Date();
  const year = now.getFullYear();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function tagClass(status: string) {
  if (status === "completed" || status === "published") return "tag pass";
  if (status === "recognition_failed") return "tag fail";
  if (["pending_review", "ready_to_publish", "grading", "ingesting", "partial"].includes(status)) {
    return "tag wait";
  }
  return "tag";
}

export default function ClassDashboard({ onNavigate }: Props) {
  const [classes, setClasses] = useState<Class[]>([]);
  const [classId, setClassId] = useState<number | null>(null);
  const [asOfDate, setAsOfDate] = useState(localDate);
  const [dashboard, setDashboard] = useState<ClassOperationsDashboard | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [refreshTick, setRefreshTick] = useState(0);

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
    if (classId === null) {
      setDashboard(null);
      return;
    }
    let current = true;
    setLoading(true);
    setError("");
    loadClassOperationsDashboard(classId, asOfDate)
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
  }, [classId, asOfDate, refreshTick]);

  const exceptionCount = useMemo(
    () =>
      (dashboard?.recitation.recognition_failure_count ?? 0)
      + (dashboard?.exam.open_pipeline_issue_count ?? 0),
    [dashboard],
  );

  if (classes.length === 0 && !loading) {
    return (
      <div className="page class-dashboard">
        <div className="page-head"><h1>班级概览</h1></div>
        <div className="sub">先建立班级并加入学生，系统才有明确、可解释的统计分母。</div>
        {error && <div className="error">{error}</div>}
        <div className="empty-state">
          暂无班级<br />
          <button className="primary" onClick={() => onNavigate("recitation", "students")}>
            去建立班级
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="page class-dashboard">
      <div className="page-head dashboard-head">
        <div>
          <h1>班级概览</h1>
          <div className="sub">只展示完成、待处理和异常事实；掌握度与能力结论将在证据规则验证后单独提供。</div>
        </div>
        <div className="dashboard-scope">
          <label>
            <span>班级</span>
            <select value={classId ?? ""} onChange={(event) => setClassId(Number(event.target.value))}>
              {classes.map((item) => <option value={item.id} key={item.id}>{item.name}</option>)}
            </select>
          </label>
          <label>
            <span>背诵日期</span>
            <input type="date" value={asOfDate} onChange={(event) => setAsOfDate(event.target.value)} />
          </label>
          <button onClick={() => setRefreshTick((value) => value + 1)}>刷新</button>
        </div>
      </div>

      {error && <div className="error">{error}</div>}
      {loading && !dashboard && <div className="loading">正在汇总班级运行事实…</div>}
      {dashboard && (
        <>
          <div className="dashboard-context">
            <b>{dashboard.class.name}</b>
            <span>{dashboard.class.enabled_student_count} 名启用学生</span>
            {dashboard.class.term && <span>{dashboard.class.term}</span>}
            {dashboard.class.textbook && <span>{dashboard.class.textbook}</span>}
            <span className="spacer" />
            <span>计算于 {new Date(dashboard.meta.calculated_at).toLocaleString()}</span>
          </div>

          <div className="dashboard-stats">
            <button className="dashboard-stat ok" onClick={() => onNavigate("recitation", "today")}>
              <span>背诵完成</span>
              <b className="num">{dashboard.recitation.completed_student_count}<small> / {dashboard.recitation.expected_student_count} 人</small></b>
              <em>{dashboard.recitation.confirmed_task_count} / {dashboard.recitation.expected_task_count} 项已终审</em>
            </button>
            <button className="dashboard-stat" onClick={() => onNavigate("exam", "exam")}>
              <span>当前作业已上传</span>
              <b className="num">{dashboard.exam.submitted_submission_count}<small> / {dashboard.exam.expected_submission_count} 份</small></b>
              <em>{dashboard.exam.active_assessment_count} 个进行中作业</em>
            </button>
            <button className="dashboard-stat warn" onClick={() => onNavigate("recitation", "today")}>
              <span>背诵待老师终审</span>
              <b className="num">{dashboard.recitation.pending_teacher_review_count}</b>
              <em>{dashboard.recitation.overdue_pending_review_count} 项已经逾期</em>
            </button>
            <button
              className="dashboard-stat bad"
              onClick={() => dashboard.recitation.recognition_failure_count > 0
                ? onNavigate("recitation", "desk")
                : onNavigate("exam", "exam")}
            >
              <span>识别 / 导入异常</span>
              <b className="num">{exceptionCount}</b>
              <em>背诵 {dashboard.recitation.recognition_failure_count} · 作业 {dashboard.exam.open_pipeline_issue_count}</em>
            </button>
            <button className="dashboard-stat warn" onClick={() => onNavigate("exam", "exam")}>
              <span>作业待发布</span>
              <b className="num">{dashboard.exam.ready_to_publish_attempt_count}</b>
              <em>成绩必须由老师显式发布</em>
            </button>
          </div>

          <div className="dashboard-grid">
            <section className="dashboard-panel">
              <div className="dashboard-panel-head">
                <div>
                  <b>现在需要处理</b>
                  <span>按风险排序，点击后回到原工作台处理</span>
                </div>
                <span className="tag">{dashboard.actions.length} 类</span>
              </div>
              {dashboard.actions.length === 0 ? (
                <div className="empty-state compact">当前没有待处理项。</div>
              ) : (
                <div className="dashboard-actions">
                  {dashboard.actions.map((action) => (
                    <button
                      key={action.kind}
                      className={`dashboard-action ${action.severity}`}
                      onClick={() => onNavigate(action.target_module, action.target_view)}
                    >
                      <span className="num">{action.count}</span>
                      <div><b>{action.title}</b><em>{action.detail}</em></div>
                      <i>去处理 →</i>
                    </button>
                  ))}
                </div>
              )}
            </section>

            <section className="dashboard-panel dashboard-rules">
              <div className="dashboard-panel-head"><b>本页统计口径</b></div>
              <div>
                <span>背诵</span>
                <p>{dashboard.recitation.denominator_note}</p>
              </div>
              <div>
                <span>作业</span>
                <p>{dashboard.exam.denominator_note}</p>
              </div>
              <div className="dashboard-rule-note">
                未提交、识别失败和证据不足都不是“能力差”；本页不读取或展示旧的 mastery 聚合。
              </div>
            </section>
          </div>

          <section className="dashboard-panel dashboard-students">
            <div className="dashboard-panel-head">
              <div><b>学生运行状态</b><span>便于核对谁没交、谁待终审；这里不做学生能力排名</span></div>
            </div>
            <div className="dashboard-table-wrap">
              <table className="tbl">
                <thead><tr><th>学生</th><th>今日背诵</th><th>当前作业</th><th>下一步</th></tr></thead>
                <tbody>
                  {dashboard.students.map((student) => (
                    <tr key={student.student_id}>
                      <td>
                        <b>{student.student_name}</b>
                        <span className="dashboard-student-no">{student.student_no}号</span>
                      </td>
                      <td>
                        <span className={tagClass(student.recitation_status)}>
                          {RECITATION_STATUS[student.recitation_status] ?? student.recitation_status}
                        </span>
                        <small className="dashboard-count">
                          {student.recitation_confirmed_task_count}/{student.recitation_due_task_count} 项
                        </small>
                      </td>
                      <td>
                        <span className={tagClass(student.exam_status)}>
                          {EXAM_STATUS[student.exam_status] ?? student.exam_status}
                        </span>
                        <small className="dashboard-count">
                          上传 {student.exam_submitted_submission_count}/{student.exam_expected_submission_count}
                          {" · "}发布 {student.exam_published_submission_count}
                        </small>
                      </td>
                      <td>
                        {["recognition_failed", "submitted", "pending_review", "partial"].includes(student.recitation_status) ? (
                          <button className="link" onClick={() => onNavigate("recitation", student.recitation_status === "recognition_failed" ? "desk" : "today")}>
                            看背诵
                          </button>
                        ) : ["ingesting", "grading", "ready_to_publish", "partial"].includes(student.exam_status) ? (
                          <button className="link" onClick={() => onNavigate("exam", "exam")}>看作业</button>
                        ) : <span className="muted">—</span>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </>
      )}
    </div>
  );
}
