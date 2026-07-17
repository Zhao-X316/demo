import { useEffect, useMemo, useState } from "react";

import {
  ClassWrongbookDashboard,
  loadClassWrongbookDashboard,
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

function ItemCard({ item }: { item: WrongbookQuestion }) {
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
          <button onClick={() => setRefreshTick((value) => value + 1)}>刷新</button>
        </div>
      </div>

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
                  <ItemCard item={item} key={`${item.student_id}-${item.question_version_id}`} />
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
